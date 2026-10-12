//! Repairs a client admin starts from the Status page (ADR 0102): each is a suspended CronJob
//! the chart defines, which the portal starts a Job from. The portal can read those CronJobs and
//! create and list Jobs in the data namespace, and nothing more, so it can't run anything the
//! chart didn't define.

use anyhow::Context as _;
use async_graphql::{connection::Connection, Context, Enum, Object, Result, SimpleObject};
use awzy_portal::{connection::page, UtcTimestamp};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::access::{require, PortalAction, PortalArea};

const LABEL: &str = "awzy.io/repair";

/// A repair the chart defines.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum RepairKind {
    /// Drops parts ClickHouse detached, empties its own query and metric logs, and applies
    /// decisions' and events' retention now. Loses none of awzy's data.
    ClickhouseDiskCleanup,
}

impl RepairKind {
    const ALL: [RepairKind; 1] = [Self::ClickhouseDiskCleanup];

    /// Its value of the `awzy.io/repair` label.
    fn label(self) -> &'static str {
        match self {
            Self::ClickhouseDiskCleanup => "disk-cleanup",
        }
    }

    fn cronjob(self) -> &'static str {
        match self {
            Self::ClickhouseDiskCleanup => "awzy-clickhouse-disk-cleanup",
        }
    }

    fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.label() == s)
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum RepairStatus {
    Running,
    Succeeded,
    Failed,
}

/// One run of a repair.
#[derive(SimpleObject, Debug, Clone, PartialEq)]
pub struct RepairRun {
    /// The Job's name, for `kubectl logs job/<name>`.
    pub name: String,
    pub kind: RepairKind,
    pub status: RepairStatus,
    /// Who started it, as the portal knew them.
    pub started_by: Option<String>,
    pub started_at: Option<UtcTimestamp>,
    pub finished_at: Option<UtcTimestamp>,
}

/// The Kubernetes API, as the portal's own service account.
struct KubeApi(awzy_kube::KubeApi);

impl KubeApi {
    fn in_cluster() -> anyhow::Result<Self> {
        Ok(Self(awzy_kube::KubeApi::in_cluster()?))
    }

    #[cfg(test)]
    fn for_test(base: String) -> Self {
        Self(awzy_kube::KubeApi::for_test(base))
    }

    async fn runs(&self, namespace: &str) -> anyhow::Result<Vec<RepairRun>> {
        let jobs = self.0.list(&format!("/apis/batch/v1/namespaces/{namespace}/jobs"), LABEL).await?;
        let mut runs: Vec<RepairRun> = jobs.iter().filter_map(run).collect();
        runs.sort_by_key(|r| std::cmp::Reverse(r.started_at));
        Ok(runs)
    }

    /// Starts a Job from `kind`'s CronJob, unless one is already running.
    async fn start(&self, namespace: &str, kind: RepairKind, started_by: &str) -> anyhow::Result<RepairRun> {
        if self.runs(namespace).await?.iter().any(|r| r.kind == kind && r.status == RepairStatus::Running) {
            anyhow::bail!("that repair is already running");
        }
        let cronjob = self.0.get(&format!("/apis/batch/v1/namespaces/{namespace}/cronjobs/{}", kind.cronjob())).await?;
        let job = json!({
            "apiVersion": "batch/v1",
            "kind": "Job",
            "metadata": {
                "generateName": format!("{}-", kind.cronjob()),
                "labels": { LABEL: kind.label() },
                "annotations": { "awzy.io/started-by": started_by },
            },
            "spec": cronjob["spec"]["jobTemplate"]["spec"],
        });
        let created = self.0.create(&format!("/apis/batch/v1/namespaces/{namespace}/jobs"), &job).await?;
        run(&created).context("the API server returned a Job without the repair's label")
    }
}

fn time(v: &Value) -> Option<UtcTimestamp> {
    v.as_str().and_then(|t| t.parse::<DateTime<Utc>>().ok()).map(UtcTimestamp)
}

/// A repair's Job, or None for any other.
fn run(job: &Value) -> Option<RepairRun> {
    let kind = RepairKind::from_label(job["metadata"]["labels"][LABEL].as_str()?)?;
    let s = &job["status"];
    let status = if s["succeeded"].as_u64().unwrap_or(0) > 0 {
        RepairStatus::Succeeded
    } else if s["conditions"].as_array().into_iter().flatten().any(|c| c["type"] == "Failed" && c["status"] == "True") {
        RepairStatus::Failed
    } else {
        RepairStatus::Running
    };
    Some(RepairRun {
        name: job["metadata"]["name"].as_str().unwrap_or_default().to_string(),
        kind,
        status,
        started_by: job["metadata"]["annotations"]["awzy.io/started-by"].as_str().map(str::to_string),
        started_at: time(&s["startTime"]).or_else(|| time(&job["metadata"]["creationTimestamp"])),
        finished_at: time(&s["completionTime"]),
    })
}

