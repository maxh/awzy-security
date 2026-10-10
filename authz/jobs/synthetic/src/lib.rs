//! awzy-synthetic: Larkspur Capital, the synthetic company awzy's demo, local development,
//! scenario tests, and scale tests run on (docs/demo-plan.md, ADR 0039).
//!
//! Generates the company from a seed ([`org::generate`]) and sends it to awzy-context through
//! the same paths real data takes ([`push`]): people and groups over SCIM, as Okta or Entra ID
//! would, and AI service accounts through the service-account API. Then its resources and work
//! items ([`world`]) through the batch API a connector uses. Activity comes in a later step of the
//! demo plan.

pub mod activity;
pub mod drive;
pub mod driver;
pub mod history;
mod names;
pub mod org;
pub mod push;
mod rng;
pub mod scim;
pub mod world;
