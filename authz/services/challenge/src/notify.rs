//! Telling people what's waiting for them (ADR 0073). Who to tell about what is decided here, as
//! challenges and containments happen, and recorded in `notifications`, once for each channel
//! configured: the client's Slack, and email through their SMTP relay (ADR 0122). A [`Worker`]
//! sends the due ones, retrying each channel's with backoff on its own.
//!
//! Messages say what's waiting and link to the client portal. They never carry document
//! contents, justifications, or the evidence behind a containment, and never approve anything.

use std::time::Duration;

use awzy_model::{ChallengeId, ChallengeKind};
use serde::{Deserialize, Serialize};
use sqlx::{types::Json, PgPool, Row};
use uuid::Uuid;

use crate::{
    email::Email,
    slack::Slack,
    store::{Challenge, ChallengeStore, Containment, StoreError},
};

/// The most attempts at one notification before it's left with its last error.
pub const MOST_ATTEMPTS: i32 = 8;
/// How many due notifications one pass sends, and how often a pass runs.
const BATCH: i64 = 20;
const EVERY: Duration = Duration::from_secs(5);

/// How a notification reaches people.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Slack,
    Email,
}

impl Channel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Slack => "slack",
            Self::Email => "email",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "slack" => Some(Self::Slack),
            "email" => Some(Self::Email),
            _ => None,
        }
    }
}

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

/// What one notification says, as `notifications.details` keeps it (#221). Its `kind` is also a
/// column of its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Notice {
    /// A challenge to answer, or, for an `approver`, a request to approve.
    ChallengeOpened {
        #[serde(flatten)]
        challenge: ChallengeNotice,
        #[serde(default)]
        approver: bool,
    },
    /// An approver's decision, for the person the challenge went to.
    ChallengeDecided {
        #[serde(flatten)]
        challenge: ChallengeNotice,
        approved: bool,
        decided_by: Option<String>,
        /// Whether that left nothing else to pass.
        passed: bool,
    },
    Containment {
        containment_qid: String,
        principal: String,
        created_by: String,
        monitor: bool,
    },
    LiftRequested {
        containment_qid: String,
        principal: String,
        requested_by: String,
        reason: String,
    },
    BreakGlass(BreakGlass),
}

impl Notice {
    fn kind(&self) -> &'static str {
        match self {
            Notice::ChallengeOpened { .. } => "challenge_opened",
            Notice::ChallengeDecided { .. } => "challenge_decided",
            Notice::Containment { .. } => "containment",
            Notice::LiftRequested { .. } => "lift_requested",
            Notice::BreakGlass(_) => "break_glass",
        }
    }
}

/// A challenge, as every message about one names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChallengeNotice {
    pub challenge_qid: String,
    pub principal: String,
    pub on_behalf_of: Option<String>,
    pub action: String,
    pub resource: String,
    pub kinds: Vec<String>,
}

impl ChallengeNotice {
    fn of(c: &Challenge) -> Self {
        Self {
            challenge_qid: c.id.to_string(),
            principal: label(&c.request.principal),
            on_behalf_of: c.request.on_behalf_of.as_ref().map(label),
            action: c.request.action.clone(),
            resource: label(&c.request.resource),
            kinds: c.kinds.iter().map(|k| k.as_str().to_string()).collect(),
        }
    }
}

/// A break-glass rollback (ADR 0022), as the PDP reported it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BreakGlass {
    pub key_id: String,
    pub reason: String,
    pub restored_version: i64,
    pub policy_bundle_qid: String,
    pub version: i64,
}

