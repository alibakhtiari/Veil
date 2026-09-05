use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::account::{self, Identity};
use crate::error::{AetherError, Result};
use crate::{aethernoize, config, consts, dns, noize, prober, quic, wg_prober, wireguard, zerotrust};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Masque,
    WireGuard,
    /// WARP-in-WARP (`gool`): two WireGuard hops. Scans and tunnels
    /// reuse the WireGuard prober/path; the GUI distinguishes it so it
    /// can request two endpoints (`wanted = 2`) and show both hops.
    WarpInWarp,
}

impl Transport {
    pub fn parse(raw: &str) -> Transport {
        match raw.trim().to_lowercase().as_str() {
            "wg" | "wireguard" | "warp" => Transport::WireGuard,
            "gool" | "wiw" | "warp-in-warp" | "warpinwarp" => Transport::WarpInWarp,
            _ => Transport::Masque,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Transport::Masque => "masque",
            Transport::WireGuard => "wireguard",
            Transport::WarpInWarp => "gool",
        }
    }

    pub fn assigned_port(&self) -> u16 {
        match self {
            Transport::Masque => 443,
            Transport::WireGuard | Transport::WarpInWarp => 2408,
        }
    }

    pub fn default_ports(&self) -> Vec<u16> {
        match self {
            Transport::Masque => prober::MASQUE_PORTS.to_vec(),
            Transport::WireGuard | Transport::WarpInWarp => wireguard::WG_PORTS.to_vec(),
        }
    }
}

#[derive(Clone)]
pub struct Cancel {
    sender: Arc<tokio::sync::watch::Sender<bool>>,
}

impl Default for Cancel {
    fn default() -> Self {
        Self::new()
    }
}

impl Cancel {
    pub fn new() -> Self {
        let (sender, _) = tokio::sync::watch::channel(false);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub fn cancel(&self) {
        self.sender.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.sender.borrow()
    }

    pub async fn wait(&self) {
        let mut receiver = self.sender.subscribe();
        if *receiver.borrow_and_update() {
            return;
        }
        while receiver.changed().await.is_ok() {
            if *receiver.borrow_and_update() {
                return;
            }
        }
        std::future::pending::<()>().await
    }
}

async fn guard<T, F>(cancel: &Cancel, work: F) -> Result<T>
where
    F: std::future::Future<Output = Result<T>>,
{
    tokio::select! {
        biased;
        _ = cancel.wait() => Err(AetherError::Cancelled),
        outcome = work => outcome,
    }
}

#[derive(Debug, Clone, Default)]
pub struct TeamCredentials {
    pub team: String,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub token: Option<String>,
    pub email: Option<String>,
}

impl TeamCredentials {
    pub fn new(team: &str) -> Result<Self> {
        let team = zerotrust::normalize_team(team).ok_or_else(|| {
            AetherError::Api(format!("'{team}' is not a usable zero trust team name"))
        })?;
        Ok(Self {
            team,
            ..Default::default()
        })
    }

    pub fn with_service_token(mut self, client_id: &str, client_secret: &str) -> Self {
        self.client_id = Some(client_id.to_string());
        self.client_secret = Some(client_secret.to_string());
        self
    }

    pub fn with_token(mut self, token: &str) -> Self {
        self.token = Some(token.to_string());
        self
    }

    pub fn with_email(mut self, email: &str) -> Self {
        self.email = Some(email.to_string());
        self
    }

    pub fn settings(&self) -> zerotrust::TeamSettings {
        zerotrust::TeamSettings {
            team: self.team.clone(),
            client_id: self.client_id.clone(),
            client_secret: self.client_secret.clone(),
            token: self.token.clone(),
            email: self.email.clone(),
        }
    }

    pub fn login_url(&self) -> String {
        self.settings().login_url()
    }
}

pub async fn team_sign_in(credentials: &TeamCredentials) -> Result<String> {
    zerotrust::resolve_token(&credentials.settings()).await
}

pub async fn team_email_code_request(
    credentials: &TeamCredentials,
    email: &str,
) -> Result<zerotrust::EmailSignIn> {
    // Tell the GUI to show its code-entry screen; the code itself
    // arrives later via team_email_code_submit.
    crate::events::emit(crate::events::ApiEvent::AuthNeeded {
        team: credentials.team.clone(),
    });
    zerotrust::begin_email_signin(&credentials.settings(), email).await
}

/// Report data-plane counters. Called by whatever observes traffic
/// (today: GUI backends polling the netstack; later: the TUN front
/// end). Emits `ApiEvent::Stats` for the session rx/tx counters.
pub fn report_stats(rx_bytes: u64, tx_bytes: u64) {
    crate::events::emit(crate::events::ApiEvent::Stats {
        rx_bytes,
        tx_bytes,
    });
}

/// The last known-good gateway behind the "Reuse?" card: reads
/// `*-lastconn.toml` next to the identity path. `None` when there is
/// no cache or the saved peer no longer parses (caller rescans).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedEndpoint {
    pub peer: SocketAddr,
    pub profile: String,
}

