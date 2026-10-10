//! Policy bundles, through awzy-policy.

use async_graphql::{Context, Enum, InputObject, Object, Result, SimpleObject, ID};
use awzy_authz_proto::{
    policy::v1::{
        get_bundle_request::Key, BundleStatus, CreateDraftRequest, GetActiveRequest, GetBundleRequest,
        ListBundlesRequest, PolicyBundle as WireBundle, PublishRequest,
    },
    prost_types::Timestamp,
};
use awzy_model::PolicyBundleId;
use awzy_portal::UtcTimestamp;
use chrono::DateTime;
use tonic::{transport::Channel, Code};

use crate::{
    access::{require, PortalAction::*, PortalArea},
    policy_client,
};

type PolicyClient = awzy_authz_proto::policy::v1::policy_service_client::PolicyServiceClient<Channel>;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub enum PolicyBundleStatus {
    Draft,
    Active,
    Superseded,
}

/// A Cedar schema plus policies. Drafts are validated; one bundle at a time is active.
pub struct PolicyBundle(pub WireBundle);

#[Object]
impl PolicyBundle {
    /// QID: policy_bundle_<uuid>.
    pub async fn id(&self) -> ID {
        ID(self.0.id.clone())
    }

    /// Increases with every bundle; decisions record the version they were made under.
    async fn version(&self) -> i64 {
        self.0.version
    }

    async fn status(&self) -> PolicyBundleStatus {
        match self.0.status() {
            BundleStatus::Active => PolicyBundleStatus::Active,
            BundleStatus::Superseded => PolicyBundleStatus::Superseded,
            BundleStatus::Draft | BundleStatus::Unspecified => PolicyBundleStatus::Draft,
        }
    }

    /// Cedar schema.
    async fn schema(&self) -> &str {
        &self.0.schema
    }

    /// Cedar policies.
    async fn policies(&self) -> &str {
        &self.0.policies
    }

    async fn created_by(&self) -> &str {
        &self.0.created_by
    }

    async fn created_at(&self) -> Option<UtcTimestamp> {
        self.0.created_at.as_ref().and_then(to_datetime)
    }

    async fn published_by(&self) -> Option<&str> {
        Some(self.0.published_by.as_str()).filter(|s| !s.is_empty())
    }

    async fn published_at(&self) -> Option<UtcTimestamp> {
        self.0.published_at.as_ref().and_then(to_datetime)
    }
}

fn to_datetime(t: &Timestamp) -> Option<UtcTimestamp> {
    DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32).map(UtcTimestamp)
}

pub async fn get_by_id(ctx: &Context<'_>, id: PolicyBundleId) -> Result<Option<PolicyBundle>> {
    require(ctx, View, PortalArea::Policies).await?;
    Ok(get(policy_client(ctx)?, id).await?.map(PolicyBundle))
}

// ---- Operations, shared by the GraphQL types below and the MCP tools (crate::mcp).

pub async fn get(mut policy: PolicyClient, id: PolicyBundleId) -> anyhow::Result<Option<WireBundle>> {
    match policy.get_bundle(GetBundleRequest { key: Some(Key::Id(id.to_string())) }).await {
        Ok(res) => Ok(res.into_inner().bundle),
        Err(s) if matches!(s.code(), Code::NotFound | Code::InvalidArgument) => Ok(None),
        Err(s) => Err(s.into()),
    }
}

/// The bundle the PDPs enforce, if one has been published.
pub async fn active(mut policy: PolicyClient) -> anyhow::Result<Option<WireBundle>> {
    Ok(policy.get_active(GetActiveRequest { known_version: 0 }).await?.into_inner().bundle)
}

/// Newest first.
pub async fn list(mut policy: PolicyClient, limit: i32) -> anyhow::Result<Vec<WireBundle>> {
    Ok(policy.list_bundles(ListBundlesRequest { limit }).await?.into_inner().bundles)
}

pub enum DraftOutcome {
    Created(Box<WireBundle>),
    /// Cedar parse or schema validation errors.
    Invalid(String),
}

/// Validates and saves a draft. Nothing is enforced until a human publishes it.
pub async fn create_draft(
    mut policy: PolicyClient,
    schema: String,
    policies: String,
    actor: &str,
) -> anyhow::Result<DraftOutcome> {
    let req = CreateDraftRequest { schema, policies, created_by: actor.into() };
    match policy.create_draft(req).await {
        Ok(res) => {
            let bundle = res.into_inner().bundle.ok_or_else(|| anyhow::anyhow!("awzy-policy returned no bundle"))?;
            Ok(DraftOutcome::Created(Box::new(bundle)))
        }
        Err(s) if s.code() == Code::InvalidArgument => Ok(DraftOutcome::Invalid(s.message().to_string())),
        Err(s) => Err(s.into()),
    }
}