impl ChallengeStore {
    /// One row for each channel, so a channel that's failing retries without resending the others.
    async fn record(&self, to: &Recipient, notice: &Notice) -> Result<(), StoreError> {
        for channel in &self.channels {
            sqlx::query(
                "INSERT INTO notifications (id, channel, kind, recipient, details) VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(Uuid::now_v7())
            .bind(channel.as_str())
            .bind(notice.kind())
            .bind(to.to_text())
            .bind(Json(notice))
            .execute(self.pool())
            .await?;
        }
        Ok(())
    }

    /// A challenge was just opened: the person it went to answers a justification or a security
    /// key; the resource's owner and the security team hear about an approval.
    pub async fn notify_opened(&self, id: ChallengeId, resource_owner: Option<&str>) -> Result<(), StoreError> {
        if self.channels.is_empty() {
            return Ok(());
        }
        let c = self.get(id).await?;
        let opened = |approver| Notice::ChallengeOpened { challenge: ChallengeNotice::of(&c), approver };
        let for_answerer = c.kinds.contains(ChallengeKind::Justification)
            || c.kinds.contains(ChallengeKind::Verification)
            || c.kinds.contains(ChallengeKind::Biometric);
        if let (true, Some(answerer)) = (for_answerer, c.answerer_id.as_deref()) {
            self.record(&Recipient::Human(answerer.into()), &opened(false)).await?;
        }
        if c.kinds.contains(ChallengeKind::Approval) {
            let parties: Vec<&str> =
                [c.request.principal.id.as_str()].into_iter().chain(c.answerer_id.as_deref()).collect();
            // Never ask someone to approve their own request.
            if let Some(owner) = resource_owner.filter(|o| !parties.contains(o)) {
                self.record(&Recipient::Human(owner.into()), &opened(true)).await?;
            }
            self.record(&Recipient::Security, &opened(true)).await?;
        }
        Ok(())
    }

    /// An approver decided: the person the challenge went to learns whether to retry.
    pub async fn notify_decided(&self, c: &Challenge, approved: bool) -> Result<(), StoreError> {
        let Some(answerer) = c.answerer_id.as_deref().filter(|_| !self.channels.is_empty()) else { return Ok(()) };
        let notice = Notice::ChallengeDecided {
            challenge: ChallengeNotice::of(c),
            approved,
            decided_by: c.decided_by.clone(),
            passed: c.status == crate::ChallengeStatus::Passed,
        };
        self.record(&Recipient::Human(answerer.into()), &notice).await
    }

    /// A break-glass rollback (ADR 0022): the security team, every time.
    pub async fn notify_break_glass(&self, rollback: BreakGlass) -> Result<(), StoreError> {
        if self.channels.is_empty() {
            return Ok(());
        }
        self.record(&Recipient::Security, &Notice::BreakGlass(rollback)).await
    }

    /// Someone asked for a containment to be lifted: the security team decides.
    pub async fn notify_lift_requested(
        &self,
        c: &Containment,
        requested_by: &str,
        reason: &str,
    ) -> Result<(), StoreError> {
        if self.channels.is_empty() {
            return Ok(());
        }
        let notice = Notice::LiftRequested {
            containment_qid: c.id.to_string(),
            principal: label(&c.principal),
            requested_by: requested_by.to_string(),
            reason: reason.to_string(),
        };
        self.record(&Recipient::Security, &notice).await
    }

    /// A containment, applied or only recorded: the security team, never the contained person.
    pub async fn notify_contained(&self, c: &Containment) -> Result<(), StoreError> {
        if self.channels.is_empty() {
            return Ok(());
        }
        let notice = Notice::Containment {
            containment_qid: c.id.to_string(),
            principal: label(&c.principal),
            created_by: c.created_by.clone(),
            monitor: c.monitor,
        };
        self.record(&Recipient::Security, &notice).await
    }
}

/// An email's subject: what's waiting, without the request's details.
pub fn subject(notice: &Notice) -> &'static str {
    match notice {
        Notice::ChallengeOpened { approver: true, .. } => "awzy: an approval is waiting for you",
        Notice::ChallengeOpened { approver: false, .. } => "awzy: a request of yours needs your answer",
        Notice::ChallengeDecided { approved: true, .. } => "awzy: your request was approved",
        Notice::ChallengeDecided { approved: false, .. } => "awzy: your request wasn't approved",
        Notice::Containment { monitor: true, .. } => "awzy: a containment in monitor",
        Notice::Containment { monitor: false, .. } => "awzy: a containment",
        Notice::LiftRequested { .. } => "awzy: a request to lift a containment",
        Notice::BreakGlass(_) => "awzy: a break-glass policy rollback",
    }
}

/// The text of one message, with its link into the client portal.
pub fn render(notice: &Notice, portal_url: &str) -> String {
    let portal = portal_url.trim_end_matches('/');
    let request = |c: &ChallengeNotice| {
        let who = match &c.on_behalf_of {
            Some(human) => format!("{} for {human}", c.principal),
            None => c.principal.clone(),
        };
        format!("{who} wants to {} {}", c.action, c.resource)
    };
    let challenges_link = |c: &ChallengeNotice| format!("{portal}/challenges?challenge={}", c.challenge_qid);
    match notice {
        Notice::ChallengeOpened { challenge: c, approver: true } => {
            format!("awzy asks for an approval. {}. Review it: {}", request(c), challenges_link(c))
        }
        Notice::ChallengeOpened { challenge: c, approver: false } => {
            let has = |k: &str| c.kinds.iter().any(|x| x == k);
            let what = match (has("justification"), has("verification")) {
                (true, true) => "a reason and your security key",
                (true, false) => "a reason",
                _ => "your security key",
            };
            format!("awzy needs {what} before this can go ahead: {}. Answer it: {}", request(c), challenges_link(c))
        }
        Notice::ChallengeDecided { challenge: c, approved, decided_by, passed } => {
            let by = decided_by.as_ref().map(|b| format!(" by {b}")).unwrap_or_default();
            let (request, link) = (request(c), challenges_link(c));
            match (approved, passed) {
                (true, true) => {
                    format!("Approved{by}: {request}. Retry it now; the approval holds for a while. {link}")
                }
                (true, false) => format!("Approved{by}: {request}. Something else is still needed: {link}"),
                (false, _) => format!("Not approved{by}: {request}. {link}"),
            }
        }
        Notice::Containment { principal, created_by, monitor, .. } => {
            let link = format!("{portal}/challenges");
            if *monitor {
                format!("awzy would have contained {principal} (automated containments are in monitor). Review it: {link}")
            } else {
                format!("{principal} is contained by {created_by}. Review it: {link}")
            }
        }
        Notice::LiftRequested { principal, requested_by, reason, .. } => format!(
            "{requested_by} asks to lift the containment of {principal}: \"{reason}\". Lift it, or leave it, on the Challenges page: {portal}/challenges"
        ),
        Notice::BreakGlass(b) => format!(
            "Break-glass: key {} rolled policy back to version {} (now active as version {}), outside the \
             portal and your IdP. Reason given: \"{}\". Review it: {portal}/policies",
            b.key_id, b.restored_version, b.version, b.reason
        ),
    }
}

