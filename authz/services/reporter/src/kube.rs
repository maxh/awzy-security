//! Reads the awzy workloads from the Kubernetes API: Deployments labelled
//! `app.kubernetes.io/part-of=awzy`, the ClickHouse StatefulSets the Altinity operator makes, and
//! their Pods. Read-only, and only in the namespaces the chart grants (a Role per namespace).
//!
//! Through `awzy_kube`, as the reporter's service account: three list calls.

use chrono::{DateTime, Utc};
use serde_json::Value;

const AWZY: &str = "app.kubernetes.io/part-of=awzy";
const CLICKHOUSE: &str = "clickhouse.altinity.com/chi";

/// The service a workload is, by the name the heartbeat uses.
#[derive(Debug, Clone, PartialEq)]
pub struct Workload {
    pub name: String,
    pub desired: u32,
    pub ready: u32,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pod {
    pub uid: String,
    /// The workload it belongs to, by the heartbeat's service name.
    pub workload: String,
    pub ip: Option<String>,
    pub ready: bool,
    pub restarts: u32,
    pub started_at: Option<DateTime<Utc>>,
    /// `sha256:<hex>` of the running image, from the container status.
    pub image_digest: Option<String>,
    /// The image tag, e.g. `25.8` for ClickHouse.
    pub image_tag: Option<String>,
}

pub struct KubeApi(awzy_kube::KubeApi);

impl KubeApi {
    /// From inside a pod, as the reporter's service account.
    pub fn in_cluster() -> anyhow::Result<Self> {
        Ok(Self(awzy_kube::KubeApi::in_cluster()?))
    }

    /// Against a fake API server over plain HTTP.
    #[cfg(test)]
    pub fn for_test(base: String) -> Self {
        Self(awzy_kube::KubeApi::for_test(base))
    }

    async fn list(&self, path: &str, selector: &str) -> anyhow::Result<Vec<Value>> {
        Ok(self.0.list(path, selector).await?)
    }

    /// awzy's Deployments and the ClickHouse StatefulSets in `namespace`.
    pub async fn workloads(&self, namespace: &str) -> anyhow::Result<Vec<Workload>> {
        let mut out: Vec<Workload> = Vec::new();
        for d in self.list(&format!("/apis/apps/v1/namespaces/{namespace}/deployments"), AWZY).await? {
            let name = d["metadata"]["labels"]["app.kubernetes.io/name"].as_str().unwrap_or_default().to_string();
            out.push(Workload {
                name,
                desired: count(&d["spec"]["replicas"]),
                ready: count(&d["status"]["readyReplicas"]),
                version: d["metadata"]["labels"]["app.kubernetes.io/version"].as_str().map(str::to_string),
            });
        }
        // The operator makes one StatefulSet per ClickHouse replica: add them up.
        for s in self.list(&format!("/apis/apps/v1/namespaces/{namespace}/statefulsets"), CLICKHOUSE).await? {
            let entry = match out.iter_mut().find(|w| w.name == "clickhouse") {
                Some(e) => e,
                None => {
                    out.push(Workload { name: "clickhouse".into(), desired: 0, ready: 0, version: None });
                    out.last_mut().expect("just pushed")
                }
            };
            entry.desired += count(&s["spec"]["replicas"]);
            entry.ready += count(&s["status"]["readyReplicas"]);
        }
        Ok(out)
    }

    /// The pods of awzy's Deployments and of ClickHouse in `namespace`.
    pub async fn pods(&self, namespace: &str) -> anyhow::Result<Vec<Pod>> {
        let path = format!("/api/v1/namespaces/{namespace}/pods");
        let awzy = self.list(&path, AWZY).await?.into_iter().map(|p| {
            let name = p["metadata"]["labels"]["app.kubernetes.io/name"].as_str().unwrap_or_default().to_string();
            pod(&p, name)
        });
        let clickhouse = self.list(&path, CLICKHOUSE).await?.into_iter().map(|p| pod(&p, "clickhouse".into()));
        Ok(awzy.chain(clickhouse).collect())
    }
}

fn count(v: &Value) -> u32 {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
}

fn pod(p: &Value, workload: String) -> Pod {
    let statuses = p["status"]["containerStatuses"].as_array().cloned().unwrap_or_default();
    let first = statuses.first().cloned().unwrap_or(Value::Null);
    Pod {
        uid: p["metadata"]["uid"].as_str().unwrap_or_default().to_string(),
        workload,
        ip: p["status"]["podIP"].as_str().map(str::to_string),
        ready: p["status"]["conditions"]
            .as_array()
            .is_some_and(|c| c.iter().any(|c| c["type"] == "Ready" && c["status"] == "True")),
        restarts: statuses.iter().map(|s| count(&s["restartCount"])).sum(),
        started_at: p["status"]["startTime"].as_str().and_then(|t| t.parse().ok()),
        image_digest: first["imageID"].as_str().and_then(digest_of),
        image_tag: first["image"].as_str().and_then(tag_of),
    }
}

/// `sha256:<hex>` from an imageID like `registry/awzy-pdp@sha256:<hex>` or
/// `docker-pullable://…@sha256:<hex>`.
fn digest_of(image_id: &str) -> Option<String> {
    let hex = image_id.rsplit_once("sha256:")?.1;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("sha256:{}", hex.to_ascii_lowercase()))
}

/// The tag of `registry:port/repo:tag`, ignoring a registry port and any digest.
fn tag_of(image: &str) -> Option<String> {
    let image = image.split('@').next()?;
    let last = image.rsplit('/').next()?;
    last.split_once(':').map(|(_, tag)| tag.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_digests_and_tags() {
        let hex = "ab".repeat(32);
        assert_eq!(digest_of(&format!("docker-pullable://reg/awzy-pdp@sha256:{hex}")), Some(format!("sha256:{hex}")));
        assert_eq!(digest_of("reg/awzy-pdp:latest"), None);
        assert_eq!(tag_of("clickhouse/clickhouse-server:25.8"), Some("25.8".into()));
        assert_eq!(tag_of("registry:5000/awzy/awzy-pdp"), None, "a registry port is not a tag");
        assert_eq!(tag_of(&format!("reg/awzy-pdp:abc123@sha256:{hex}")), Some("abc123".into()));
    }
}
