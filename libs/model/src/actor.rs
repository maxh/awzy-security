//! Who did something: published a policy bundle, contained an accessor, answered a challenge
//! (ADR 0060). An entity, and the person it acted for when it acted for one, never a free-form
//! string, so a reader can tell a person from a service and look either up.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{entity_type, EntityRef};

/// Kinds of actor that aren't Cedar entity types: awzy itself and the vendor's staff.
pub mod actor_type {
    /// One of awzy's own components, by service name: `awzy-reasoner`, `awzy-bot`.
    pub const AWZY_SERVICE: &str = "AwzyService";
    /// A break-glass key (ADR 0021), by its fingerprint. Whoever held it isn't known.
    pub const BREAK_GLASS_KEY: &str = "BreakGlassKey";
    /// An awzy staff member in the corp portal, by user name. Never in a client's install.
    pub const AWZY_STAFF: &str = "AwzyStaff";
}

const KINDS: [&str; 5] = [
    entity_type::HUMAN,
    entity_type::SERVICE_ACCOUNT,
    actor_type::AWZY_SERVICE,
    actor_type::BREAK_GLASS_KEY,
    actor_type::AWZY_STAFF,
];

/// Who acted, and for whom: a person (`Human`), a service account or awzy service acting on its
/// own or for a person, a break-glass key, or a corp portal staff member.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Actor {
    pub subject: EntityRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_behalf_of: Option<EntityRef>,
}

impl Actor {
    pub fn new(subject: EntityRef, on_behalf_of: Option<EntityRef>) -> Self {
        Self { subject, on_behalf_of }
    }

    /// A person, by the ID the client's IdP gives them.
    pub fn human(id: impl Into<String>) -> Self {
        Self::new(EntityRef::new(entity_type::HUMAN, id), None)
    }

    /// One of awzy's own services acting on its own.
    pub fn awzy(service: impl Into<String>) -> Self {
        Self::new(EntityRef::new(actor_type::AWZY_SERVICE, service), None)
    }

    pub fn awzy_staff(user_name: impl Into<String>) -> Self {
        Self::new(EntityRef::new(actor_type::AWZY_STAFF, user_name), None)
    }

    /// The same actor acting for `person`.
    pub fn for_human(mut self, human: impl Into<String>) -> Self {
        self.on_behalf_of = Some(EntityRef::new(entity_type::HUMAN, human));
        self
    }

    /// Reads the stored form ([`fmt::Display`]), and the free-form strings written before ADR
    /// 0060: `alice`, `sales-genie for alice`, `Awzy Bot for alice`, `break-glass:<key>`, and
    /// service names like `awzy-synthetic`. Anything else is taken as a person's ID.
    pub fn parse(s: &str) -> Self {
        if let Some(actor) = Self::parse_typed(s) {
            return actor;
        }
        if let Some(key) = s.strip_prefix("break-glass:") {
            return Self::new(EntityRef::new(actor_type::BREAK_GLASS_KEY, key), None);
        }
        if let Some(human) = s.strip_prefix("Awzy Bot for ") {
            return Self::awzy("awzy-bot").for_human(human);
        }
        if let Some((account, human)) = s.split_once(" for ") {
            return Self::new(EntityRef::new(entity_type::SERVICE_ACCOUNT, account), None).for_human(human);
        }
        if s.starts_with("awzy-") || s == "unknown" {
            return Self::awzy(s);
        }
        Self::human(s)
    }

    /// For people to read, in messages: `alice`, or `sales-genie for alice`.
    pub fn label(&self) -> String {
        match &self.on_behalf_of {
            Some(p) => format!("{} for {}", self.subject.id, p.id),
            None => self.subject.id.clone(),
        }
    }

    fn parse_typed(s: &str) -> Option<Self> {
        let entity = |part: &str| {
            let (kind, id) = part.split_once("::")?;
            (KINDS.contains(&kind) && !id.is_empty()).then(|| EntityRef::new(kind, id))
        };
        // The person's half never contains " for " (a type and an ID), so the last one splits.
        match s.rsplit_once(" for ") {
            Some((subject, human)) if entity(human).is_some() => Some(Self::new(entity(subject)?, entity(human))),
            _ => Some(Self::new(entity(s)?, None)),
        }
    }
}

/// The stored form: `Human::alice`, or `ServiceAccount::sales-genie for Human::alice`.
impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.subject.kind, self.subject.id)?;
        if let Some(p) = &self.on_behalf_of {
            write!(f, " for {}::{}", p.kind, p.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_form_reads_back_as_the_same_actor() {
        for actor in [
            Actor::human("alice@larkspur.example"),
            Actor::awzy("awzy-reasoner"),
            Actor::awzy("awzy-bot").for_human("alice"),
            Actor::new(EntityRef::new(entity_type::SERVICE_ACCOUNT, "sales-genie"), None).for_human("bob"),
            Actor::new(EntityRef::new(actor_type::BREAK_GLASS_KEY, "sha256:ab12"), None),
            Actor::awzy_staff("max"),
        ] {
            assert_eq!(Actor::parse(&actor.to_string()), actor, "{actor}");
        }
    }

    #[test]
    fn strings_written_before_actors_were_typed_still_read() {
        assert_eq!(Actor::parse("alice"), Actor::human("alice"));
        assert_eq!(
            Actor::parse("sales-genie for alice"),
            Actor::new(EntityRef::new(entity_type::SERVICE_ACCOUNT, "sales-genie"), None).for_human("alice")
        );
        assert_eq!(Actor::parse("Awzy Bot for alice"), Actor::awzy("awzy-bot").for_human("alice"));
        assert_eq!(
            Actor::parse("break-glass:sha256:ab12").subject,
            EntityRef::new(actor_type::BREAK_GLASS_KEY, "sha256:ab12")
        );
        assert_eq!(Actor::parse("awzy-synthetic"), Actor::awzy("awzy-synthetic"));
        assert_eq!(Actor::parse("unknown"), Actor::awzy("unknown"));
    }

    #[test]
    fn an_unknown_type_is_not_mistaken_for_a_typed_actor() {
        assert_eq!(Actor::parse("Group::admins"), Actor::human("Group::admins"));
    }
}