/// Sends due notifications through each channel configured.
pub struct Worker {
    pub pool: PgPool,
    pub slack: Option<Slack>,
    pub email: Option<Email>,
    pub portal_url: String,
    /// awzy-context, for the email a person's IdP provisioned (#65). Without it, only people
    /// whose `userName` is an email address are reached.
    pub contacts: Option<awzy_context_api::ContextApi>,
}

impl Worker {
    /// Where to message `user_name`: the email awzy-context has for them, else the name itself.
    /// A lookup that fails falls back too, and the send says why it can't reach them.
    async fn address_of(&self, user_name: &str) -> String {
        let Some(contacts) = &self.contacts else { return user_name.to_string() };
        match contacts.human_contact(user_name).await {
            Ok(Some(c)) => c.email.unwrap_or_else(|| user_name.to_string()),
            Ok(None) => user_name.to_string(),
            Err(error) => {
                tracing::warn!(%error, "couldn't read a person's email from awzy-context; trying their userName");
                user_name.to_string()
            }
        }
    }

    /// Sends one notification through its channel.
    async fn send(&self, channel: Channel, to: Recipient, notice: &Notice) -> anyhow::Result<()> {
        let text = render(notice, &self.portal_url);
        match (channel, &self.slack, &self.email) {
            (Channel::Slack, Some(slack), _) => match to {
                Recipient::Human(id) => slack.to_human(&self.address_of(&id).await, &text).await,
                Recipient::Security => slack.to_security(&text).await,
            },
            (Channel::Email, _, Some(email)) => match to {
                Recipient::Human(id) => email.to_human(&self.address_of(&id).await, subject(notice), &text).await,
                Recipient::Security => email.to_security(subject(notice), &text).await,
            },
            // Recorded while the channel was on, and it's been turned off since.
            _ => Err(anyhow::anyhow!("{} isn't configured any more", channel.as_str())),
        }
    }

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
            "SELECT id, channel, kind, recipient, details, attempts FROM notifications
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
            let attempts: i32 = row.try_get("attempts")?;
            let notice: Result<Json<Notice>, _> = row.try_get("details");
            let channel = Channel::parse(row.try_get("channel")?);
            let outcome = match (notice, channel, Recipient::parse(row.try_get("recipient")?)) {
                (Err(e), _, _) => Err(anyhow::anyhow!("unreadable notification: {e}")),
                (Ok(n), Some(channel), Some(to)) => self.send(channel, to, &n.0).await,
                (Ok(_), None, _) => Err(anyhow::anyhow!("unknown channel")),
                (Ok(_), _, None) => Err(anyhow::anyhow!("unknown recipient")),
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

    fn challenge(kinds: &[&str]) -> ChallengeNotice {
        ChallengeNotice {
            challenge_qid: "challenge_1".into(),
            principal: "ServiceAccount::sales-genie".into(),
            on_behalf_of: Some("Human::nora".into()),
            action: "read".into(),
            resource: "Resource::plan".into(),
            kinds: kinds.iter().map(|k| k.to_string()).collect(),
        }
    }

    fn opened(kinds: &[&str], approver: bool) -> Notice {
        Notice::ChallengeOpened { challenge: challenge(kinds), approver }
    }

    #[test]
    fn messages_say_whats_waiting_and_link_to_the_portal() {
        let portal = "https://awzy.example.com/";
        let reason = render(&opened(&["justification"], false), portal);
        assert!(reason.contains("needs a reason"), "{reason}");
        assert!(
            reason.contains("ServiceAccount::sales-genie for Human::nora wants to read Resource::plan"),
            "{reason}"
        );
        assert!(reason.ends_with("https://awzy.example.com/challenges?challenge=challenge_1"), "{reason}");
        let key = render(&opened(&["verification"], false), portal);
        assert!(key.contains("your security key"), "{key}");
        let approval = render(&opened(&["approval"], true), portal);
        assert!(approval.starts_with("awzy asks for an approval"), "{approval}");
    }

    #[test]
    fn a_break_glass_rollback_tells_the_security_team_what_and_why() {
        let rollback = BreakGlass {
            key_id: "sha256:0123456789abcdef".into(),
            reason: "forbid-all locked us out".into(),
            restored_version: 4,
            policy_bundle_qid: "policy_bundle_1".into(),
            version: 6,
        };
        let text = render(&Notice::BreakGlass(rollback), "https://p/");
        assert!(text.starts_with("Break-glass: key sha256:0123456789abcdef rolled policy back to version 4"), "{text}");
        assert!(text.contains("now active as version 6") && text.contains("\"forbid-all locked us out\""), "{text}");
        assert!(text.ends_with("https://p/policies"), "{text}");
    }

    #[test]
    fn a_lift_request_says_who_asks_and_why() {
        let lift = Notice::LiftRequested {
            containment_qid: "containment_1".into(),
            principal: "Human::nora".into(),
            requested_by: "ServiceAccount::ops-bot for Human::alice".into(),
            reason: "reviewed: a false positive".into(),
        };
        let text = render(&lift, "https://p");
        assert!(
            text.starts_with("ServiceAccount::ops-bot for Human::alice asks to lift the containment of Human::nora"),
            "{text}"
        );
        assert!(text.contains("\"reviewed: a false positive\"") && text.ends_with("https://p/challenges"), "{text}");
    }

    #[test]
    fn decisions_and_containments_read_plainly() {
        let decided = |approved, passed| Notice::ChallengeDecided {
            challenge: challenge(&["approval"]),
            approved,
            decided_by: Some("Human::priya".into()),
            passed,
        };
        assert!(render(&decided(false, false), "https://p").starts_with("Not approved by Human::priya"));
        assert!(render(&decided(true, true), "https://p").contains("Retry it now"));

        let would = Notice::Containment {
            containment_qid: "containment_1".into(),
            principal: "Human::nora".into(),
            created_by: "AwzyService::awzy-reasoner".into(),
            monitor: true,
        };
        assert!(render(&would, "https://p").contains("would have contained Human::nora"));
    }

    /// What's stored reads back as it was written, its kind beside the rest.
    #[test]
    fn a_notice_round_trips_through_its_stored_form() {
        let notice = opened(&["approval"], true);
        let stored = serde_json::to_value(&notice).unwrap();
        assert_eq!(
            (stored["kind"].as_str(), stored["challenge_qid"].as_str()),
            (Some("challenge_opened"), Some("challenge_1"))
        );
        assert_eq!(serde_json::from_value::<Notice>(stored).unwrap(), notice);
    }

    #[test]
    fn an_email_subject_says_whats_waiting_and_nothing_more() {
        assert_eq!(subject(&opened(&["approval"], true)), "awzy: an approval is waiting for you");
        assert_eq!(subject(&opened(&["justification"], false)), "awzy: a request of yours needs your answer");
        assert!(!subject(&opened(&["justification"], false)).contains("Resource::plan"));
    }

    #[test]
    fn recipients_and_channels_round_trip() {
        for c in [Channel::Slack, Channel::Email] {
            assert_eq!(Channel::parse(c.as_str()), Some(c));
        }
        for r in [Recipient::Human("nora@larkspur.example".into()), Recipient::Security] {
            assert_eq!(Recipient::parse(&r.to_text()), Some(r));
        }
        assert_eq!(Recipient::parse("human:"), None);
    }

    /// With the local stack: notifications are recorded only when turned on, for whom, and once
    /// for each channel.
    #[tokio::test]
    async fn an_approval_is_recorded_for_the_owner_and_the_security_team() {
        use crate::store::{tests::test_store, Lifetimes, RequestKey};
        use awzy_model::{Challenges, DecisionId, EntityRef};

        let Some(store) = test_store(Lifetimes::default()).await else { return };
        let store = store.with_notifications(vec![Channel::Slack, Channel::Email]);
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

        let recipients: Vec<String> =
            sqlx::query_scalar("SELECT channel || ' ' || recipient FROM notifications ORDER BY channel, recipient")
                .fetch_all(store.pool())
                .await
                .unwrap();
        assert_eq!(
            recipients,
            vec![
                "email human:nora",
                "email human:priya",
                "email security",
                "slack human:nora",
                "slack human:priya",
                "slack security"
            ]
        );
    }
}
