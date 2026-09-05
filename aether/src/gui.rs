//! GUI-owned settings + validation helpers (Phase 0.4).
//!
//! Identity files (`aether.toml`, `aether-masque.toml`,
//! `aether-team-<name>.toml`) hold secrets and are owned by
//! `config.rs` — the GUI never writes them directly. This module owns
//! the second file, `aether-gui.toml`, which mirrors the `AETHER_*`
//! knobs so a GUI session is reproducible from environment variables and vice versa.
//!
//! Validation here produces standard errors where
//! the engine validates (endpoint needs `ip:port`, gool hops must
//! differ). New checks (bind address, upstream URL, route entry) reuse
//! the core parsers (`Upstream::parse`, `ipnet`, `regex`) instead of
//! reimplementing grammar.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

use crate::error::{AetherError, Result};

/// File name (next to the identity path, or in the OS config dir).
pub const GUI_SETTINGS_FILE: &str = "aether-gui.toml";

/// Defaults mirror engine specifications + `api::TunnelSpec::for_transport`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiSettings {
    #[serde(default = "d_protocol")]
    pub protocol: String,
    #[serde(default = "d_scan")]
    pub scan: String,
    #[serde(default = "d_ip")]
    pub ip: String,
    #[serde(default = "d_noize")]
    pub noize: String,
    #[serde(default = "d_true")]
    pub quick_reconnect: bool,
    #[serde(default = "d_socks")]
    pub socks: String,
    #[serde(default)]
    pub http_proxy: String,
    #[serde(default)]
    pub upstream: String,
    #[serde(default)]
    pub h2: bool,
    #[serde(default)]
    pub fragment: bool,
    #[serde(default = "d_lang")]
    pub language: String,
    #[serde(default)]
    pub autoconnect: bool,
    #[serde(default)]
    pub system_proxy: bool,
    // Gool / peer overrides (empty = scan).
    #[serde(default)]
    pub peer: String,
    #[serde(default)]
    pub wiw_outer: String,
    #[serde(default)]
    pub wiw_inner: String,
    // Zero Trust
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub access_token: String,
    // Routing rules
    #[serde(default)]
    pub route_direct: String,
    #[serde(default)]
    pub route_block: String,
    // Updates
    #[serde(default = "d_true")]
    pub auto_update: bool,
}

fn d_protocol() -> String {
    "masque".to_string()
}
fn d_scan() -> String {
    "balanced".to_string()
}
fn d_ip() -> String {
    "v4".to_string()
}
fn d_noize() -> String {
    "firewall".to_string()
}
fn d_true() -> bool {
    true
}
fn d_socks() -> String {
    "127.0.0.1:1819".to_string()
}
fn d_lang() -> String {
    "en".to_string()
}

impl Default for GuiSettings {
    fn default() -> Self {
        Self {
            protocol: d_protocol(),
            scan: d_scan(),
            ip: d_ip(),
            noize: d_noize(),
            quick_reconnect: true,
            socks: d_socks(),
            http_proxy: String::new(),
            upstream: String::new(),
            h2: false,
            fragment: false,
            language: d_lang(),
            autoconnect: false,
            system_proxy: false,
            peer: String::new(),
            wiw_outer: String::new(),
            wiw_inner: String::new(),
            team: String::new(),
            access_token: String::new(),
            route_direct: String::new(),
            route_block: String::new(),
            auto_update: true,
        }
    }
}

