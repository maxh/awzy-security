//! The cache's recent drops, so a fetch that raced a change isn't cached after it (ADR 0126).

use std::collections::{HashSet, VecDeque};

/// Drops remembered for fetches in flight. A fetch that began before the oldest isn't cached.
pub(super) const DROPS_KEPT: usize = 256;

/// Recent drops from the cache, for telling whether a fetch in flight saw what they dropped.
#[derive(Default)]
pub(super) struct Drops {
    /// The number the next drop gets.
    pub(super) next: u64,
    /// The last [`DROPS_KEPT`], oldest first, each with its number: the IDs, or None for all.
    recent: VecDeque<(u64, Option<HashSet<String>>)>,
}

impl Drops {
    pub(super) fn record(&mut self, ids: Option<HashSet<String>>) {
        self.recent.push_back((self.next, ids));
        self.next += 1;
        if self.recent.len() > DROPS_KEPT {
            self.recent.pop_front();
        }
    }

    /// Whether a drop numbered `since` or later touches any of `ids`, or might have: one that
    /// dropped everything, or one no longer remembered.
    pub(super) fn touched_since(&self, since: u64, ids: &[&str]) -> bool {
        if since >= self.next {
            return false;
        }
        if self.recent.front().is_none_or(|(n, _)| *n > since) {
            return true;
        }
        self.recent
            .iter()
            .filter(|(n, _)| *n >= since)
            .any(|(_, dropped)| dropped.as_ref().is_none_or(|d| ids.iter().any(|i| d.contains(*i))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_touches_the_fetches_in_flight_for_what_it_dropped() {
        let mut drops = Drops::default();
        drops.record(Some(HashSet::from(["synthetic:folder:board".to_string()])));
        let chain = ["synthetic:doc:deck", "synthetic:folder:board"];
        assert!(drops.touched_since(0, &chain), "the folder above the document changed");
        assert!(!drops.touched_since(1, &chain), "a fetch that began after it");
        assert!(!drops.touched_since(0, &["synthetic:doc:other"]));
        drops.record(None);
        assert!(drops.touched_since(1, &["synthetic:doc:other"]), "a reset touches everything");
        for _ in 0..DROPS_KEPT {
            drops.record(Some(HashSet::new()));
        }
        assert!(drops.touched_since(0, &["synthetic:doc:other"]), "one no longer remembered might have");
    }
}
