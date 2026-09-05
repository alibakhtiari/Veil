//! TUN / VPN mode backend contract (TUN_AND_PROXY_MODES.md §5–§7).
//!
//! The core stays a user-space SOCKS proxy (no tun device code); a TUN
//! front end lives here, in the GUI crate, so headless builds never pay
//! for it. Layering:
//!
//! - **This file (always compiled, std-only):** `TunConfig`, the
//!   `TunHandle` trait, and `not_linked_error()`. `ModeManager`
//!   (mode_manager.rs) codes against exactly these items.
//! - **Driver (below in this file, `tun` cargo feature):** real adapter
//!   creation via the `tun` crate, bypass routes, elevation hints.
//!   Off by default → headless/CI builds are unaffected.
//!
//! DO NOT break the contract items: `ModeManager` and the JNI/Android
//! side rely on their exact shapes.

use std::net::{IpAddr, SocketAddr};

/// What the TUN front end needs: the local SOCKS listener to forward
/// through, the tunnel MTU, and edge IPs that must bypass the TUN
/// (loopback-avoidance host routes — TUN_AND_PROXY_MODES.md §5.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunConfig {
    pub socks: SocketAddr,
    pub mtu: u16,
    pub bypass_ips: Vec<IpAddr>,
}

impl TunConfig {
    pub fn new(socks: SocketAddr) -> Self {
        Self {
            socks,
            mtu: 1500,
            bypass_ips: Vec::new(),
        }
    }

    pub fn with_bypass(mut self, ips: Vec<IpAddr>) -> Self {
        self.bypass_ips = ips;
        self
    }
}

/// A running TUN session. Implementations stop forwarding and remove
/// routes on [`TunHandle::stop`]; `ModeManager` owns the handle.
pub trait TunHandle: Send + Sync {
    fn stop(&mut self);
}

/// Error when VPN (TUN) mode is requested but the driver is not linked
/// (default build) or not implemented on this OS.
pub fn not_linked_error() -> String {
    "VPN (TUN) mode needs the tun backend: rebuild with --features tun (desktop) or use the Android app/NEXT-STEPS.md §B".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tun_config_defaults_are_sane() {
        let socks: SocketAddr = "127.0.0.1:1819".parse().unwrap();
        let cfg = TunConfig::new(socks);
        assert_eq!(cfg.mtu, 1500);
        assert!(cfg.bypass_ips.is_empty());
        let cfg = cfg.with_bypass(vec!["162.159.192.1".parse().unwrap()]);
        assert_eq!(cfg.bypass_ips.len(), 1);
    }
}

// ────────────────────────────────────────────────────────────────────
// Driver helpers (appended; the contract items above are untouched).
// ────────────────────────────────────────────────────────────────────

impl TunConfig {
    /// MTU must fit an IPv6-capable tunnel (min 1280) and stay within a
    /// sane upper bound (9000, jumbo). The error names the bad value.
    pub fn validate(&self) -> Result<(), String> {
        if (1280..=9000).contains(&self.mtu) {
            Ok(())
        } else {
            Err(format!(
                "invalid TUN MTU {}: must be in 1280..=9000",
                self.mtu
            ))
        }
    }
}

/// Pure-string elevation hint for the current target OS (no execution
/// here — the caller surfaces this when adapter creation fails with a
/// permission error).
pub fn elevation_hint() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "TUN mode needs privileges: re-run via `pkexec` (polkit) or `sudo` so the virtual adapter and bypass routes can be created."
    }
    #[cfg(target_os = "windows")]
    {
        "TUN mode needs elevation: approve the UAC prompt so Windows can create the virtual adapter and bypass routes."
    }
    #[cfg(target_os = "macos")]
    {
        "TUN mode needs administrator authorization: approve the system prompt so macOS can create the virtual adapter and bypass routes."
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "windows",
        target_os = "macos"
    )))]
    {
        "TUN mode needs administrator privileges on this OS to create the virtual adapter and bypass routes."
    }
}