/// The data namespace the repairs are in, from AWZY_PORTAL_REPAIRS_NAMESPACE; unset, there are none.
fn namespace() -> Option<String> {
    std::env::var("AWZY_PORTAL_REPAIRS_NAMESPACE").ok().filter(|n| !n.is_empty())
}

#[derive(Default)]
pub struct RepairQuery;

#[Object]
impl RepairQuery {
    /// The repairs this install defines; empty where it defines none. For those who may
    /// administer the enforcement area.
    async fn repair_kinds(&self, ctx: &Context<'_>) -> Result<Vec<RepairKind>> {
        require(ctx, PortalAction::Administer, PortalArea::Enforcement).await?;
        Ok(if namespace().is_some() { RepairKind::ALL.to_vec() } else { vec![] })
    }

    /// Recent repair runs, newest first; a finished one is kept for a week. For those who may
    /// administer the enforcement area.
    async fn repair_runs(
        &self,
        ctx: &Context<'_>,
        after: Option<String>,
        first: Option<i32>,
    ) -> Result<Connection<String, RepairRun>> {
        require(ctx, PortalAction::Administer, PortalArea::Enforcement).await?;
        let runs = match namespace() {
            Some(ns) => KubeApi::in_cluster()?.runs(&ns).await?,
            None => vec![],
        };
        page(runs, |r| r.name.clone(), after, first)
    }
}

#[derive(Default)]
pub struct RepairMutation;

#[Object]
impl RepairMutation {
    /// Starts a repair, unless one of its kind is running. For those who may administer the
    /// enforcement area.
    async fn start_repair(&self, ctx: &Context<'_>, kind: RepairKind) -> Result<RepairRun> {
        let viewer = require(ctx, PortalAction::Administer, PortalArea::Enforcement).await?;
        let ns = namespace().ok_or("this install defines no repairs")?;
        let by = viewer.subject.id.clone();
        let run = KubeApi::in_cluster()?.start(&ns, kind, &by).await?;
        tracing::warn!(repair = kind.label(), job = %run.name, by = %by, "repair started");
        Ok(run)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{extract::Path, routing::get, Json, Router};

    use super::*;

    #[tokio::test]
    async fn a_repair_starts_from_its_cronjob_once_at_a_time() {
        let jobs: Arc<Mutex<Vec<Value>>> = Arc::default();
        let (list, create) = (jobs.clone(), jobs.clone());
        let app = Router::new()
            .route(
                "/apis/batch/v1/namespaces/{ns}/cronjobs/{name}",
                get(|Path((_, name)): Path<(String, String)>| async move {
                    Json(json!({ "metadata": { "name": name }, "spec": { "jobTemplate": { "spec": { "backoffLimit": 1 } } } }))
                }),
            )
            .route(
                "/apis/batch/v1/namespaces/{ns}/jobs",
                get(move || {
                    let list = list.clone();
                    async move { Json(json!({ "items": *list.lock().unwrap() })) }
                })
                .post(move |Json(mut job): Json<Value>| {
                    let create = create.clone();
                    async move {
                        assert_eq!(job["spec"]["backoffLimit"], 1, "the CronJob's template, as the chart wrote it");
                        job["metadata"]["name"] = json!("awzy-clickhouse-disk-cleanup-x7k2p");
                        job["metadata"]["creationTimestamp"] = json!("2026-10-11T12:00:00Z");
                        create.lock().unwrap().push(job.clone());
                        Json(job)
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let kube = KubeApi::for_test(base);

        let started = kube.start("awzy-data", RepairKind::ClickhouseDiskCleanup, "alice").await.unwrap();
        assert_eq!((started.status, started.started_by.as_deref()), (RepairStatus::Running, Some("alice")));
        let again = kube.start("awzy-data", RepairKind::ClickhouseDiskCleanup, "bob").await.unwrap_err();
        assert!(again.to_string().contains("already running"), "{again}");

        jobs.lock().unwrap()[0]["status"] = json!({ "succeeded": 1, "completionTime": "2026-10-11T12:01:00Z" });
        let runs = kube.runs("awzy-data").await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(
            (runs[0].status, runs[0].name.as_str()),
            (RepairStatus::Succeeded, "awzy-clickhouse-disk-cleanup-x7k2p")
        );
    }

    #[test]
    fn only_a_repairs_jobs_are_runs() {
        assert!(run(&json!({ "metadata": { "name": "other", "labels": {} } })).is_none());
        let failed = run(&json!({
            "metadata": { "name": "r", "labels": { LABEL: "disk-cleanup" } },
            "status": { "conditions": [{ "type": "Failed", "status": "True" }] },
        }))
        .unwrap();
        assert_eq!(failed.status, RepairStatus::Failed);
    }
}
