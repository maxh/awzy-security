//! `awzy.authz.v1.Authorizer` over gRPC.

use awzy_authz_proto::authz::v1::{
    authorizer_server::{Authorizer, AuthorizerServer},
    CheckRequest, CheckResponse, WaitForChallengeRequest, WaitForChallengeResponse,
};
use awzy_model::Check;
use axum::Router;
use tonic::{Request, Response, Status};

use crate::AppState;

struct Service(AppState);

#[tonic::async_trait]
impl Authorizer for Service {
    async fn check(&self, request: Request<CheckRequest>) -> Result<Response<CheckResponse>, Status> {
        let mut req = request.into_inner();
        let entities = awzy_authz_proto::entities_to_json(std::mem::take(&mut req.entities));
        let access = Check::try_from(req).map_err(|e| Status::invalid_argument(e.to_string()))?;
        let decision = self.0.check(&access, entities).await.map_err(|e| match e {
            crate::CheckError::NotReady | crate::CheckError::Unavailable(_) => Status::unavailable(e.to_string()),
            crate::CheckError::Invalid(_) => Status::invalid_argument(e.to_string()),
        })?;
        Ok(Response::new(decision.into()))
    }

    async fn wait_for_challenge(
        &self,
        request: Request<WaitForChallengeRequest>,
    ) -> Result<Response<WaitForChallengeResponse>, Status> {
        let req = request.into_inner();
        let id: awzy_model::ChallengeId =
            req.challenge_qid.parse().map_err(|e| Status::invalid_argument(format!("challenge_qid: {e}")))?;
        let principal: awzy_model::EntityRef =
            req.principal.ok_or_else(|| Status::invalid_argument("principal is required"))?.into();
        let on_behalf_of: Option<awzy_model::EntityRef> = req.on_behalf_of.map(Into::into);
        let wait = std::time::Duration::from_secs(req.wait_secs.into());
        let challenges = self.0.challenges.clone();
        crate::waiting::wait(&principal, on_behalf_of.as_ref(), wait, || challenges.get(id))
            .await
            .map(Response::new)
            .map_err(|e| match e {
                crate::waiting::WaitError::NotFound => Status::not_found(e.to_string()),
                crate::waiting::WaitError::NotConfigured => Status::failed_precondition(e.to_string()),
                crate::waiting::WaitError::Unavailable(_) => Status::unavailable(e.to_string()),
            })
    }
}

pub fn router(state: AppState) -> Router {
    tonic::service::Routes::new(AuthorizerServer::new(Service(state))).into_axum_router()
}

#[cfg(test)]
mod tests {
    use super::*;
    use awzy_authz_proto::authz::v1::{authorizer_client::AuthorizerClient, ChallengeKind, Entity, EntityRef, Verdict};
    use serde_json::json;

    fn r(kind: &str, id: &str) -> EntityRef {
        EntityRef { r#type: kind.into(), id: id.into() }
    }

    fn obj(v: serde_json::Value) -> Option<awzy_authz_proto::prost_types::Struct> {
        Some(awzy_authz_proto::json_to_struct(v.as_object().unwrap().clone()))
    }

    #[tokio::test]
    async fn check_over_grpc_on_the_shared_port() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::router(crate::AppState::new(crate::shared(crate::Pdp::with_defaults().unwrap())));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let mut client = AuthorizerClient::connect(format!("http://{addr}")).await.unwrap();
        let res = client
            .check(CheckRequest {
                principal: Some(r("ServiceAccount", "sales-genie")),
                action: "read".into(),
                resource: Some(r("Resource", "design-doc")),
                context: obj(json!({ "risk_score": 95 })),
                on_behalf_of: Some(r("Human", "bob")),
                human_present: Some(true),
                always_enforced: false,
                entities: vec![
                    Entity {
                        uid: Some(r("ServiceAccount", "sales-genie")),
                        attrs: obj(json!({ "owner": { "__entity": { "type": "Human", "id": "bob" } }, "ai": true })),
                        parents: vec![r("Group", "engineering")],
                    },
                    Entity { uid: Some(r("Human", "bob")), attrs: None, parents: vec![r("Group", "engineering")] },
                    Entity {
                        uid: Some(r("Resource", "design-doc")),
                        attrs: obj(json!({ "sensitivity": "internal" })),
                        parents: vec![],
                    },
                ],
            })
            .await
            .unwrap()
            .into_inner();

        assert_eq!(res.verdict(), Verdict::Challenge);
        assert_eq!(res.challenges().collect::<Vec<_>>(), vec![ChallengeKind::Verification]);
        assert_eq!(res.policies, vec!["high-risk-verify"]);
        assert_eq!(res.mode(), awzy_authz_proto::authz::v1::EnforcementMode::Monitor);
        let human = res.on_behalf_of.as_ref().expect("a delegated request reports the person's check");
        assert_eq!(human.verdict(), Verdict::Challenge);
        let id: awzy_model::DecisionId = res.decision_qid.parse().unwrap();
        assert_eq!(id.uuid().get_version_num(), 7);
    }
}
