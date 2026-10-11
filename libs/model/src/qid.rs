//! Typed IDs ("QIDs"): `<entity_type>_<uuid>`, e.g. `decision_0199a8f2-6c1e-7b3a-9d4e-2f8c1a5b7e90`.
//!
//! Each entity gets its own type, so IDs of different entities can't be mixed up. The QID is
//! the form an ID takes wherever it leaves a service: GraphQL, gRPC, Kafka records, logs.
//! Storage keeps the bare UUID in native `uuid` / `UUID` columns.

use std::{fmt, str::FromStr};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{value:?} is not a {prefix} ID; expected {prefix}_<uuid>")]
pub struct QidError {
    prefix: &'static str,
    value: String,
}

/// What every typed ID has in common, for code generic over them, like the portals' `Qid` scalar.
pub trait TypedQid: Copy + fmt::Display + FromStr<Err = QidError> + Send + Sync + 'static {
    /// The entity type its QIDs start with, e.g. `decision`.
    const PREFIX: &'static str;
}

macro_rules! qid {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
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
            const PREFIX: &'static str = $prefix;
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
                    .ok_or_else(|| QidError { prefix: $prefix, value: s.to_string() })
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
    };
}

qid!(
    /// One authorization decision, as returned to the enforcement point and logged.
    DecisionId,
    "decision"
);
qid!(
    /// A versioned Cedar policy bundle.
    PolicyBundleId,
    "policy_bundle"
);
qid!(
    /// One client install of awzy, as registered in the corp portal.
    InstallId,
    "install"
);
qid!(
    /// One event awzy-intake took in from an app or audit log.
    EventId,
    "event"
);
qid!(
    /// One challenge awzy-challenge opened for a request: what the principal must pass first.
    ChallengeId,
    "challenge"
);
qid!(
    /// One containment: a principal whose access awzy-challenge revoked until it's lifted.
    ContainmentId,
    "containment"
);
qid!(
    /// A person's security key (a WebAuthn credential) in awzy-challenge.
    SecurityKeyId,
    "security_key"
);
qid!(
    /// One WebAuthn ceremony: a challenge awzy-challenge issued to a browser, used once.
    CeremonyId,
    "ceremony"
);
qid!(
    /// One investigation awzy-reasoner ran: a model's checked finding about one principal.
    InvestigationId,
    "investigation"
);
qid!(
    /// One support session (ADR 0095): awzy staff asked for it in the corp portal, which minted
    /// the ID, and a client admin decides it in the client portal.
    SupportSessionId,
    "support_session"
);

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
        assert_eq!(err.to_string(), format!("{qid:?} is not a policy_bundle ID; expected policy_bundle_<uuid>"));
        assert!("decision_not-a-uuid".parse::<DecisionId>().is_err());
        assert!(id.uuid().to_string().parse::<DecisionId>().is_err(), "a bare UUID is not a QID");
    }

    #[test]
    fn serializes_as_the_qid_string() {
        let id = PolicyBundleId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        assert_eq!(serde_json::from_str::<PolicyBundleId>(&json).unwrap(), id);
    }
}