pub async fn publish(mut policy: PolicyClient, id: PolicyBundleId, actor: &str) -> anyhow::Result<WireBundle> {
    let req = PublishRequest { id: id.to_string(), published_by: actor.into() };
    policy.publish(req).await?.into_inner().bundle.ok_or_else(|| anyhow::anyhow!("awzy-policy returned no bundle"))
}

// ---- GraphQL

#[derive(Default)]
pub struct PolicyQuery;

#[Object]
impl PolicyQuery {
    /// The bundle the PDPs enforce, if one has been published.
    async fn active_policy_bundle(&self, ctx: &Context<'_>) -> Result<Option<PolicyBundle>> {
        require(ctx, View, PortalArea::Policies).await?;
        Ok(active(policy_client(ctx)?).await?.map(PolicyBundle))
    }

    /// Newest first.
    async fn policy_bundles(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 50)] first: i32,
    ) -> Result<Vec<PolicyBundle>> {
        require(ctx, View, PortalArea::Policies).await?;
        Ok(list(policy_client(ctx)?, first).await?.into_iter().map(PolicyBundle).collect())
    }
}

#[derive(InputObject)]
pub struct CreatePolicyDraftInput {
    pub schema: String,
    pub policies: String,
}

#[derive(SimpleObject)]
pub struct CreatePolicyDraftPayload {
    /// The new draft, unless validation failed.
    pub bundle: Option<PolicyBundle>,
    /// Why the bundle was rejected: Cedar parse or schema validation errors.
    pub validation_error: Option<String>,
}

#[derive(InputObject)]
pub struct PublishPolicyBundleInput {
    /// A draft bundle's ID.
    pub id: ID,
}

#[derive(SimpleObject)]
pub struct PublishPolicyBundlePayload {
    pub bundle: PolicyBundle,
}

#[derive(Default)]
pub struct PolicyMutation;

#[Object]
impl PolicyMutation {
    /// Validates and saves a draft. Nothing is enforced until it is published.
    async fn create_policy_draft(
        &self,
        ctx: &Context<'_>,
        input: CreatePolicyDraftInput,
    ) -> Result<CreatePolicyDraftPayload> {
        let viewer = require(ctx, Administer, PortalArea::Policies).await?;
        Ok(match create_draft(policy_client(ctx)?, input.schema, input.policies, &viewer.actor()).await? {
            DraftOutcome::Created(b) => {
                CreatePolicyDraftPayload { bundle: Some(PolicyBundle(*b)), validation_error: None }
            }
            DraftOutcome::Invalid(msg) => CreatePolicyDraftPayload { bundle: None, validation_error: Some(msg) },
        })
    }

