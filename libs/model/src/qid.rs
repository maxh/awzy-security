//! Typed IDs ("QIDs"): `<entity_type>_<uuid>`, e.g. `decision_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90`.
//!
//! Each entity gets its own type, so IDs of different entities can't be mixed up. The QID is
//! the form an ID takes wherever it leaves a service: GraphQL, gRPC, Kafka records, logs.
//! Storage keeps the bare UUID in native `uuid` / `UUID` columns.

use std::{fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// A string that isn't a QID of the entity expected. Says which entity's it is, when it's
/// another of awzy's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QidError {
    expected: QidEntity,
    value: String,
}

impl fmt::Display for QidError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (value, expected) = (&self.value, self.expected);
        match QidEntity::of(value) {
            Some(other) => write!(f, "{value:?} is a {other} ID, not a {expected} ID"),
            None => write!(f, "{value:?} is not a {expected} ID; expected {expected}_<uuid>"),
        }
    }
}

impl std::error::Error for QidError {}

impl QidError {
    /// The entity the QID should have been of.
    pub fn expected(&self) -> QidEntity {
        self.expected
    }
}

/// What every typed ID has in common, for code generic over them, like the portals' `Qid` scalar.
pub trait TypedQid: Copy + fmt::Display + FromStr<Err = QidError> + Send + Sync + 'static {
    /// The entity its QIDs are of.
    const ENTITY: QidEntity;
}

macro_rules! qids {
    ($($(#[$doc:meta])* $name:ident, $variant:ident, $prefix:literal;)*) => {
        /// Every kind of entity awzy mints QIDs for, to tell at run time which one a QID is of
        /// (`node(id)` routes on it). Not Cedar's entity types, which are `entity_type`.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum QidEntity {
            $($variant),*
        }

        impl QidEntity {
            pub const ALL: &'static [QidEntity] = &[$(QidEntity::$variant),*];

            /// What its QIDs start with, e.g. `policy_bundle`.
            pub fn prefix(self) -> &'static str {
                match self {
                    $(QidEntity::$variant => $prefix),*
                }
            }
        }

        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
            pub struct $name(Uuid);

            impl $name {
                pub const PREFIX: &'static str = $prefix;

                /// A new time-ordered (UUIDv7) ID.
                #[expect(clippy::new_without_default, reason = "a fresh ID each call isn't a default value")]
                pub fn new() -> Self {
                    Self(Uuid::now_v7())
                }

                pub fn from_uuid(uuid: Uuid) -> Self {
                    Self(uuid)
                }

                pub fn uuid(&self) -> Uuid {
                    self.0
                }
            }

            impl TypedQid for $name {
                const ENTITY: QidEntity = QidEntity::$variant;
            }

            impl fmt::Display for $name {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    write!(f, "{}_{}", $prefix, self.0)
                }
            }

            impl FromStr for $name {
                type Err = QidError;

                fn from_str(s: &str) -> Result<Self, QidError> {
                    s.strip_prefix(concat!($prefix, "_"))
                        .and_then(|rest| Uuid::parse_str(rest).ok())
                        .map(Self)
                        .ok_or_else(|| QidError { expected: QidEntity::$variant, value: s.to_string() })
                }
            }

            impl Serialize for $name {
                fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                    s.collect_str(self)
                }
            }

            impl<'de> Deserialize<'de> for $name {
                fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                    String::deserialize(d)?.parse().map_err(de::Error::custom)
                }
            }
        )*
    };
}

impl QidEntity {
    /// Which entity `qid` is of, if it's one of awzy's: `challenge_<uuid>` is `Challenge`.
    pub fn of(qid: &str) -> Option<QidEntity> {
        // A UUID has hyphens, never underscores, so the prefix is everything before the last one.
        let (prefix, uuid) = qid.rsplit_once('_')?;
        Uuid::parse_str(uuid).ok()?;
        Self::ALL.iter().copied().find(|e| e.prefix() == prefix)
    }
}

impl fmt::Display for QidEntity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.prefix())
    }
}