pub fn cached_endpoint(identity_path: &str) -> Option<CachedEndpoint> {
    let saved = crate::lastconn::load(&lastconn_path(identity_path))?;
    let peer: SocketAddr = saved.peer.trim().parse().ok()?;
    Some(CachedEndpoint {
        peer,
        profile: saved.profile,
    })
}

pub async fn team_email_code_resend(session: &mut zerotrust::EmailSignIn) -> Result<()> {
    session.resend_code().await
}

pub async fn team_email_code_submit(
    session: &zerotrust::EmailSignIn,
    code: &str,
) -> Result<Option<String>> {
    match session.submit_code(code).await? {
        zerotrust::CodeOutcome::Token(token) => {
            zerotrust::store_token(&token).await?;
            Ok(Some(token))
        }
        zerotrust::CodeOutcome::Rejected(status) => {
            log::warn!("[-] the login code was not accepted (status {status})");
            Ok(None)
        }
    }
}

pub async fn team_use_token(token: &str) -> Result<()> {
    zerotrust::store_token(token).await
}

pub async fn team_current_token() -> Option<String> {
    zerotrust::cached_token().await
}

pub async fn team_forget_token() {
    zerotrust::clear_token().await
}

#[derive(Debug, Clone)]
pub struct ProvisionRequest {
    pub model: String,
    pub locale: String,
    pub team: Option<TeamCredentials>,
    pub masque_cert: bool,
}

impl Default for ProvisionRequest {
    fn default() -> Self {
        Self {
            model: consts::DEFAULT_MODEL.to_string(),
            locale: consts::DEFAULT_LOCALE.to_string(),
            team: None,
            masque_cert: false,
        }
    }
}

impl ProvisionRequest {
    pub fn for_transport(transport: Transport) -> Self {
        Self {
            masque_cert: matches!(transport, Transport::Masque),
            ..Default::default()
        }
    }

