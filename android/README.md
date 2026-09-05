# Android app (Phase 3 skeleton, Track B)

Native Kotlin app. All crypto/scan/tunnel logic stays in Rust
(`libaether.so` via JNI → `aether/src/ffi.rs` job API). No Java
reimplementation of probing or obfuscation, ever.

## JNI surface (implemented in `CoreBridge.kt`)

Only the polled job model — no callbacks (matches `ffi.rs` invariant):

- `aether_identity_open` → `aether_job_poll/free`
- `aether_scan_start` → poll → `{"endpoint"}`
- `aether_verify_start` → poll → `{"reachable"}`
- `aether_tunnel_start` → long-running job → `aether_job_cancel`
- `aether_events_poll(max)` → `{"events": [...]}` (progress UI)
- `aether_log_poll(max, level)` → `{"logs": [...]}` (log viewer)
- `aether_team_code_request/resend/submit` (ZeroTrust OTP)

JSON payload shapes reuse the desktop `ScanPayload`/`TunnelPayload`
(including `wanted`, `outer`/`inner` for gool).

## Packet path (no core TUN code)

`VpnService.Builder` creates the TUN fd; the app forwards TCP/UDP
through the local SOCKS listener (tun2socks-style). The core still
only sees SOCKS clients — identical to desktop. SNI sniffing
(`AETHER_ROUTE_SNIFF*`) keeps name routing working behind the TUN
front end; per-app allow/block uses
`addAllowedApplication`/`addDisallowedApplication`.

## Files

- `app/src/main/kotlin/studio/cluvex/aether/CoreBridge.kt` — JNI
  declarations, 1:1 with the C shim below.
- `app/src/main/kotlin/studio/cluvex/aether/VpnTunnelService.kt` —
  TUN front end (SOCKS-forward mode), per-app allow/disallow lists,
  foreground notification. tun2socks worker hookup is TODO.
- `app/src/main/kotlin/studio/cluvex/aether/HevSocks5Tunnel.kt` — JNI
  facade over the hev-socks5-tunnel worker (`start(fd, host, port)` /
  `stop()`); native sources not yet vendored (see KDoc for exact steps).
  ⚠️ Not yet compiled here (no JDK/NDK in this environment); first NDK
  build must verify it.
- `app/src/main/kotlin/studio/cluvex/aether/MainActivity.kt` — LAUNCHER
  Compose UI (status text + Connect/Disconnect button) with the
  `VpnService.prepare()` consent flow firing `VpnTunnelService`
  start/stop intents.
  ⚠️ Not yet compiled here (no SDK in this environment); first Gradle
  build must verify it.
- `app/src/main/kotlin/studio/cluvex/aether/AetherTileService.kt` —
  Quick Settings tile firing start/stop intents.
- `app/src/main/AndroidManifest.xml` — launcher activity, VPN + tile
  services, permissions.
- `app/src/main/cpp/aether_jni.c` — ownership-safe shim
  (jstring → UTF chars → `aether_*` → jstring → `aether_string_free`).
  ⚠️ Not yet compiled here (no JDK/NDK in this environment); first NDK
  build must verify it.
- `app/src/main/cpp/CMakeLists.txt` — links the shim against the
  per-ABI `jniLibs/<abi>/libaether.so` (see wiring snippet inside).
- `.gitignore` — ignores `build/`, `.gradle/`, `local.properties`,
  `*.iml`, and the CI-generated `app/src/main/jniLibs/`.
- `settings.gradle`, `build.gradle`, `app/build.gradle`,
  `gradle.properties`, `gradlew`, `gradlew.bat`,
  `gradle/wrapper/` — Gradle 8.9 + AGP 8.5 + NDK r26d, Kotlin 2.0.20
  with the Compose compiler plugin.

## Build (CI-first: no local Android setup needed)

Push a `v*` tag and the `gui-android` CI job builds `libaether.so` per
ABI (NDK r26d, platform 24, `cargo-ndk`), compiles the app with Gradle,
signs it (`ANDROID_*` secrets), and uploads APK/AAB + checksums. The
`release` job waits for it (`needs`) and publishes everything with
`SHA256SUMS.txt` covering all formats.

Local iteration only (no Android
Studio required, headless cmdline-tools + Gradle suffice):

```sh
# From the repo root (same as the gui-android CI job):
cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs/ \
  build --manifest-path aether/Cargo.toml --release
# ... repeat for armeabi-v7a, x86_64, then:
cd android && gradle assembleDebug   # no signing keys needed for debug
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

`libaether.so` ABIs: `aarch64-linux-android`, `armv7-linux-androideabi`,
`x86_64-linux-android` → `app/src/main/jniLibs/<abi>/`.

Compose UI + tun2socks worker hookup arrive with the Phase-3 app work;
this tree locks the bridge contract so the core can evolve without
breaking it.
