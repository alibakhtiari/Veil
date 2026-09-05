//! GUI connection state machine (§01-§7).
//!
//! ```text
//! Idle → Provisioning → Scanning ⇄ Verifying → Connecting → Connected
//!    ↑                      │          │              │            │
//!    └──────── Stopped / Error ◄──────┴──────────────┴── Reconnecting
//! ```
//!
//! Extra edges: Scanning → Connecting (forced peer skips Verify),
//! Verifying → Scanning (rescan after a failed verify),
//! Stopped → Connecting (reconnect with the cached identity),
//! Idle → Connecting (connect() auto-provisions first, so a cold
//! start never fails with "no identity yet").
//!
//! The backend owns the transition; the frontend only renders
//! `phase()` + the event stream. Retry loops stay in the core
//! (`run_masque`/`run_wireguard`/`run_gool`); here we only surface
//! `Reconnecting { in_secs }` with a Stop button (a `Cancel` fire).

use std::sync::Arc;

use tokio::sync::RwLock;

/// Renderable phases (mirrors `ApiEvent` variants 1:1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionPhase {
    #[default]
    Idle,
    Provisioning,
    Scanning,
    Verifying,
    Connecting,
    Connected,
    Reconnecting,
    Stopped,
    Error,
}

impl ConnectionPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectionPhase::Idle => "idle",
            ConnectionPhase::Provisioning => "provisioning",
            ConnectionPhase::Scanning => "scanning",
            ConnectionPhase::Verifying => "verifying",
            ConnectionPhase::Connecting => "connecting",
            ConnectionPhase::Connected => "connected",
            ConnectionPhase::Reconnecting => "reconnecting",
            ConnectionPhase::Stopped => "stopped",
            ConnectionPhase::Error => "error",
        }
    }

    /// Allowed transitions; illegal jumps (e.g. Idle → Connected) are
    /// rejected so UI bugs surface as errors, not silent states.
    pub fn can_go(self, next: ConnectionPhase) -> bool {
        use ConnectionPhase::*;
        matches!(
            (self, next),
            (Idle, Provisioning)
                | (Idle, Connecting)
                | (Provisioning, Scanning)
                | (Provisioning, Stopped)
                | (Provisioning, Error)
                | (Scanning, Verifying)
                | (Scanning, Connecting)
                | (Scanning, Error)
                | (Scanning, Stopped)
                | (Stopped, Connecting)
                | (Verifying, Scanning)
                | (Verifying, Connecting)
                | (Verifying, Error)
                | (Verifying, Stopped)
                | (Connecting, Connected)
                | (Connecting, Error)
                | (Connecting, Stopped)
                | (Connected, Reconnecting)
                | (Connected, Stopped)
                | (Connected, Error)
                | (Reconnecting, Connecting)
                | (Reconnecting, Stopped)
                | (Reconnecting, Error)
                | (Error, Provisioning)
                | (Error, Idle)
                | (Stopped, Provisioning)
                | (Stopped, Idle)
        )
    }
}

/// Shared backend state: current phase + cancel token for the active
/// `api::connect` task + last error (redacted — never secrets).
#[derive(Debug, Default)]
pub struct BackendState {
    pub phase: ConnectionPhase,
    pub last_error: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Clone, Default)]
pub struct AppState {
    inner: Arc<RwLock<BackendState>>,
    cancel: Arc<RwLock<Option<aether::api::Cancel>>>,
    identity: Arc<RwLock<Option<aether::account::Identity>>>,
    gool: Arc<RwLock<Option<aether::api::GoolIdentities>>>,
    traffic_mode: Arc<RwLock<crate::mode_manager::TrafficMode>>,
    mode_manager: Arc<RwLock<crate::mode_manager::ModeManager>>,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn phase(&self) -> ConnectionPhase {
        self.inner.read().await.phase
    }

    /// Transition or return an error describing the illegal jump.
    pub async fn set_phase(&self, next: ConnectionPhase) -> Result<(), String> {
        let mut guard = self.inner.write().await;
        if guard.phase == next {
            return Ok(());
        }
        if !guard.phase.can_go(next) {
            return Err(format!(
                "illegal phase transition {} → {}",
                guard.phase.as_str(),
                next.as_str()
            ));
        }
        guard.phase = next;
        aether::events::emit(aether::events::ApiEvent::StateChanged {
            state: next.as_str().to_string(),
        });
        Ok(())
    }

    pub async fn set_error(&self, message: impl Into<String>) {
        let mut guard = self.inner.write().await;
        guard.phase = ConnectionPhase::Error;
        guard.last_error = Some(message.into());
        aether::events::emit(aether::events::ApiEvent::StateChanged {
            state: ConnectionPhase::Error.as_str().to_string(),
        });
        aether::events::emit(aether::events::ApiEvent::Error {
            message: guard.last_error.clone().unwrap_or_default(),
        });
    }

    pub async fn set_gateway(&self, gateway: Option<String>) {
        self.inner.write().await.gateway = gateway;
    }

    pub async fn snapshot(&self) -> (ConnectionPhase, Option<String>, Option<String>) {
        let guard = self.inner.read().await;
        (guard.phase, guard.gateway.clone(), guard.last_error.clone())
    }