    pub fn in_team(mut self, credentials: TeamCredentials) -> Self {
        self.team = Some(credentials);
        self
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IdentitySummary {
    pub device_id: String,
    pub ipv4: String,
    pub ipv6: String,
    pub organization: String,
    pub gateway_proxy: String,
    pub assigned_endpoint: String,
    pub has_masque_cert: bool,
    pub cert_issued_at: u64,
    pub cert_usable: bool,
}

impl IdentitySummary {
    pub fn of(identity: &Identity) -> Self {
        Self {
            device_id: identity.device_id.clone(),
            ipv4: identity.ipv4.clone(),
            ipv6: identity.ipv6.clone(),
            organization: identity.organization.clone(),
            gateway_proxy: identity.gateway_proxy.clone(),
            assigned_endpoint: identity.assigned_endpoint.clone(),
            has_masque_cert: identity.has_masque_credentials(),
            cert_issued_at: identity.cert_issued_at,
            cert_usable: account::cert_still_usable(identity),
        }
    }
}

pub fn identity_path(base: &str, transport: Transport, team: Option<&str>) -> String {
    match team {
        Some(team) => crate::derive_sibling_path(base, &format!("team-{team}")),
        None => match transport {
            Transport::Masque => crate::derive_sibling_path(base, "masque"),
            Transport::WireGuard | Transport::WarpInWarp => base.to_string(),
        },
    }
}

pub fn lastconn_path(identity_path: &str) -> String {
    crate::lastconn_path(identity_path)
}

pub fn load_identity(path: &str) -> Result<Option<Identity>> {
    config::load(path)
}

pub fn save_identity(path: &str, identity: &Identity) -> Result<()> {
    config::save(path, identity)
}

pub async fn provision_identity(request: &ProvisionRequest) -> Result<Identity> {
    let identity = match &request.team {
        Some(team) => {
            let settings = team.settings();
            log::info!(
                "[*] enrolling this device into the zero trust organization {} ({})",
                settings.team,
                settings.team_domain()
            );
            let identity =
                account::provision_team(&request.model, &request.locale, &settings).await?;
            account::refresh_profile(identity).await
        }
        None => account::provision_wg(&request.model, &request.locale, None).await?,
    };

    if request.masque_cert {
        return attach_masque_cert(identity).await;
    }
    Ok(identity)
}

pub async fn refresh_identity(identity: Identity) -> Identity {
    account::refresh_profile(identity).await
}

pub async fn attach_masque_cert(identity: Identity) -> Result<Identity> {
    if identity.has_masque_credentials() && !account::masque_cert_expiring(identity.cert_issued_at) {
        return Ok(identity);
    }

    let enrollment = account::ensure_masque_enrolled(&identity).await?;
    Ok(Identity {
        cert_pem: enrollment.cert_pem,
        key_pem: enrollment.key_pem,
        cert_issued_at: enrollment.issued_at,
        ..identity
    })
}

/// Both WireGuard identities a warp-in-warp tunnel needs. Paths mirror
/// the engine (`lib.rs::run_gool`): the primary lives at the warp path and
/// the secondary beside it as `<config>-secondary.toml`.
#[derive(Debug, Clone)]
pub struct GoolIdentities {
    pub primary: Identity,
    pub secondary: Identity,
    pub primary_path: String,
    pub secondary_path: String,
}

/// Pure path derivation for a gool pair (no I/O): primary at the
/// WireGuard identity path (team-aware), secondary beside it.
pub fn gool_paths(base: &str, team: Option<&str>) -> (String, String) {
    let primary = identity_path(base, Transport::WireGuard, team);
    let secondary = crate::derive_sibling_path(&primary, "secondary");
    (primary, secondary)
}

/// Provision (or load) both gool identities. `team` carries whichever
/// sign-in method the caller configured (service token, token, email);
/// `None` means a personal (non-team) pair.
pub async fn open_gool_identities(
    base: &str,
    team: Option<TeamCredentials>,
) -> Result<GoolIdentities> {
    let team_name = team.as_ref().map(|t| t.team.clone());
    let (primary_path, secondary_path) = gool_paths(base, team_name.as_deref());

    let mut primary_req = ProvisionRequest::for_transport(Transport::WireGuard);
    primary_req.team = team.clone();
    let primary = open_identity(&primary_path, &primary_req).await?;

    let mut secondary_req = ProvisionRequest::for_transport(Transport::WireGuard);
    secondary_req.team = team;
    let secondary = open_identity(&secondary_path, &secondary_req).await?;

    Ok(GoolIdentities {
        primary,
        secondary,
        primary_path,
        secondary_path,
    })
}

pub async fn open_identity(path: &str, request: &ProvisionRequest) -> Result<Identity> {
    if let Some(identity) = load_identity(path)? {
        log::info!("[+] loaded an existing identity from {path}");
        let identity = match request.team.is_some() {
            true => refresh_identity(identity).await,
            false => identity,
        };
        let identity = match request.masque_cert {
            true => attach_masque_cert(identity).await?,
            false => identity,
        };
        save_identity(path, &identity)?;
        return Ok(identity);
    }

    log::info!("[+] no identity at {path}; provisioning a new one");
    let identity = provision_identity(request).await?;
    save_identity(path, &identity)?;
    Ok(identity)
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Endpoint {
    pub ip: IpAddr,
    pub port: u16,
    pub rtt_ms: u64,
}

impl Endpoint {
    pub fn socket(&self) -> SocketAddr {
        SocketAddr::new(self.ip, self.port)
    }
}

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub transport: Transport,
    pub mode: String,
    pub ip: prober::IpScan,
    pub ports: Vec<u16>,
    pub excluded: HashSet<SocketAddr>,
    pub ech_config_list: Option<Vec<u8>>,
    pub noize: noize::NoizeConfig,
    pub aethernoize: aethernoize::AetherNoizeConfig,
    /// How many endpoints the caller wants. `1` for masque/wg, `2` for
    /// a gool scan (outer + inner). GUI sets this; `scan()` for gool
    /// currently returns the best endpoint and the GUI scans twice
    /// with `excluded` carrying the other hop (mirrors
    /// `lib.rs::select_wg_peers(..., avoid)`).
    pub wanted: usize,
}

impl ScanRequest {
    pub fn for_transport(transport: Transport) -> Self {
        let wanted = match transport {
            Transport::WarpInWarp => 2,
            _ => 1,
        };
        Self {
            transport,
            mode: "balanced".to_string(),
            ip: prober::IpScan::V4,
            ports: transport.default_ports(),
            excluded: HashSet::new(),
            ech_config_list: None,
            noize: noize::from_profile("firewall"),
            aethernoize: aethernoize::from_profile("balanced"),
            wanted,
        }
    }

    pub fn with_wanted(mut self, wanted: usize) -> Self {
        self.wanted = wanted.max(1);
        self
    }

    pub fn with_mode(mut self, mode: &str) -> Self {
        self.mode = mode.to_string();
        self
    }

    pub fn with_ip(mut self, ip: prober::IpScan) -> Self {
        self.ip = ip;
        self
    }

    pub fn with_profile(mut self, profile: &str) -> Self {
        self.noize = noize::from_profile(profile);
        self.aethernoize = aethernoize::from_profile(profile);
        self
    }
}

pub async fn scan(
    identity: &Identity,
    request: &ScanRequest,
    cancel: &Cancel,
) -> Result<Endpoint> {
    crate::events::emit(crate::events::ApiEvent::ScanStarted {
        transport: request.transport.label().to_string(),
        mode: request.mode.clone(),
    });
    crate::guilog::push(
        crate::guilog::GuiLogLevel::Info,
        "aether::api",
        &format!(
            "scan started: transport={} mode={}",
            request.transport.label(),
            request.mode
        ),
    );
    let outcome = scan_inner(identity, request, cancel).await;
    match &outcome {
        Ok(ep) => {
            let peer = ep.socket();
            crate::events::emit(crate::events::ApiEvent::scan_done(&peer, ep.rtt_ms));
            crate::events::emit(crate::events::ApiEvent::StateChanged {
                state: "verifying".to_string(),
            });
        }
        Err(e) => {
            crate::events::emit(crate::events::ApiEvent::Error {
                message: e.to_string(),
            });
        }
    }
    outcome
}

async fn scan_inner(
    identity: &Identity,
    request: &ScanRequest,
    cancel: &Cancel,
) -> Result<Endpoint> {
    match request.transport {
        Transport::Masque => {
            let probe = prober::MasqueProbe {
                sni: consts::CONNECT_SNI.to_string(),
                authority: quic::default_authority().to_string(),
                path: quic::default_path().to_string(),
                cert_pem: Arc::from(identity.cert_pem.clone()),
                key_pem: Arc::from(identity.key_pem.clone()),
                ech_config_list: request.ech_config_list.clone().map(Arc::from),
                noize: request.noize.clone(),
                ports: request.ports.clone(),
                ip: request.ip,
                local_ipv4: crate::parse_local_v4(&identity.ipv4),
            };
            let mode = prober::ScanMode::parse(&request.mode);
            let best = guard(cancel, prober::hunt_best_gateway(&probe, mode)).await?;
            Ok(Endpoint {
                ip: best.ip,
                port: best.port,
                rtt_ms: best.rtt.as_millis() as u64,
            })
        }
        Transport::WireGuard | Transport::WarpInWarp => {
            let probe = wg_prober::WgProbe {
                private_key: Arc::new(identity.private_key_bytes()?),
                peer_public_key: Arc::new(identity.peer_public_key_bytes()?),
                client_id: identity.client_id,
                local_ipv4: wg_local_v4(identity)?,
                aethernoize: request.aethernoize.clone(),
                ports: request.ports.clone(),
                ip: request.ip,
                excluded: request.excluded.clone(),
            };
            let mode = wg_prober::WgScanMode::parse(&request.mode);
            let best = guard(cancel, wg_prober::hunt_best_wg_endpoint(&probe, mode)).await?;
            Ok(Endpoint {
                ip: best.ip,
                port: best.port,
                rtt_ms: best.rtt.as_millis() as u64,
            })
        }
    }
}

fn wg_local_v4(identity: &Identity) -> Result<Ipv4Addr> {
    identity
        .ipv4
        .parse()
        .map_err(|_| AetherError::Other(format!("identity has an unusable ipv4 {}", identity.ipv4)))
}

#[derive(Debug, Clone)]
pub struct TunnelSpec {
    pub transport: Transport,
    pub socks: SocketAddr,
    pub http: Option<SocketAddr>,
    pub ech: Option<Vec<u8>>,
    pub aethernoize: aethernoize::AetherNoizeConfig,
    pub keepalive: u16,
    pub verify_timeout: Duration,
    /// Gool hops. `None` for masque/wg. For gool, `outer` is the peer
    /// the network sees; [`connect_gool`] dials both. Plain [`connect`]
    /// with a gool spec dials `outer` (or the `peer` fallback) as a
    /// single hop so existing single-peer callers keep working.
    pub outer: Option<SocketAddr>,
    pub inner: Option<SocketAddr>,
}

impl TunnelSpec {
    pub fn for_transport(transport: Transport) -> Self {
        Self {
            transport,
            socks: SocketAddr::from(([127, 0, 0, 1], 1819)),
            http: None,
            ech: None,
            aethernoize: aethernoize::from_profile("balanced"),
            keepalive: 5,
            verify_timeout: Duration::from_secs(10),
            outer: None,
            inner: None,
        }
    }

    /// Gool convenience: both hops at once. Validates they differ,
    /// with the same wording as `lib.rs` / `gui::validate_wiw_pair`.
    pub fn with_gool_peers(mut self, outer: SocketAddr, inner: SocketAddr) -> Result<Self> {
        if outer.ip() == inner.ip() {
            return Err(AetherError::Other(format!(
                "warp-in-warp needs two separate edges, but both hops point at {}",
                outer.ip()
            )));
        }
        self.transport = Transport::WarpInWarp;
        self.outer = Some(outer);
        self.inner = Some(inner);
        Ok(self)
    }

    pub fn is_gool(&self) -> bool {
        matches!(self.transport, Transport::WarpInWarp)
    }

    pub fn with_socks(mut self, listen: SocketAddr) -> Self {
        self.socks = listen;
        self
    }

    pub fn with_http(mut self, listen: SocketAddr) -> Self {
        self.http = Some(listen);
        self
    }

    pub fn with_profile(mut self, profile: &str) -> Self {
        self.aethernoize = aethernoize::from_profile(profile);
        self
    }
}

pub async fn fetch_ech_config() -> Option<Vec<u8>> {
    match dns::fetch_ech_config().await {
        Ok(raw) => {
            log::info!("[+] fetched an ECHConfigList ({} bytes)", raw.len());
            Some(raw)
        }
        Err(e) => {
            log::warn!("[-] could not fetch an ECHConfigList ({e}); continuing without ECH");
            None
        }
    }
}

pub async fn verify_endpoint(
    identity: &Identity,
    peer: SocketAddr,
    spec: &TunnelSpec,
    cancel: &Cancel,
) -> Result<bool> {
    crate::events::emit(crate::events::ApiEvent::Validating {
        peer: peer.to_string(),
    });
    let outcome = verify_inner(identity, peer, spec, cancel).await;
    if outcome.as_ref().is_ok_and(|ok| *ok) {
        crate::events::emit(crate::events::ApiEvent::VerifyOk {
            peer: peer.to_string(),
        });
    }
    outcome
}

async fn verify_inner(
    identity: &Identity,
    peer: SocketAddr,
    spec: &TunnelSpec,
    cancel: &Cancel,
) -> Result<bool> {
    match spec.transport {
        Transport::Masque => {
            let attempt = async { Ok(crate::quick_verify_masque_peer(identity, peer).await) };
            guard(cancel, attempt).await
        }
        Transport::WireGuard | Transport::WarpInWarp => {
            let private_key = identity.private_key_bytes()?;
            let peer_public = identity.peer_public_key_bytes()?;
            let local_ipv4 = wg_local_v4(identity)?;
            let attempt = wireguard::verify_endpoint(
                peer,
                private_key,
                peer_public,
                identity.client_id,
                local_ipv4,
                &spec.aethernoize,
                spec.verify_timeout,
                Some(spec.keepalive),
            );
            match guard(cancel, attempt).await {
                Ok(_) => Ok(true),
                Err(AetherError::Cancelled) => Err(AetherError::Cancelled),
                Err(e) => {
                    log::debug!("[-] {peer} did not verify: {e}");
                    Ok(false)
                }
            }
        }
    }
}

pub async fn connect(
    identity: &Identity,
    peer: SocketAddr,
    spec: &TunnelSpec,
    cancel: &Cancel,
) -> Result<()> {
    match spec.http {
        Some(listen) => std::env::set_var("AETHER_HTTP_PROXY", listen.to_string()),
        None => std::env::remove_var("AETHER_HTTP_PROXY"),
    }

    // A gool spec through plain `connect` dials its outer hop exactly
    // like WireGuard (single-hop fallback); the full two-hop tunnel is
    // `connect_gool`. The GUI drives both hops' scan+verify either way.
    let peer = match spec.transport {
        Transport::WarpInWarp => spec.outer.unwrap_or(peer),
        _ => peer,
    };
    // NOTE: this is "connecting", not "up": the tunnel reports
    // readiness internally (SOCKS opens after data-plane validation)
    // and there is no readiness hook back into this function yet.
    // Emitting TunnelUp here would show Connected during the
    // handshake, so we emit a state change and let a future netstack
    // hook emit the true TunnelUp (see Phase-4 hardening).
    crate::events::emit(crate::events::ApiEvent::StateChanged {
        state: "connecting".to_string(),
    });
    crate::guilog::push(
        crate::guilog::GuiLogLevel::Info,
        "aether::api",
        &format!("tunnel connecting: transport={} peer={peer}", spec.transport.label()),
    );

    let outcome = match spec.transport {
        Transport::Masque => {
            let attempt = crate::run_masque_tunnel(identity, peer, spec.ech.clone(), spec.socks);
            guard(cancel, attempt).await
        }
        Transport::WireGuard | Transport::WarpInWarp => {
            let attempt = crate::run_wireguard_tunnel(
                identity.clone(),
                peer,
                spec.aethernoize.clone(),
                spec.socks,
            );
            guard(cancel, attempt).await
        }
    };
    match &outcome {
        Ok(()) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: "closed".to_string(),
        }),
        Err(AetherError::Cancelled) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: "stopped".to_string(),
        }),
        Err(e) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: e.to_string(),
        }),
    }
    outcome
}

