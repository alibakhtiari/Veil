//! Thin backend commands over `aether::api` (Phase 1 scaffold).
//!
//! Rules (from §01): no stdout scraping, no duplicated scan/tunnel
//! logic, core error strings surface verbatim.
//! Every function is `async` + `Send` so wrapping it in
//! `#[tauri::command]` later is mechanical.

use std::net::SocketAddr;

use aether::gui::GuiSettings;

use crate::mode_manager::TrafficMode;
use crate::state::{AppState, ConnectionPhase};

/// Tauri/IPC-friendly snapshot: the tuple from `AppState::snapshot`
/// with the phase rendered to its label. Mirrors the frontend
/// `Snapshot` interface in `frontend/src/lib/api.ts`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotDto {
    pub phase: String,
    pub gateway: Option<String>,
    pub error: Option<String>,
}

pub async fn snapshot(app: &AppState) -> SnapshotDto {
    let (phase, gateway, error) = app.snapshot().await;
    SnapshotDto {
        phase: phase.as_str().to_string(),
        gateway,
        error,
    }
}

/// Stored traffic-mode preference label (`proxy`/`system`/`vpn`).
pub async fn get_traffic_mode(app: &AppState) -> String {
    app.traffic_mode().await.label().to_string()
}

/// Store the traffic-mode preference (no side effects: they run through
/// `ModeManager::on_connected` when a tunnel comes up). Returns the
/// canonical label. Unknown names are an error, never a silent default.
pub async fn set_traffic_mode(app: &AppState, mode: &str) -> Result<String, String> {
    let parsed = TrafficMode::parse(mode)?;
    app.set_traffic_mode(parsed).await;
    Ok(parsed.label().to_string())
}