impl GuiSettings {
    /// Load from `path`. Missing file → defaults (first-run wizard).
    /// Corrupt file → error naming the file (caller offers Reset).
    /// Never quarantines/deletes — only identity files get that treatment.
    pub fn load(path: &str) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(AetherError::Other(format!("gui settings read {path}: {e}"))),
            Ok(text) => toml::from_str(&text)
                .map_err(|e| AetherError::Other(format!("gui settings parse {path}: {e}"))),
        }
    }

    /// Atomic save (tmp + rename). GUI settings are not secret, but keep
    /// restrictive perms on multi-user machines anyway.
    pub fn save(&self, path: &str) -> Result<()> {
        self.validate()?;
        let text = toml::to_string_pretty(self)
            .map_err(|e| AetherError::Other(format!("gui settings encode: {e}")))?;
        if let Some(dir) = std::path::Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = unique_tmp(path);
        std::fs::write(&tmp, text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Validate every field a GUI form can produce. Returns the first
    /// problem with standard wording where applicable.
    pub fn validate(&self) -> Result<()> {
        validate_protocol(&self.protocol)?;
        validate_scan(&self.scan)?;
        validate_ip(&self.ip)?;
        validate_noize(&self.noize)?;
        validate_socks(&self.socks)?;
        if !self.http_proxy.trim().is_empty() {
            validate_socks(&self.http_proxy)?;
        }
        if !self.upstream.trim().is_empty() {
            validate_upstream(&self.upstream)?;
        }
        if !self.peer.trim().is_empty() {
            validate_endpoint(&self.peer)?;
        }
        validate_wiw_pair(&self.wiw_outer, &self.wiw_inner)?;
        validate_language(&self.language)?;
        Ok(())
    }

    /// Apply as `AETHER_*` env vars so the core's existing
    /// prompt-suppression logic (`if env set → don't ask`) drives a
    /// fully headless run. The GUI calls this exactly once on Connect.
    pub fn apply_env(&self) {
        std::env::set_var("AETHER_PROTOCOL", proto_env(&self.protocol));
        std::env::set_var("AETHER_SCAN", &self.scan);
        std::env::set_var("AETHER_IP", ip_env(&self.ip));
        std::env::set_var("AETHER_NOIZE", &self.noize);
        std::env::set_var(
            "AETHER_QUICK_RECONNECT",
            if self.quick_reconnect { "1" } else { "0" },
        );
        std::env::set_var("AETHER_SOCKS", &self.socks);
        match self.http_proxy.trim().is_empty() {
            true => std::env::remove_var("AETHER_HTTP_PROXY"),
            false => std::env::set_var("AETHER_HTTP_PROXY", self.http_proxy.trim()),
        }
        match self.upstream.trim().is_empty() {
            true => std::env::remove_var("AETHER_UPSTREAM"),
            false => std::env::set_var("AETHER_UPSTREAM", self.upstream.trim()),
        }
        match self.h2 {
            true => std::env::set_var("AETHER_MASQUE_HTTP2", "1"),
            false => std::env::remove_var("AETHER_MASQUE_HTTP2"),
        }
        match self.fragment {
            true => std::env::set_var("AETHER_MASQUE_H2_FRAGMENT", "1"),
            false => std::env::remove_var("AETHER_MASQUE_H2_FRAGMENT"),
        }
        match self.peer.trim().is_empty() {
            true => std::env::remove_var("AETHER_PEER"),
            false => std::env::set_var("AETHER_PEER", self.peer.trim()),
        }
        match self.wiw_outer.trim().is_empty() {
            true => std::env::remove_var("AETHER_WIW_OUTER_PEER"),
            false => std::env::set_var("AETHER_WIW_OUTER_PEER", self.wiw_outer.trim()),
        }
        match self.wiw_inner.trim().is_empty() {
            true => std::env::remove_var("AETHER_WIW_INNER_PEER"),
            false => std::env::set_var("AETHER_WIW_INNER_PEER", self.wiw_inner.trim()),
        }
        match self.team.trim().is_empty() {
            true => std::env::remove_var("AETHER_TEAM"),
            false => std::env::set_var("AETHER_TEAM", self.team.trim()),
        }
        match self.access_token.trim().is_empty() {
            true => std::env::remove_var("AETHER_ACCESS_TOKEN"),
            false => std::env::set_var("AETHER_ACCESS_TOKEN", self.access_token.trim()),
        }
        match self.route_direct.trim().is_empty() {
            true => std::env::remove_var("AETHER_ROUTE_DIRECT"),
            false => std::env::set_var("AETHER_ROUTE_DIRECT", self.route_direct.trim()),
        }
        match self.route_block.trim().is_empty() {
            true => std::env::remove_var("AETHER_ROUTE_BLOCK"),
            false => std::env::set_var("AETHER_ROUTE_BLOCK", self.route_block.trim()),
        }
    }

    /// True when a bind needs the "I understand" confirm dialog
    /// (anything not loopback — same rule as README/Docker warning).
    pub fn socks_needs_confirm(&self) -> bool {
        needs_confirm_for(&self.socks)
    }

    /// True when ANY listener bind (SOCKS or HTTP proxy) needs the
    /// confirm dialog. The Connect button checks this, not the SOCKS
    /// field alone — the HTTP listener is equally unauthenticated.
    pub fn binds_need_confirm(&self) -> bool {
        needs_confirm_for(&self.socks) || needs_confirm_for(&self.http_proxy)
    }
}

/// Empty/unparsable binds never need confirming: `validate()` rejects
/// them before Connect, so this only answers the loopback question.
fn needs_confirm_for(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    match trimmed.parse::<SocketAddr>() {
        Ok(addr) => !addr.ip().is_loopback(),
        Err(_) => false,
    }
}

/// Collision-free temp path for atomic saves. Threads share a PID, so
/// `{path}.{pid}.tmp` alone collides under concurrent saves; the
/// monotonic counter makes every writer unique.
fn unique_tmp(path: &str) -> String {
    static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{path}.{}.{n}.tmp", std::process::id())
}

fn proto_env(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "wg" | "wireguard" | "warp" => "wg".to_string(),
        "gool" | "wiw" | "warp-in-warp" | "warpinwarp" => "gool".to_string(),
        _ => "masque".to_string(),
    }
}