    /// Makes a draft the active bundle. PDPs pick it up within about 10 seconds.
    async fn publish_policy_bundle(
        &self,
        ctx: &Context<'_>,
        input: PublishPolicyBundleInput,
    ) -> Result<PublishPolicyBundlePayload> {
        let viewer = require(ctx, Administer, PortalArea::Policies).await?;
        let id: PolicyBundleId = input.id.parse()?;
        let bundle = publish(policy_client(ctx)?, id, &viewer.actor()).await?;
        Ok(PublishPolicyBundlePayload { bundle: PolicyBundle(bundle) })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use awzy_authz_proto::policy::v1::{
        policy_service_client::PolicyServiceClient,
        policy_service_server::{PolicyService, PolicyServiceServer},
        CreateDraftResponse, EnforcementSetting, GetActiveResponse, GetBundleResponse, GetEnforcementRequest,
        GetEnforcementResponse, ListBundlesResponse, ListEnforcementChangesRequest, ListEnforcementChangesResponse,
        PublishResponse, SetEnforcementRequest, SetEnforcementResponse,
    };
    use tonic::{Request, Response, Status};

    use super::*;
    use crate::{testing, Services};

    /// In-memory awzy-policy: validates with the real PDP loader, versions from 1, and keeps
    /// enforcement switches newest first.
    #[derive(Default, Clone)]
    pub(crate) struct FakePolicy(Arc<Mutex<Vec<WireBundle>>>, Arc<Mutex<Vec<EnforcementSetting>>>);

    #[tonic::async_trait]
    impl PolicyService for FakePolicy {
        async fn create_draft(
            &self,
            req: Request<CreateDraftRequest>,
        ) -> Result<Response<CreateDraftResponse>, Status> {
            let req = req.into_inner();
            awzy_pdp::validate(&req.schema, &req.policies).map_err(|e| Status::invalid_argument(e.to_string()))?;
            let mut all = self.0.lock().unwrap();
            let bundle = WireBundle {
                id: PolicyBundleId::new().to_string(),
                version: all.len() as i64 + 1,
                schema: req.schema,
                policies: req.policies,
                status: BundleStatus::Draft.into(),
                created_by: req.created_by,
                ..Default::default()
            };
            all.push(bundle.clone());
            Ok(Response::new(CreateDraftResponse { bundle: Some(bundle) }))
        }
        async fn publish(&self, req: Request<PublishRequest>) -> Result<Response<PublishResponse>, Status> {
            let req = req.into_inner();
            let mut all = self.0.lock().unwrap();
            for b in all.iter_mut().filter(|b| b.status() == BundleStatus::Active) {
                b.status = BundleStatus::Superseded.into();
            }
            let b = all.iter_mut().find(|b| b.id == req.id).ok_or_else(|| Status::not_found("no such bundle"))?;
            b.status = BundleStatus::Active.into();
            b.published_by = req.published_by;
            Ok(Response::new(PublishResponse { bundle: Some(b.clone()) }))
        }
        async fn get_active(&self, _: Request<GetActiveRequest>) -> Result<Response<GetActiveResponse>, Status> {
            let all = self.0.lock().unwrap();
            let bundle = all.iter().find(|b| b.status() == BundleStatus::Active).cloned();
            Ok(Response::new(GetActiveResponse { bundle, unchanged: false }))
        }
        async fn list_bundles(&self, _: Request<ListBundlesRequest>) -> Result<Response<ListBundlesResponse>, Status> {
            let bundles = self.0.lock().unwrap().iter().rev().cloned().collect();
            Ok(Response::new(ListBundlesResponse { bundles }))
        }
        async fn get_bundle(&self, req: Request<GetBundleRequest>) -> Result<Response<GetBundleResponse>, Status> {
            let all = self.0.lock().unwrap();
            let found = match req.into_inner().key {
                Some(Key::Id(id)) => all.iter().find(|b| b.id == id),
                Some(Key::Version(v)) => all.iter().find(|b| b.version == v),
                None => None,
            };
            let bundle = found.cloned().ok_or_else(|| Status::not_found("no such bundle"))?;
            Ok(Response::new(GetBundleResponse { bundle: Some(bundle) }))
        }
        async fn get_enforcement(
            &self,
            _: Request<GetEnforcementRequest>,
        ) -> Result<Response<GetEnforcementResponse>, Status> {
            let current = self.1.lock().unwrap().first().cloned().unwrap_or(EnforcementSetting {
                scope: "install".into(),
                mode: awzy_authz_proto::authz::v1::EnforcementMode::Monitor.into(),
                ..Default::default()
            });
            Ok(Response::new(GetEnforcementResponse { setting: Some(current) }))
        }
        async fn set_enforcement(
            &self,
            req: Request<SetEnforcementRequest>,
        ) -> Result<Response<SetEnforcementResponse>, Status> {
            let req = req.into_inner();
            let setting = EnforcementSetting {
                scope: "install".into(),
                mode: req.mode,
                changed_by: req.changed_by,
                changed_at: None,
            };
            self.1.lock().unwrap().insert(0, setting.clone());
            Ok(Response::new(SetEnforcementResponse { setting: Some(setting) }))
        }
        async fn break_glass_rollback(
            &self,
            _: Request<awzy_authz_proto::policy::v1::BreakGlassRollbackRequest>,
        ) -> Result<Response<awzy_authz_proto::policy::v1::BreakGlassRollbackResponse>, Status> {
            Err(Status::unimplemented("fake"))
        }
        async fn list_enforcement_changes(
            &self,
            _: Request<ListEnforcementChangesRequest>,
        ) -> Result<Response<ListEnforcementChangesResponse>, Status> {
            Ok(Response::new(ListEnforcementChangesResponse { changes: self.1.lock().unwrap().clone() }))
        }
    }

    pub(crate) async fn serve_fake_policy(fake: FakePolicy) -> PolicyServiceClient<tonic::transport::Channel> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = tonic::service::Routes::new(PolicyServiceServer::new(fake)).into_axum_router();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        PolicyServiceClient::connect(format!("http://{addr}")).await.unwrap()
    }

