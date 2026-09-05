package studio.cluvex.aether

/**
 * JNI facade over the `hev-socks5-tunnel` tun2socks worker (GUI_PLAN.md §4.3).
 *
 * Contract:
 * - [start] takes `fd`, the TUN file descriptor returned by
 *   `VpnService.Builder.establish()` (i.e. `tun.fd` in [VpnTunnelService]),
 *   plus the local SOCKS5 listener `host`/`port` the `tunnelStart` job
 *   serves (default `127.0.0.1:1819`). Returns `0` on success, non-zero
 *   on failure.
 * - [stop] terminates the worker. Call it before closing the TUN fd
 *   (see `VpnTunnelService.teardown()`).
 *
 * The native symbols live in `libaether_jni.so`, which links the worker.
 * Vendoring (NDK build — exact steps, do NOT vendor C sources by hand):
 * 1. Clone `hev-socks5-tunnel` at a pinned revision into
 *    `app/src/main/cpp/hev/`.
 * 2. In `app/src/main/cpp/CMakeLists.txt` add `add_subdirectory(hev)` and
 *    `target_link_libraries(aether_jni hev-socks5-tunnel)`.
 *
 * Until the worker is vendored and started, the tunnel stays in the
 * fail-closed skeleton mode described on [VpnTunnelService].
 *
 * ⚠️ Uncompiled here (no Android SDK/NDK); first Gradle/NDK build must verify.
 */
object HevSocks5Tunnel {
    init {
        System.loadLibrary("aether_jni")
    }

    external fun start(fd: Int, host: String, port: Int): Int
    external fun stop()
}
