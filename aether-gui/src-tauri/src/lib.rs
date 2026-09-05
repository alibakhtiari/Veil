//! Aether desktop GUI backend — Phase 1 scaffold.
//!
//! This crate is the future Tauri `src-tauri` backend, extracted early so
//! the state machine, settings flow, and event forwarding can be built
//! and tested with zero webview dependencies (keeps headless / musl /
//! router builds untouched — the GUI depends on `aether`, never the
//! reverse).
//!
//! Tauri integration (next step, needs network + webview SDKs):
//! each `pub async fn` in `commands` becomes a `#[tauri::command]`,
//! `AppState` becomes Tauri-managed state, and `drain_events` /
//! `drain_logs` become Tauri `emit("aether://event", …)` streams.

pub mod autostart;
pub mod commands;
pub mod mode_manager;
pub mod proxy;
pub mod state;
pub mod tun;

pub use state::{AppState, ConnectionPhase};
