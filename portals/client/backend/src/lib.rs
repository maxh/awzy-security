//! awzy-client-portal: GraphQL API behind the client portal. Runs in each client cluster.
//!
//! Serves the portal's GraphQL API at `/graphql`, an MCP server for the client's AI agents at
//! `/mcp`, and (in the image) the built frontend. Policies come from awzy-policy over gRPC;
//! decisions come from ClickHouse. The GraphQL schema follows the Relay server specification
//! (global IDs, `node`, cursor connections).
//!
//! Both need a signed-in viewer, and every operation is checked with awzy-pdp (see [`access`]).

pub mod access;
mod audit;
pub mod awzy_bot;
mod challenges;
mod connector_config;
mod connectors;
mod decisions;
mod diagnostics;
pub mod directory;
mod dry_run;
mod enforcement;
mod fields;
mod investigations;
mod lookup;
mod mcp;
mod node;
mod outage;
mod policies;
mod repairs;
mod security_keys;
mod status;
pub mod support;

use access::{Auth, PortalAction, PortalArea, Surface, Viewer};
use async_graphql::{
    http::{GraphiQLSource, ALL_WEBSOCKET_PROTOCOLS},
    Context, ErrorExtensions, MergedObject, MergedSubscription, Object, Result, Schema, SimpleObject,
};
use async_graphql_axum::{GraphQLProtocol, GraphQLRequest, GraphQLResponse, GraphQLWebSocket};
use awzy_authz_proto::{
    authz::v1::authorizer_client::AuthorizerClient, policy::v1::policy_service_client::PolicyServiceClient,
};
use awzy_service_auth::{grpc_channel, AuthChannel};
use axum::{
    extract::{State, WebSocketUpgrade},
    middleware,
    response::{Html, IntoResponse},
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
    /// awzy-context's API, typed (#222): the areas `awzy_context_api` covers so far.
    pub context_api: Option<awzy_context_api::ContextApi>,
    /// Awzy Bot's model (ADR 0048); None when the install hasn't turned it on.
    pub bot: Option<awzy_bot::BotModel>,
    /// awzy-challenge, for challenges and containments (ADR 0054).
    pub challenge:
        Option<awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient<AuthChannel>>,
    /// awzy-intake, where calls to the MCP endpoint are recorded as activity (ADR 0084); off
    /// without it.
    pub activity: awzy_activity::ActivityLog,
}

impl Services {
    /// AWZY_CHALLENGE_ADDR, AWZY_CLICKHOUSE_{URL,USER,PASSWORD,DATABASE}, AWZY_CONTEXT_ADDR,
    /// AWZY_INTAKE_{URL,TOKEN}, AWZY_PDP_ADDR, and AWZY_POLICY_ADDR.
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
        let context_api = std::env::var("AWZY_CONTEXT_ADDR")
            .ok()
            .map(|a| awzy_context_api::ContextApi::new(&a, awzy_service_auth::TokenSource::from_env()));
        let challenge = channel("AWZY_CHALLENGE_ADDR")?.map(|c| {
            awzy_authz_proto::challenge::v1::challenge_service_client::ChallengeServiceClient::new(grpc_channel(
                c,
                "awzy-challenge",
            ))
        });
        let activity = awzy_activity::ActivityLog::from_env("awzy-client-portal");
        Ok(Self { policy, clickhouse, pdp, context, context_api, bot: None, challenge, activity })
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
        let v = access::signed_in(ctx)?;
        Ok(ViewerInfo {
            id: v.subject.id.clone(),
            kind: v.subject.kind.clone(),
            name: v.name.clone(),
            on_behalf_of: v.on_behalf_of.as_ref().map(|p| p.id.clone()),
        })
    }

    /// Whether the viewer may do `action` in `area`, so the portal can hide what they can't use.
    async fn viewer_can(&self, ctx: &Context<'_>, action: PortalAction, area: PortalArea) -> Result<bool> {
        let viewer = access::signed_in(ctx)?;
        match access::authorize(ctx.data::<Services>()?, viewer, Surface::Portal, action, area).await {
            Ok(()) => Ok(true),
            Err(access::AccessError::Denied { .. } | access::AccessError::Challenged { .. }) => Ok(false),
            Err(e) => Err(e.extend()),
        }
    }

    /// The areas the viewer may view, so the portal shows only the pages they can open. Asks the
    /// PDP about each area at once.
    async fn viewer_areas(&self, ctx: &Context<'_>) -> Result<Vec<PortalArea>> {
        let viewer = access::signed_in(ctx)?;
        let services = ctx.data::<Services>()?;
        let checks = PortalArea::ALL.iter().map(|&area| async move {
            match access::authorize(services, viewer, Surface::Portal, PortalAction::View, area).await {
                Ok(()) => Ok(Some(area)),
                Err(access::AccessError::Denied { .. } | access::AccessError::Challenged { .. }) => Ok(None),
                Err(e) => Err(e),
            }
        });
        let results = async_graphql::futures_util::future::join_all(checks).await;
        let areas = results.into_iter().collect::<Result<Vec<_>, _>>().map_err(|e| e.extend())?;
        Ok(areas.into_iter().flatten().collect())
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
    status::StatusQuery,
    connectors::ConnectorsQuery,
    connector_config::ConnectorConfigQuery,
    outage::OutageQuery,
    support::SupportQuery,
    lookup::LookupQuery,
    diagnostics::DiagnosticsQuery,
    repairs::RepairQuery,
);

