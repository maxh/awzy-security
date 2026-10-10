//! Who did something, for both portals' GraphQL (ADR 0060).

use async_graphql::Object;

/// Who did something: a person, a service account or awzy service (perhaps acting for a person),
/// a break-glass key, or corp portal staff.
#[derive(Debug, Clone, PartialEq)]
pub struct Actor(pub awzy_model::Actor);

impl Actor {
    /// From the stored or wire text form; None when nobody has acted.
    pub fn from_text(text: &str) -> Option<Self> {
        (!text.is_empty()).then(|| Self(awzy_model::Actor::parse(text)))
    }
}

#[Object]
impl Actor {
    /// `Human`, `ServiceAccount`, `AwzyService`, `BreakGlassKey`, or `AwzyStaff`.
    #[graphql(name = "type")]
    async fn kind(&self) -> &str {
        &self.0.subject.kind
    }

    /// A person's IdP ID, a service account ID, an awzy service name, a key fingerprint, or a staff user name.
    async fn id(&self) -> &str {
        &self.0.subject.id
    }

    /// The person it acted for, when it acted for one.
    async fn on_behalf_of(&self) -> Option<Actor> {
        self.0.on_behalf_of.clone().map(|p| Actor(awzy_model::Actor::new(p, None)))
    }

    /// For people to read: `alice`, or `sales-genie for alice`.
    async fn label(&self) -> String {
        self.0.label()
    }
}
