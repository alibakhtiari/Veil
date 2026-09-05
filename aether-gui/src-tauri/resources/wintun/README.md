# Bundled Wintun driver (Windows TUN mode)

Precompiled `wintun.dll` per architecture, loaded by the TUN driver for
the Windows virtual adapter (GUI_PLAN.md §3.3).

- **Provenance:** https://www.wintun.net/builds/wintun-0.14.1.zip
  (official WireGuard project release), fetched 2026-09-05, 750540 bytes.
- **License:** `LICENSE.txt` in this directory (same archive).
- **Layout:** renamed per arch (`wintun-<arch>.dll`) so the backend can
  pick by `std::env::consts::ARCH` (`x86_64` → amd64, `aarch64` →
  arm64; x86/arm kept for completeness).
- **Bundling:** referenced from `tauri.conf.json`
  `bundle.resources`; at runtime they resolve under `$RESOURCES`.
- **Loading:** NOT yet wired — the `tun` crate driver (`tun.rs`,
  `--features tun`) is adapter-creation only; wintun load + packet
  forwarding is the remaining TUN work (NEXT-STEPS.md tracks it).
- **`wintun.h`:** C header from the same archive, kept as the reference
  for the future loader (`WintunCreateAdapter`, session/ring APIs).