fn ip_env(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "v6" | "6" | "ipv6" => "v6".to_string(),
        "both" | "dual" | "v4v6" | "4+6" => "both".to_string(),
        _ => "v4".to_string(),
    }
}

pub fn validate_protocol(raw: &str) -> Result<()> {
    match raw.trim().to_lowercase().as_str() {
        "masque" | "wg" | "wireguard" | "warp" | "gool" | "wiw" | "warp-in-warp"
        | "warpinwarp" => Ok(()),
        other => Err(AetherError::Other(format!(
            "'{other}' is not a protocol; use masque, wg, or gool"
        ))),
    }
}

pub fn validate_scan(raw: &str) -> Result<()> {
    match raw.trim().to_lowercase().as_str() {
        "turbo" | "balanced" | "thorough" | "stealth" | "ironclad" => Ok(()),
        other => Err(AetherError::Other(format!(
            "'{other}' is not a scan mode; use turbo, balanced, thorough, stealth, or ironclad"
        ))),
    }
}

pub fn validate_ip(raw: &str) -> Result<()> {
    match raw.trim().to_lowercase().as_str() {
        "v4" | "4" | "ipv4" | "v6" | "6" | "ipv6" | "both" | "dual" | "v4v6" | "4+6" => Ok(()),
        other => Err(AetherError::Other(format!(
            "'{other}' is not an IP version; use v4, v6, or both"
        ))),
    }
}

pub fn validate_noize(raw: &str) -> Result<()> {
    match raw.trim().to_lowercase().as_str() {
        "off" | "light" | "firewall" | "balanced" | "gfw" | "aggressive" => Ok(()),
        other => Err(AetherError::Other(format!(
            "'{other}' is not an obfuscation profile; use off, light, firewall, balanced, gfw, or aggressive"
        ))),
    }
}

pub fn validate_language(raw: &str) -> Result<()> {
    match raw.trim().to_lowercase().as_str() {
        "en" | "fa" => Ok(()),
        other => Err(AetherError::Other(format!(
            "'{other}' is not a language; use en or fa"
        ))),
    }
}

/// Same wording as `lib.rs::parse_endpoint`: the port is required.
pub fn validate_endpoint(raw: &str) -> Result<SocketAddr> {
    let text = raw.trim();
    if let Ok(addr) = text.parse::<SocketAddr>() {
        return Ok(addr);
    }
    if let Ok(ip) = text.parse::<std::net::IpAddr>() {
        return Err(AetherError::Other(format!(
            "{text} carries no port, and the port is required; write it out, as in {ip}:2408"
        )));
    }
    Err(AetherError::Other(format!(
        "'{text}' is not an endpoint; write an address and a port together, such as 162.159.192.1:2408"
    )))
}

