//! Proofs about Cedar policy sets, with Cedar's symbolic compiler (SymCC) and the cvc5 solver
//! (#428): whether a policy set allows anything a property doesn't, and whether a forbid can
//! error. Each answer covers every request the schema allows, not a sample, and a "no" comes with
//! a request that shows it.
//!
//! A property is itself Cedar: one permit that allows everything the property does, such as
//! anything but a deactivated person's requests. A policy set keeps the property when it allows
//! nothing the property doesn't ([`Prover::allowed_outside`]).
//!
//! Cedar skips a policy that errors, so a forbid that errors fails open.
//! [`Prover::forbids_that_error`] finds them.

use cedar_policy::{Effect, Policy, PolicySet, Schema};
use cedar_policy_symcc::solver::LocalSolver;
use cedar_policy_symcc::{CedarSymCompiler, CompiledPolicy, CompiledPolicySet};
use std::path::Path;
use tokio::process::Command;

#[cfg(test)]
mod tests;

/// The annotation on a forbid that turns its deny into a challenge (see awzy-pdp).
pub const CHALLENGE_ANNOTATION: &str = "challenge";

// SymCC's errors are large, so they're boxed.
#[derive(Debug, thiserror::Error)]
pub enum ProofError {
    #[error("starting cvc5 at {path}: {source}")]
    Solver {
        path: String,
        #[source]
        source: Box<cedar_policy_symcc::err::Error>,
    },
    #[error("proving: {0}")]
    Symcc(Box<cedar_policy_symcc::err::Error>),
    #[error("setting aside the challenge policies: {0}")]
    PolicySet(Box<cedar_policy::PolicySetError>),
}

impl From<cedar_policy_symcc::err::Error> for ProofError {
    fn from(e: cedar_policy_symcc::err::Error) -> Self {
        Self::Symcc(Box::new(e))
    }
}

impl From<cedar_policy::PolicySetError> for ProofError {
    fn from(e: cedar_policy::PolicySetError) -> Self {
        Self::PolicySet(Box::new(e))
    }
}

/// A request, with its entities, that shows a property doesn't hold, as SymCC prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counterexample(pub String);

impl std::fmt::Display for Counterexample {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Answers questions about policy sets, with a cvc5 process of its own.
pub struct Prover {
    compiler: CedarSymCompiler<LocalSolver>,
}

impl Prover {
    /// Starts cvc5 from `cvc5`, its executable.
    pub fn new(cvc5: &Path) -> Result<Self, ProofError> {
        let solver_error = |source: cedar_policy_symcc::err::Error| ProofError::Solver {
            path: cvc5.display().to_string(),
            source: Box::new(source),
        };
        // SymCC's own default: a minute of solving per question, and SMT-LIB on stdin.
        let solver = LocalSolver::from_command(Command::new(cvc5).args(["--lang", "smt", "--tlimit=60000"]))
            .map_err(|e| solver_error(e.into()))?;
        let compiler = CedarSymCompiler::new(solver).map_err(solver_error)?;
        Ok(Self { compiler })
    }

    /// A request `policies` allows and `property` doesn't, if there is one.
    pub async fn allowed_outside(
        &mut self,
        schema: &Schema,
        policies: &PolicySet,
        property: &PolicySet,
    ) -> Result<Option<Counterexample>, ProofError> {
        for env in schema.request_envs() {
            let allowed = CompiledPolicySet::compile(policies, &env, schema)?;
            let promised = CompiledPolicySet::compile(property, &env, schema)?;
            if let Some(cex) = self.compiler.check_implies_with_counterexample_opt(&allowed, &promised).await? {
                return Ok(Some(Counterexample(cex.to_string())));
            }
        }
        Ok(None)
    }

    /// The forbids in `policies` that error on some request, by [`name`], each with one.
    pub async fn forbids_that_error(
        &mut self,
        schema: &Schema,
        policies: &PolicySet,
    ) -> Result<Vec<(String, Counterexample)>, ProofError> {
        let mut erring = Vec::new();
        for policy in policies.policies().filter(|p| p.effect() == Effect::Forbid) {
            for env in schema.request_envs() {
                let compiled = CompiledPolicy::compile(policy, &env, schema)?;
                if let Some(cex) = self.compiler.check_never_errors_with_counterexample_opt(&compiled).await? {
                    erring.push((name(policy), Counterexample(cex.to_string())));
                    break;
                }
            }
        }
        Ok(erring)
    }
}

/// A policy's name: its `@id` annotation, as awzy's policies and properties are named, or else
/// the ID Cedar gave it when parsing (`policy0`).
pub fn name(policy: &Policy) -> String {
    policy.annotation("id").map_or_else(|| policy.id().to_string(), str::to_string)
}

/// `policies` once every challenge is passed: without the `@challenge` forbids. What it allows,
/// a request can get to.
pub fn after_challenges(policies: &PolicySet) -> Result<PolicySet, ProofError> {
    let rest: Vec<Policy> =
        policies.policies().filter(|p| p.annotation(CHALLENGE_ANNOTATION).is_none()).cloned().collect();
    Ok(PolicySet::from_policies(rest)?)
}
