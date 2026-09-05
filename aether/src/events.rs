//! GUI event stream (Phase 0.1).
//!
//! The GUI must never parse stdout. Instead the core emits
//! structured [`ApiEvent`]s at every phase transition
//! (provisioning → scanning → verifying → tunnel up/down →
//! reconnecting) and the GUI drains them — either via
//! [`subscribe_events`] (Rust/Tauri backend) or via the polled
//! `aether_events_poll` C export (JNI/Swift, see `ffi.rs`).
//!
//! Design notes:
//! - Synchronous, runtime-free push path: `emit` only locks a
//!   `parking_lot::Mutex<VecDeque>` ring buffer (cap 512). It never
//!   blocks on async, never panics, and is safe to call from any
//!   thread, including inside the FFI Tokio runtime.
//! - Async fan-out: a `tokio::sync::broadcast` channel mirrors every
//!   event for long-lived Rust subscribers (Tauri `connect` task).
//!   If no receiver exists the broadcast send error is ignored — the
//!   ring buffer remains the source of truth for `drain_events`.

use std::collections::VecDeque;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Maximum events kept for late pollers (GUI opened mid-scan, etc.).
pub const MAX_EVENTS: usize = 512;

/// Broadcast lag policy for async subscribers.
const BROADCAST_CAP: usize = 256;

/// Every state change the GUI is allowed to display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ApiEvent {
    Provisioning {
        transport: String,
    },
    IdentityReady {
        device_id: String,
        transport: String,
    },
    ScanStarted {
        transport: String,
        mode: String,
    },
    ScanCandidate {
        ip: String,
        port: u16,
        rtt_ms: u64,
    },
    ScanDone {
        endpoint: String,
        rtt_ms: u64,
    },
    Validating {
        peer: String,
    },
    VerifyOk {
        peer: String,
    },
    TunnelUp {
        peer: String,
        transport: String,
    },
    TunnelDown {
        reason: String,
    },
    Reconnecting {
        in_secs: u64,
    },
    Stats {
        rx_bytes: u64,
        tx_bytes: u64,
    },
    AuthNeeded {
        team: String,
    },
    StateChanged {
        state: String,
    },
    Error {
        message: String,
    },
}

impl ApiEvent {
    /// Endpoint helper used by scan paths.
    pub fn scan_done(endpoint: &std::net::SocketAddr, rtt_ms: u64) -> Self {
        ApiEvent::ScanDone {
            endpoint: endpoint.to_string(),
            rtt_ms,
        }
    }

    /// Tunnel-up helper.
    pub fn tunnel_up(peer: &std::net::SocketAddr, transport: &str) -> Self {
        ApiEvent::TunnelUp {
            peer: peer.to_string(),
            transport: transport.to_string(),
        }
    }
}

fn queue() -> &'static parking_lot::Mutex<VecDeque<ApiEvent>> {
    static QUEUE: OnceLock<parking_lot::Mutex<VecDeque<ApiEvent>>> = OnceLock::new();
    QUEUE.get_or_init(|| parking_lot::Mutex::new(VecDeque::with_capacity(MAX_EVENTS)))
}

fn broadcaster() -> &'static tokio::sync::broadcast::Sender<ApiEvent> {
    static TX: OnceLock<tokio::sync::broadcast::Sender<ApiEvent>> = OnceLock::new();
    TX.get_or_init(|| tokio::sync::broadcast::channel(BROADCAST_CAP).0)
}

/// Emit one event. Never panics, never blocks on async.
pub fn emit(event: ApiEvent) {
    {
        let mut q = queue().lock();
        if q.len() >= MAX_EVENTS {
            q.pop_front();
        }
        q.push_back(event.clone());
    }
    let _ = broadcaster().send(event);
}

/// Subscribe to live events (Rust/Tauri backend path).
pub fn subscribe_events() -> tokio::sync::broadcast::Receiver<ApiEvent> {
    broadcaster().subscribe()
}

/// Drain up to `max` buffered events, oldest first.
/// `max == 0` means "all".
pub fn drain_events(max: usize) -> Vec<ApiEvent> {
    let mut q = queue().lock();
    if max == 0 || max >= q.len() {
        return q.drain(..).collect();
    }
    let mut out = Vec::with_capacity(max);
    for _ in 0..max {
        match q.pop_front() {
            Some(e) => out.push(e),
            None => break,
        }
    }
    out
}

/// Number of buffered (undrained) events. Useful for tests.
pub fn buffered_len() -> usize {
    queue().lock().len()
}

/// Clear the buffer. Tests and "Reconnect" flows use this to avoid
/// showing stale candidates from a previous run.
pub fn clear() {
    queue().lock().clear();
}

/// Process-wide lock for tests touching the global event/log buffers
/// or process env. Rust runs tests in threads sharing one process, so
/// `clear → emit → drain` sequences in different test modules would
/// otherwise flake. Hold the guard for the whole test body.
#[cfg(test)]
pub(crate) fn lock_for_test() -> parking_lot::MutexGuard<'static, ()> {
    static LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    LOCK.lock()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_then_drain_returns_fifo_order() {
        let _guard = lock_for_test();
        clear();
        emit(ApiEvent::StateChanged {
            state: "scanning".to_string(),
        });
        emit(ApiEvent::ScanCandidate {
            ip: "162.159.192.1".to_string(),
            port: 443,
            rtt_ms: 42,
        });
        assert_eq!(buffered_len(), 2);
        let drained = drain_events(0);
        assert_eq!(drained.len(), 2);
        assert!(matches!(drained[0], ApiEvent::StateChanged { .. }));
        assert!(matches!(drained[1], ApiEvent::ScanCandidate { .. }));
        assert_eq!(buffered_len(), 0);
    }

    #[test]
    fn drain_respects_max() {
        let _guard = lock_for_test();
        clear();
        for i in 0..5 {
            emit(ApiEvent::Reconnecting { in_secs: i });
        }
        let first = drain_events(2);
        assert_eq!(first.len(), 2);
        assert_eq!(buffered_len(), 3);
        clear();
    }

    #[test]
    fn ring_buffer_caps_at_max_events() {
        let _guard = lock_for_test();
        clear();
        for i in 0..(MAX_EVENTS + 50) {
            emit(ApiEvent::Reconnecting { in_secs: i as u64 });
        }
        assert_eq!(buffered_len(), MAX_EVENTS);
        // Oldest entries were evicted; newest survives.
        let all = drain_events(0);
        assert!(matches!(
            all.last(),
            Some(ApiEvent::Reconnecting { in_secs }) if *in_secs == (MAX_EVENTS + 49) as u64
        ));
    }

    #[test]
    fn events_serialize_with_type_tag_for_ffi() {
        let e = ApiEvent::ScanDone {
            endpoint: "1.2.3.4:443".to_string(),
            rtt_ms: 12,
        };
        let v = serde_json::to_value(&e).expect("serializable");
        assert_eq!(v["type"], serde_json::json!("scan_done"));
        assert_eq!(v["endpoint"], serde_json::json!("1.2.3.4:443"));
    }
}
