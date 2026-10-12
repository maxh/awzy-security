//! The enforcement mode this PDP stamps on decisions (ADR 0007), and whether it has synced it:
//! the install's, and any an action or a tool has of its own (ADR 0109).

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        PoisonError, RwLock,
    },
};

use awzy_model::{EnforcementMode, EntityRef};

/// An action's or a tool's mode, and when it was set (Unix seconds; `i64::MIN` when unsigned).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopedMode {
    pub mode: EnforcementMode,
    pub changed_at: i64,
}

/// Shared between request handlers and the sync loop.
#[derive(Debug)]
pub struct Enforcement {
    enforce: AtomicBool,
    synced: AtomicBool,
    /// When the mode in force was set, in Unix seconds; `i64::MIN` before any signed change.
    changed_at: AtomicI64,
    /// Modes of actions and tools that have been switched, by scope (`action:write`).
    scoped: RwLock<HashMap<String, ScopedMode>>,
}

impl Enforcement {
    /// A fixed mode, ready at once: for a PDP with no awzy-policy to sync from.
    pub fn fixed(mode: EnforcementMode) -> Self {
        Self {
            enforce: AtomicBool::new(mode == EnforcementMode::Enforce),
            synced: AtomicBool::new(true),
            changed_at: AtomicI64::new(i64::MIN),
            scoped: RwLock::default(),
        }
    }

    /// Not ready until the first successful sync. Until then checks are refused, so a PDP that
    /// can't reach awzy-policy never guesses a mode: enforcement points use their fallback.
    pub fn awaiting_sync() -> Self {
        Self {
            enforce: AtomicBool::new(false),
            synced: AtomicBool::new(false),
            changed_at: AtomicI64::new(i64::MIN),
            scoped: RwLock::default(),
        }
    }

    pub fn mode(&self) -> EnforcementMode {
        if self.enforce.load(Ordering::Acquire) {
            EnforcementMode::Enforce
        } else {
            EnforcementMode::Monitor
        }
    }

    /// The mode for a check of `action` on `resource`: its tool's, then its action's, if either was
    /// switched, else the install's (ADR 0109).
    pub fn mode_for(&self, action: &str, resource: &EntityRef) -> EnforcementMode {
        let scoped = self.scoped.read().unwrap_or_else(PoisonError::into_inner);
        if !scoped.is_empty() {
            for scope in awzy_model::mode_scopes(action, resource) {
                if let Some(s) = scoped.get(&scope) {
                    return s.mode;
                }
            }
        }
        self.mode()
    }

    /// The scoped modes in force, for checking that a newer one is no older (ADR 0080).
    pub(crate) fn scoped(&self) -> HashMap<String, ScopedMode> {
        self.scoped.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Replaces the scoped modes with awzy-policy's. Returns the scopes whose mode changed.
    pub fn set_scoped(&self, next: HashMap<String, ScopedMode>) -> Vec<String> {
        let mut scoped = self.scoped.write().unwrap_or_else(PoisonError::into_inner);
        let mut changed: Vec<String> = next
            .iter()
            .filter(|(scope, s)| scoped.get(*scope).map(|p| p.mode) != Some(s.mode))
            .map(|(scope, _)| scope.clone())
            .collect();
        changed.extend(scoped.keys().filter(|scope| !next.contains_key(*scope)).cloned());
        *scoped = next;
        changed.sort();
        changed
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scoped(entries: &[(&str, EnforcementMode)]) -> HashMap<String, ScopedMode> {
        entries.iter().map(|(s, m)| (s.to_string(), ScopedMode { mode: *m, changed_at: 1 })).collect()
    }

    /// Writes enforced while reads stay monitored, and one tool apart from its action (ADR 0109).
    #[test]
    fn a_check_takes_its_tools_mode_then_its_actions_then_the_installs() {
        let e = Enforcement::fixed(EnforcementMode::Monitor);
        let doc = EntityRef::new("Resource", "doc");
        let tool = EntityRef::new("McpTool", "drive/delete_file");
        assert_eq!(e.mode_for("write", &doc), EnforcementMode::Monitor, "nothing switched: the install's");

        let changed = e.set_scoped(scoped(&[
            ("action:write", EnforcementMode::Enforce),
            ("action:call", EnforcementMode::Monitor),
            ("tool:drive/delete_file", EnforcementMode::Enforce),
        ]));
        assert_eq!(changed, ["action:call", "action:write", "tool:drive/delete_file"]);
        assert_eq!(e.mode_for("write", &doc), EnforcementMode::Enforce);
        assert_eq!(e.mode_for("read", &doc), EnforcementMode::Monitor);
        assert_eq!(e.mode_for("call", &tool), EnforcementMode::Enforce, "the tool's beats its action's");
        assert_eq!(e.mode_for("call", &EntityRef::new("McpTool", "drive/search")), EnforcementMode::Monitor);

        assert_eq!(
            e.set_scoped(scoped(&[("action:write", EnforcementMode::Enforce)])),
            ["action:call", "tool:drive/delete_file"]
        );
    }

    /// A scope switched to monitor holds while the install enforces.
    #[test]
    fn a_monitored_scope_stays_monitored_under_an_enforcing_install() {
        let e = Enforcement::fixed(EnforcementMode::Enforce);
        e.set_scoped(scoped(&[("action:read", EnforcementMode::Monitor)]));
        let doc = EntityRef::new("Resource", "doc");
        assert_eq!(
            (e.mode_for("read", &doc), e.mode_for("write", &doc)),
            (EnforcementMode::Monitor, EnforcementMode::Enforce)
        );
    }
}
