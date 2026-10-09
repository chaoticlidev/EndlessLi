//! Python sidecar lifecycle (full implementation in Phase 5).
//!
//! Why this exists: Rust owns the sidecar (lazy start, health check, restart,
//! kill on exit). This Phase 0 stub defines the contract so later phases fill
//! in JSON-RPC over stdio without changing call sites.

use std::sync::atomic::{AtomicU32, Ordering};

/// Consecutive crash count before AI features are disabled (plan Section 7).
pub const MAX_RESTARTS: u32 = 3;

/// Lifecycle state of the Python sidecar process.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum SidecarStatus {
    /// Never started (lazy start on first request).
    #[default]
    Stopped,
    /// Process is running and passed the last health check.
    Running,
    /// Restarting after a crash.
    Restarting,
    /// Disabled after [`MAX_RESTARTS`] failures; diagnostics surfaced to UI.
    Disabled,
}

/// Minimal Phase 0 manager. Phase 5 adds process spawn, health checks,
/// restart backoff, and `sidecar://status` events.
#[derive(Debug, Default)]
pub struct SidecarManager {
    restarts: AtomicU32,
    status: std::sync::Mutex<SidecarStatus>,
}

impl SidecarManager {
    /// Creates a stopped manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Current lifecycle status.
    pub fn status(&self) -> SidecarStatus {
        *self.status.lock().expect("sidecar status lock")
    }

    /// Records a crash. Returns the new status after applying the restart cap.
    pub fn record_crash(&self) -> SidecarStatus {
        let restarts = self.restarts.fetch_add(1, Ordering::SeqCst) + 1;
        let next = if restarts >= MAX_RESTARTS {
            SidecarStatus::Disabled
        } else {
            SidecarStatus::Restarting
        };
        *self.status.lock().expect("sidecar status lock") = next;
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disables_after_max_restarts() {
        let manager = SidecarManager::new();
        assert_eq!(manager.status(), SidecarStatus::Stopped);
        assert_eq!(manager.record_crash(), SidecarStatus::Restarting);
        assert_eq!(manager.record_crash(), SidecarStatus::Restarting);
        assert_eq!(manager.record_crash(), SidecarStatus::Disabled);
    }
}
