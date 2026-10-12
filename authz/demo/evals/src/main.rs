//! awzy-evals [--compromised]
//!
//! Runs the investigation evals (ADR 0067) against Claude on Bedrock (AWZY_LLM_MODEL,
//! AWZY_LLM_EFFORT, and the AWS region and credentials from the environment) and prints the
//! report as Markdown, for `docs/evals/`. `--compromised` runs them against the model that obeys
//! every injection instead, which needs no credentials. Exits non-zero if anything got past the
//! checks.

use awzy_evals::{cases, compromised::CompromisedModel, report};
use awzy_llm::CloudModel;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cases = cases::all();
    let (model, results) = if std::env::args().any(|a| a == "--compromised") {
        ("compromised".to_string(), report::run(&CompromisedModel, &cases).await?)
    } else {
        let model = CloudModel::from_env().await?;
        (model.model().to_string(), report::run(&model, &cases).await?)
    };
    println!("{}", report::markdown(&model, &results));
    let violations: usize = results.iter().map(|r| r.violations.len()).sum();
    anyhow::ensure!(violations == 0, "{violations} violations got past the checks");
    Ok(())
}