/// Connect the full two-hop warp-in-warp tunnel (GUI_PLAN.md Gap 2).
/// Unlike [`connect`] (single peer), both hops are explicit: GUI scans
/// each hop with [`scan`] first (excluding the other hop), provisions
/// with [`open_gool_identities`], then calls this. Runs until `cancel`
/// fires or either leg drops.
pub async fn connect_gool(
    ids: &GoolIdentities,
    outer: SocketAddr,
    inner: SocketAddr,
    spec: &TunnelSpec,
    cancel: &Cancel,
) -> Result<()> {
    if outer.ip() == inner.ip() {
        return Err(AetherError::Other(format!(
            "warp-in-warp needs two separate edges, but both hops point at {}",
            outer.ip()
        )));
    }
    match spec.http {
        Some(listen) => std::env::set_var("AETHER_HTTP_PROXY", listen.to_string()),
        None => std::env::remove_var("AETHER_HTTP_PROXY"),
    }

    crate::events::emit(crate::events::ApiEvent::StateChanged {
        state: "connecting".to_string(),
    });
    crate::guilog::push(
        crate::guilog::GuiLogLevel::Info,
        "aether::api",
        &format!("gool tunnel connecting: outer={outer} inner={inner}"),
    );

    let attempt = crate::run_warp_in_warp(
        ids.primary.clone(),
        ids.secondary.clone(),
        outer,
        inner,
        spec.socks,
    );
    let outcome = guard(cancel, attempt).await;
    match &outcome {
        Ok(()) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: "closed".to_string(),
        }),
        Err(AetherError::Cancelled) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: "stopped".to_string(),
        }),
        Err(e) => crate::events::emit(crate::events::ApiEvent::TunnelDown {
            reason: e.to_string(),
        }),
    }
    outcome
}