    #[tokio::test]
    async fn draft_publish_and_refetch_through_graphql() {
        let client = serve_fake_policy(FakePolicy::default()).await;
        let schema = crate::schema_with(Services {
            policy: Some(client),
            challenge: None,
            clickhouse: None,
            pdp: Some(testing::pdp().await),
            context: None,
            bot: None,
        });
        let alice = testing::person("alice", "security");
        let run = |q: String| {
            let (schema, alice) = (schema.clone(), alice.clone());
            async move { testing::run(&schema, &alice, q).await }
        };
        let gql = |s: &str| serde_json_string(s);

        let invalid = run(format!(
            "mutation {{ createPolicyDraft(input: {{ schema: {}, policies: {} }}) {{ bundle {{ id }} validationError }} }}",
            gql(awzy_pdp::DEFAULT_SCHEMA),
            gql(r#"permit (principal, action == Action::"delete", resource);"#)
        ))
        .await;
        assert!(invalid["createPolicyDraft"]["bundle"].is_null());
        assert!(invalid["createPolicyDraft"]["validationError"].as_str().unwrap().contains("delete"));

        let draft = run(format!(
            "mutation {{ createPolicyDraft(input: {{ schema: {}, policies: {} }}) {{ bundle {{ id status version }} }} }}",
            gql(awzy_pdp::DEFAULT_SCHEMA),
            gql(awzy_pdp::DEFAULT_POLICIES)
        ))
        .await;
        let id = draft["createPolicyDraft"]["bundle"]["id"].as_str().unwrap().to_string();
        assert_eq!(draft["createPolicyDraft"]["bundle"]["status"], "DRAFT");

        let published = run(format!(
            "mutation {{ publishPolicyBundle(input: {{ id: \"{id}\" }}) {{ bundle {{ status publishedBy }} }} }}"
        ))
        .await;
        assert_eq!(published["publishPolicyBundle"]["bundle"]["status"], "ACTIVE");

        let active = run("{ activePolicyBundle { id version } policyBundles { status } }".into()).await;
        assert_eq!(active["activePolicyBundle"]["id"], id.as_str());
        assert_eq!(active["policyBundles"][0]["status"], "ACTIVE");

        let refetched = run(format!("{{ node(id: \"{id}\") {{ ... on PolicyBundle {{ version }} }} }}")).await;
        assert_eq!(refetched["node"]["version"], 1, "the rejected draft was never stored");

        let mode = run("{ enforcement { mode changedBy } }".into()).await;
        assert_eq!(mode["enforcement"]["mode"], "MONITOR", "a new install starts in monitor");
        assert!(mode["enforcement"]["changedBy"].is_null());
        let switched =
            run("mutation { setEnforcementMode(input: { mode: ENFORCE }) { enforcement { mode changedBy } } }".into())
                .await;
        assert_eq!(switched["setEnforcementMode"]["enforcement"]["mode"], "ENFORCE");
        let history = run("{ enforcement { mode } enforcementHistory { mode changedBy } }".into()).await;
        assert_eq!(history["enforcement"]["mode"], "ENFORCE");
        assert_eq!(history["enforcementHistory"][0]["changedBy"], "alice");
        assert_eq!(published["publishPolicyBundle"]["bundle"]["publishedBy"], "alice");
    }

    /// Only the security group runs the portal (base.cedar), even in monitor mode.
    #[tokio::test]
    async fn the_portal_applies_pdp_verdicts_even_in_monitor_mode() {
        let client = serve_fake_policy(FakePolicy::default()).await;
        let schema = crate::schema_with(Services {
            policy: Some(client),
            challenge: None,
            clickhouse: None,
            pdp: Some(testing::pdp().await),
            context: None,
            bot: None,
        });
        let bob = testing::person("bob", "engineering");

        let err = testing::run_err(&schema, &bob, "{ policyBundles { id } }").await;
        assert!(err.starts_with("bob may not view policies (decision decision_"), "{err}");
        let err = testing::run_err(
            &schema,
            &bob,
            "mutation { setEnforcementMode(input: { mode: MONITOR }) { enforcement { mode } } }",
        )
        .await;
        assert!(err.starts_with("bob may not administer enforcement"), "{err}");

        let can = testing::run(&schema, &bob, "{ viewer { id kind } viewerCan(action: VIEW, area: DECISIONS) }").await;
        assert_eq!(can, serde_json::json!({ "viewer": { "id": "bob", "kind": "Human" }, "viewerCan": false }));
        let alice = testing::person("alice", "security");
        let can = testing::run(&schema, &alice, "{ viewerCan(action: ADMINISTER, area: ENFORCEMENT) }").await;
        assert_eq!(can["viewerCan"], true);

        let anonymous = schema.execute("{ policyBundles { id } }").await;
        assert_eq!(anonymous.errors[0].message, "sign in first");
        let no_pdp = crate::schema_with(Services {
            policy: None,
            clickhouse: None,
            pdp: None,
            context: None,
            bot: None,
            challenge: None,
        });
        let err = testing::run_err(&no_pdp, &alice, "{ policyBundles { id } }").await;
        assert!(err.contains("AWZY_PDP_ADDR"), "nothing is authorized without a PDP: {err}");
    }

    /// A GraphQL string literal.
    fn serde_json_string(s: &str) -> String {
        async_graphql::Value::String(s.to_string()).to_string()
    }
}