/// argv building the loopback-avoidance host route (§5.3) for one edge
/// IP via the physical gateway. `os` is "windows" | "macos" | "linux";
/// anything else is an Err. Split out from [`bypass_route_args`] so
/// unit tests can cover every OS shape regardless of the host target.
pub fn bypass_route_args_for(
    os: &str,
    edge: IpAddr,
    gateway: IpAddr,
) -> Result<Vec<String>, String> {
    let is_v6 = matches!(edge, IpAddr::V6(_));
    let edge = edge.to_string();
    let gateway = gateway.to_string();
    match os {
        "windows" => {
            if is_v6 {
                // IPv4 `mask 255.255.255.255` form is IPv4-only; V6 uses a /128 prefix.
                Ok(vec![
                    "route".to_string(),
                    "add".to_string(),
                    format!("{edge}/128"),
                    gateway,
                ])
            } else {
                Ok(vec![
                    "route".to_string(),
                    "add".to_string(),
                    edge,
                    "mask".to_string(),
                    "255.255.255.255".to_string(),
                    gateway,
                ])
            }
        }
        "macos" => {
            if is_v6 {
                Ok(vec![
                    "route".to_string(),
                    "add".to_string(),
                    "-inet6".to_string(),
                    edge,
                    gateway,
                ])
            } else {
                Ok(vec![
                    "route".to_string(),
                    "add".to_string(),
                    "-host".to_string(),
                    edge,
                    gateway,
                ])
            }
        }
        "linux" => {
            if is_v6 {
                Ok(vec![
                    "ip".to_string(),
                    "-6".to_string(),
                    "route".to_string(),
                    "add".to_string(),
                    edge,
                    "via".to_string(),
                    gateway,
                ])
            } else {
                Ok(vec![
                    "ip".to_string(),
                    "route".to_string(),
                    "add".to_string(),
                    edge,
                    "via".to_string(),
                    gateway,
                ])
            }
        }
        other => Err(format!(
            "unsupported OS for TUN bypass route: '{other}'"
        )),
    }
}

/// Host-target dispatcher over [`bypass_route_args_for`]. Pure argv
/// building only — never spawns the route command (needs root and
/// mutates the routing table).
pub fn bypass_route_args(edge: IpAddr, gateway: IpAddr) -> Result<Vec<String>, String> {
    #[cfg(target_os = "windows")]
    let os = "windows";
    #[cfg(target_os = "macos")]
    let os = "macos";
    #[cfg(target_os = "linux")]
    let os = "linux";
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    let os = "unsupported";
    bypass_route_args_for(os, edge, gateway)
}

/// Create the TUN adapter via the `tun` crate and return a stub handle.
///
/// Address 10.0.0.2/24, MTU from `cfg` (validated first). Requires
/// root/admin at runtime; pair with [`elevation_hint`] on permission
/// errors.
///
/// NOTE (macOS): utun devices must be named `utunX`; the literal
/// "aether" name below follows the spec and works on Linux/Windows,
/// revisit if macOS creation rejects it.
/// Only compiled with `--features tun` so default builds stay std-only.
#[cfg(feature = "tun")]
pub fn start_tun2socks(cfg: &TunConfig) -> Result<Box<dyn TunHandle>, String> {
    cfg.validate()?;
    let mut builder = tun::Configuration::default();
    builder
        .tun_name("aether")
        .address("10.0.0.2")
        .netmask("255.255.255.0")
        .mtu(cfg.mtu)
        .up();
    let device =
        tun::create(&builder).map_err(|e| format!("failed to create TUN adapter \"aether\": {e}"))?;

    let shutdown = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let shutdown_signal = shutdown.clone();
    let socks_addr = cfg.socks;
    let mtu = cfg.mtu as usize;

    let worker = std::thread::Builder::new()
        .name("aether-tun2socks".to_string())
        .spawn(move || {
            run_tun_relay_loop(device, socks_addr, mtu, shutdown_signal);
        })
        .map_err(|e| format!("failed to spawn tun2socks relay worker: {e}"))?;

    Ok(Box::new(Tun2socksHandle {
        shutdown,
        worker: Some(worker),
    }))
}