/// Where the GUI stores `aether-gui.toml` per OS (§02-§3).
pub fn default_settings_path() -> String {
    if let Ok(path) = std::env::var("AETHER_GUI_CONFIG") {
        if !path.trim().is_empty() {
            return path;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(base) = std::env::var("APPDATA") {
            return format!("{base}\\Aether\\{0}", aether::gui::GUI_SETTINGS_FILE);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return format!(
                "{home}/Library/Application Support/Aether/{}",
                aether::gui::GUI_SETTINGS_FILE
            );
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/.config/aether/{}", aether::gui::GUI_SETTINGS_FILE);
        }
    }
    aether::gui::GUI_SETTINGS_FILE.to_string()
}

pub async fn get_settings(path: &str) -> Result<GuiSettings, String> {
    GuiSettings::load(path).map_err(|e| e.to_string())
}

pub async fn save_settings(path: &str, settings: GuiSettings) -> Result<(), String> {
    settings.validate().map_err(|e| e.to_string())?;
    settings.save(path).map_err(|e| e.to_string())
}

/// Provision (or load) the identity for the selected transport.
/// Returns the human-readable summary the header card shows.
pub async fn provision(
    app: &AppState,
    settings: &GuiSettings,
) -> Result<aether::api::IdentitySummary, String> {
    app.set_phase(ConnectionPhase::Provisioning).await?;
    settings.validate().map_err(|e| e.to_string())?;

    let transport = aether::api::Transport::parse(&settings.protocol);
    let base = std::env::var("AETHER_CONFIG").unwrap_or_else(|_| "aether.toml".to_string());
    let team: Option<String> = std::env::var("AETHER_TEAM").ok().filter(|t| !t.trim().is_empty());
    let path = aether::api::identity_path(&base, transport, team.as_deref());

    let mut request = aether::api::ProvisionRequest::for_transport(transport);
    if matches!(transport, aether::api::Transport::Masque) {
        request.masque_cert = true;
    }
    let identity = match aether::api::open_identity(&path, &request).await {
        Ok(identity) => identity,
        Err(e) => {
            app.set_error(e.to_string()).await;
            return Err(e.to_string());
        }
    };
    let summary = aether::api::IdentitySummary::of(&identity);
    app.store_identity(identity).await;
    aether::events::emit(aether::events::ApiEvent::IdentityReady {
        device_id: summary.device_id.clone(),
        transport: transport.label().to_string(),
    });
    app.set_phase(ConnectionPhase::Scanning).await?;
    Ok(summary)
}

/// Team credentials from the environment (`AETHER_TEAM` plus whichever
/// `AETHER_ACCESS_*` sign-in method is set). `None` means personal WARP.
/// Secrets are only read, never stored — they stay in env/keychain.
fn team_from_env() -> Result<Option<aether::api::TeamCredentials>, String> {
    let team = std::env::var("AETHER_TEAM")
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    let Some(team) = team else {
        return Ok(None);
    };
    let mut creds =
        aether::api::TeamCredentials::new(&team).map_err(|e| e.to_string())?;
    let opt = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    creds.client_id = opt("AETHER_ACCESS_CLIENT_ID");
    creds.client_secret = opt("AETHER_ACCESS_CLIENT_SECRET");
    creds.token = opt("AETHER_ACCESS_TOKEN");
    creds.email = opt("AETHER_ACCESS_EMAIL");
    Ok(Some(creds))
}

/// Provision (or load) both warp-in-warp identities. Stores the primary
/// as the app identity (hop scans use the primary's keys)
/// plus the full pair for `connect_gool_cmd`.
pub async fn provision_gool(
    app: &AppState,
    settings: &GuiSettings,
) -> Result<(aether::api::IdentitySummary, Vec<String>), String> {
    app.set_phase(ConnectionPhase::Provisioning).await?;
    settings.validate().map_err(|e| e.to_string())?;
    if aether::api::Transport::parse(&settings.protocol) != aether::api::Transport::WarpInWarp {
        return Err("provision_gool needs protocol \"gool\"".to_string());
    }

    let base = std::env::var("AETHER_CONFIG").unwrap_or_else(|_| "aether.toml".to_string());
    let team = team_from_env()?;
    let ids = match aether::api::open_gool_identities(&base, team).await {
        Ok(ids) => ids,
        Err(e) => {
            app.set_error(e.to_string()).await;
            return Err(e.to_string());
        }
    };
    let summary = aether::api::IdentitySummary::of(&ids.primary);
    aether::events::emit(aether::events::ApiEvent::IdentityReady {
        device_id: summary.device_id.clone(),
        transport: "gool".to_string(),
    });
    app.store_identity(ids.primary.clone()).await;
    app.store_gool(ids.clone()).await;
    app.set_phase(ConnectionPhase::Scanning).await?;
    Ok((
        summary,
        vec![ids.primary_path.clone(), ids.secondary_path.clone()],
    ))
}

/// Connect the full two-hop tunnel. Both hops must be set (scan each
/// first); cold start provisions the pair automatically, mirroring
/// `connect()`.
pub async fn connect_gool_cmd(app: &AppState, settings: &GuiSettings) -> Result<(), String> {
    if app.gool().await.is_none() {
        provision_gool(app, settings).await?;
    }
    if matches!(
        app.phase().await,
        ConnectionPhase::Connecting | ConnectionPhase::Connected
    ) {
        return Err("already connected or connecting".to_string());
    }
    let ids = app
        .gool()
        .await
        .ok_or_else(|| "no gool identities — provision first".to_string())?;
    let spec = tunnel_spec_for(settings)?;
    let (Some(outer), Some(inner)) = (spec.outer, spec.inner) else {
        return Err("both gool hops are needed — scan the outer and inner endpoints first".to_string());
    };
    let cancel = aether::api::Cancel::new();
    app.store_cancel(cancel.clone()).await;
    match app.phase().await {
        ConnectionPhase::Idle
        | ConnectionPhase::Scanning
        | ConnectionPhase::Verifying
        | ConnectionPhase::Stopped => {
            app.set_phase(ConnectionPhase::Connecting).await?;
        }
        _ => {}
    }
    match aether::api::connect_gool(&ids, outer, inner, &spec, &cancel).await {
        Ok(()) => {
            app.set_phase(ConnectionPhase::Stopped).await?;
            Ok(())
        }
        Err(aether::error::AetherError::Cancelled) => {
            app.set_phase(ConnectionPhase::Stopped).await?;
            Ok(())
        }
        Err(e) => {
            app.set_error(e.to_string()).await;
            Err(e.to_string())
        }
    }
}

/// Build the `ScanRequest` for the current settings (pure, testable).
pub fn scan_request_for(settings: &GuiSettings) -> Result<aether::api::ScanRequest, String> {
    settings.validate().map_err(|e| e.to_string())?;
    let transport = aether::api::Transport::parse(&settings.protocol);
    let mut request = aether::api::ScanRequest::for_transport(transport);
    request.mode = settings.scan.clone();
    request.ip = aether::prober::IpScan::parse(&settings.ip);
    request = request.with_profile(&settings.noize);
    if transport == aether::api::Transport::WarpInWarp {
        request = request.with_wanted(2);
        // A pinned hop stays out of the other hop's sweep (§01-§7).
        for raw in [&settings.wiw_outer, &settings.wiw_inner] {
            if let Ok(addr) = raw.trim().parse::<SocketAddr>() {
                for port in aether::wireguard::WG_PORTS {
                    request.excluded.insert(SocketAddr::new(addr.ip(), *port));
                }
            }
        }
    }
    Ok(request)
}

/// Build the `TunnelSpec` for the current settings (pure, testable).
/// Refuses non-loopback binds only via `needs_confirm` — the caller
/// shows the typed-confirm dialog; this function never blocks.
pub fn tunnel_spec_for(settings: &GuiSettings) -> Result<aether::api::TunnelSpec, String> {
    settings.validate().map_err(|e| e.to_string())?;
    let transport = aether::api::Transport::parse(&settings.protocol);
    let mut spec = aether::api::TunnelSpec::for_transport(transport);
    spec.socks = settings
        .socks
        .trim()
        .parse()
        .map_err(|_| format!("'{}' is not an address:port", settings.socks.trim()))?;
    if !settings.http_proxy.trim().is_empty() {
        spec.http = Some(settings.http_proxy.trim().parse().map_err(|_| {
            format!("'{}' is not an address:port", settings.http_proxy.trim())
        })?);
    }
    spec = spec.with_profile(&settings.noize);
    if transport == aether::api::Transport::WarpInWarp {
        let (outer, inner) =
            aether::gui::validate_wiw_pair(&settings.wiw_outer, &settings.wiw_inner)
                .map_err(|e| e.to_string())?;
        spec.outer = outer;
        spec.inner = inner;
    }
    Ok(spec)
}

/// True when the Connect button must show the loopback-confirm dialog
/// (covers the HTTP listener too — equally unauthenticated).
pub fn needs_bind_confirm(settings: &GuiSettings) -> bool {
    settings.binds_need_confirm()
}

async fn require_identity(app: &AppState) -> Result<aether::account::Identity, String> {
    app.identity()
        .await
        .ok_or_else(|| "no identity yet — provision first".to_string())
}

/// Run one endpoint scan (network I/O). Phase: Scanning → Verifying.
/// For gool this finds ONE hop; call twice with the other hop excluded
/// (see `scan_request_for`) and verify each.
pub async fn scan_once(
    app: &AppState,
    settings: &GuiSettings,
) -> Result<aether::api::Endpoint, String> {
    let identity = require_identity(app).await?;
    let request = scan_request_for(settings)?;
    let cancel = aether::api::Cancel::new();
    app.store_cancel(cancel.clone()).await;
    // Fresh scan and rescan-after-failed-verify both land on Scanning.
    match app.phase().await {
        ConnectionPhase::Provisioning | ConnectionPhase::Verifying => {
            app.set_phase(ConnectionPhase::Scanning).await?;
        }
        _ => {}
    }
    let endpoint = match aether::api::scan(&identity, &request, &cancel).await {
        Ok(endpoint) => endpoint,
        Err(e) => {
            app.set_error(e.to_string()).await;
            return Err(e.to_string());
        }
    };
    app.set_gateway(Some(endpoint.socket().to_string())).await;
    app.set_phase(ConnectionPhase::Verifying).await?;
    Ok(endpoint)
}

/// Verify one peer passes data-plane validation (network I/O).
/// Returns true when the gateway is usable; false means rescan.
pub async fn verify_once(
    app: &AppState,
    settings: &GuiSettings,
    peer: SocketAddr,
) -> Result<bool, String> {
    let identity = require_identity(app).await?;
    let spec = tunnel_spec_for(settings)?;
    let cancel = aether::api::Cancel::new();
    app.store_cancel(cancel.clone()).await;
    aether::api::verify_endpoint(&identity, peer, &spec, &cancel)
        .await
        .map_err(|e| e.to_string())
}

/// Connect the tunnel. Returns once the tunnel task ENDS (core
/// reconnects internally, so this normally runs until `disconnect`).
/// Phase: Verifying → Connecting; on return the phase becomes
/// Stopped (user asked) or Error (failure) — never Connected, because
/// tunnel readiness has no hook back yet (see `api::connect` note);
/// the frontend treats Connecting + live `Stats` events as "up".
pub async fn connect(
    app: &AppState,
    settings: &GuiSettings,
    peer: SocketAddr,
) -> Result<(), String> {
    // Cold start: provision first instead of failing with "no identity
    // yet" (GUI_PLAN.md Gap 1). After this the phase is Scanning; a
    // cached identity skips straight to the transition below.
    if app.identity().await.is_none() {
        provision(app, settings).await?;
    }
    if matches!(
        app.phase().await,
        ConnectionPhase::Connecting | ConnectionPhase::Connected
    ) {
        return Err("already connected or connecting".to_string());
    }
    let identity = require_identity(app).await?;
    let spec = tunnel_spec_for(settings)?;
    let cancel = aether::api::Cancel::new();
    app.store_cancel(cancel.clone()).await;
    match app.phase().await {
        // Forced-peer flows skip Verify; reconnects come from Stopped
        // with the cached identity; cold starts land here via provision.
        ConnectionPhase::Idle
        | ConnectionPhase::Scanning
        | ConnectionPhase::Verifying
        | ConnectionPhase::Stopped => {
            app.set_phase(ConnectionPhase::Connecting).await?;
        }
        _ => {}
    }
    match aether::api::connect(&identity, peer, &spec, &cancel).await {
        Ok(()) => {
            app.set_phase(ConnectionPhase::Stopped).await?;
            Ok(())
        }
        Err(aether::error::AetherError::Cancelled) => {
            app.set_phase(ConnectionPhase::Stopped).await?;
            Ok(())
        }
        Err(e) => {
            app.set_error(e.to_string()).await;
            Err(e.to_string())
        }
    }
}

/// Stop the active scan/verify/connect task. Safe to call in any phase
/// (Idle included — it becomes a no-op success). Always disarms the
/// traffic mode first so the system proxy / TUN never outlive the tunnel.
pub async fn disconnect(app: &AppState) -> Result<(), String> {
    app.cancel_active().await;
    app.take_cancel().await;
    app.mode_cleanup().await;
    match app.phase().await {
        ConnectionPhase::Idle => Ok(()),
        ConnectionPhase::Stopped | ConnectionPhase::Error => Ok(()),
        _ => app.set_phase(ConnectionPhase::Stopped).await,
    }
}

/// Drain new core events (frontend polls on a 500 ms timer until the
/// Tauri event stream lands).
pub async fn drain_events(max: usize) -> Vec<aether::events::ApiEvent> {
    aether::events::drain_events(max)
}

/// Drain new log lines. `level` is `error|warn|info|debug|trace`.
pub async fn drain_logs(max: usize, level: Option<String>) -> Vec<aether::guilog::GuiLogRecord> {
    let min = level
        .as_deref()
        .map(aether::guilog::GuiLogLevel::parse);
    aether::guilog::drain_logs(max, min)
}

/// Redacted diagnostics bundle (the only artifact bug reports need).
/// Secrets are never included: device id + versions + settings minus
/// tokens/keys + last 200 log lines. Peeks (never drains) the buffer.
pub async fn diagnostics(settings: &GuiSettings) -> serde_json::Value {
    let logs = aether::guilog::peek_logs(200);
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "protocol": settings.protocol,
        "scan": settings.scan,
        "ip": settings.ip,
        "noize": settings.noize,
        "socks": settings.socks,
        "http_proxy": settings.http_proxy,
        "upstream_set": !settings.upstream.trim().is_empty(),
        "h2": settings.h2,
        "team_set": std::env::var("AETHER_TEAM").is_ok(),
        "logs": logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> GuiSettings {
        GuiSettings::default()
    }

    #[test]
    fn scan_request_defaults_match_engine_spec() {
        let req = scan_request_for(&sample()).expect("valid");
        assert_eq!(req.mode, "balanced");
        assert_eq!(req.wanted, 1);
    }

    #[test]
    fn gool_request_excludes_pinned_hop() {
        let mut s = sample();
        s.protocol = "gool".to_string();
        s.wiw_outer = "162.159.192.1:2408".to_string();
        let req = scan_request_for(&s).expect("valid");
        assert_eq!(req.wanted, 2);
        assert!(!req.excluded.is_empty());
    }

    #[test]
    fn bad_settings_fail_before_any_io() {
        let mut s = sample();
        s.protocol = "nonsense".to_string();
        assert!(scan_request_for(&s).is_err());
        assert!(tunnel_spec_for(&s).is_err());
    }

    #[tokio::test]
    async fn snapshot_dto_renders_phase_label() {
        let app = crate::AppState::new();
        let snap = snapshot(&app).await;
        assert_eq!(
            snap,
            SnapshotDto {
                phase: "idle".to_string(),
                gateway: None,
                error: None,
            }
        );
        app.set_gateway(Some("1.2.3.4:443".to_string())).await;
        let snap = snapshot(&app).await;
        assert_eq!(snap.gateway.as_deref(), Some("1.2.3.4:443"));
    }

    #[tokio::test]
    async fn traffic_mode_roundtrips_and_rejects_garbage() {
        let app = crate::AppState::new();
        assert_eq!(get_traffic_mode(&app).await, "proxy");
        assert_eq!(
            set_traffic_mode(&app, "system").await.as_deref(),
            Ok("system")
        );
        assert_eq!(get_traffic_mode(&app).await, "system");
        assert_eq!(
            set_traffic_mode(&app, " TUN ").await.as_deref(),
            Ok("vpn")
        );
        assert!(set_traffic_mode(&app, "bogus").await.is_err());
        // Rejected input leaves the stored mode untouched.
        assert_eq!(get_traffic_mode(&app).await, "vpn");
    }

    #[test]
    fn loopback_bind_needs_no_confirm() {
        assert!(!needs_bind_confirm(&sample()));
        let mut s = sample();
        s.socks = "0.0.0.0:1819".to_string();
        assert!(needs_bind_confirm(&s));
        let mut s = sample();
        s.http_proxy = "0.0.0.0:1820".to_string();
        assert!(needs_bind_confirm(&s));
    }

    #[tokio::test]
    async fn commands_need_an_identity_first() {
        let app = crate::AppState::new();
        let peer: SocketAddr = "127.0.0.1:1819".parse().unwrap();
        assert!(scan_once(&app, &sample()).await.is_err());
        assert!(verify_once(&app, &sample(), peer).await.is_err());
    }

    #[tokio::test]
    async fn disconnect_is_safe_in_any_phase() {
        let app = crate::AppState::new();
        disconnect(&app).await.expect("Idle disconnect is a no-op");
        assert_eq!(app.phase().await, crate::ConnectionPhase::Idle);
    }

    #[tokio::test]
    async fn tunnel_up_arms_proxy_only_without_side_effects() {
        let app = crate::AppState::new();
        let socks: SocketAddr = "127.0.0.1:1819".parse().unwrap();
        // Default preference is ProxyOnly: pure no-op arm.
        app.on_tunnel_up(socks)
            .await
            .expect("proxy-only arm must succeed");
        app.mode_cleanup().await;
    }

    #[test]
    fn team_from_env_reads_absent_and_present() {
        // One sequential test: tests share one process env, so two
        // mutators would race each other.
        let old_team = std::env::var("AETHER_TEAM").ok();
        let old_email = std::env::var("AETHER_ACCESS_EMAIL").ok();
        fn restore(old: Option<String>, key: &str) {
            match old {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }

        std::env::remove_var("AETHER_TEAM");
        assert!(team_from_env().expect("no team").is_none());

        std::env::set_var("AETHER_TEAM", "acme");
        std::env::set_var("AETHER_ACCESS_EMAIL", "me@example.com");
        let team = team_from_env().expect("team").expect("present");
        assert_eq!(team.team, "acme");
        assert_eq!(team.email.as_deref(), Some("me@example.com"));

        restore(old_team, "AETHER_TEAM");
        restore(old_email, "AETHER_ACCESS_EMAIL");
    }

    #[tokio::test]
    async fn gool_connect_needs_both_hops_first() {
        // Stored pair but unscanned hops: spec has no outer/inner, so
        // the command fails before any socket — no network touched.
        let app = crate::AppState::new();
        let ids = aether::api::GoolIdentities {
            primary: fake_identity(),
            secondary: fake_identity(),
            primary_path: "a.toml".to_string(),
            secondary_path: "a-secondary.toml".to_string(),
        };
        app.store_gool(ids).await;
        let err = connect_gool_cmd(&app, &sample())
            .await
            .expect_err("hops unscanned");
        assert!(err.contains("both gool hops"));
    }

    fn fake_identity() -> aether::account::Identity {
        aether::account::Identity {
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
        }
    }
}