#[derive(MergedObject, Default)]
#[graphql(name = "Mutation")]
pub struct ClientPortalMutation(
    policies::PolicyMutation,
    enforcement::EnforcementMutation,
    outage::OutageMutation,
    support::SupportMutation,
    directory::DirectoryMutation,
    awzy_bot::AwzyBotMutation,
    challenges::ChallengeMutation,
    security_keys::SecurityKeyMutation,
    investigations::InvestigationMutation,
    repairs::RepairMutation,
    connector_config::ConnectorConfigMutation,
);

/// Live updates: the Challenges page's challenges and containments.
#[derive(MergedSubscription, Default)]
pub struct ClientPortalSubscription(challenges::ChallengeSubscription);

pub type ClientPortalSchema = Schema<ClientPortalQuery, ClientPortalMutation, ClientPortalSubscription>;

/// The schema without backends: for printing the SDL and for tests that don't need them.
pub fn schema() -> ClientPortalSchema {
    schema_with(Services::default())
}

pub fn schema_with(services: Services) -> ClientPortalSchema {
    schema_builder(services).finish()
}

fn schema_builder(
    services: Services,
) -> async_graphql::SchemaBuilder<ClientPortalQuery, ClientPortalMutation, ClientPortalSubscription> {
    Schema::build(ClientPortalQuery::default(), ClientPortalMutation::default(), ClientPortalSubscription::default())
        .data(services)
        .limit_depth(awzy_portal::MAX_QUERY_DEPTH)
        .limit_complexity(awzy_portal::MAX_QUERY_COMPLEXITY)
        // Each operation a support session runs is recorded (ADR 0095).
        .extension(support::SupportAudit)
        // Every mutation, recorded as the viewer's activity.
        .extension(audit::MutationAudit)
        .extension(fields::FieldAuthz)
}

/// GraphQL at `/graphql` and MCP at `/mcp`, each behind sign-in with its own tokens (`auth` and
/// `mcp_auth`, ADR 0081), plus the unauthenticated sign-in discovery routes. `mcp_allowed_hosts`
/// are the Host headers the MCP endpoint accepts; empty means loopback only (see
/// AWZY_MCP_ALLOWED_HOSTS in main). GraphQL resolves the frontend's queries by ID from
/// `persisted`, and with its `only`, runs nothing else (ADR 0088, 0090).
pub fn router(
    services: Services,
    auth: Auth,
    mcp_auth: Auth,
    mcp_allowed_hosts: Vec<String>,
    persisted: awzy_portal::PersistedQueries,
) -> Router {
    let signed_in = middleware::from_fn_with_state(auth.clone(), access::require_viewer);
    let mcp_signed_in = middleware::from_fn_with_state(mcp_auth.clone(), access::require_viewer);
    let schema = schema_builder(services.clone()).extension(persisted).finish();
    // GET: subscriptions over a WebSocket (graphql-ws), or GraphiQL in a browser.
    let graphql = get(graphql_get)
        .with_state((schema.clone(), auth.clone()))
        .merge(post(graphql).layer(signed_in).with_state(schema));
    Router::new()
        .route("/graphql", graphql)
        .merge(mcp::router(services, mcp_allowed_hosts).layer(mcp_signed_in))
        .merge(access::routes(auth, mcp_auth))
}

