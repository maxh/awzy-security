//! Any text, read as a stored actor: reading never panics, and what it reads, stored again,
//! reads back the same, for IDs awzy allows (no spaces: service accounts are checked for that,
//! and IdPs don't issue them). Otherwise an ID could make the audit log say someone acted for a
//! person they didn't.
#![no_main]

use awzy_model::Actor;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|s: &str| {
    let actor = Actor::parse(s);
    let ids = std::iter::once(&actor.subject).chain(&actor.on_behalf_of);
    if ids.clone().all(|e| !e.id.is_empty() && !e.id.contains(' ')) {
        assert_eq!(Actor::parse(&actor.to_string()), actor, "{s:?} stored as {actor}");
    }
});
