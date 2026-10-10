//! awzy-client-portal: GraphQL API behind the client portal. Runs in each client cluster.
//!
//! Serves the portal's GraphQL API at `/graphql`, an MCP server for the client's AI agents at
//! `/mcp`, and (in the image) the built frontend. Policies come from awzy-policy over gRPC;
//! decisions come from ClickHouse. The GraphQL schema follows the Relay server specification
//! (global IDs, `node`, cursor connections).
//!
//! Both need a signed-in viewer, and every operation is checked with awzy-pdp (see [`access`]).

pub mod access;
pub mod awzy_bot;
mod challenges;
mod decisions;
pub mod directory;
mod dry_run;
mod enforcement;
mod investigations;
mod mcp;
mod node;
mod policies;
mod security_keys;

use access::{Auth, PortalAction, PortalArea, Surface, Viewer};
use async_graphql::{
    http::GraphiQLSource, Context, EmptySubscription, MergedObject, Object, Result, Schema, SimpleObject,
};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use awzy_authz_proto::{
    authz::v1::authorizer_client::AuthorizerClient, policy::v1::policy_service_client::PolicyServiceClient,
};
use awzy_service_auth::{grpc_channel, AuthChannel};
use axum::{
    extract::State,
    middleware,
    response::Html,
    routing::{get, post},
    Extension, Router,
};
use tonic::transport::Channel;

pub const API_VERSION: &str = "0.1.0";

pub(crate) type PolicyClient = PolicyServiceClient<AuthChannel>;

/// Backends the resolvers call. Any may be absent in development; resolvers that need a
/// missing one return an error naming it. Without the PDP, nothing is authorized.
#[derive(Clone, Default)]
pub struct Services {
    pub policy: Option<PolicyServiceClient<AuthChannel>>,
    pub clickhouse: Option<clickhouse::Client>,
    pub pdp: Option<AuthorizerClient<AuthChannel>>,
    /// awzy-context, for the directory.
    pub context: Option<directory::ContextClient>,
    /// Awzy Bot's model (ADR 0048); None when the install hasn't turned it on.
    pub bot: Option<awzy_bot::BotModel>,
    /// awzy-challenge, for challenges and containments (ADR 0054).
    pub challenge:
        Option<awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient<AuthChannel>>,
}

impl Services {
    /// AWZY_CHALLENGE_ADDR, AWZY_CLICKHOUSE_{URL,USER,PASSWORD,DATABASE}, AWZY_CONTEXT_ADDR,
    /// AWZY_PDP_ADDR, and AWZY_POLICY_ADDR.
    pub fn from_env() -> anyhow::Result<Self> {
        let channel = |k: &str| -> anyhow::Result<Option<Channel>> {
            Ok(match std::env::var(k) {
                Ok(addr) => Some(Channel::from_shared(addr)?.connect_lazy()),
                Err(_) => None,
            })
        };
        // With this portal's service tokens for each (ADR 0055).
        let policy = channel("AWZY_POLICY_ADDR")?.map(|c| PolicyServiceClient::new(grpc_channel(c, "awzy-policy")));
        let pdp = channel("AWZY_PDP_ADDR")?.map(|c| AuthorizerClient::new(grpc_channel(c, "awzy-pdp")));
        let clickhouse = std::env::var("AWZY_CLICKHOUSE_URL").ok().map(|url| {
            let var = |k: &str, default: &str| std::env::var(k).unwrap_or_else(|_| default.to_string());
            clickhouse::Client::default()
                .with_url(url)
                .with_user(var("AWZY_CLICKHOUSE_USER", "default"))
                .with_password(var("AWZY_CLICKHOUSE_PASSWORD", ""))
                .with_database(var("AWZY_CLICKHOUSE_DATABASE", "awzy"))
        });
        let context = std::env::var("AWZY_CONTEXT_ADDR").ok().map(directory::ContextClient::new);
        let challenge = channel("AWZY_CHALLENGE_ADDR")?.map(|c| {
            awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient::new(grpc_channel(
                c,
                "awzy-challenge",
            ))
        });
        Ok(Self { policy, clickhouse, pdp, context, bot: None, challenge })
    }
}

pub(crate) fn policy_client(ctx: &Context<'_>) -> async_graphql::Result<PolicyServiceClient<AuthChannel>> {
    ctx.data::<Services>()
        .ok()
        .and_then(|s| s.policy.clone())
        .ok_or_else(|| "the policy service is not configured (AWZY_POLICY_ADDR)".into())
}

pub(crate) fn clickhouse(ctx: &Context<'_>) -> async_graphql::Result<clickhouse::Client> {
    ctx.data::<Services>()
        .ok()
        .and_then(|s| s.clickhouse.clone())
        .ok_or_else(|| "ClickHouse is not configured (AWZY_CLICKHOUSE_URL)".into())
}

#[derive(Default)]
pub struct MetaQuery;

