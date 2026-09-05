/* JNI shim: studio.cluvex.aether.CoreBridge -> libaether C API.
 *
 * Phase-3 skeleton. Every `aether_*` symbol lives in libaether.so
 * (built from aether/src/ffi.rs via cargo-ndk); this file only adapts
 * jstring/jlong to the C ABI. The FFI contract it relies on:
 *
 *   - every call returns an OWNED *mut c_char holding JSON
 *     {"ok":true,...} or {"ok":false,"error":...};
 *   - the shim converts it to jstring, then releases it with
 *     aether_string_free -- no leaks, no use-after-free;
 *   - jobs are polled, never callbacks (see ffi.rs header).
 *
 * Build: app/CMakeLists.txt links this file against the per-ABI
 * libaether.so under app/src/main/jniLibs/<abi>/ (produced by the
 * existing release.yml:android job + cargo-ndk).
 */

#include <jni.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>

#include "hev-main.h"

/* ---- libaether C API (mirrors aether/src/ffi.rs) ---- */
extern char *aether_version(void);
extern void aether_string_free(char *raw);
extern char *aether_job_poll(uint64_t id);
extern char *aether_job_cancel(uint64_t id);
extern char *aether_job_free(uint64_t id);
extern char *aether_identity_open(const char *payload);
extern char *aether_identity_summary(uint64_t id);
extern char *aether_identity_free(uint64_t id);
extern char *aether_scan_start(uint64_t identity, const char *payload);
extern char *aether_verify_start(uint64_t identity, const char *payload);
extern char *aether_tunnel_start(uint64_t identity, const char *payload);
extern char *aether_core_start(const char *arguments);
extern char *aether_events_poll(uint64_t max);
extern char *aether_events_clear(void);
extern char *aether_log_poll(uint64_t max, const char *level);
extern char *aether_gui_validate(const char *toml_text);
extern char *aether_gui_defaults(void);
extern char *aether_team_sign_in(const char *payload);
extern char *aether_team_code_request(const char *payload);
extern char *aether_team_code_resend(uint64_t session);
extern char *aether_team_code_submit(uint64_t session, const char *code);
extern char *aether_team_session_free(uint64_t id);
extern char *aether_team_token_set(const char *token);
extern char *aether_team_token_clear(void);

/* ---- helpers ---- */

static jstring owned_to_jstring(JNIEnv *env, char *owned) {
    jstring out;
    if (owned == NULL) {
        return (*env)->NewStringUTF(env, "{\"ok\":false,\"error\":\"null reply\"}");
    }
    out = (*env)->NewStringUTF(env, owned);
    aether_string_free(owned);
    return out;
}

static const char *jstr(JNIEnv *env, jstring s) {
    if (s == NULL) {
        return NULL;
    }
    return (*env)->GetStringUTFChars(env, s, NULL);
}

static void jstr_end(JNIEnv *env, jstring s, const char *c) {
    if (s != NULL && c != NULL) {
        (*env)->ReleaseStringUTFChars(env, s, c);
    }
}

#define CLS(x) Java_studio_cluvex_aether_CoreBridge_##x

/* ---- lifecycle ---- */

JNIEXPORT jstring JNICALL CLS(version)(JNIEnv *env, jobject thiz) {
    (void)thiz;
    return owned_to_jstring(env, aether_version());
}

JNIEXPORT void JNICALL CLS(stringFree)(JNIEnv *env, jobject thiz, jlong ptr) {
    (void)env;
    (void)thiz;
    if (ptr != 0) {
        aether_string_free((char *)(uintptr_t)ptr);
    }
}

JNIEXPORT jstring JNICALL CLS(jobPoll)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_job_poll((uint64_t)id));
}

JNIEXPORT jstring JNICALL CLS(jobCancel)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_job_cancel((uint64_t)id));
}

JNIEXPORT jstring JNICALL CLS(jobFree)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_job_free((uint64_t)id));
}

/* ---- identity ---- */