/// Wait until the local SOCKS listener accepts connections, proving the
/// tunnel is ready without touching the tunnel loops: polls TCP connect
/// until `timeout`. Emits `ApiEvent::TunnelUp` on success — this is what
/// the GUI backend waits on before arming the traffic mode
/// (`AppState::on_tunnel_up`), so the system proxy / TUN never points
/// at a socket that is not listening yet.
pub async fn wait_for_socks(
    peer: SocketAddr,
    transport: Transport,
    timeout: Duration,
) -> Result<()> {
    let start = Instant::now();
    loop {
        match tokio::net::TcpStream::connect(peer).await {
            Ok(_) => {
                crate::events::emit(crate::events::ApiEvent::tunnel_up(
                    &peer,
                    transport.label(),
                ));
                crate::guilog::push(
                    crate::guilog::GuiLogLevel::Info,
                    "aether::api",
                    &format!(
                        "tunnel ready: transport={} socks={peer} after {:?}",
                        transport.label(),
                        start.elapsed()
                    ),
                );
                return Ok(());
            }
            Err(_) if start.elapsed() < timeout => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => {
                return Err(AetherError::Other(format!(
                    "socks listener {peer} not accepting after {:?}: {e}",
                    start.elapsed()
                )));
            }
        }
    }
}

