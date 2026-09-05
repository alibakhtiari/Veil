//! OS traffic-mode orchestration (TUN_AND_PROXY_MODES.md §6.2).
//!
//! Testability split: [`TrafficMode::parse`]/[`TrafficMode::label`] and
//! [`plan`] are pure (unit-tested, no I/O). [`ModeManager::on_connected`]
//! performs the OS side effects via the `proxy` helpers; the TUN driver is
//! not linked in the default build, so `VpnTun` returns
//! [`crate::tun::not_linked_error()`]. The real driver
//! (`Box<dyn crate::tun::TunHandle>`) lands separately.
//!
//! Host-mutating paths (the `SystemProxy` arm of `on_connected`) are NOT
//! covered by unit tests — they would toggle the dev/CI machine's proxy.

use std::net::SocketAddr;

use crate::proxy::ProxyGuard;
use crate::tun::TunConfig;
#[cfg(not(feature = "tun"))]
use crate::tun::not_linked_error;

/// Which traffic-capture mode the GUI is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TrafficMode {
    /// SOCKS5 up, OS settings untouched.
    #[default]
    ProxyOnly,
    /// OS system proxy points at the local SOCKS listener.
    SystemProxy,
    /// Full virtual-adapter capture (needs the tun driver).
    VpnTun,
}

impl TrafficMode {
    /// Parse `proxy`/`system`/`vpn` (aliases: `manual` → proxy-only,
    /// `tun` → VPN). Case-insensitive, surrounding whitespace ignored.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "proxy" | "manual" => Ok(Self::ProxyOnly),
            "system" => Ok(Self::SystemProxy),
            "vpn" | "tun" => Ok(Self::VpnTun),
            other => Err(format!(
                "unknown traffic mode: '{other}' (expected proxy|system|vpn)"
            )),
        }
    }

    /// Canonical label; round-trips through [`TrafficMode::parse`].
    pub fn label(&self) -> &'static str {
        match self {
            Self::ProxyOnly => "proxy",
            Self::SystemProxy => "system",
            Self::VpnTun => "vpn",
        }
    }
}

/// Pure description of what a mode needs (no I/O).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModePlan {
    /// Proxy-only: nothing to do at the OS level.
    NoOp,
    /// System-proxy mode: enable the OS proxy via `proxy` helpers.
    ApplyProxy,
    /// VPN mode: start the TUN front end with this config.
    StartTun(TunConfig),
}

/// Pure, no I/O: describe what `mode` needs.
pub fn plan(mode: TrafficMode, socks: SocketAddr) -> ModePlan {
    match mode {
        TrafficMode::ProxyOnly => ModePlan::NoOp,
        TrafficMode::SystemProxy => ModePlan::ApplyProxy,
        TrafficMode::VpnTun => ModePlan::StartTun(TunConfig::new(socks)),
    }
}

/// Owns the OS side effects for the active [`TrafficMode`].
pub struct ModeManager {
    current: TrafficMode,
    proxy_guard: Option<ProxyGuard>,
    tun_handle: Option<Box<dyn crate::tun::TunHandle>>,
    tun_on: bool,
}

impl ModeManager {
    pub fn new() -> Self {
        Self {
            current: TrafficMode::ProxyOnly,
            proxy_guard: None,
            tun_handle: None,
            tun_on: false,
        }
    }

    pub fn current(&self) -> TrafficMode {
        self.current
    }

    pub fn is_tun_on(&self) -> bool {
        self.tun_on
    }

    /// Pure, no I/O (same as the free [`plan`]).
    pub fn plan(mode: TrafficMode, socks: SocketAddr) -> ModePlan {
        crate::mode_manager::plan(mode, socks)
    }

    /// Switch modes after the core SOCKS listener is up. Always runs
    /// [`ModeManager::cleanup`] first so the previous mode cannot leak.
    pub async fn on_connected(
        &mut self,
        mode: TrafficMode,
        socks: SocketAddr,
    ) -> Result<(), String> {
        self.cleanup().await;
        match mode {
            TrafficMode::ProxyOnly => {
                // SOCKS5 is up; no OS changes needed.
                self.current = TrafficMode::ProxyOnly;
                Ok(())
            }
            TrafficMode::SystemProxy => {
                let guard = crate::proxy::enable_system_proxy(&socks)?;
                self.proxy_guard = Some(guard);
                self.current = TrafficMode::SystemProxy;
                Ok(())
            }
            TrafficMode::VpnTun => {
                #[cfg(feature = "tun")]
                {
                    let cfg = TunConfig::new(socks);
                    let handle = crate::tun::start_tun2socks(&cfg)?;
                    self.tun_handle = Some(handle);
                    self.tun_on = true;
                    self.current = TrafficMode::VpnTun;
                    Ok(())
                }
                #[cfg(not(feature = "tun"))]
                {
                    Err(not_linked_error())
                }
            }
        }
    }

