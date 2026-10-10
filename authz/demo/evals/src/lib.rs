//! awzy-evals: the investigation evals (ADR 0067). Cases come from Larkspur, the synthetic
//! company, which knows which activity is an attack ([`cases`]). [`compromised::CompromisedModel`]
//! obeys every injection, and CI runs every case with it to show the checks hold whatever the
//! model does; the `awzy-evals` binary runs the same cases against Bedrock and reports how the
//! real model did ([`report`]).

pub mod cases;
pub mod compromised;
pub mod report;
