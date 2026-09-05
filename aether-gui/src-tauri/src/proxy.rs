//! OS system-proxy integration (GUI_PLAN.md §3.2).
//!
//! Split by testability: `*_args` builders are pure (unit-tested, no
//! side effects) and `apply_argv` executes them. Tests NEVER toggle the
//! real system proxy of the dev/CI machine.
//!
//! Platform notes:
//! - Windows uses the registry (`HKCU\...\Internet Settings`) via
//!   `reg.exe` — std-only, no `windows` crate needed. A live-session
//!   refresh (`InternetSetOptionW ... SETTINGS_CHANGED`) is a Tauri-phase
//!   enhancement; browsers pick the registry up on next launch regardless.
//! - macOS uses `networksetup` on the primary service, detected by
//!   parsing `networksetup -listallnetworkservices`.
//! - Linux uses GNOME `gsettings`; KDE users get copy-paste `kwriteconfig5`
//!   commands from [`kde_hint`].

use std::net::SocketAddr;

/// One OS command as argv (`argv[0]` = program). Display with
/// [`argv_to_string`] for logs and the "what will change" preview.
pub type Argv = Vec<String>;

pub fn argv_to_string(argv: &[String]) -> String {
    argv.iter()
        .map(|a| match a.contains(' ') {
            true => format!("\"{a}\""),
            false => a.clone(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Execute argv, returning stdout on success.
pub fn apply_argv(argv: &[String]) -> std::io::Result<String> {
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "empty argv"))?;
    let output = std::process::Command::new(program).args(args).output()?;
    match output.status.success() {
        true => Ok(String::from_utf8_lossy(&output.stdout).to_string()),
        false => Err(std::io::Error::other(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

fn host_port(socks: &SocketAddr) -> (String, String) {
    (socks.ip().to_string(), socks.port().to_string())
}

// ---------------------------------------------------------------- Windows

/// `reg add` enabling a SOCKS proxy for WinINET apps.
pub fn win_enable_args(socks: &SocketAddr) -> Vec<Argv> {
    let (host, port) = host_port(socks);
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    vec![
        vec![
            "reg".to_string(),
            "add".to_string(),
            key.to_string(),
            "/v".to_string(),
            "ProxyEnable".to_string(),
            "/t".to_string(),
            "REG_DWORD".to_string(),
            "/d".to_string(),
            "1".to_string(),
            "/f".to_string(),
        ],
        vec![
            "reg".to_string(),
            "add".to_string(),
            key.to_string(),
            "/v".to_string(),
            "ProxyServer".to_string(),
            "/t".to_string(),
            "REG_SZ".to_string(),
            "/d".to_string(),
            format!("socks={host}:{port}"),
            "/f".to_string(),
        ],
    ]
}

/// `reg add` restoring direct connection.
pub fn win_disable_args() -> Vec<Argv> {
    let key = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    vec![vec![
        "reg".to_string(),
        "add".to_string(),
        key.to_string(),
        "/v".to_string(),
        "ProxyEnable".to_string(),
        "/t".to_string(),
        "REG_DWORD".to_string(),
        "/d".to_string(),
        "0".to_string(),
        "/f".to_string(),
    ]]
}

/// Broadcast the proxy change to live Windows sessions so already-open
/// browsers (Chrome/Edge) pick it up without restart (plan §3.2).
/// Windows-only; the registry write stays the durable effect.
#[cfg(target_os = "windows")]
pub fn refresh_system_proxy() -> Result<(), String> {
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    // SAFETY: NULL handle + NULL buffer at length 0 is the documented
    // broadcast form of both options (no buffer is read or written).
    let changed = unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_SETTINGS_CHANGED,
            std::ptr::null_mut(),
            0,
        ) != 0
    };
    let refreshed = unsafe {
        InternetSetOptionW(
            std::ptr::null_mut(),
            INTERNET_OPTION_REFRESH,
            std::ptr::null_mut(),
            0,
        ) != 0
    };
    match changed && refreshed {
        true => Ok(()),
        false => Err("InternetSetOptionW settings broadcast failed".to_string()),
    }
}

// ------------------------------------------------------------------ macOS

/// Parse `networksetup -listallnetworkservices` output into enabled
/// service names. Skips the header line and `*`-disabled entries.
pub fn parse_network_services(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('*'))
        .map(str::to_string)
        .collect()
}

pub fn mac_enable_args(service: &str, socks: &SocketAddr) -> Argv {
    let (host, port) = host_port(socks);
    vec![
        "networksetup".to_string(),
        "-setsocksfirewallproxy".to_string(),
        service.to_string(),
        host,
        port,
    ]
}

pub fn mac_disable_args(service: &str) -> Argv {
    vec![
        "networksetup".to_string(),
        "-setsocksfirewallproxystate".to_string(),
        service.to_string(),
        "off".to_string(),
    ]
}

// ------------------------------------------------------------------ Linux

pub fn gnome_enable_args(socks: &SocketAddr) -> Vec<Argv> {
    let (host, port) = host_port(socks);
    vec![
        vec![
            "gsettings".to_string(),
            "set".to_string(),
            "org.gnome.system.proxy".to_string(),
            "mode".to_string(),
            "manual".to_string(),
        ],
        vec![
            "gsettings".to_string(),
            "set".to_string(),
            "org.gnome.system.proxy.socks".to_string(),
            "host".to_string(),
            host,
        ],
        vec![
            "gsettings".to_string(),
            "set".to_string(),
            "org.gnome.system.proxy.socks".to_string(),
            "port".to_string(),
            port,
        ],
    ]
}

pub fn gnome_disable_args() -> Vec<Argv> {
    vec![vec![
        "gsettings".to_string(),
        "set".to_string(),
        "org.gnome.system.proxy".to_string(),
        "mode".to_string(),
        "none".to_string(),
    ]]
}

/// Copy-paste fallback for KDE/Plasma (no `kwriteconfig5` execution here:
// printed for the user, applied by them).
pub fn kde_hint(socks: &SocketAddr) -> String {
    let (host, port) = host_port(socks);
    format!(
        "kwriteconfig5 --file kioslaverc --group 'Proxy Settings' --key ProxyType 1\n\
         kwriteconfig5 --file kioslaverc --group 'Proxy Settings' --key socksProxy \"socks://{host}:{port}\""
    )
}

// ------------------------------------------------------------------ guard

/// Best-effort cleanup: runs the "off" argv list on drop so a crash or
/// early return cannot leave the system proxy pointing at a dead SOCKS
/// listener (clean-exit guarantee, GUI_PLAN.md §3.2).
pub struct ProxyGuard {
    off: Vec<Argv>,
    disarmed: bool,
}

impl ProxyGuard {
    pub fn new(off: Vec<Argv>) -> Self {
        Self {
            off,
            disarmed: false,
        }
    }

    /// The tunnel closed normally and already restored the proxy: skip
    /// the drop-time cleanup.
    pub fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for ProxyGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        for argv in &self.off {
            let _ = apply_argv(argv);
        }
    }
}

// ------------------------------------------- system-proxy orchestration

/// First enabled macOS network service from `networksetup
/// -listallnetworkservices` output (pure wrapper over
/// [`parse_network_services`]; `None` when no enabled service exists).
pub fn detect_macos_service(listall_output: &str) -> Option<String> {
    parse_network_services(listall_output).into_iter().next()
}

// NOTE: no unit test executes `enable_system_proxy` (or the execution path
// of `ProxyGuard::disarm_and_disable` below) because they mutate the host OS
// proxy settings of the dev/CI machine. The pure `*_args` builders plus
// `detect_macos_service` above are the tested surface; the Drop-skip
// behavior after disarm is covered implicitly by `disarm` usage in
// `ModeManager::cleanup`.
pub fn enable_system_proxy(socks: &SocketAddr) -> Result<ProxyGuard, String> {
    #[cfg(target_os = "windows")]
    {
        for argv in win_enable_args(socks) {
            apply_argv(&argv).map_err(|e| e.to_string())?;
        }
        // Best-effort live refresh: already-open browsers pick the new
        // settings up without restart. Failure is ignored on purpose —
        // the registry write above is the durable effect and applies to
        // every new process regardless.
        let _ = refresh_system_proxy();
        Ok(ProxyGuard::new(win_disable_args()))
    }
    #[cfg(target_os = "macos")]
    {
        let list_argv = [
            "networksetup".to_string(),
            "-listallnetworkservices".to_string(),
        ];
        let output = apply_argv(&list_argv).map_err(|e| e.to_string())?;
        let service = detect_macos_service(&output)
            .ok_or_else(|| "no enabled network service found".to_string())?;
        apply_argv(&mac_enable_args(&service, socks)).map_err(|e| e.to_string())?;
        Ok(ProxyGuard::new(vec![mac_disable_args(&service)]))
    }
    #[cfg(target_os = "linux")]
    {
        for argv in gnome_enable_args(socks) {
            apply_argv(&argv).map_err(|e| e.to_string())?;
        }
        Ok(ProxyGuard::new(gnome_disable_args()))
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux"
    )))]
    {
        let _ = socks;
        Err("unsupported".to_string())
    }
}

