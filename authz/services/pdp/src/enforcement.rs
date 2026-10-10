//! The enforcement mode this PDP stamps on decisions (ADR 0007), and whether it has synced it.

use std::sync::atomic::{AtomicBool, Ordering};

use awzy_model::EnforcementMode;

/// Shared between request handlers and the sync loop.
#[derive(Debug)]
pub struct Enforcement {
    enforce: AtomicBool,
    synced: AtomicBool,
}

impl Enforcement {
    /// A fixed mode, ready at once: for a PDP with no awzy-policy to sync from.
    pub fn fixed(mode: EnforcementMode) -> Self {
        Self { enforce: AtomicBool::new(mode == EnforcementMode::Enforce), synced: AtomicBool::new(true) }
    }

    /// Not ready until the first successful sync. Until then checks are refused, so a PDP that
    /// can't reach awzy-policy never guesses a mode: enforcement points use their fallback.
    pub fn awaiting_sync() -> Self {
        Self { enforce: AtomicBool::new(false), synced: AtomicBool::new(false) }
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

    pub fn is_ready(&self) -> bool {
        self.synced.load(Ordering::Acquire)
    }
}
