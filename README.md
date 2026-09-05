# Veil

### اینترنت آزاد برای همه :))
**[راهنمای فارسی](README.fa.md)**

Veil is an encrypted stealth tunnel client designed for heavily restricted networks. It automatically discovers reachable routes, establishes an obfuscated tunnel, and exposes a local SOCKS5 proxy or provides full device VPN routing through modern desktop and mobile graphical applications.

Unlike traditional VPN clients, Veil is engineered for environments where Deep Packet Inspection (DPI), TLS/SNI filtering, protocol fingerprinting, UDP throttling, and endpoint blocking are prevalent.

## Features

- **Modern Graphical Clients:** Dedicated Desktop GUI (Windows, macOS, Linux) with system proxy and TUN routing, plus a native Android app (`VpnService`).
- **Data-Plane Validation:** Automatic endpoint discovery with end-to-end data-plane validation — gateways are only accepted once they pass real traffic.
- **MASQUE (HTTP/3 & HTTP/2):** Obfuscated encapsulation with optional TLS ClientHello fragmentation on HTTP/2.
- **WireGuard & Nested WireGuard (`gool`):** Double-hop WireGuard routing with automated hop discovery.
- **Advanced Obfuscation:** Configurable noise profiles to defeat DPI and handshake fingerprinting.
- **Flexible Routing Rules (Split Tunneling):** Bypass or block traffic by domain, CIDR, or port, extracted from TLS SNI to work transparently behind TUN mode.
- **Upstream Proxy Chaining:** Route tunnel egress through existing proxies or VPNs.
- **Zero Trust Support:** Direct Cloudflare Zero Trust team enrollment with one-time codes and service tokens.

## Download

Prebuilt installers and applications are available on the Releases page:

- **Windows:** `.msi` installer, `.exe` setup (x86_64 & ARM64)
- **macOS:** `.dmg` (Universal Binary, Apple Silicon ARM64, and Intel/AMD x86_64)
- **Linux:** `.AppImage`, `.deb`, `.rpm` (x86_64 & ARM64)
- **Android:** `.apk` (Native Android App with `VpnService`), `.aab`

## Desktop & Mobile GUI

- **Desktop:** Run `veil-gui` for system tray integration, one-click connect, live throughput stats, and traffic mode switching (Proxy Only / System Proxy / TUN VPN).
- **Android:** Launch the Veil app, grant the standard VPN permission, and tap Connect.

## Building from Source

### Requirements
- Rust 1.91 or newer
- C/C++ compiler and CMake
- Node.js 20+ (for building the Desktop GUI frontend)

Ensure `quiche` is present in the repository tree:

```bash
# Run core test suite
cargo test --manifest-path aether/Cargo.toml --lib

# Build the desktop GUI application
cargo tauri build --manifest-path aether-gui/src-tauri/Cargo.toml
```

## Credits & License

Veil is an independent personal fork based on the **Aether** engine (Copyright © 2024–2025 CluvexStudio / Aether Contributors), licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**.
- MASQUE protocol implementation is built on Cloudflare's **Quiche** library.
- Desktop application is built with **Tauri v2** and **Svelte 5**.
- Android client is built with **Kotlin** and Jetpack Compose.

See the `LICENSE` file for details.