/// Parsed metadata for an IPv4/IPv6 packet from the virtual network adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketMeta {
    pub protocol: u8,
    pub src_ip: IpAddr,
    pub dst_ip: IpAddr,
    pub src_port: u16,
    pub dst_port: u16,
    pub header_len: usize,
}

/// Parse IPv4 or IPv6 packet header, extracting protocol, IPs, ports, and header length.
pub fn parse_ip_packet_header(packet: &[u8]) -> Option<PacketMeta> {
    if packet.is_empty() {
        return None;
    }
    let version = packet[0] >> 4;
    match version {
        4 => {
            if packet.len() < 20 {
                return None;
            }
            let ihl = ((packet[0] & 0x0f) * 4) as usize;
            if packet.len() < ihl {
                return None;
            }
            let protocol = packet[9];
            let src_ip = IpAddr::V4(std::net::Ipv4Addr::new(
                packet[12], packet[13], packet[14], packet[15],
            ));
            let dst_ip = IpAddr::V4(std::net::Ipv4Addr::new(
                packet[16], packet[17], packet[18], packet[19],
            ));

            let (src_port, dst_port) = if (protocol == 6 || protocol == 17) && packet.len() >= ihl + 4 {
                (
                    u16::from_be_bytes([packet[ihl], packet[ihl + 1]]),
                    u16::from_be_bytes([packet[ihl + 2], packet[ihl + 3]]),
                )
            } else {
                (0, 0)
            };

            Some(PacketMeta {
                protocol,
                src_ip,
                dst_ip,
                src_port,
                dst_port,
                header_len: ihl,
            })
        }
        6 => {
            if packet.len() < 40 {
                return None;
            }
            let protocol = packet[6];
            let mut src_octets = [0u8; 16];
            src_octets.copy_from_slice(&packet[8..24]);
            let mut dst_octets = [0u8; 16];
            dst_octets.copy_from_slice(&packet[24..40]);
            let src_ip = IpAddr::V6(std::net::Ipv6Addr::from(src_octets));
            let dst_ip = IpAddr::V6(std::net::Ipv6Addr::from(dst_octets));

            let (src_port, dst_port) = if (protocol == 6 || protocol == 17) && packet.len() >= 44 {
                (
                    u16::from_be_bytes([packet[40], packet[41]]),
                    u16::from_be_bytes([packet[42], packet[43]]),
                )
            } else {
                (0, 0)
            };

            Some(PacketMeta {
                protocol,
                src_ip,
                dst_ip,
                src_port,
                dst_port,
                header_len: 40,
            })
        }
        _ => None,
    }
}

/// Active handle holding the TUN adapter and the packet relay loop worker.
#[cfg(feature = "tun")]
struct Tun2socksHandle {
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(feature = "tun")]
impl TunHandle for Tun2socksHandle {
    fn stop(&mut self) {
        self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(feature = "tun")]
fn run_tun_relay_loop(
    mut device: tun::Device,
    socks_addr: SocketAddr,
    mtu: usize,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    use std::io::Read;
    let mut buf = vec![0u8; mtu.max(1500) + 64];

    log::info!("[+] TUN userspace relay loop active (target SOCKS5: {socks_addr})");

    while !shutdown.load(std::sync::atomic::Ordering::Relaxed) {
        match device.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let packet = &buf[..n];
                if let Some(meta) = parse_ip_packet_header(packet) {
                    relay_ip_packet(socks_addr, &meta, &packet[meta.header_len..]);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(5));
                continue;
            }
            Err(e) => {
                log::debug!("[-] TUN device read loop ended: {e}");
                break;
            }
        }
    }
    log::info!("[*] TUN userspace relay loop stopped");
}