/// Both hops empty = scan (valid). One set = scan-for-other (valid).
/// Both set = must be different addresses (mirrors `lib.rs` check).
pub fn validate_wiw_pair(outer_raw: &str, inner_raw: &str) -> Result<(Option<SocketAddr>, Option<SocketAddr>)> {
    let outer = match outer_raw.trim().is_empty() {
        true => None,
        false => Some(validate_endpoint(outer_raw)?),
    };
    let inner = match inner_raw.trim().is_empty() {
        true => None,
        false => Some(validate_endpoint(inner_raw)?),
    };
    if let (Some(o), Some(i)) = (outer, inner) {
        if o.ip() == i.ip() {
            return Err(AetherError::Other(format!(
                "warp-in-warp needs two separate edges, but both hops point at {}",
                o.ip()
            )));
        }
    }
    Ok((outer, inner))
}

pub fn validate_socks(raw: &str) -> Result<SocketAddr> {
    raw.trim().parse::<SocketAddr>().map_err(|_| {
        AetherError::Other(format!(
            "'{}' is not an address:port; use 127.0.0.1:1819",
            raw.trim()
        ))
    })
}

/// Reuses the core upstream parser so URLs are validated uniformly.
pub fn validate_upstream(raw: &str) -> Result<()> {
    crate::upstream::Upstream::parse(raw.trim())
        .map(|_| ())
        .map_err(|e| {
            AetherError::Other(format!("upstream proxy '{raw}' is not usable: {e}"))
        })
}

/// Validate one routing-list entry without depending on the private
/// `routing::Matcher`. Grammar mirrors `routing.rs`.
pub fn validate_route_entry(raw: &str) -> Result<()> {
    let entry = raw.trim();
    if entry.is_empty() || entry.starts_with('#') {
        return Err(AetherError::Other("empty routing entry".to_string()));
    }
    if entry.eq_ignore_ascii_case("private") {
        return Ok(());
    }
    let (kind, value) = match entry.split_once(':') {
        Some((k, v)) if !k.contains('.') && !k.contains('/') => {
            (k.trim().to_lowercase(), v.trim())
        }
        _ => (String::new(), entry),
    };
    match kind.as_str() {
        "full" | "exact" | "domain" | "suffix" => match value.is_empty() {
            true => Err(AetherError::Other(format!("'{entry}' needs a domain"))),
            false => Ok(()),
        },
        "keyword" => match value.is_empty() {
            true => Err(AetherError::Other(format!("'{entry}' needs a keyword"))),
            false => Ok(()),
        },
        "regexp" | "regex" => regex::Regex::new(value)
            .map(|_| ())
            .map_err(|e| AetherError::Other(format!("bad regexp '{entry}': {e}"))),
        "port" => match parse_port_range(value) {
            Some(_) => Ok(()),
            None => Err(AetherError::Other(format!(
                "'{entry}' is not a port or range; use port:25 or port:3000-3010"
            ))),
        },
        "ip" | "cidr" => match value.parse::<ipnet::IpNet>() {
            Ok(_) => Ok(()),
            Err(_) => Err(AetherError::Other(format!("'{entry}' is not a CIDR"))),
        },
        // Core accepts geoip:/geosite: only for `private` (anything else
        // is silently ignored there, so the GUI rejects it loudly).
        "geoip" | "geosite" => match value.eq_ignore_ascii_case("private") {
            true => Ok(()),
            false => Err(AetherError::Other(format!(
                "'{entry}' is not usable; geoip rules only cover 'private'"
            ))),
        },
        "" => {
            // Bare network, bare IP, or domain suffix.
            if value.parse::<ipnet::IpNet>().is_ok() || value.parse::<std::net::IpAddr>().is_ok()
            {
                return Ok(());
            }
            match value.is_empty() {
                true => Err(AetherError::Other(format!("'{entry}' is empty"))),
                false => Ok(()),
            }
        }
        other => Err(AetherError::Other(format!(
            "'{entry}' has an unknown prefix '{other}:'; use full:, keyword:, regexp:, port:, or a domain/CIDR"
        ))),
    }
}

/// Mirrors `routing::parse_ports` exactly: reversed ranges are swapped
/// (not rejected) and 0 is accepted.
fn parse_port_range(raw: &str) -> Option<(u16, u16)> {
    let raw = raw.trim();
    if let Some((a, b)) = raw.split_once('-') {
        let lo: u16 = a.trim().parse().ok()?;
        let hi: u16 = b.trim().parse().ok()?;
        return Some(if hi < lo { (hi, lo) } else { (lo, hi) });
    }
    let p: u16 = raw.parse().ok()?;
    Some((p, p))
}