    /// Best-effort restore: disable the system proxy (if armed), stop the
    /// TUN adapter (if running), and reset to [`TrafficMode::ProxyOnly`]. Idempotent.
    pub async fn cleanup(&mut self) {
        if let Some(mut guard) = self.proxy_guard.take() {
            guard.disarm_and_disable();
        }
        if let Some(mut handle) = self.tun_handle.take() {
            handle.stop();
        }
        self.tun_on = false;
        self.current = TrafficMode::ProxyOnly;
    }
}

impl Default for ModeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socks() -> SocketAddr {
        "127.0.0.1:1819".parse().unwrap()
    }

    #[test]
    fn parse_label_roundtrip_incl_bad_input() {
        for mode in [
            TrafficMode::ProxyOnly,
            TrafficMode::SystemProxy,
            TrafficMode::VpnTun,
        ] {
            assert_eq!(TrafficMode::parse(mode.label()), Ok(mode));
        }
        assert_eq!(
            TrafficMode::parse("proxy"),
            Ok(TrafficMode::ProxyOnly)
        );
        assert_eq!(
            TrafficMode::parse("manual"),
            Ok(TrafficMode::ProxyOnly)
        );
        assert_eq!(
            TrafficMode::parse("system"),
            Ok(TrafficMode::SystemProxy)
        );
        assert_eq!(TrafficMode::parse("vpn"), Ok(TrafficMode::VpnTun));
        assert_eq!(TrafficMode::parse("tun"), Ok(TrafficMode::VpnTun));
        assert_eq!(
            TrafficMode::parse(" PROXY "),
            Ok(TrafficMode::ProxyOnly)
        );
        assert!(TrafficMode::parse("bogus").is_err());
        assert!(TrafficMode::parse("").is_err());
        assert_eq!(TrafficMode::default(), TrafficMode::ProxyOnly);
    }

    #[test]
    fn plan_covers_all_modes() {
        let socks = socks();
        assert_eq!(plan(TrafficMode::ProxyOnly, socks), ModePlan::NoOp);
        assert_eq!(plan(TrafficMode::SystemProxy, socks), ModePlan::ApplyProxy);
        match plan(TrafficMode::VpnTun, socks) {
            ModePlan::StartTun(cfg) => assert_eq!(cfg, TunConfig::new(socks)),
            other => panic!("expected StartTun, got {other:?}"),
        }
        // Associated fn agrees with the free fn.
        assert_eq!(
            ModeManager::plan(TrafficMode::ProxyOnly, socks),
            ModePlan::NoOp
        );
    }

    #[tokio::test]
    async fn cleanup_idempotent_and_resets_mode() {
        let mut mgr = ModeManager::new();
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
        assert!(!mgr.is_tun_on());
        // Simulate an armed state without touching the host.
        mgr.current = TrafficMode::SystemProxy;
        mgr.tun_on = true;
        mgr.cleanup().await;
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
        assert!(!mgr.is_tun_on());
        mgr.cleanup().await;
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
        assert!(!mgr.is_tun_on());
    }

    #[tokio::test]
    async fn vpn_arm_errors_mentioning_tun() {
        let mut mgr = ModeManager::new();
        let err = mgr
            .on_connected(TrafficMode::VpnTun, socks())
            .await
            .expect_err("VpnTun without the driver linked must fail");
        assert!(
            err.to_ascii_lowercase().contains("tun"),
            "error should mention tun, got: {err}"
        );
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
        assert!(!mgr.is_tun_on());
    }

    #[tokio::test]
    async fn proxy_only_arm_ok_without_side_effects() {
        let mut mgr = ModeManager::new();
        mgr.on_connected(TrafficMode::ProxyOnly, socks())
            .await
            .expect("ProxyOnly must succeed");
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
        assert!(!mgr.is_tun_on());
        // A second arm after cleanup is still clean.
        mgr.cleanup().await;
        assert_eq!(mgr.current(), TrafficMode::ProxyOnly);
    }
}