qids! {
    /// One authorization decision, as returned to the enforcement point and logged.
    DecisionId, Decision, "decision";
    /// A versioned Cedar policy bundle.
    PolicyBundleId, PolicyBundle, "policy_bundle";
    /// One client install of awzy, as registered in the corp portal.
    InstallId, Install, "install";
    /// One event awzy-intake took in from an app or audit log.
    EventId, Event, "event";
    /// One challenge awzy-challenge opened for a request: what the principal must pass first.
    ChallengeId, Challenge, "challenge";
    /// One containment: a principal whose access awzy-challenge revoked until it's lifted.
    ContainmentId, Containment, "containment";
    /// A person's security key (a WebAuthn credential) in awzy-challenge.
    SecurityKeyId, SecurityKey, "security_key";
    /// One WebAuthn ceremony: a challenge awzy-challenge issued to a browser, used once.
    CeremonyId, Ceremony, "ceremony";
    /// One investigation awzy-reasoner ran: a model's checked finding about one principal.
    InvestigationId, Investigation, "investigation";
    /// One support session (ADR 0095): awzy staff asked for it in the corp portal, which minted
    /// the ID, and a client admin decides it in the client portal.
    SupportSessionId, SupportSession, "support_session";
    /// One query plan (ADR 0113): which resources of a type a principal may act on, as a filter.
    PlanId, Plan, "plan";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_other_entities() {
        let id = DecisionId::new();
        let qid = id.to_string();
        assert!(qid.starts_with("decision_"));
        assert_eq!(qid.parse::<DecisionId>().unwrap(), id);
        assert_eq!(id.uuid().get_version_num(), 7);

        let err = qid.parse::<PolicyBundleId>().unwrap_err();
        assert_eq!(err.to_string(), format!("{qid:?} is a decision ID, not a policy_bundle ID"));
        assert_eq!(err.expected(), QidEntity::PolicyBundle);
        let err = "decision_not-a-uuid".parse::<PolicyBundleId>().unwrap_err();
        assert_eq!(err.to_string(), "\"decision_not-a-uuid\" is not a policy_bundle ID; expected policy_bundle_<uuid>");
        assert!("decision_not-a-uuid".parse::<DecisionId>().is_err());
        assert!(id.uuid().to_string().parse::<DecisionId>().is_err(), "a bare UUID is not a QID");
    }

    #[test]
    fn a_qid_says_which_entity_it_is_of() {
        assert_eq!(QidEntity::of(&PolicyBundleId::new().to_string()), Some(QidEntity::PolicyBundle));
        assert_eq!(QidEntity::of(&SupportSessionId::new().to_string()), Some(QidEntity::SupportSession));
        assert_eq!(QidEntity::of("widget_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90"), None, "not an entity awzy has");
        assert_eq!(QidEntity::of("decision_nope"), None);
        let prefixes: std::collections::BTreeSet<_> = QidEntity::ALL.iter().map(|e| e.prefix()).collect();
        assert_eq!(prefixes.len(), QidEntity::ALL.len(), "every entity has its own prefix");
    }

    #[test]
    fn serializes_as_the_qid_string() {
        let id = PolicyBundleId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<PolicyBundleId>(&json).unwrap(), id);
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        fn any_uuid() -> impl Strategy<Value = Uuid> {
            any::<u128>().prop_map(Uuid::from_u128)
        }

        proptest! {
            /// Every entity's QIDs say which entity they're of, so no prefix can be mistaken for
            /// another's (`policy` would collide with `policy_bundle`).
            #[test]
            fn every_qid_names_its_own_entity(i in 0..QidEntity::ALL.len(), uuid in any_uuid()) {
                let entity = QidEntity::ALL[i];
                prop_assert_eq!(QidEntity::of(&format!("{entity}_{uuid}")), Some(entity));
            }

            #[test]
            fn a_typed_qid_round_trips(uuid in any_uuid()) {
                let id = SecurityKeyId::from_uuid(uuid);
                prop_assert_eq!(id.to_string().parse::<SecurityKeyId>().ok(), Some(id));
                prop_assert!(id.to_string().parse::<ChallengeId>().is_err());
            }

            /// Whatever arrives, parsing answers rather than panicking, and what it accepts
            /// prints as a QID that parses back to the same ID.
            #[test]
            fn parsing_anything_is_total(s in ".*") {
                if let Ok(id) = s.parse::<DecisionId>() {
                    prop_assert_eq!(id.to_string().parse::<DecisionId>().ok(), Some(id));
                }
                let _ = QidEntity::of(&s);
            }
        }
    }
}
