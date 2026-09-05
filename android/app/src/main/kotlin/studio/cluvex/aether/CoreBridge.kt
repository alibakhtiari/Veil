package studio.cluvex.aether

/**
 * JNI bridge over libaether.so (Phase 3 skeleton).
 *
 * Contract: polled jobs only, no callbacks — mirrors aether/src/ffi.rs.
 * Every `*_start` returns a JSON string `{"ok":true,"job":id}`;
 * the caller polls `jobPoll(id)` until `state == "done"`.
 *
 * String lifetime: every `String` returned here is an ordinary
 * garbage-collected JVM string. The native shim (`aether_jni.c`
 * `owned_to_jstring`) already releases the Rust-owned buffer with
 * `aether_string_free` before returning — callers must NOT call
 * `stringFree` on returned strings. `stringFree(ptr)` exists only for
 * raw native pointers, which this bridge never hands out; it is kept
 * for ABI completeness.
 *
 * JSON payloads reuse the desktop shapes:
 * ScanPayload  {transport, mode, ip, profile, ports, excluded, ech, wanted}
 * TunnelPayload{peer, transport, socks, http, profile, keepalive, ech,
 *               outer, inner}  // outer/inner = gool hops; peer stays
 *                              // as the outer-hop alias for back-compat.
 */
object CoreBridge {
    init {
        System.loadLibrary("aether")
    }

    // --- lifecycle -------------------------------------------------
    external fun version(): String
    external fun stringFree(ptr: Long)
    external fun jobPoll(id: Long): String
    external fun jobCancel(id: Long): String
    external fun jobFree(id: Long): String

    // --- identity --------------------------------------------------
    external fun identityOpen(payloadJson: String): String
    external fun identitySummary(identityId: Long): String
    external fun identityFree(identityId: Long): String

    // --- scan / verify / tunnel ------------------------------------
    external fun scanStart(identityId: Long, payloadJson: String): String
    external fun verifyStart(identityId: Long, payloadJson: String): String
    external fun tunnelStart(identityId: Long, payloadJson: String): String
    external fun coreStart(argsJsonArray: String): String

    // --- GUI streams (Phase 0) --------------------------------------
    /** Returns `{"ok":true,"events":[...]}`; max==0 means all buffered. */
    external fun eventsPoll(max: Long): String
    external fun eventsClear(): String

    /**
     * Returns `{"ok":true,"logs":[...]}`.
     * level: "" (all) or error|warn|info|debug|trace. Anything else is
     * an `{"ok":false}` error (never silently remapped).
     */
    external fun logPoll(max: Long, level: String): String

    // --- ZeroTrust ---------------------------------------------------
    external fun teamSignIn(payloadJson: String): String
    external fun teamCodeRequest(payloadJson: String): String
    external fun teamCodeResend(sessionId: Long): String
    external fun teamCodeSubmit(sessionId: Long, code: String): String
    external fun teamSessionFree(sessionId: Long): String

    /**
     * Stores a pre-obtained enrolment JWT for later team sign-ins.
     * Synchronous and blocking — call off the main thread.
     * Returns `{"ok":true,"stored":true}` or `{"ok":false,"error":...}`.
     */
    external fun teamTokenSet(token: String): String

    /**
     * Forgets the stored enrolment token. Synchronous; safe anywhere.
     * Returns `{"ok":true,"cleared":true}`.
     */
    external fun teamTokenClear(): String

    // --- settings validation (pure, no I/O) --------------------------
    /** Returns `{"ok":true,"valid":true}` or `{"ok":false,"error":...}`. */
    external fun guiValidate(tomlText: String): String

    /** Returns `{"ok":true,"defaults_toml":"..."}` for reset-to-defaults. */
    external fun guiDefaults(): String
}
