//! awzy-corp-portal: GraphQL API behind the corp portal for awzy staff. Runs in the vendor corp account.
//!
//! Holds install records and health metrics only. It must never depend on client data:
//! Bazel visibility (//tools:client_side) blocks links to authz services and libs/store.
//!
//! `/graphql` needs a signed-in staff member. Sign-in is username and password, for demos and
//! testing only (see [`auth`], ADR 0025). Installs report to `/v1/heartbeat` with their own
//! token instead (see [`fleet`], ADR 0026).

pub mod alerts;
pub mod auth;
pub mod fleet;
mod installs;
pub mod licenses;
pub mod mcp;
pub mod support;

use async_graphql::{http::GraphiQLSource, Context, EmptySubscription, MergedObject, Object, Result, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use auth::{Auth, Staff};
use awzy_portal::ErrorCode;
use axum::{
    extract::State,
    middleware,
    response::Html,
    routing::{get, post},
    Extension, Router,
};
use fleet::FleetStore;

pub const API_VERSION: &str = "0.1.0";

#[derive(Default)]
pub struct MetaQuery;

#[Object]
impl MetaQuery {
    /// Version of the corp portal API.
    async fn api_version(&self) -> &'static str {
        API_VERSION
    }

    /// The signed-in staff member.
    async fn viewer(&self, ctx: &Context<'_>) -> Result<Staff> {
        Ok(ctx.data::<Staff>().map_err(|_| ErrorCode::Unauthenticated.error("sign in first"))?.clone())
    }
}

#[async_graphql::Object]
impl Staff {
    async fn username(&self) -> &str {
        &self.username
    }

    async fn name(&self) -> &str {
        &self.name
    }
}

#[derive(MergedObject, Default)]
#[graphql(name = "Query")]
pub struct CorpPortalQuery(MetaQuery, installs::FleetQuery);

#[derive(MergedObject, Default)]
#[graphql(name = "Mutation")]
pub struct CorpPortalMutation(
    installs::FleetMutation,
    mcp::McpTokenMutation,
    support::SupportMutation,
    licenses::LicenseMutation,
);

pub type CorpPortalSchema = Schema<CorpPortalQuery, CorpPortalMutation, EmptySubscription>;

/// The schema without Postgres: for printing the SDL and for tests that don't need it.
pub fn schema() -> CorpPortalSchema {
    schema_builder().finish()
}

pub fn schema_with(store: FleetStore) -> CorpPortalSchema {
    schema_builder().data(store).finish()
}

fn schema_builder() -> async_graphql::SchemaBuilder<CorpPortalQuery, CorpPortalMutation, EmptySubscription> {
    Schema::build(CorpPortalQuery::default(), CorpPortalMutation::default(), EmptySubscription)
        .limit_depth(awzy_portal::MAX_QUERY_DEPTH)
        .limit_complexity(awzy_portal::MAX_QUERY_COMPLEXITY)
}

/// `/graphql` behind sign-in, the sign-in routes under `/auth`, heartbeat intake at
/// `/v1/heartbeat`, and MCP for awzy's agents at `/mcp`, behind a staff member's MCP token, for
/// the Host headers in AWZY_CORP_PORTAL_MCP_ALLOWED_HOSTS (comma-separated; unset: loopback only).
/// GraphQL resolves the frontend's queries by ID from `persisted` (ADR 0088, 0090).
pub fn router(auth: Auth, store: FleetStore, persisted: awzy_portal::PersistedQueries) -> Router {
    let signed_in = middleware::from_fn_with_state(auth.clone(), auth::require_staff);
    let graphql = get(|| async { Html(GraphiQLSource::build().endpoint("/graphql").finish()) }).merge(
        post(graphql)
            .layer(signed_in)
            .with_state(schema_builder().data(store.clone()).data(auth.clone()).extension(persisted).finish()),
    );
    let mcp_hosts = std::env::var("AWZY_CORP_PORTAL_MCP_ALLOWED_HOSTS")
        .map(|v| v.split(',').map(|h| h.trim().to_string()).filter(|h| !h.is_empty()).collect())
        .unwrap_or_default();
    let mcp = mcp::router(store.clone(), mcp_hosts)
        .layer(middleware::from_fn_with_state(auth.clone(), auth::require_mcp_staff));
    Router::new().route("/graphql", graphql).merge(mcp).merge(auth::routes(auth)).merge(fleet::intake_routes(store))
}

