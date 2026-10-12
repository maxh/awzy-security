//! `GET /mcp/challenges/{qid}?wait=<secs>`: an agent waits for the answer to a challenge one of its
//! tool calls opened (ADR 0111), signed in with the same token. The PDP answers only for the
//! principal the challenge was opened for, so an agent learns nothing about anyone else's.

use awzy_authz_proto::authz::v1::{ChallengeOutcome, WaitForChallengeRequest};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, Response, StatusCode},
};
use serde::Deserialize;
use serde_json::json;

use super::{json_response, plain, GatewayState};

/// The path an MCP tool error's `_meta` points an agent at.
pub(super) fn path(challenge_qid: &str) -> String {
    format!("/mcp/challenges/{challenge_qid}?wait=25")
}

#[derive(Deserialize)]
pub(super) struct WaitParams {
    #[serde(default)]
    wait: u32,
}

pub(super) async fn wait(
    State(state): State<GatewayState>,
    Path(challenge_qid): Path<String>,
    Query(params): Query<WaitParams>,
    headers: HeaderMap,
) -> Response<Body> {
    let authorization = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    // Registration doesn't matter here: an unregistered agent has no challenges to wait on.
    let principal = match state.0.verifier.principal(authorization, &awzy_identity::DeferToPdp).await {
        Ok(p) => p,
        Err(e) => return plain(StatusCode::UNAUTHORIZED, e.to_string()),
    };
    let req = WaitForChallengeRequest {
        challenge_qid,
        principal: Some(principal.subject.into()),
        on_behalf_of: principal.on_behalf_of.map(Into::into),
        wait_secs: params.wait.min(25),
    };
    let mut pdp = state.0.pdp.clone();
    // The call's own timeout is a check's; a wait needs as long as it asked for.
    let mut request = tonic::Request::new(req);
    request.set_timeout(std::time::Duration::from_secs(30));
    match pdp.wait_for_challenge(request).await {
        Ok(res) => {
            let res = res.into_inner();
            let time = |t: Option<awzy_authz_proto::prost_types::Timestamp>| {
                t.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
            };
            let outcome = res.outcome();
            json_response(json!({
                "outcome": outcome.as_str_name().trim_start_matches("CHALLENGE_OUTCOME_").to_lowercase(),
                "expires_at": time(res.expires_at),
                "grant_expires_at": time(res.grant_expires_at),
                "retry": outcome == ChallengeOutcome::Passed,
            }))
        }
        Err(s) if s.code() == tonic::Code::NotFound => plain(StatusCode::NOT_FOUND, s.message().to_string()),
        Err(s) if s.code() == tonic::Code::InvalidArgument => plain(StatusCode::BAD_REQUEST, s.message().to_string()),
        Err(s) => {
            plain(StatusCode::SERVICE_UNAVAILABLE, format!("awzy couldn't look up the challenge: {}", s.message()))
        }
    }
}