    pub async fn store_cancel(&self, cancel: aether::api::Cancel) {
        *self.cancel.write().await = Some(cancel);
    }

    pub async fn take_cancel(&self) -> Option<aether::api::Cancel> {
        self.cancel.write().await.take()
    }

    pub async fn cancel_active(&self) {
        if let Some(cancel) = self.cancel.read().await.as_ref() {
            cancel.cancel();
        }
    }

    pub async fn store_identity(&self, identity: aether::account::Identity) {
        *self.identity.write().await = Some(identity);
    }

    pub async fn identity(&self) -> Option<aether::account::Identity> {
        self.identity.read().await.clone()
    }

    pub async fn clear_identity(&self) {
        *self.identity.write().await = None;
    }

    pub async fn store_gool(&self, ids: aether::api::GoolIdentities) {
        *self.gool.write().await = Some(ids);
    }

    pub async fn gool(&self) -> Option<aether::api::GoolIdentities> {
        self.gool.read().await.clone()
    }

    pub async fn clear_gool(&self) {
        *self.gool.write().await = None;
    }

    /// Stored traffic-mode preference (ProxyOnly default). This is the
    /// setting, not the applied state: side effects run through
    /// `ModeManager::on_connected` when a tunnel comes up.
    pub async fn traffic_mode(&self) -> crate::mode_manager::TrafficMode {
        *self.traffic_mode.read().await
    }

    pub async fn set_traffic_mode(&self, mode: crate::mode_manager::TrafficMode) {
        *self.traffic_mode.write().await = mode;
    }

    /// Arm the stored traffic mode now that the tunnel is up: applies the
    /// OS side effects (system proxy / TUN) for the preference. Called by
    /// the Tauri layer once readiness is known (see
    /// `aether::api::wait_for_socks`); a no-op for ProxyOnly.
    pub async fn on_tunnel_up(&self, socks: std::net::SocketAddr) -> Result<(), String> {
        let mode = self.traffic_mode().await;
        self.mode_manager
            .write()
            .await
            .on_connected(mode, socks)
            .await
    }

    /// Disarm whatever `on_tunnel_up` armed (system proxy / TUN).
    /// Idempotent; safe to call on a fresh state.
    pub async fn mode_cleanup(&self) {
        self.mode_manager.write().await.cleanup().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn happy_path_transitions_are_legal() {
        let app = AppState::new();
        for next in [
            ConnectionPhase::Provisioning,
            ConnectionPhase::Scanning,
            ConnectionPhase::Verifying,
            ConnectionPhase::Connecting,
            ConnectionPhase::Connected,
            ConnectionPhase::Stopped,
        ] {
            app.set_phase(next).await.expect("legal transition");
        }
        assert_eq!(app.phase().await, ConnectionPhase::Stopped);
    }

    #[tokio::test]
    async fn skipping_phases_is_rejected() {
        let app = AppState::new();
        let err = app
            .set_phase(ConnectionPhase::Connected)
            .await
            .expect_err("Idle → Connected must fail");
        assert!(err.contains("illegal phase transition"));
    }

    #[tokio::test]
    async fn forced_peer_may_skip_verify() {
        let app = AppState::new();
        app.set_phase(ConnectionPhase::Provisioning).await.unwrap();
        app.set_phase(ConnectionPhase::Scanning).await.unwrap();
        // No verify step: peer was forced.
        app.set_phase(ConnectionPhase::Connecting).await.unwrap();
        assert_eq!(app.phase().await, ConnectionPhase::Connecting);
    }

    #[tokio::test]
    async fn failed_verify_can_rescan() {
        let app = AppState::new();
        app.set_phase(ConnectionPhase::Provisioning).await.unwrap();
        app.set_phase(ConnectionPhase::Scanning).await.unwrap();
        app.set_phase(ConnectionPhase::Verifying).await.unwrap();
        // Gateway failed validation: back to Scanning, no reconnect loop.
        app.set_phase(ConnectionPhase::Scanning).await.unwrap();
        assert_eq!(app.phase().await, ConnectionPhase::Scanning);
    }

    #[tokio::test]
    async fn stopped_can_reconnect_without_reprovision() {
        let app = AppState::new();
        app.set_phase(ConnectionPhase::Provisioning).await.unwrap();
        app.set_phase(ConnectionPhase::Scanning).await.unwrap();
        app.set_phase(ConnectionPhase::Connecting).await.unwrap();
        app.set_phase(ConnectionPhase::Stopped).await.unwrap();
        // Cached identity: straight back to Connecting.
        app.set_phase(ConnectionPhase::Connecting).await.unwrap();
        assert_eq!(app.phase().await, ConnectionPhase::Connecting);
    }

    #[tokio::test]
    async fn set_error_records_message_and_phase() {
        let app = AppState::new();
        app.set_phase(ConnectionPhase::Provisioning).await.unwrap();
        app.set_error("boom").await;
        assert_eq!(app.phase().await, ConnectionPhase::Error);
        let (_, _, err) = app.snapshot().await;
        assert_eq!(err.as_deref(), Some("boom"));
    }
}
