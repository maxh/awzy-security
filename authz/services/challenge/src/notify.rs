//! Telling people what's waiting for them (ADR 0073). Who to tell about what is decided here, as
//! challenges and containments happen, and recorded in `notifications`; a [`Worker`] sends the due
//! ones through the client's Slack, retrying with backoff.
//!
//! Messages say what's waiting and link to the client portal. They never carry document
//! contents, justifications, or the evidence behind a containment, and never approve anything.

use std::time::Duration;

use awzy_model::{ChallengeId, ChallengeKind};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{
    slack::Slack,
    store::{Challenge, ChallengeStore, Containment, StoreError},
};

/// The most attempts at one notification before it's left with its last error.
pub const MOST_ATTEMPTS: i32 = 8;
/// How many due notifications one pass sends, and how often a pass runs.
const BATCH: i64 = 20;
const EVERY: Duration = Duration::from_secs(5);

/// Who a notification is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipient {
    /// A person, by `userName`.
    Human(String),
    /// The security team's channel.
    Security,
}

impl Recipient {
    fn to_text(&self) -> String {
        match self {
            Self::Human(id) => format!("human:{id}"),
            Self::Security => "security".into(),
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.split_once(':') {
            Some(("human", id)) if !id.is_empty() => Some(Self::Human(id.to_string())),
            _ => (s == "security").then_some(Self::Security),
        }
    }
}

fn label(r: &awzy_model::EntityRef) -> String {
    format!("{}::{}", r.kind, r.id)
}

/// The details every challenge message needs.
fn challenge_details(c: &Challenge) -> Value {
    json!({
        "challenge_qid": c.id.to_string(),
        "principal": label(&c.request.principal),
        "on_behalf_of": c.request.on_behalf_of.as_ref().map(label),
        "action": c.request.action,
        "resource": label(&c.request.resource),
        "kinds": c.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
    })
}

impl ChallengeStore {
    async fn record(&self, kind: &str, to: &Recipient, details: &Value) -> Result<(), StoreError> {
        sqlx::query("INSERT INTO notifications (id, kind, recipient, details) VALUES ($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(kind)
            .bind(to.to_text())
            .bind(sqlx::types::Json(details))
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// A challenge was just opened: the person it went to answers a justification or a security
    /// key; the resource's owner and the security team hear about an approval.
    pub async fn notify_opened(&self, id: ChallengeId, resource_owner: Option<&str>) -> Result<(), StoreError> {
        if !self.notify {
            return Ok(());
        }
        let c = self.get(id).await?;
        let details = challenge_details(&c);
        let for_answerer = c.kinds.contains(ChallengeKind::Justification)
            || c.kinds.contains(ChallengeKind::Verification)
            || c.kinds.contains(ChallengeKind::Biometric);
        if let (true, Some(answerer)) = (for_answerer, c.answerer_id.as_deref()) {
            self.record("challenge_opened", &Recipient::Human(answerer.into()), &details).await?;
        }
        if c.kinds.contains(ChallengeKind::Approval) {
            let parties: Vec<&str> =
                [c.request.principal.id.as_str()].into_iter().chain(c.answerer_id.as_deref()).collect();
            // Never ask someone to approve their own request.
            if let Some(owner) = resource_owner.filter(|o| !parties.contains(o)) {
                let mut details = details.clone();
                details["approver"] = json!(true);
                self.record("challenge_opened", &Recipient::Human(owner.into()), &details).await?;
            }
            let mut details = details;
            details["approver"] = json!(true);
            self.record("challenge_opened", &Recipient::Security, &details).await?;
        }
        Ok(())
    }

    /// An approver decided: the person the challenge went to learns whether to retry.
    pub async fn notify_decided(&self, c: &Challenge, approved: bool) -> Result<(), StoreError> {
        let Some(answerer) = c.answerer_id.as_deref().filter(|_| self.notify) else { return Ok(()) };
        let mut details = challenge_details(c);
        details["approved"] = json!(approved);
        details["decided_by"] = json!(c.decided_by);
        details["passed"] = json!(c.status == crate::ChallengeStatus::Passed);
        self.record("challenge_decided", &Recipient::Human(answerer.into()), &details).await
    }

    /// A containment, applied or only recorded: the security team, never the contained person.
    pub async fn notify_contained(&self, c: &Containment) -> Result<(), StoreError> {
        if !self.notify {
            return Ok(());
        }
        let details = json!({
            "containment_qid": c.id.to_string(),
            "principal": label(&c.principal),
            "created_by": c.created_by,
            "monitor": c.monitor,
        });
        self.record("containment", &Recipient::Security, &details).await
    }
}

/// The text of one message, with its link into the client portal.
pub fn render(kind: &str, details: &Value, portal_url: &str) -> String {
    let portal = portal_url.trim_end_matches('/');
    let s = |k: &str| details[k].as_str().unwrap_or("").to_string();
    let challenges_link = format!("{portal}/challenges?challenge={}", s("challenge_qid"));
    let who = match details["on_behalf_of"].as_str() {
        Some(human) => format!("{} for {human}", s("principal")),
        None => s("principal"),
    };
    let request = format!("{who} wants to {} {}", s("action"), s("resource"));
    match kind {
        "challenge_opened" if details["approver"] == true => {
            format!("awzy asks for an approval. {request}. Review it: {challenges_link}")
        }
        "challenge_opened" => {
            let kinds: Vec<&str> =
                details["kinds"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let what = match (kinds.contains(&"justification"), kinds.contains(&"verification")) {
                (true, true) => "a reason and your security key",
                (true, false) => "a reason",
                _ => "your security key",
            };
            format!("awzy needs {what} before this can go ahead: {request}. Answer it: {challenges_link}")
        }
        "challenge_decided" => {
            let by = details["decided_by"].as_str().map(|b| format!(" by {b}")).unwrap_or_default();
            if details["approved"] == true && details["passed"] == true {
                format!("Approved{by}: {request}. Retry it now; the approval holds for a while. {challenges_link}")
            } else if details["approved"] == true {
                format!("Approved{by}: {request}. Something else is still needed: {challenges_link}")
            } else {
                format!("Not approved{by}: {request}. {challenges_link}")
            }
        }
        "containment" => {
            let link = format!("{portal}/challenges");
            if details["monitor"] == true {
                format!(
                    "awzy would have contained {} (automated containments are in monitor). Review it: {link}",
                    s("principal")
                )
            } else {
                format!("{} is contained by {}. Review it: {link}", s("principal"), s("created_by"))
            }
        }
        other => format!("awzy: {other}. {portal}"),
    }
}

/// Sends due notifications through Slack.
pub struct Worker {
    pub pool: PgPool,
    pub slack: Slack,
    pub portal_url: String,
}

impl Worker {
    /// Sends what's due, every few seconds, for as long as the service runs.
    pub async fn run(self) {
        let mut ticks = tokio::time::interval(EVERY);
        loop {
            ticks.tick().await;
            if let Err(e) = self.pass().await {
                tracing::warn!(error = %e, "sending notifications failed; trying again shortly");
            }
        }
    }

    /// One pass: claims up to a batch of due notifications and sends each.
    pub async fn pass(&self) -> anyhow::Result<usize> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query(
            "SELECT id, kind, recipient, details, attempts FROM notifications
             WHERE sent_at IS NULL AND next_attempt_at <= now() AND attempts < $1
             ORDER BY created_at LIMIT $2 FOR UPDATE SKIP LOCKED",
        )
        .bind(MOST_ATTEMPTS)
        .bind(BATCH)
        .fetch_all(&mut *tx)
        .await?;
        let mut sent = 0;
        for row in &rows {
            let id: Uuid = row.try_get("id")?;
            let kind: String = row.try_get("kind")?;
            let attempts: i32 = row.try_get("attempts")?;
            let details: sqlx::types::Json<Value> = row.try_get("details")?;
            let text = render(&kind, &details.0, &self.portal_url);
            let outcome = match Recipient::parse(row.try_get("recipient")?) {
                Some(Recipient::Human(id)) => self.slack.to_human(&id, &text).await,
                Some(Recipient::Security) => self.slack.to_security(&text).await,
                None => Err(anyhow::anyhow!("unknown recipient")),
            };
            match outcome {
                Ok(()) => {
                    sqlx::query("UPDATE notifications SET sent_at = now(), attempts = attempts + 1 WHERE id = $1")
                        .bind(id)
                        .execute(&mut *tx)
                        .await?;
                    sent += 1;
                }
                Err(e) => {
                    // 30 s, 1 min, 2 min, … up to about an hour.
                    let backoff = 30.0 * 2f64.powi(attempts.min(7));
                    sqlx::query(
                        "UPDATE notifications SET attempts = attempts + 1, last_error = $2,
                            next_attempt_at = now() + make_interval(secs => $3) WHERE id = $1",
                    )
                    .bind(id)
                    .bind(e.to_string())
                    .bind(backoff)
                    .execute(&mut *tx)
                    .await?;
                    tracing::warn!(notification = %id, error = %e, attempt = attempts + 1, "notification not sent");
                }
            }
        }
        tx.commit().await?;
        Ok(sent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened(kinds: &[&str], approver: bool) -> Value {
        json!({ "challenge_qid": "challenge_1", "principal": "ServiceAccount::sales-genie",
                "on_behalf_of": "Human::nora", "action": "read", "resource": "Resource::plan",
                "kinds": kinds, "approver": approver })
    }

    #[test]
    fn messages_say_whats_waiting_and_link_to_the_portal() {
        let portal = "https://awzy.example.com/";
        let reason = render("challenge_opened", &opened(&["justification"], false), portal);
        assert!(reason.contains("needs a reason"), "{reason}");
        assert!(
            reason.contains("ServiceAccount::sales-genie for Human::nora wants to read Resource::plan"),
            "{reason}"
        );
        assert!(reason.ends_with("https://awzy.example.com/challenges?challenge=challenge_1"), "{reason}");
        let key = render("challenge_opened", &opened(&["verification"], false), portal);
        assert!(key.contains("your security key"), "{key}");
        let approval = render("challenge_opened", &opened(&["approval"], true), portal);
        assert!(approval.starts_with("awzy asks for an approval"), "{approval}");
    }

    #[test]
    fn decisions_and_containments_read_plainly() {
        let mut d = opened(&["approval"], false);
        d["approved"] = json!(false);
        d["decided_by"] = json!("Human::priya");
        assert!(render("challenge_decided", &d, "https://p").starts_with("Not approved by Human::priya"));
        d["approved"] = json!(true);
        d["passed"] = json!(true);
        assert!(render("challenge_decided", &d, "https://p").contains("Retry it now"));

        let would = json!({ "containment_qid": "containment_1", "principal": "Human::nora",
                            "created_by": "AwzyService::awzy-reasoner", "monitor": true });
        assert!(render("containment", &would, "https://p").contains("would have contained Human::nora"));
    }

    #[test]
    fn recipients_round_trip() {
        for r in [Recipient::Human("nora@larkspur.example".into()), Recipient::Security] {
            assert_eq!(Recipient::parse(&r.to_text()), Some(r));
        }
        assert_eq!(Recipient::parse("human:"), None);
    }

    /// With the local stack: notifications are recorded only when turned on, and for whom.
    #[tokio::test]
    async fn an_approval_is_recorded_for_the_owner_and_the_security_team() {
        use crate::store::{tests::test_store, Lifetimes, RequestKey};
        use awzy_model::{Challenges, DecisionId, EntityRef};

        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let store = store.with_notifications();
        let request = RequestKey {
            principal: EntityRef::new("ServiceAccount", "sales-genie"),
            on_behalf_of: Some(EntityRef::new("Human", "nora")),
            action: "read".into(),
            resource: EntityRef::new("Resource", "plan"),
        };
        let required = Challenges::of(ChallengeKind::Justification).union(Challenges::of(ChallengeKind::Approval));
        let r = store.resolve(DecisionId::new(), &request, required, Some("nora"), true).await.unwrap();
        assert!(r.opened);
        store.notify_opened(r.challenge_id.unwrap(), Some("priya")).await.unwrap();
        // A retry finds it open, and tells no one again.
        let again = store.resolve(DecisionId::new(), &request, required, Some("nora"), true).await.unwrap();
        assert_eq!((again.challenge_id, again.opened), (r.challenge_id, false));

        let recipients: Vec<String> = sqlx::query_scalar("SELECT recipient FROM notifications ORDER BY recipient")
            .fetch_all(store.pool())
            .await
            .unwrap();
        assert_eq!(recipients, vec!["human:nora", "human:priya", "security"]);
    }
}
