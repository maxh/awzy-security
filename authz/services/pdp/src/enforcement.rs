//! The enforcement mode this PDP stamps on decisions (ADR 0007), and whether it has synced it.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

use awzy_model::EnforcementMode;

/// Shared between request handlers and the sync loop.
#[derive(Debug)]
pub struct Enforcement {
    enforce: AtomicBool,
    synced: AtomicBool,
    /// When the mode in force was set, in Unix seconds; `i64::MIN` before any signed change.
    changed_at: AtomicI64,
}

impl Enforcement {
    /// A fixed mode, ready at once: for a PDP with no awzy-policy to sync from.
    pub fn fixed(mode: EnforcementMode) -> Self {
        Self {
            enforce: AtomicBool::new(mode == EnforcementMode::Enforce),
            synced: AtomicBool::new(true),
            changed_at: AtomicI64::new(i64::MIN),
        }
    }

    /// Not ready until the first successful sync. Until then checks are refused, so a PDP that
    /// can't reach awzy-policy never guesses a mode: enforcement points use their fallback.
    pub fn awaiting_sync() -> Self {
        Self { enforce: AtomicBool::new(false), synced: AtomicBool::new(false), changed_at: AtomicI64::new(i64::MIN) }
    }

    pub fn mode(&self) -> EnforcementMode {
        if self.enforce.load(Ordering::Acquire) {
            EnforcementMode::Enforce
        } else {
            EnforcementMode::Monitor
        }
    }

    /// Records the mode from awzy-policy and marks the PDP ready. Returns true if it changed.
    pub fn synced(&self, mode: EnforcementMode) -> bool {
        let previous = self.enforce.swap(mode == EnforcementMode::Enforce, Ordering::AcqRel);
        let first = !self.synced.swap(true, Ordering::AcqRel);
        first || previous != (mode == EnforcementMode::Enforce)
    }

    /// When the mode in force was set (Unix seconds), if it came from a verified change.
    pub(crate) fn changed_at(&self) -> Option<i64> {
        Some(self.changed_at.load(Ordering::Acquire)).filter(|t| *t != i64::MIN)
    }

    /// Records when the verified mode now in force was set, so an older change offered later is
    /// refused (ADR 0080).
    pub(crate) fn set_changed_at(&self, unix: i64) {
        self.changed_at.store(unix, Ordering::Release);
    }

    pub fn is_ready(&self) -> bool {
        self.synced.load(Ordering::Acquire)
    }
}