/// Split a route list the way the core does (`routing::parse_list`:
/// comma, newline, or semicolon) and validate each entry. Empty parts
/// and `#` comment lines are skipped, exactly as the core ignores them.
/// Returns the clean entries.
pub fn validate_route_list(raw: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for part in raw.split([',', '\n', ';']) {
        let entry = part.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        validate_route_entry(entry)?;
        out.push(entry.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        GuiSettings::default().validate().expect("defaults");
    }

    #[test]
    fn endpoint_needs_a_port_like_the_cli() {
        let err = validate_endpoint("162.159.192.1").expect_err("port required");
        assert!(err.to_string().contains("162.159.192.1:2408"));
        assert!(validate_endpoint("162.159.192.1:2408").is_ok());
    }

    #[test]
    fn gool_hops_must_differ() {
        assert!(validate_wiw_pair("", "").is_ok());
        assert!(validate_wiw_pair("162.159.192.1:2408", "").is_ok());
        let err = validate_wiw_pair("162.159.192.1:2408", "162.159.192.1:894")
            .expect_err("same edge twice");
        assert!(err.to_string().contains("162.159.192.1"));
    }

    #[test]
    fn route_entries_follow_cli_grammar() {
        for good in [
            "example.com",
            "full:example.com",
            "*.example.com",
            "keyword:doubleclick",
            "regexp:^ad[0-9]+",
            "10.0.0.0/8",
            "1.2.3.4",
            "port:25",
            "port:0",
            "port:3000-3010",
            "port:3010-3000",
            "private",
            "geoip:private",
            "geosite:private",
        ] {
            validate_route_entry(good).expect(good);
        }
        assert!(validate_route_entry("port:abc").is_err());
        assert!(validate_route_entry("regexp:([bad").is_err());
        assert!(validate_route_entry("geoip:ir").is_err());
    }

    #[test]
    fn route_lists_split_like_the_core_and_skip_comments() {
        let got = validate_route_list("a.com, b.com\nc.com;d.com\n# ignored\n\nport:25")
            .expect("valid list");
        assert_eq!(got, vec!["a.com", "b.com", "c.com", "d.com", "port:25"]);
        assert!(validate_route_list("good.com, port:abc").is_err());
    }

    #[test]
    fn non_loopback_bind_needs_confirm() {
        let mut s = GuiSettings::default();
        assert!(!s.socks_needs_confirm());
        assert!(!s.binds_need_confirm());
        s.socks = "0.0.0.0:1819".to_string();
        assert!(s.socks_needs_confirm());
        assert!(s.binds_need_confirm());

        // The HTTP listener is checked too: SOCKS on loopback but HTTP
        // shared still needs the dialog.
        let mut s = GuiSettings::default();
        s.http_proxy = "0.0.0.0:1820".to_string();
        assert!(!s.socks_needs_confirm());
        assert!(s.binds_need_confirm());
    }

    #[test]
    fn missing_file_gives_defaults() {
        let s = GuiSettings::load("/nonexistent-dir-xyz/aether-gui.toml").expect("defaults");
        assert_eq!(s, GuiSettings::default());
    }

    #[test]
    fn concurrent_saves_share_no_temp_file() {
        let dir = std::env::temp_dir().join(format!(
            "aether-gui-concurrent-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("aether-gui.toml");
        let path_str = path.to_str().unwrap().to_string();

        // Same path from many threads: every writer needs its own temp
        // file or renames clobber each other mid-write.
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let path = path_str.clone();
                std::thread::spawn(move || {
                    let mut s = GuiSettings::default();
                    s.language = "fa".to_string();
                    s.save(&path).expect("concurrent save");
                })
            })
            .collect();
        for h in handles {
            h.join().expect("thread");
        }

        // Final file parses (one writer won the last rename, atomically).
        let back = GuiSettings::load(&path_str).expect("load");
        assert_eq!(back.language, "fa");
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_roundtrip_in_tmp() {
        let dir = std::env::temp_dir().join(format!("aether-gui-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("aether-gui.toml");
        let path = path.to_str().unwrap();
        let mut s = GuiSettings::default();
        s.language = "fa".to_string();
        s.save(path).expect("save");
        let back = GuiSettings::load(path).expect("load");
        assert_eq!(back.language, "fa");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
