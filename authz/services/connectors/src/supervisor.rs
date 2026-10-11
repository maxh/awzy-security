//! Runs each connector as the client portal configured it (ADR 0104). Every minute it reads the
//! connectors' settings from awzy-context and their credentials from the files External Secrets
//! keeps current, and starts, restarts, or stops each connector to match. A connector whose
//! settings or credential changed starts again from a full crawl, as at startup.

use std::{collections::HashMap, sync::Arc, time::Duration};

use serde_json::Value;
use tokio::task::JoinHandle;

use crate::{
    context::ContextClient,
    drive::{Credentials, DriveConfig, DriveSync},
    google::Auth,
    http::Http,
    jira::{JiraConfig, JiraSync},
    status,
};

const EVERY: Duration = Duration::from_secs(60);

/// Where each connector's credential is mounted.
#[derive(Debug, Clone)]
pub struct CredentialFiles {
    pub drive_key: String,
    pub jira_token: String,
}

/// What a connector should be running as: its settings and credential, compared whole.
#[derive(Debug, Clone, PartialEq)]
enum Want {
    Off,
    /// Configured, but its credential hasn't reached the cluster yet.
    NoCredential(Value),
    Run {
        settings: Value,
        credential: String,
    },
}

fn want(all: &[Value], name: &str, credential: Option<String>) -> Want {
    let Some(c) = all.iter().find(|c| c["name"] == name) else { return Want::Off };
    if c["enabled"] != true {
        return Want::Off;
    }
    let settings = c["settings"].clone();
    match credential.filter(|t| !t.trim().is_empty()) {
        Some(credential) => Want::Run { settings, credential },
        None => Want::NoCredential(settings),
    }
}

fn read(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

pub struct Supervisor {
    context: ContextClient,
    http: Http,
    files: CredentialFiles,
    running: HashMap<&'static str, (Want, Option<JoinHandle<()>>)>,
}

impl Supervisor {
    pub fn new(context: ContextClient, http: Http, files: CredentialFiles) -> Self {
        Self { context, http, files, running: HashMap::new() }
    }

    /// Forever: brings the connectors in line with their settings, every minute.
    pub async fn run(mut self) {
        loop {
            match self.context.connector_settings().await {
                Ok(all) => {
                    let drive = want(&all, "drive", read(&self.files.drive_key));
                    self.apply("drive", drive);
                    let jira = want(&all, "jira", read(&self.files.jira_token));
                    self.apply("jira", jira);
                }
                // Keep running as before: awzy-context being away is no reason to stop.
                Err(e) => tracing::warn!(error = format!("{e:#}"), "couldn't read connector settings"),
            }
            tokio::time::sleep(EVERY).await;
        }
    }

    fn apply(&mut self, name: &'static str, want: Want) {
        if self.running.get(name).is_some_and(|(w, _)| *w == want) {
            return;
        }
        if let Some((_, Some(task))) = self.running.remove(name) {
            task.abort();
        }
        status::unregister(name);
        let task = match &want {
            Want::Off => {
                tracing::info!(connector = name, "connector off");
                None
            }
            Want::NoCredential(settings) => {
                register(name, settings);
                status::failed(name, &anyhow::anyhow!("waiting for its credential; set it in the client portal"));
                None
            }
            Want::Run { settings, credential } => match self.start(name, settings, credential) {
                Ok(task) => {
                    register(name, settings);
                    Some(task)
                }
                Err(e) => {
                    register(name, settings);
                    status::failed(name, &e);
                    None
                }
            },
        };
        self.running.insert(name, (want, task));
    }

    fn start(&self, name: &'static str, settings: &Value, credential: &str) -> anyhow::Result<JoinHandle<()>> {
        let (http, context) = (self.http.clone(), self.context.clone());
        match name {
            "drive" => {
                let cfg = DriveConfig::from_settings(settings)?;
                let auth = Auth::from_json(credential, http.clone())?;
                tracing::info!(subject = %cfg.subject, domain = %cfg.domain, "syncing Google Drive");
                let credentials = Arc::new(Credentials::ServiceAccount(Box::new(auth)));
                Ok(tokio::spawn(DriveSync::new(cfg, credentials, http, context).run()))
            }
            "jira" => {
                let cfg = JiraConfig::from_settings(settings, credential)?;
                tracing::info!(site = %cfg.base_url, projects = ?cfg.projects, "syncing Jira");
                Ok(tokio::spawn(JiraSync::new(cfg, http, context).run()))
            }
            other => anyhow::bail!("{other} isn't a connector"),
        }
    }
}

/// The status page's view of `name`'s settings, by label.
fn register(name: &'static str, s: &Value) {
    let text = |k: &str| s[k].as_str().unwrap_or_default().to_string();
    let interval = s["interval_secs"].as_u64().unwrap_or(300);
    let settings = match name {
        "drive" => {
            let mut m = std::collections::BTreeMap::from([("Acts as", text("subject"))]);
            if !text("domain").is_empty() {
                m.insert("Domain", text("domain"));
            }
            if !text("admin_subject").is_empty() {
                m.insert("Reads groups as", text("admin_subject"));
            }
            m
        }
        _ => {
            let projects: Vec<&str> =
                s["projects"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let projects =
                if projects.is_empty() { "every project it can browse".to_string() } else { projects.join(", ") };
            let watchers = if s["watchers"] == true { "yes" } else { "no" }.to_string();
            std::collections::BTreeMap::from([
                ("Site", text("url")),
                ("Account", text("user")),
                ("Projects", projects),
                ("Watchers count as working on an issue", watchers),
            ])
        }
    };
    status::register(name, settings, interval);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_connector_runs_only_when_enabled_and_its_credential_is_there() {
        let settings = json!({ "url": "https://acme.atlassian.net", "user": "bot" });
        let all = vec![json!({ "name": "jira", "enabled": true, "settings": settings })];
        assert_eq!(want(&all, "drive", Some("{}".into())), Want::Off, "not configured");
        assert_eq!(want(&all, "jira", None), Want::NoCredential(settings.clone()));
        assert_eq!(want(&all, "jira", Some(" \n".into())), Want::NoCredential(settings.clone()), "an empty file");
        assert_eq!(
            want(&all, "jira", Some("token".into())),
            Want::Run { settings: settings.clone(), credential: "token".into() }
        );
        let off = vec![json!({ "name": "jira", "enabled": false, "settings": settings })];
        assert_eq!(want(&off, "jira", Some("token".into())), Want::Off);
    }
}