#[Object]
impl MetaQuery {
    /// Version of the client portal API.
    async fn api_version(&self) -> &'static str {
        API_VERSION
    }

    /// Who is signed in.
    async fn viewer(&self, ctx: &Context<'_>) -> Result<ViewerInfo> {
        let v = ctx.data::<Viewer>().map_err(|_| "sign in first")?;
        Ok(ViewerInfo {
            id: v.subject.id.clone(),
            kind: v.subject.kind.clone(),
            name: v.name.clone(),
            on_behalf_of: v.on_behalf_of.as_ref().map(|p| p.id.clone()),
        })
    }

    /// Whether the viewer may do `action` in `area`, so the portal can hide what they can't use.
    async fn viewer_can(&self, ctx: &Context<'_>, action: PortalAction, area: PortalArea) -> Result<bool> {
        let viewer = ctx.data::<Viewer>().map_err(|_| "sign in first")?;
        match access::authorize(ctx.data::<Services>()?, viewer, Surface::Portal, action, area).await {
            Ok(()) => Ok(true),
            Err(access::AccessError::Denied { .. } | access::AccessError::Challenged { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(SimpleObject)]
pub struct ViewerInfo {
    /// The person's or service account's ID in the IdP.
    pub id: String,
    /// `Human` or `ServiceAccount`.
    pub kind: String,
    pub name: Option<String>,
    /// For a service account acting for someone: that person's ID.
    pub on_behalf_of: Option<String>,
}

#[derive(MergedObject, Default)]
#[graphql(name = "Query")]
pub struct ClientPortalQuery(
    MetaQuery,
    node::NodeQuery,
    policies::PolicyQuery,
    decisions::DecisionQuery,
    enforcement::EnforcementQuery,
    directory::DirectoryQuery,
    dry_run::DryRunQuery,
    awzy_bot::AwzyBotQuery,
    challenges::ChallengeQuery,
    security_keys::SecurityKeyQuery,
    investigations::InvestigationQuery,
);

#[derive(MergedObject, Default)]
#[graphql(name = "Mutation")]
pub struct ClientPortalMutation(
    policies::PolicyMutation,
    enforcement::EnforcementMutation,
    directory::DirectoryMutation,
    awzy_bot::AwzyBotMutation,
    challenges::ChallengeMutation,
    security_keys::SecurityKeyMutation,
    investigations::InvestigationMutation,
);

pub type ClientPortalSchema = Schema<ClientPortalQuery, ClientPortalMutation, EmptySubscription>;

/// The schema without backends: for printing the SDL and for tests that don't need them.
pub fn schema() -> ClientPortalSchema {
    schema_with(Services::default())
}

pub fn schema_with(services: Services) -> ClientPortalSchema {
    Schema::build(ClientPortalQuery::default(), ClientPortalMutation::default(), EmptySubscription)
        .data(services)
        .finish()
}

/// GraphQL at `/graphql` and MCP at `/mcp`, both behind sign-in, plus the unauthenticated
/// sign-in discovery routes. `mcp_allowed_hosts` are the Host headers the MCP endpoint
/// accepts; empty means loopback only (see AWZY_MCP_ALLOWED_HOSTS in main).
pub fn router(services: Services, auth: Auth, mcp_allowed_hosts: Vec<String>) -> Router {
    let signed_in = middleware::from_fn_with_state(auth.clone(), access::require_viewer);
    let graphql = get(|| async { Html(GraphiQLSource::build().endpoint("/graphql").finish()) })
        .merge(post(graphql).layer(signed_in.clone()).with_state(schema_with(services.clone())));
    Router::new()
        .route("/graphql", graphql)
        .merge(mcp::router(services, mcp_allowed_hosts).layer(signed_in))
        .merge(access::routes(auth))
}

async fn graphql(
    State(schema): State<ClientPortalSchema>,
    Extension(viewer): Extension<Viewer>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner().data(viewer)).await.into()
}

#[cfg(test)]
pub(crate) mod testing {
    use std::sync::Arc;

    use super::*;

    /// An in-process PDP with the default policies, in monitor mode: the portal must apply its
    /// verdicts anyway.
    pub(crate) async fn pdp() -> AuthorizerClient<AuthChannel> {
        let channel = Channel::from_shared(pdp_endpoint().await).unwrap().connect().await.unwrap();
        AuthorizerClient::new(grpc_channel(channel, "awzy-pdp"))
    }

    /// The same PDP's address, for other services under test that call it.
    pub(crate) async fn pdp_endpoint() -> String {
        let enforcement = Arc::new(awzy_pdp::Enforcement::fixed(awzy_model::EnforcementMode::Monitor));
        let app = awzy_pdp::router(
            awzy_pdp::AppState::new(awzy_pdp::shared(awzy_pdp::Pdp::with_defaults().unwrap()))
                .with_enforcement(enforcement),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    pub(crate) fn human(id: &str, group: &str) -> Viewer {
        Viewer(Arc::new(awzy_identity::Accessor::human(id, None, &[group.to_string()])))
    }

    /// Runs `query` as `viewer`; panics on GraphQL errors.
    pub(crate) async fn run(
        schema: &ClientPortalSchema,
        viewer: &Viewer,
        query: impl Into<String>,
    ) -> serde_json::Value {
        let res = schema.execute(async_graphql::Request::new(query.into()).data(viewer.clone())).await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        res.data.into_json().unwrap()
    }

    /// Runs `query` as `viewer` and returns the first error message.
    pub(crate) async fn run_err(schema: &ClientPortalSchema, viewer: &Viewer, query: impl Into<String>) -> String {
        let res = schema.execute(async_graphql::Request::new(query.into()).data(viewer.clone())).await;
        res.errors.first().map(|e| e.message.clone()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn api_version_query() {
        let res = super::schema().execute("{ apiVersion }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{apiVersion: "0.1.0"}"#);
    }

    #[test]
    fn schema_follows_relay_conventions() {
        let sdl = super::schema().sdl();
        for expected in ["interface Node", "node(id: ID!): Node", "type DecisionConnection", "type PageInfo"] {
            assert!(sdl.contains(expected), "missing {expected:?} in SDL");
        }
    }

    #[tokio::test]
    async fn missing_backends_are_named_in_errors() {
        let schema =
            super::schema_with(super::Services { pdp: Some(super::testing::pdp().await), ..Default::default() });
        let err = super::testing::run_err(
            &schema,
            &super::testing::human("alice", "security"),
            "{ activePolicyBundle { id } }",
        )
        .await;
        assert!(err.contains("AWZY_POLICY_ADDR"), "{err}");
    }
}