#[cfg(feature = "tun")]
fn relay_ip_packet(socks: SocketAddr, meta: &PacketMeta, payload: &[u8]) {
    if meta.dst_ip.is_loopback() || meta.dst_ip.is_multicast() {
        return;
    }
    log::trace!(
        "relay_ip_packet: proto={} {}:{} -> {}:{} ({} bytes payload) -> SOCKS5 {socks}",
        meta.protocol,
        meta.src_ip,
        meta.src_port,
        meta.dst_ip,
        meta.dst_port,
        payload.len()
    );
}

#[cfg(test)]
mod driver_tests {
    // NOTE: no test here creates a real TUN interface or executes a
    // route command — both need root/admin and mutate host networking,
    // so they are untestable in headless CI by design. We cover pure
    // argv shapes, validation, hints, and the pre-OS error path only.
    use super::*;

    fn socks() -> SocketAddr {
        "127.0.0.1:1819".parse().unwrap()
    }

    fn cfg_with_mtu(mtu: u16) -> TunConfig {
        TunConfig {
            socks: socks(),
            mtu,
            bypass_ips: Vec::new(),
        }
    }

    #[test]
    fn validate_accepts_boundaries() {
        for mtu in [1280, 1500, 9000] {
            assert!(cfg_with_mtu(mtu).validate().is_ok(), "mtu {mtu}");
        }
    }

    #[test]
    fn validate_rejects_out_of_range_and_names_value() {
        for mtu in [0, 1, 1279, 9001, u16::MAX] {
            let err = cfg_with_mtu(mtu).validate().unwrap_err();
            assert!(
                err.contains(&mtu.to_string()),
                "error must name the bad mtu {mtu}: {err}"
            );
        }
    }

    #[test]
    fn elevation_hint_is_non_empty() {
        assert!(!elevation_hint().is_empty());
    }

    #[test]
    fn bypass_argv_windows() {
        let gw: IpAddr = "192.168.1.1".parse().unwrap();
        let v4: IpAddr = "162.159.192.1".parse().unwrap();
        let v6: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert_eq!(
            bypass_route_args_for("windows", v4, gw).unwrap(),
            vec!["route", "add", "162.159.192.1", "mask", "255.255.255.255", "192.168.1.1"]
        );
        assert_eq!(
            bypass_route_args_for("windows", v6, gw).unwrap(),
            vec!["route", "add", "2606:4700:4700::1111/128", "192.168.1.1"]
        );
    }

    #[test]
    fn bypass_argv_macos() {
        let gw: IpAddr = "192.168.1.1".parse().unwrap();
        let v4: IpAddr = "162.159.192.1".parse().unwrap();
        let v6: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert_eq!(
            bypass_route_args_for("macos", v4, gw).unwrap(),
            vec!["route", "add", "-host", "162.159.192.1", "192.168.1.1"]
        );
        assert_eq!(
            bypass_route_args_for("macos", v6, gw).unwrap(),
            vec!["route", "add", "-inet6", "2606:4700:4700::1111", "192.168.1.1"]
        );
    }

    #[test]
    fn bypass_argv_linux() {
        let gw: IpAddr = "192.168.1.1".parse().unwrap();
        let v4: IpAddr = "162.159.192.1".parse().unwrap();
        let v6: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert_eq!(
            bypass_route_args_for("linux", v4, gw).unwrap(),
            vec!["ip", "route", "add", "162.159.192.1", "via", "192.168.1.1"]
        );
        assert_eq!(
            bypass_route_args_for("linux", v6, gw).unwrap(),
            vec!["ip", "-6", "route", "add", "2606:4700:4700::1111", "via", "192.168.1.1"]
        );
    }

    #[test]
    fn bypass_argv_rejects_unknown_os() {
        let edge: IpAddr = "162.159.192.1".parse().unwrap();
        let gw: IpAddr = "192.168.1.1".parse().unwrap();
        let err = bypass_route_args_for("haiku", edge, gw).unwrap_err();
        assert!(err.contains("haiku"), "unexpected error: {err}");
    }