async fn graphql(
    State(schema): State<CorpPortalSchema>,
    Extension(staff): Extension<Staff>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner().data(staff)).await.into()
}

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use super::*;
    use crate::auth::{hash_password, StaffAccount};

    #[tokio::test]
    async fn api_version_query() {
        let res = super::schema().execute("{ apiVersion }").await;
        assert!(res.errors.is_empty(), "{:?}", res.errors);
        assert_eq!(res.data.to_string(), r#"{apiVersion: "0.1.0"}"#);
    }

    fn auth() -> Auth {
        let account = StaffAccount {
            username: "max".into(),
            name: "Max".into(),
            password_hash: hash_password("correct horse battery").unwrap(),
        };
        Auth::new(vec![account], vec![7; 32], true).unwrap()
    }

    /// Sign-in doesn't touch Postgres, so the store never connects.
    fn app() -> Router {
        let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://unused@localhost:1/unused").unwrap();
        router(auth(), FleetStore::new(pool), awzy_portal::PersistedQueries::none())
    }

    async fn send(app: &Router, req: Request<Body>) -> (StatusCode, Option<String>, String) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let cookie = res.headers().get("set-cookie").map(|v| v.to_str().unwrap().to_string());
        let body = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
        (status, cookie, String::from_utf8(body.to_vec()).unwrap())
    }

    fn login(username: &str, password: &str) -> Request<Body> {
        Request::post("/auth/login")
            .header("content-type", "application/json")
            .body(Body::from(json!({ "username": username, "password": password }).to_string()))
            .unwrap()
    }

    fn query(cookie: Option<&str>, content_type: &str) -> Request<Body> {
        let mut req = Request::post("/graphql").header("content-type", content_type);
        if let Some(c) = cookie {
            req = req.header("cookie", c);
        }
        req.body(Body::from(json!({ "query": "{ viewer { username name } }" }).to_string())).unwrap()
    }

    #[tokio::test]
    async fn signs_in_and_out_with_a_session_cookie() {
        let app = app();
        assert_eq!(send(&app, query(None, "application/json")).await.0, StatusCode::UNAUTHORIZED);

        let (status, cookie, _) = send(&app, login("max", "correct horse battery")).await;
        assert_eq!(status, StatusCode::OK);
        let cookie = cookie.unwrap();
        assert!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict") && cookie.contains("Secure"),
            "{cookie}"
        );
        let session = cookie.split(';').next().unwrap();

        let (status, _, body) = send(&app, query(Some(session), "application/json")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["data"]["viewer"],
            json!({ "username": "max", "name": "Max" })
        );

        // A form post can't carry the query, even with the cookie.
        assert_eq!(
            send(&app, query(Some(session), "application/x-www-form-urlencoded")).await.0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
        // Tampering with the cookie's body breaks the signature.
        let (name, value) = session.split_once('=').unwrap();
        let tampered = format!("{name}=x{value}");
        assert_eq!(send(&app, query(Some(&tampered), "application/json")).await.0, StatusCode::UNAUTHORIZED);

        let (_, cleared, _) = send(&app, Request::post("/auth/logout").body(Body::empty()).unwrap()).await;
        assert!(cleared.unwrap().contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn locks_a_username_after_five_failures_without_revealing_which_exist() {
        let app = app();
        let (status, _, unknown) = send(&app, login("nobody", "whatever")).await;
        let (_, _, wrong) = send(&app, login("max", "wrong password")).await;
        assert_eq!((status, unknown.as_str()), (StatusCode::UNAUTHORIZED, wrong.as_str()), "same answer either way");

        for _ in 0..4 {
            send(&app, login("max", "wrong password")).await;
        }
        let (status, _, _) = send(&app, login("max", "correct horse battery")).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "locked even with the right password");
    }

    async fn session(app: &Router) -> String {
        let (_, cookie, _) = send(app, login("max", "correct horse battery")).await;
        cookie.unwrap().split(';').next().unwrap().to_string()
    }

    fn gql(cookie: &str, query: &str) -> Request<Body> {
        Request::post("/graphql")
            .header("content-type", "application/json")
            .header("cookie", cookie)
            .body(Body::from(json!({ "query": query }).to_string()))
            .unwrap()
    }

    fn heartbeat(token: &str, body: Value) -> Request<Body> {
        Request::post("/v1/heartbeat")
            .header("content-type", "application/json")
            .header("authorization", format!("Bearer {token}"))
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    /// With --config=local-stack: register, report, and see the fleet through GraphQL.
    #[tokio::test]
    async fn registered_installs_report_and_show_in_the_fleet() {
        let Some(store) = crate::fleet::tests::test_store().await else { return };
        let app = router(auth(), store, awzy_portal::PersistedQueries::none());
        let cookie = session(&app).await;

        let (_, _, body) = send(
            &app,
            gql(&cookie, r#"mutation { registerInstall(input: { client: "acme", environment: "prod", cloud: "aws", region: "us-east-2", channel: FAST }) { install { id health } reportingToken } }"#),
        )
        .await;
        let reg: Value = serde_json::from_str(&body).unwrap();
        let id = reg["data"]["registerInstall"]["install"]["id"].as_str().unwrap().to_string();
        let token = reg["data"]["registerInstall"]["reportingToken"].as_str().unwrap().to_string();
        assert_eq!(reg["data"]["registerInstall"]["install"]["health"], "NEVER_REPORTED");

        let beat = |chart: &str, ready: u32| {
            json!({
                "schema_version": 1, "install_qid": id, "sent_at": chrono::Utc::now(), "chart_version": chart,
                "services": [
                    { "service": "awzy-pdp", "version": chart, "image_digest": null, "replicas_desired": 3, "replicas_ready": ready, "restarts_24h": 0, "ready": true },
                    { "service": "awzy-policy", "version": chart, "image_digest": null, "replicas_desired": 2, "replicas_ready": 2, "restarts_24h": null, "ready": true },
                ],
                "metrics": { "decisions_per_second": 41.5, "pdp_p99_ms": 1.3 },
            })
        };
        assert_eq!(send(&app, heartbeat("awzyrt_nope", beat("0.4.2", 3))).await.0, StatusCode::UNAUTHORIZED);
        let mut other = beat("0.4.2", 3);
        other["install_qid"] = json!(awzy_model::InstallId::new().to_string());
        assert_eq!(
            send(&app, heartbeat(&token, other)).await.0,
            StatusCode::FORBIDDEN,
            "a token reports only for its own install"
        );
        let mut free_form = beat("0.4.2", 3);
        free_form["hostname"] = json!("db01.acme.internal");
        assert_eq!(send(&app, heartbeat(&token, free_form)).await.0, StatusCode::BAD_REQUEST);

        assert_eq!(send(&app, heartbeat(&token, beat("0.4.2", 3))).await.0, StatusCode::NO_CONTENT);
        assert_eq!(send(&app, heartbeat(&token, beat("0.5.0", 2))).await.0, StatusCode::NO_CONTENT);

        let (_, _, body) = send(
            &app,
            gql(
                &cookie,
                r#"{ installs { edges { node { client environment health chartVersion
                services { name replicasReady replicasDesired healthy } metrics { decisionsPerSecond }
                metricHistory(window: DAY) { pdpP99Ms degraded }
                versionHistory { chartVersion } } } }
              releases { version installCount } }"#,
            ),
        )
        .await;
        let fleet: Value = serde_json::from_str(&body).unwrap();
        let install = &fleet["data"]["installs"]["edges"][0]["node"];
        assert_eq!(install["health"], "DEGRADED", "2 of 3 PDP replicas ready: {body}");
        assert_eq!(install["chartVersion"], "0.5.0");
        assert_eq!(
            install["services"][0],
            json!({ "name": "awzy-pdp", "replicasReady": 2, "replicasDesired": 3, "healthy": false })
        );
        assert_eq!(install["metrics"]["decisionsPerSecond"], 41.5);
        // Both heartbeats land in the current 5-minute bucket, unless it rolled over between them.
        let history = install["metricHistory"].as_array().unwrap();
        assert!(matches!(history.len(), 1 | 2), "{body}");
        assert_eq!(history.last().unwrap()["pdpP99Ms"], 1.3);
        assert!(history.iter().any(|s| s["degraded"] == true), "the 2-of-3 heartbeat was degraded");
        assert_eq!(install["versionHistory"], json!([{ "chartVersion": "0.5.0" }, { "chartVersion": "0.4.2" }]));
        assert_eq!(fleet["data"]["releases"], json!([{ "version": "0.5.0", "installCount": 1 }]));
    }
}
