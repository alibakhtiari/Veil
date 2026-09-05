//! Structured in-memory log buffer for GUI clients (Phase 0.2).
//!
//! Standard logging uses `env_logger` on stdout. GUI processes link the
//! core as a library and have no stdout to scrape, so they drain this
//! ring buffer instead — via `drain_logs` (Rust) or the polled
//! `aether_log_poll` C export (FFI).
//!
//! Deliberately NOT a `log::Log` implementation in v1: `env_logger`
//! already installs the global logger and `log` allows only one.
//! Callers that want a record in both places use [`gui_log`] (which
//! emits via `log::*!` AND buffers). Hot paths that only need the GUI
//! buffer use [`push`]. A forwarding appender that mirrors every
//! `log::Record` into this buffer is Phase-4 hardening, not Phase 0.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Ring capacity: enough for a full scan + tunnel session at `info`,
/// small enough to stay resident on low-end Android devices.
pub const MAX_LOGS: usize = 5000;

/// Log levels, mirroring `AETHER_LOG_LEVEL` + `RUST_LOG`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuiLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl GuiLogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            GuiLogLevel::Error => "error",
            GuiLogLevel::Warn => "warn",
            GuiLogLevel::Info => "info",
            GuiLogLevel::Debug => "debug",
            GuiLogLevel::Trace => "trace",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_lowercase().as_str() {
            "error" => GuiLogLevel::Error,
            "warn" | "warning" => GuiLogLevel::Warn,
            "debug" => GuiLogLevel::Debug,
            "trace" => GuiLogLevel::Trace,
            _ => GuiLogLevel::Info,
        }
    }
}

/// One buffered log line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiLogRecord {
    pub seq: u64,
    pub level: GuiLogLevel,
    pub target: String,
    pub message: String,
    /// Milliseconds since UNIX epoch (GUI formats locally).
    pub ts_ms: u64,
}

fn buffer() -> &'static parking_lot::Mutex<VecDeque<GuiLogRecord>> {
    static BUF: OnceLock<parking_lot::Mutex<VecDeque<GuiLogRecord>>> = OnceLock::new();
    BUF.get_or_init(|| parking_lot::Mutex::new(VecDeque::with_capacity(1024)))
}

fn next_seq() -> u64 {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    SEQ.fetch_add(1, Ordering::Relaxed)
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Push a record into the buffer only (no `log::!` emission).
pub fn push(level: GuiLogLevel, target: &str, message: &str) {
    let mut buf = buffer().lock();
    if buf.len() >= MAX_LOGS {
        buf.pop_front();
    }
    buf.push_back(GuiLogRecord {
        seq: next_seq(),
        level,
        target: target.to_string(),
        message: message.to_string(),
        ts_ms: now_ms(),
    });
}

/// Log via `log::*` AND buffer, so standard logging and GUI ring buffer see the same line.
/// Prefer this at phase transitions (scan done, tunnel up/down).
pub fn gui_log(level: GuiLogLevel, target: &str, message: &str) {
    match level {
        GuiLogLevel::Error => log::error!(target: target, "{message}"),
        GuiLogLevel::Warn => log::warn!(target: target, "{message}"),
        GuiLogLevel::Info => log::info!(target: target, "{message}"),
        GuiLogLevel::Debug => log::debug!(target: target, "{message}"),
        GuiLogLevel::Trace => log::trace!(target: target, "{message}"),
    }
    push(level, target, message);
}

/// Drain up to `max` records (oldest first). `max == 0` means all.
/// Optional `min_level` filters below a severity (e.g. only warn+).
pub fn drain_logs(max: usize, min_level: Option<GuiLogLevel>) -> Vec<GuiLogRecord> {
    fn rank(l: GuiLogLevel) -> u8 {
        match l {
            GuiLogLevel::Error => 0,
            GuiLogLevel::Warn => 1,
            GuiLogLevel::Info => 2,
            GuiLogLevel::Debug => 3,
            GuiLogLevel::Trace => 4,
        }
    }
    let mut buf = buffer().lock();
    // Fast path: unfiltered full drain moves the deque out whole — no
    // per-record work and no scratch allocation. This is the hot path
    // (GUI log viewer polling every second).
    if max == 0 && min_level.is_none() {
        return std::mem::take(&mut *buf).into_iter().collect();
    }
    let mut out = Vec::new();
    let limit = if max == 0 { usize::MAX } else { max };
    // Single pass: pop from front, keep what matches, stash the rest.
    // Buffer is small (<= 5000) so a temp Vec is fine.
    let mut kept = VecDeque::new();
    while let Some(rec) = buf.pop_front() {
        let ok = match min_level {
            Some(min) => rank(rec.level) <= rank(min),
            None => true,
        };
        if ok && out.len() < limit {
            out.push(rec);
        } else {
            kept.push_back(rec);
        }
        if out.len() >= limit && min_level.is_none() {
            // Unfiltered limit hit — move the rest as-is.
            kept.extend(buf.drain(..));
            break;
        }
    }
    *buf = kept;
    out
}

/// Clone up to `max` records (newest last) without draining.
/// Diagnostics snapshots use this so reading logs never reorders them.
pub fn peek_logs(max: usize) -> Vec<GuiLogRecord> {
    let buf = buffer().lock();
    if max == 0 || max >= buf.len() {
        return buf.iter().cloned().collect();
    }
    buf.iter().skip(buf.len() - max).cloned().collect()
}

/// Buffered record count (for tests / "N new lines" badges).
pub fn buffered_len() -> usize {
    buffer().lock().len()
}

/// Clear the buffer (tests, "Clear log" button).
pub fn clear() {
    buffer().lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_then_drain_fifo() {
        let _guard = crate::events::lock_for_test();
        clear();
        push(GuiLogLevel::Info, "test", "hello");
        push(GuiLogLevel::Warn, "test", "careful");
        assert_eq!(buffered_len(), 2);
        let got = drain_logs(0, None);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].message, "hello");
        assert_eq!(got[1].level, GuiLogLevel::Warn);
        assert!(got[0].seq < got[1].seq);
    }

    #[test]
    fn level_filter_keeps_severe_records_buffered() {
        let _guard = crate::events::lock_for_test();
        clear();
        push(GuiLogLevel::Info, "t", "info-line");
        push(GuiLogLevel::Error, "t", "boom");
        let got = drain_logs(0, Some(GuiLogLevel::Warn));
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].message, "boom");
        // The filtered-out info line stays buffered.
        assert_eq!(buffered_len(), 1);
        clear();
    }

    #[test]
    fn ring_caps_memory() {
        let _guard = crate::events::lock_for_test();
        clear();
        for i in 0..(MAX_LOGS + 10) {
            push(GuiLogLevel::Debug, "t", &format!("line {i}"));
        }
        assert_eq!(buffered_len(), MAX_LOGS);
        clear();
    }

    #[test]
    fn level_parse_defaults_to_info() {
        assert_eq!(GuiLogLevel::parse("debug"), GuiLogLevel::Debug);
        assert_eq!(GuiLogLevel::parse("nonsense"), GuiLogLevel::Info);
    }

    #[test]
    fn peek_does_not_consume_or_reorder() {
        let _guard = crate::events::lock_for_test();
        clear();
        push(GuiLogLevel::Info, "t", "first");
        push(GuiLogLevel::Info, "t", "second");
        let peeked = peek_logs(10);
        assert_eq!(peeked.len(), 2);
        assert_eq!(peeked[0].message, "first");
        assert_eq!(buffered_len(), 2);
        let tail = peek_logs(1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].message, "second");
        clear();
    }
}