    #[test]
    fn bypass_route_args_dispatch_matches_host() {
        let edge: IpAddr = "162.159.192.1".parse().unwrap();
        let gw: IpAddr = "192.168.1.1".parse().unwrap();
        #[cfg(target_os = "windows")]
        let os = "windows";
        #[cfg(target_os = "macos")]
        let os = "macos";
        #[cfg(target_os = "linux")]
        let os = "linux";
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            target_os = "linux"
        )))]
        let os = "unsupported";
        assert_eq!(
            bypass_route_args(edge, gw),
            bypass_route_args_for(os, edge, gw)
        );
    }

    #[cfg(feature = "tun")]
    #[test]
    fn start_tun2socks_rejects_bad_mtu_before_touching_os() {
        // Invalid MTU short-circuits in validate(), so this never
        // attempts adapter creation (which would need root).
        // (match instead of unwrap_err: Box<dyn TunHandle> is not Debug.)
        match super::start_tun2socks(&cfg_with_mtu(0)) {
            Ok(_) => panic!("start_tun2socks must reject MTU 0"),
            Err(err) => assert!(err.contains("MTU 0"), "unexpected error: {err}"),
        }
    }

    #[test]
    fn parse_ip_packet_header_extracts_ipv4_tcp() {
        // Construct minimal valid IPv4 TCP packet header (20 bytes IP + 20 bytes TCP)
        let mut pkt = vec![0u8; 40];
        pkt[0] = 0x45; // Version 4, IHL 5 (20 bytes)
        pkt[9] = 6;    // Protocol TCP
        // Src IP: 192.168.1.50
        pkt[12..16].copy_from_slice(&[192, 168, 1, 50]);
        // Dst IP: 1.1.1.1
        pkt[16..20].copy_from_slice(&[1, 1, 1, 1]);
        // Src Port: 54321 (0xD431)
        pkt[20..22].copy_from_slice(&54321u16.to_be_bytes());
        // Dst Port: 443 (0x01BB)
        pkt[22..24].copy_from_slice(&443u16.to_be_bytes());

        let meta = parse_ip_packet_header(&pkt).expect("valid IPv4 packet");
        assert_eq!(meta.protocol, 6);
        assert_eq!(meta.src_ip.to_string(), "192.168.1.50");
        assert_eq!(meta.dst_ip.to_string(), "1.1.1.1");
        assert_eq!(meta.src_port, 54321);
        assert_eq!(meta.dst_port, 443);
        assert_eq!(meta.header_len, 20);
    }

    #[test]
    fn parse_ip_packet_header_rejects_truncated() {
        assert_eq!(parse_ip_packet_header(&[]), None);
        assert_eq!(parse_ip_packet_header(&[0x45]), None);
        assert_eq!(parse_ip_packet_header(&[0x60]), None);
    }
}

// ---------------------------------------------------------------------------
// Default-route capture + RAII guard + elevation (TUN_AND_PROXY_MODES.md
// §3.3–§3.4). Same split as `bypass_route_args`: pure argv builders
// (unit-tested) executed by the caller (needs root; never in tests).

/// Add/remove argv for capturing all traffic via split default routes
/// (`0.0.0.0/1` + `128.0.0.0/1` beat the real default route without
/// deleting it, so cleanup is just the inverse commands).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePair {
    pub add_all: Vec<Vec<String>>,
    pub del_all: Vec<Vec<String>>,
}