/// Headless GUI flow driver (no stdin): open → scan → verify → connect.
/// Used by the GUI backend and the Phase-0 integration test. Returns
/// the verified endpoint; `connect` runs until `cancel` fires.
pub async fn connect_headless(
    identity: &Identity,
    scan_req: &ScanRequest,
    spec: &TunnelSpec,
    cancel: &Cancel,
) -> Result<Endpoint> {
    crate::events::emit(crate::events::ApiEvent::Provisioning {
        transport: scan_req.transport.label().to_string(),
    });
    let endpoint = scan(identity, scan_req, cancel).await?;
    let peer = endpoint.socket();
    if !verify_endpoint(identity, peer, spec, cancel).await? {
        return Err(AetherError::Other(format!(
            "{peer} did not pass data-plane verification"
        )));
    }
    connect(identity, peer, spec, cancel).await?;
    Ok(endpoint)
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transport_is_read_from_standard_names_and_aliases() {
        assert_eq!(Transport::parse("wg"), Transport::WireGuard);
        assert_eq!(Transport::parse("WireGuard"), Transport::WireGuard);
        assert_eq!(Transport::parse("warp"), Transport::WireGuard);
        assert_eq!(Transport::parse("gool"), Transport::WarpInWarp);
        assert_eq!(Transport::parse("wiw"), Transport::WarpInWarp);
        assert_eq!(Transport::parse("warp-in-warp"), Transport::WarpInWarp);
        assert_eq!(Transport::parse("masque"), Transport::Masque);
        assert_eq!(Transport::parse("anything else"), Transport::Masque);
    }

    #[test]
    fn each_transport_carries_the_port_the_edge_assigns() {
        assert_eq!(Transport::Masque.assigned_port(), 443);
        assert_eq!(Transport::WireGuard.assigned_port(), 2408);
        assert_eq!(Transport::WarpInWarp.assigned_port(), 2408);
        assert_eq!(Transport::WarpInWarp.label(), "gool");
        assert!(!Transport::Masque.default_ports().is_empty());
        assert!(!Transport::WireGuard.default_ports().is_empty());
        assert!(!Transport::WarpInWarp.default_ports().is_empty());
    }

    #[test]
    fn a_team_name_is_normalized_before_it_is_kept() {
        let credentials = TeamCredentials::new("https://My-Org.cloudflareaccess.com/warp")
            .expect("a full url is usable");
        assert_eq!(credentials.team, "my-org");
        assert_eq!(
            credentials.login_url(),
            "https://my-org.cloudflareaccess.com/warp"
        );
        assert!(TeamCredentials::new("bad name!").is_err());
    }

    #[test]
    fn credentials_carry_whichever_sign_in_method_was_given() {
        let service = TeamCredentials::new("acme")
            .expect("team")
            .with_service_token("id.access", "secret");
        assert!(service.settings().has_service_token());

        let by_email = TeamCredentials::new("acme")
            .expect("team")
            .with_email("me@example.com");
        assert!(!by_email.settings().has_service_token());
        assert_eq!(by_email.email.as_deref(), Some("me@example.com"));
    }

    #[test]
    fn identity_paths_follow_standard_naming_convention() {
        assert_eq!(
            identity_path("aether.toml", Transport::Masque, None),
            "aether-masque.toml"
        );
        assert_eq!(
            identity_path("aether.toml", Transport::WireGuard, None),
            "aether.toml"
        );
        assert_eq!(
            identity_path("aether.toml", Transport::Masque, Some("acme")),
            "aether-team-acme.toml"
        );
        assert_eq!(
            identity_path("/var/lib/aether/aether.toml", Transport::WireGuard, Some("acme")),
            "/var/lib/aether/aether-team-acme.toml"
        );
    }

    #[test]
    fn a_scan_request_starts_from_standard_engine_defaults() {
        let masque = ScanRequest::for_transport(Transport::Masque);
        assert_eq!(masque.mode, "balanced");
        assert_eq!(masque.ip, prober::IpScan::V4);
        assert_eq!(masque.ports, prober::MASQUE_PORTS.to_vec());

        let wg = ScanRequest::for_transport(Transport::WireGuard).with_mode("turbo");
        assert_eq!(wg.mode, "turbo");
        assert_eq!(wg.ports, wireguard::WG_PORTS.to_vec());
    }

    #[test]
    fn a_tunnel_spec_defaults_to_the_loopback_socks_port() {
        let spec = TunnelSpec::for_transport(Transport::Masque);
        assert_eq!(spec.socks.port(), 1819);
        assert!(spec.socks.ip().is_loopback());
        assert!(spec.http.is_none());

        let with_http = spec.with_http(SocketAddr::from(([127, 0, 0, 1], 8086)));
        assert_eq!(with_http.http.map(|addr| addr.port()), Some(8086));
    }

    #[tokio::test]
    async fn a_cancel_token_is_seen_by_everyone_holding_a_copy() {
        let cancel = Cancel::new();
        let copy = cancel.clone();
        assert!(!cancel.is_cancelled());

        copy.cancel();
        assert!(cancel.is_cancelled());
        cancel.wait().await;
    }

    #[tokio::test]
    async fn cancelling_stops_the_work_it_guards() {
        let cancel = Cancel::new();
        cancel.cancel();

        let outcome: Result<u8> = guard(&cancel, async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(1)
        })
        .await;

        assert!(matches!(outcome, Err(AetherError::Cancelled)));
    }

    #[tokio::test]
    async fn work_that_finishes_first_is_not_treated_as_cancelled() {
        let cancel = Cancel::new();
        let outcome: Result<u8> = guard(&cancel, async { Ok(7) }).await;
        assert!(matches!(outcome, Ok(7)));
    }

    #[tokio::test]
    async fn cancelling_part_way_through_stops_the_work() {
        let cancel = Cancel::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            trigger.cancel();
        });

        let outcome: Result<u8> = guard(&cancel, async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            Ok(1)
        })
        .await;

        assert!(matches!(outcome, Err(AetherError::Cancelled)));
    }

    #[test]
    fn gool_scan_wants_two_endpoints_by_default() {
        assert_eq!(ScanRequest::for_transport(Transport::WarpInWarp).wanted, 2);
        assert_eq!(ScanRequest::for_transport(Transport::Masque).wanted, 1);
        let custom = ScanRequest::for_transport(Transport::Masque).with_wanted(0);
        assert_eq!(custom.wanted, 1);
    }

    #[test]
    fn gool_tunnel_spec_validates_distinct_hops() {
        let outer: SocketAddr = "162.159.192.1:2408".parse().unwrap();
        let inner: SocketAddr = "188.114.96.1:2408".parse().unwrap();
        let spec = TunnelSpec::for_transport(Transport::WireGuard)
            .with_gool_peers(outer, inner)
            .expect("distinct hops");
        assert!(spec.is_gool());
        assert_eq!(spec.outer, Some(outer));
        let same: SocketAddr = "162.159.192.1:2408".parse().unwrap();
        assert!(TunnelSpec::for_transport(Transport::WireGuard)
            .with_gool_peers(same, same)
            .is_err());
    }

    #[test]
    fn gool_identity_uses_the_wireguard_file() {
        assert_eq!(
            identity_path("aether.toml", Transport::WarpInWarp, None),
            "aether.toml"
        );
    }

    #[test]
    fn gool_paths_put_the_secondary_beside_the_primary() {
        assert_eq!(
            gool_paths("aether.toml", None),
            (
                "aether.toml".to_string(),
                "aether-secondary.toml".to_string()
            )
        );
        assert_eq!(
            gool_paths("aether.toml", Some("acme")),
            (
                "aether-team-acme.toml".to_string(),
                "aether-team-acme-secondary.toml".to_string()
            )
        );
        assert_eq!(
            gool_paths("/var/lib/aether/aether.toml", None),
            (
                "/var/lib/aether/aether.toml".to_string(),
                "/var/lib/aether/aether-secondary.toml".to_string()
            )
        );
    }

    #[test]
    fn gool_connect_rejects_same_edge_before_any_io() {
        // Fake identities + a pre-cancelled token would also work, but
        // the distinct-edges check must fire first: no socket, no events
        // needed, pure validation.
        let _guard = crate::events::lock_for_test();
        let identity = crate::account::Identity {
            device_id: "test".to_string(),
            access_token: "test".to_string(),
            cert_pem: Vec::new(),
            key_pem: Vec::new(),
            cert_issued_at: 0,
            ipv4: "172.16.0.2".to_string(),
            ipv6: "::1".to_string(),
            wg_private_key: [7u8; 32],
            wg_peer_public_key: [9u8; 32],
            client_id: [1, 2, 3],
            organization: String::new(),
            gateway_proxy: String::new(),
            assigned_endpoint: String::new(),
            refused: false,
        };
        let ids = GoolIdentities {
            primary: identity.clone(),
            secondary: identity,
            primary_path: "a.toml".to_string(),
            secondary_path: "a-secondary.toml".to_string(),
        };
        let spec = TunnelSpec::for_transport(Transport::WarpInWarp);
        let cancel = Cancel::new();
        let same: SocketAddr = "162.159.192.1:2408".parse().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let outcome = runtime.block_on(connect_gool(&ids, same, same, &spec, &cancel));
        assert!(outcome.is_err());
        assert!(outcome
            .expect_err("same edge twice")
            .to_string()
            .contains("two separate edges"));
    }

    #[test]
    fn cached_endpoint_reads_the_lastconn_file() {
        let dir = std::env::temp_dir().join(format!("aether-cache-test-{}", std::process::id()));
        // Remove first: a previously killed run could leave a stale
        // lastconn file that would break the is_none() assert below.
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let base = dir.join("aether.toml");
        let base = base.to_str().unwrap();
        assert!(cached_endpoint(base).is_none());
        crate::lastconn::save(&lastconn_path(base), "162.159.192.1:443", "firewall");
        let cached = cached_endpoint(base).expect("just saved");
        assert_eq!(cached.peer.port(), 443);
        assert_eq!(cached.profile, "firewall");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reported_stats_show_up_in_the_event_stream() {
        let _guard = crate::events::lock_for_test();
        crate::events::clear();
        report_stats(1024, 2048);
        let events = crate::events::drain_events(0);
        assert!(events.iter().any(|e| matches!(
            e,
            crate::events::ApiEvent::Stats {
                rx_bytes: 1024,
                tx_bytes: 2048
            }
        )));
    }

    #[tokio::test]
    async fn wait_for_socks_reports_readiness_and_timeout() {
        let _guard = crate::events::lock_for_test();
        crate::events::clear();

        // Ready listener:bind ephemeral loopback, TunnelUp must fire.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback bind");
        let addr = listener.local_addr().expect("local addr");
        wait_for_socks(addr, Transport::Masque, Duration::from_secs(5))
            .await
            .expect("listener is up");
        let events = crate::events::drain_events(0);
        assert!(events.iter().any(|e| matches!(
            e,
            crate::events::ApiEvent::TunnelUp { peer, transport }
            if peer == &addr.to_string() && transport == "masque"
        )));

        // Closed port: must time out quickly with an error naming it.
        let closed: SocketAddr = "127.0.0.1:1".parse().unwrap();
        let err = wait_for_socks(closed, Transport::WireGuard, Duration::from_millis(250))
            .await
            .expect_err("nothing listens on port 1");
        assert!(err.to_string().contains("127.0.0.1:1"));
    }

    #[tokio::test]
    async fn headless_flow_with_precancelled_token_stops_without_network() {
        // Phase-0 headless contract: a cancelled token short-circuits
        // before any socket is opened, and emits at least ScanStarted.
        let _guard = crate::events::lock_for_test();
        crate::events::clear();
        let cancel = Cancel::new();
        cancel.cancel();
        // Minimal fake identity: only fields `scan` touches before the
        // network are read after the cancel guard fires, so empty keys
        // are fine — the guard wins first.
        let identity = crate::account::Identity {
            device_id: "test".to_string(),
            access_token: "test".to_string(),
            cert_pem: Vec::new(),
            key_pem: Vec::new(),
            cert_issued_at: 0,
            ipv4: "172.16.0.2".to_string(),
            ipv6: "2606:4700::2".to_string(),
            wg_private_key: [7u8; 32],
            wg_peer_public_key: [9u8; 32],
            client_id: [1, 2, 3],
            organization: String::new(),
            gateway_proxy: String::new(),
            assigned_endpoint: String::new(),
            refused: false,
        };
        let scan = ScanRequest::for_transport(Transport::WireGuard);
        let spec = TunnelSpec::for_transport(Transport::WireGuard);
        let outcome = connect_headless(&identity, &scan, &spec, &cancel).await;
        assert!(matches!(outcome, Err(AetherError::Cancelled)));
        let events = crate::events::drain_events(0);
        assert!(
            events.iter().any(|e| matches!(
                e,
                crate::events::ApiEvent::ScanStarted { .. }
                    | crate::events::ApiEvent::Provisioning { .. }
            )),
            "headless run must emit progress events, got: {events:?}"
        );
    }
}