/// A WebSocket for subscriptions, signed in by the bearer token in its first message
/// (`connectionParams.authorization`), since a browser can't set headers on one. Anything else
/// gets GraphiQL.
async fn graphql_get(
    State((schema, auth)): State<(ClientPortalSchema, Auth)>,
    protocol: Result<GraphQLProtocol, axum::http::StatusCode>,
    upgrade: Result<WebSocketUpgrade, axum::extract::ws::rejection::WebSocketUpgradeRejection>,
) -> axum::response::Response {
    let (Ok(protocol), Ok(upgrade)) = (protocol, upgrade) else {
        return Html(GraphiQLSource::build().endpoint("/graphql").subscription_endpoint("/graphql").finish())
            .into_response();
    };
    upgrade.protocols(ALL_WEBSOCKET_PROTOCOLS).on_upgrade(move |socket| {
        GraphQLWebSocket::new(socket, schema, protocol)
            .on_connection_init(move |params| async move {
                let authorization = params["authorization"].as_str();
                let viewer = auth
                    .viewer(authorization)
                    .await
                    .map_err(|e| awzy_portal::ErrorCode::Unauthenticated.error(e.to_string()))?;
                let mut data = async_graphql::Data::default();
                data.insert(viewer);
                Ok(data)
            })
            .serve()
    })
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
        pdp_endpoint_with(awzy_pdp::Pdp::with_defaults().unwrap()).await
    }

    /// A PDP like [`pdp`], under `policies` with the default schema.
    pub(crate) async fn pdp_with(policies: &str) -> AuthorizerClient<AuthChannel> {
        let endpoint = pdp_endpoint_with(awzy_pdp::Pdp::new(awzy_pdp::DEFAULT_SCHEMA, policies).unwrap()).await;
        let channel = Channel::from_shared(endpoint).unwrap().connect().await.unwrap();
        AuthorizerClient::new(grpc_channel(channel, "awzy-pdp"))
    }

    async fn pdp_endpoint_with(pdp: awzy_pdp::Pdp) -> String {
        let enforcement = Arc::new(awzy_pdp::Enforcement::fixed(awzy_model::EnforcementMode::Monitor));
        let app = awzy_pdp::router(awzy_pdp::AppState::new(awzy_pdp::shared(pdp)).with_enforcement(enforcement));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    pub(crate) fn human(id: &str, group: &str) -> Viewer {
        Viewer(Arc::new(awzy_identity::Principal::human(id, None, &[group.to_string()])), None)
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
    async fn queries_too_deep_or_too_wide_are_refused_before_they_run() {
        let schema = super::schema();
        let deep =
            format!("{{ __schema {{ types {{ {}name{} }} }} }}", "fields { type { ".repeat(10), " } }".repeat(10));
        let res = schema.execute(deep).await;
        assert!(res.errors.iter().any(|e| e.message.contains("nested too deep")), "{:?}", res.errors);

        let wide: String = (0..1_500).map(|i| format!("a{i}: apiVersion ")).collect();
        let res = schema.execute(format!("{{ {wide} }}")).await;
        assert!(res.errors.iter().any(|e| e.message.contains("too complex")), "{:?}", res.errors);

        // As deep as GraphiQL's introspection goes: a field's argument types, seven `ofType`s down.
        let introspection = format!(
            "{{ __schema {{ types {{ fields {{ args {{ type {{ {}name{} }} }} }} }} }} }}",
            "ofType { ".repeat(7),
            " }".repeat(7)
        );
        let res = schema.execute(introspection).await;
        assert!(res.errors.is_empty(), "GraphiQL can still introspect: {:?}", res.errors);
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