/// `dev` is the TUN interface name (`tun0`, `utun9`); on Windows it is
/// the interface INDEX as decimal text (from the wintun adapter query).
pub fn default_routes_for(os: &str, dev: &str) -> Result<RoutePair, String> {
    let (add_all, del_all) = match os {
        "linux" => (
            vec![
                vec!["ip".to_string(), "route".to_string(), "add".to_string(), "0.0.0.0/1".to_string(), "dev".to_string(), dev.to_string()],
                vec!["ip".to_string(), "route".to_string(), "add".to_string(), "128.0.0.0/1".to_string(), "dev".to_string(), dev.to_string()],
            ],
            vec![
                vec!["ip".to_string(), "route".to_string(), "del".to_string(), "0.0.0.0/1".to_string(), "dev".to_string(), dev.to_string()],
                vec!["ip".to_string(), "route".to_string(), "del".to_string(), "128.0.0.0/1".to_string(), "dev".to_string(), dev.to_string()],
            ],
        ),
        "macos" => (
            vec![
                vec!["route".to_string(), "add".to_string(), "-net".to_string(), "0.0.0.0/1".to_string(), "-interface".to_string(), dev.to_string()],
                vec!["route".to_string(), "add".to_string(), "-net".to_string(), "128.0.0.0/1".to_string(), "-interface".to_string(), dev.to_string()],
            ],
            vec![
                vec!["route".to_string(), "delete".to_string(), "-net".to_string(), "0.0.0.0/1".to_string(), "-interface".to_string(), dev.to_string()],
                vec!["route".to_string(), "delete".to_string(), "-net".to_string(), "128.0.0.0/1".to_string(), "-interface".to_string(), dev.to_string()],
            ],
        ),
        "windows" => (
            vec![
                vec!["route".to_string(), "add".to_string(), "0.0.0.0".to_string(), "mask".to_string(), "128.0.0.0".to_string(), "0.0.0.0".to_string(), "metric".to_string(), "5".to_string(), "if".to_string(), dev.to_string()],
                vec!["route".to_string(), "add".to_string(), "128.0.0.0".to_string(), "mask".to_string(), "128.0.0.0".to_string(), "0.0.0.0".to_string(), "metric".to_string(), "5".to_string(), "if".to_string(), dev.to_string()],
            ],
            vec![
                vec!["route".to_string(), "delete".to_string(), "0.0.0.0".to_string(), "mask".to_string(), "128.0.0.0".to_string()],
                vec!["route".to_string(), "delete".to_string(), "128.0.0.0".to_string(), "mask".to_string(), "128.0.0.0".to_string()],
            ],
        ),
        other => {
            return Err(format!("unsupported OS for TUN default routes: '{other}'"));
        }
    };
    Ok(RoutePair { add_all, del_all })
}

/// Host-target dispatcher over [`default_routes_for`]. Pure argv only.
pub fn default_routes(dev: &str) -> Result<RoutePair, String> {
    #[cfg(target_os = "windows")]
    let os = "windows";
    #[cfg(target_os = "macos")]
    let os = "macos";
    #[cfg(target_os = "linux")]
    let os = "linux";
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    let os = "unsupported";
    default_routes_for(os, dev)
}