impl ProxyGuard {
    /// Best-effort restore: applies each off-argv, collects one error
    /// string per failed command (empty vec = clean), then disarms so
    /// `Drop` skips the repeat run.
    ///
    /// NOTE: no unit test executes this — it mutates host OS proxy settings.
    pub fn disarm_and_disable(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        for argv in &self.off {
            if let Err(e) = apply_argv(argv) {
                failures.push(e.to_string());
            }
        }
        self.disarm();
        failures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socks() -> SocketAddr {
        "127.0.0.1:1819".parse().unwrap()
    }

    #[test]
    fn windows_registry_commands_point_at_socks() {
        let on = win_enable_args(&socks());
        assert_eq!(on.len(), 2);
        assert!(on[1].contains(&"socks=127.0.0.1:1819".to_string()));
        let off = win_disable_args();
        assert!(off[0].contains(&"/d".to_string()));
        assert!(off[0].contains(&"0".to_string()));
    }

    #[test]
    fn mac_service_list_skips_header_and_disabled() {
        let out = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\n*Disabled Adapter\nEthernet\n";
        assert_eq!(parse_network_services(out), vec!["Wi-Fi", "Ethernet"]);
    }

    #[test]
    fn mac_args_carry_service_and_port() {
        let argv = mac_enable_args("Wi-Fi", &socks());
        assert_eq!(
            argv,
            vec!["networksetup", "-setsocksfirewallproxy", "Wi-Fi", "127.0.0.1", "1819"]
        );
        assert!(mac_disable_args("Wi-Fi").contains(&"off".to_string()));
    }

    #[test]
    fn gnome_args_set_manual_then_host_port() {
        let on = gnome_enable_args(&socks());
        assert_eq!(on.len(), 3);
        assert!(on[0].contains(&"manual".to_string()));
        assert!(gnome_disable_args()[0].contains(&"none".to_string()));
        assert!(kde_hint(&socks()).contains("127.0.0.1:1819"));
    }

    #[test]
    fn empty_argv_is_an_error_not_a_spawn() {
        assert!(apply_argv(&[]).is_err());
    }

    #[test]
    fn detect_macos_service_returns_first_enabled() {
        let out = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\nEthernet\n";
        assert_eq!(detect_macos_service(out), Some("Wi-Fi".to_string()));
        assert_eq!(detect_macos_service("An asterisk (*) denotes nothing.\n"), None);
        assert_eq!(detect_macos_service(""), None);
    }
}
