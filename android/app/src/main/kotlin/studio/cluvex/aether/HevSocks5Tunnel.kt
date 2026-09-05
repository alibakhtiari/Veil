package studio.cluvex.aether

/**
 * JNI facade over the vendored `hev-socks5-tunnel` worker.
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
 */
object HevSocks5Tunnel {
    init {
        System.loadLibrary("aether_jni")
    }

    external fun start(fd: Int, host: String, port: Int): Int
    external fun stop()
}