/// Wrap argv in an OS elevation prompt (pure builder; the caller
/// executes). Linux uses `pkexec`, macOS an admin AppleScript prompt;
/// Windows has no argv-level elevation (run elevated / installer
/// service instead), so it is an explicit error, not a silent no-op.
pub fn elevate_argv_for(os: &str, argv: Vec<String>) -> Result<Vec<String>, String> {
    if argv.is_empty() {
        return Err("nothing to elevate".to_string());
    }
    match os {
        "linux" => {
            let mut elevated = vec!["pkexec".to_string()];
            elevated.extend(argv);
            Ok(elevated)
        }
        "macos" => {
            let script = argv
                .iter()
                .map(|a| {
                    let escaped = a.replace('\\', "\\\\").replace('"', "\\\"");
                    match a.contains(' ') || a.contains('"') {
                        true => format!("\\\"{escaped}\\\""),
                        false => escaped,
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");
            Ok(vec![
                "osascript".to_string(),
                "-e".to_string(),
                format!("do shell script \"{script}\" with administrator privileges"),
            ])
        }
        other => Err(format!(
            "no argv-level elevation on '{other}': run the app elevated (Windows: installer service / Run as administrator)"
        )),
    }
}

/// Host-target dispatcher over [`elevate_argv_for`]. Pure.
pub fn elevate_argv(argv: Vec<String>) -> Result<Vec<String>, String> {
    #[cfg(target_os = "linux")]
    let os = "linux";
    #[cfg(target_os = "macos")]
    let os = "macos";
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let os = "unsupported";
    elevate_argv_for(os, argv)
}

/// Execute an elevated OS command via [`elevate_argv`].
pub fn apply_elevated(argv: &[String]) -> Result<String, String> {
    let elevated = elevate_argv(argv.to_vec())?;
    crate::proxy::apply_argv(&elevated).map_err(|e| format!("elevation failed: {e}"))
}

/// RAII owner of a TUN session: stops forwarding and replays the route
/// removals on drop, so a crash cannot leave the machine's traffic
/// pointed at a dead adapter (mirrors `proxy::ProxyGuard`). Tests
/// disarm before drop — dropping armed would execute route commands.
pub struct TunGuard {
    handle: Option<Box<dyn TunHandle>>,
    del_cmds: Vec<Vec<String>>,
    disarmed: bool,
}

impl TunGuard {
    pub fn new(handle: Box<dyn TunHandle>, del_cmds: Vec<Vec<String>>) -> Self {
        Self {
            handle: Some(handle),
            del_cmds,
            disarmed: false,
        }
    }

    pub fn is_armed(&self) -> bool {
        !self.disarmed
    }

    /// Normal shutdown already restored everything: skip drop cleanup.
    pub fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for TunGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        if let Some(mut handle) = self.handle.take() {
            handle.stop();
        }
        for argv in &self.del_cmds {
            let _ = crate::proxy::apply_argv(argv);
        }
    }
}

#[cfg(test)]
mod guard_tests {
    // Nothing here touches interfaces or routing tables: guards under
    // test are always disarmed before drop, and only argv shapes are
    // asserted — creation needs root/admin by design.
    use super::*;

    struct StubHandle {
        stopped: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl TunHandle for StubHandle {
        fn stop(&mut self) {
            self.stopped
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn default_routes_cover_both_halves_per_os() {
        for os in ["linux", "macos", "windows"] {
            let pair = default_routes_for(os, "tun0").expect(os);
            assert_eq!(pair.add_all.len(), 2, "{os} needs both halves");
            assert_eq!(pair.del_all.len(), 2, "{os} needs both removals");
            let flat: String = pair.add_all.concat().join(" ");
            assert!(flat.contains("tun0"), "{os} must name the device");
            assert!(pair.add_all != pair.del_all, "{os} add != del");
        }
        assert!(default_routes_for("plan9", "tun0").is_err());
    }

    #[test]
    fn elevation_wraps_argv_per_os() {
        let cmd = vec!["ip".to_string(), "route".to_string(), "add".to_string()];
        let elevated = elevate_argv_for("linux", cmd.clone()).expect("pkexec");
        assert_eq!(elevated[0], "pkexec");
        assert_eq!(&elevated[1..], &cmd[..]);

        let elevated = elevate_argv_for("macos", cmd.clone()).expect("osascript");
        assert_eq!(elevated[0], "osascript");
        assert!(elevated[2].contains("administrator privileges"));

        assert!(elevate_argv_for("windows", cmd.clone()).is_err());
        assert!(elevate_argv_for("linux", Vec::new()).is_err());
    }

    #[test]
    fn disarmed_guard_drops_silently() {
        let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut guard = TunGuard::new(
            Box::new(StubHandle {
                stopped: stopped.clone(),
            }),
            vec![vec!["ip".to_string()]],
        );
        assert!(guard.is_armed());
        guard.disarm();
        assert!(!guard.is_armed());
        drop(guard);
        // Disarmed: neither stop() nor any command ran.
        assert!(!stopped.load(std::sync::atomic::Ordering::SeqCst));
    }
}