JNIEXPORT jstring JNICALL CLS(identityOpen)(JNIEnv *env, jobject thiz, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_identity_open(c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(identitySummary)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_identity_summary((uint64_t)id));
}

JNIEXPORT jstring JNICALL CLS(identityFree)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_identity_free((uint64_t)id));
}

/* ---- scan / verify / tunnel ---- */

JNIEXPORT jstring JNICALL CLS(scanStart)(JNIEnv *env, jobject thiz, jlong id, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_scan_start((uint64_t)id, c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(verifyStart)(JNIEnv *env, jobject thiz, jlong id, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_verify_start((uint64_t)id, c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(tunnelStart)(JNIEnv *env, jobject thiz, jlong id, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_tunnel_start((uint64_t)id, c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(coreStart)(JNIEnv *env, jobject thiz, jstring args) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, args);
    reply = aether_core_start(c);
    jstr_end(env, args, c);
    return owned_to_jstring(env, reply);
}

/* ---- GUI streams (Phase 0) ---- */

JNIEXPORT jstring JNICALL CLS(eventsPoll)(JNIEnv *env, jobject thiz, jlong max) {
    (void)thiz;
    return owned_to_jstring(env, aether_events_poll((uint64_t)max));
}

JNIEXPORT jstring JNICALL CLS(eventsClear)(JNIEnv *env, jobject thiz) {
    (void)thiz;
    return owned_to_jstring(env, aether_events_clear());
}

JNIEXPORT jstring JNICALL CLS(logPoll)(JNIEnv *env, jobject thiz, jlong max, jstring level) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, level);
    reply = aether_log_poll((uint64_t)max, c);
    jstr_end(env, level, c);
    return owned_to_jstring(env, reply);
}

/* ---- ZeroTrust ---- */

JNIEXPORT jstring JNICALL CLS(teamSignIn)(JNIEnv *env, jobject thiz, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_team_sign_in(c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(teamCodeRequest)(JNIEnv *env, jobject thiz, jstring payload) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, payload);
    reply = aether_team_code_request(c);
    jstr_end(env, payload, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(teamCodeResend)(JNIEnv *env, jobject thiz, jlong session) {
    (void)thiz;
    return owned_to_jstring(env, aether_team_code_resend((uint64_t)session));
}

JNIEXPORT jstring JNICALL CLS(teamCodeSubmit)(JNIEnv *env, jobject thiz, jlong session, jstring code) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, code);
    reply = aether_team_code_submit((uint64_t)session, c);
    jstr_end(env, code, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(teamSessionFree)(JNIEnv *env, jobject thiz, jlong id) {
    (void)thiz;
    return owned_to_jstring(env, aether_team_session_free((uint64_t)id));
}

/* NOTE: teamTokenSet blocks on the core runtime (block_on). The Kotlin
 * declaration requires callers to run it off the main thread. */
JNIEXPORT jstring JNICALL CLS(teamTokenSet)(JNIEnv *env, jobject thiz, jstring token) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, token);
    reply = aether_team_token_set(c);
    jstr_end(env, token, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(teamTokenClear)(JNIEnv *env, jobject thiz) {
    (void)thiz;
    return owned_to_jstring(env, aether_team_token_clear());
}

/* ---- settings validation (pure, no I/O) ---- */

JNIEXPORT jstring JNICALL CLS(guiValidate)(JNIEnv *env, jobject thiz, jstring toml) {
    const char *c;
    char *reply;
    (void)thiz;
    c = jstr(env, toml);
    reply = aether_gui_validate(c);
    jstr_end(env, toml, c);
    return owned_to_jstring(env, reply);
}

JNIEXPORT jstring JNICALL CLS(guiDefaults)(JNIEnv *env, jobject thiz) {
    (void)thiz;
    return owned_to_jstring(env, aether_gui_defaults());
}

/* ---- hev-socks5-tunnel worker (HevSocks5Tunnel facade) ----
 *
 * The public API (hev/src/hev-main.h) is:
 *   int  hev_socks5_tunnel_main_from_str(const unsigned char *, unsigned int, int);
 *   void hev_socks5_tunnel_quit(void);
 * (plus the file-backed twin hev_socks5_tunnel_main_from_file and the
 * stats accessor hev_socks5_tunnel_stats, which the Kotlin facade does
 * not expose, so both stay unwired.)
 *
 * start() therefore renders host/port into a minimal YAML config and runs
 * the blocking main on a worker thread — the same shape as upstream
 * hev/src/hev-jni.c (single guarded thread, config owned by the thread).
 * stop() signals quit and joins. Return codes: 0 = worker launched;
 * negative = not launched (bad args / already running / OOM / spawn fail).
 */

#define HEV_HOST_MAX 256

struct hev_work {
    unsigned char *config;
    unsigned int config_len;
    int fd;
};

static pthread_t hev_thread = 0;
static pthread_mutex_t hev_mutex = PTHREAD_MUTEX_INITIALIZER;

static void *hev_thread_main(void *arg) {
    struct hev_work *w = (struct hev_work *)arg;
    hev_socks5_tunnel_main_from_str(w->config, w->config_len, w->fd);
    free(w->config);
    free(w);
    return NULL;
}

JNIEXPORT jint JNICALL
Java_studio_cluvex_aether_HevSocks5Tunnel_start(JNIEnv *env, jobject thiz, jint fd, jstring host, jint port) {
    const char *h;
    char stack[512];
    int n;
    struct hev_work *w;
    (void)thiz;
    if (fd < 0 || port <= 0 || port > 65535 || host == NULL) {
        return -1;
    }
    pthread_mutex_lock(&hev_mutex);
    if (hev_thread != 0) {
        pthread_mutex_unlock(&hev_mutex);
        return -2;
    }
    h = jstr(env, host);
    if (h == NULL) {
        pthread_mutex_unlock(&hev_mutex);
        return -3;
    }
    if (strlen(h) == 0 || strlen(h) > HEV_HOST_MAX) {
        jstr_end(env, host, h);
        pthread_mutex_unlock(&hev_mutex);
        return -4;
    }
    n = snprintf(stack, sizeof(stack),
        "tunnel:\n"
        "  mtu: 1500\n"
        "  ipv4: 198.18.0.1\n"
        "  ipv6: 'fc00::1'\n"
        "socks5:\n"
        "  address: %s\n"
        "  port: %d\n"
        "  udp: 'udp'\n",
        h, (int)port);
    jstr_end(env, host, h);
    if (n <= 0 || (size_t)n >= sizeof(stack)) {
        pthread_mutex_unlock(&hev_mutex);
        return -5;
    }
    w = (struct hev_work *)malloc(sizeof(*w));
    if (w == NULL) {
        pthread_mutex_unlock(&hev_mutex);
        return -6;
    }
    w->config = (unsigned char *)malloc((size_t)n + 1);
    if (w->config == NULL) {
        free(w);
        pthread_mutex_unlock(&hev_mutex);
        return -6;
    }
    memcpy(w->config, stack, (size_t)n + 1);
    w->config_len = (unsigned int)n;
    w->fd = (int)fd;
    if (pthread_create(&hev_thread, NULL, hev_thread_main, w) != 0) {
        hev_thread = 0;
        free(w->config);
        free(w);
        pthread_mutex_unlock(&hev_mutex);
        return -7;
    }
    pthread_mutex_unlock(&hev_mutex);
    return 0;
}

JNIEXPORT void JNICALL
Java_studio_cluvex_aether_HevSocks5Tunnel_stop(JNIEnv *env, jobject thiz) {
    pthread_t t;
    (void)env;
    (void)thiz;
    pthread_mutex_lock(&hev_mutex);
    t = hev_thread;
    if (t == 0) {
        pthread_mutex_unlock(&hev_mutex);
        return;
    }
    hev_socks5_tunnel_quit();
    pthread_join(t, NULL);
    hev_thread = 0;
    pthread_mutex_unlock(&hev_mutex);
}
