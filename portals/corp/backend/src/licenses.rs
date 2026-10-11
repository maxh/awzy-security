//! Licenses and entitlements (#142): what each client is licensed for, which staff set on the
//! install page. One per client, shared by all its installs. The corp portal only records them;
//! nothing in an install reads them yet.

use async_graphql::{Context, Enum, InputObject, Object, Result};
use awzy_portal::{Actor, ErrorCode, UtcTimestamp};
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgRow, Row};

use crate::{auth::Staff, fleet::FleetStore};

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum LicensePlan {
    Trial,
    Standard,
    Enterprise,
}

impl LicensePlan {
    fn as_str(self) -> &'static str {
        match self {
            Self::Trial => "trial",
            Self::Standard => "standard",
            Self::Enterprise => "enterprise",
        }
    }

    fn parse(s: &str) -> Result<Self, sqlx::Error> {
        match s {
            "trial" => Ok(Self::Trial),
            "standard" => Ok(Self::Standard),
            "enterprise" => Ok(Self::Enterprise),
            other => Err(sqlx::Error::Decode(format!("unknown plan {other:?}").into())),
        }
    }
}

/// A feature beyond the base product, which a license may include.
#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, PartialOrd, Ord)]
pub enum Entitlement {
    /// Awzy Bot drafting policy in the client portal (ADR 0048).
    AwzyBot,
    /// awzy-reasoner's investigations with Claude (ADR 0071).
    Investigations,
    /// The Google Drive connector (ADR 0069).
    Drive,
    /// The Jira connector (ADR 0070).
    Jira,
}

impl Entitlement {
    const ALL: [Entitlement; 4] = [Self::AwzyBot, Self::Investigations, Self::Drive, Self::Jira];

    fn as_str(self) -> &'static str {
        match self {
            Self::AwzyBot => "awzy_bot",
            Self::Investigations => "investigations",
            Self::Drive => "drive",
            Self::Jira => "jira",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|e| e.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LicenseRecord {
    pub client: String,
    pub plan: LicensePlan,
    pub humans: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub entitlements: Vec<Entitlement>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum LicenseError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

const COLUMNS: &str = "client, plan, humans, expires_at, entitlements, updated_by, updated_at";

fn from_row(row: &PgRow) -> Result<LicenseRecord, sqlx::Error> {
    let entitlements: Vec<String> = row.try_get("entitlements")?;
    // One a later release added and this one doesn't know is left out, not an error.
    let mut entitlements: Vec<Entitlement> = entitlements.iter().filter_map(|e| Entitlement::parse(e)).collect();
    entitlements.sort();
    Ok(LicenseRecord {
        client: row.try_get("client")?,
        plan: LicensePlan::parse(row.try_get("plan")?)?,
        humans: row.try_get("humans")?,
        expires_at: row.try_get("expires_at")?,
        entitlements,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl FleetStore {
    /// `client`'s license, if staff have set one.
    pub async fn license(&self, client: &str) -> Result<Option<LicenseRecord>, sqlx::Error> {
        let row = sqlx::query(&format!("SELECT {COLUMNS} FROM licenses WHERE client = $1"))
            .bind(client)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(from_row).transpose()
    }

    /// Sets `client`'s license, replacing any it had. The client must have an active install.
    pub async fn set_license(
        &self,
        client: &str,
        plan: LicensePlan,
        humans: i32,
        expires_at: Option<DateTime<Utc>>,
        entitlements: &[Entitlement],
        by: &str,
    ) -> Result<LicenseRecord, LicenseError> {
        if humans <= 0 {
            return Err(LicenseError::Invalid("a license covers at least one person".into()));
        }
        let known: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM installs WHERE client = $1 AND retired_at IS NULL)")
                .bind(client)
                .fetch_one(self.pool())
                .await?;
        if !known {
            return Err(LicenseError::Invalid(format!("{client:?} has no active install")));
        }
        let mut names: Vec<&str> = entitlements.iter().map(|e| e.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        let row = sqlx::query(&format!(
            "INSERT INTO licenses (client, plan, humans, expires_at, entitlements, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (client) DO UPDATE SET plan = $2, humans = $3, expires_at = $4, entitlements = $5,
                 updated_by = $6, updated_at = now()
             RETURNING {COLUMNS}"
        ))
        .bind(client)
        .bind(plan.as_str())
        .bind(humans)
        .bind(expires_at)
        .bind(&names)
        .bind(by)
        .fetch_one(self.pool())
        .await?;
        Ok(from_row(&row)?)
    }
}

/// What a client is licensed for.
pub struct License(pub LicenseRecord);

#[Object]
impl License {
    async fn client(&self) -> &str {
        &self.0.client
    }

    async fn plan(&self) -> LicensePlan {
        self.0.plan
    }

    /// How many people it covers.
    async fn humans(&self) -> i32 {
        self.0.humans
    }

    /// Null when it doesn't end.
    async fn expires_at(&self) -> Option<UtcTimestamp> {
        self.0.expires_at.map(UtcTimestamp)
    }

    /// Whether it has ended.
    async fn expired(&self) -> bool {
        self.0.expires_at.is_some_and(|t| t <= Utc::now())
    }

    /// Features beyond the base product.
    async fn entitlements(&self) -> &[Entitlement] {
        &self.0.entitlements
    }

    async fn updated_by(&self) -> Actor {
        Actor(awzy_model::Actor::awzy_staff(&self.0.updated_by))
    }

    async fn updated_at(&self) -> UtcTimestamp {
        UtcTimestamp(self.0.updated_at)
    }
}

#[derive(InputObject)]
pub struct SetLicenseInput {
    /// The client's slug, as its installs carry it, e.g. acme.
    pub client: String,
    pub plan: LicensePlan,
    /// How many people it covers: at least one.
    pub humans: i32,
    /// When it ends; null for never.
    pub expires_at: Option<UtcTimestamp>,
    #[graphql(default)]
    pub entitlements: Vec<Entitlement>,
}

#[derive(Default)]
pub struct LicenseMutation;

#[Object]
impl LicenseMutation {
    /// Sets a client's license, replacing any it had. Shared by all the client's installs.
    async fn set_license(&self, ctx: &Context<'_>, input: SetLicenseInput) -> Result<License> {
        let staff = ctx.data::<Staff>().map_err(|_| ErrorCode::Unauthenticated.error("sign in first"))?;
        let store = ctx
            .data::<FleetStore>()
            .map_err(|_| ErrorCode::Unavailable.error("Postgres is not configured (AWZY_POSTGRES_URL)"))?;
        let record = store
            .set_license(
                &input.client,
                input.plan,
                input.humans,
                input.expires_at.map(|t| t.0),
                &input.entitlements,
                &staff.username,
            )
            .await
            .map_err(|e| match e {
                LicenseError::Invalid(m) => async_graphql::Error::new(m),
                LicenseError::Db(e) => e.into(),
            })?;
        tracing::warn!(client = %record.client, plan = record.plan.as_str(), humans = record.humans, by = %staff.username, "license set");
        Ok(License(record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet::tests::{new_install, test_store};

    #[tokio::test]
    async fn a_client_has_one_license_its_installs_share() {
        let Some(store) = test_store().await else { return };
        store.register(&new_install("acme", "prod"), "max").await.unwrap();
        assert!(store.license("acme").await.unwrap().is_none());

        let set = store
            .set_license("acme", LicensePlan::Trial, 25, None, &[Entitlement::Jira, Entitlement::Drive], "max")
            .await
            .unwrap();
        assert_eq!(
            (set.plan, set.humans, set.entitlements.clone()),
            (LicensePlan::Trial, 25, vec![Entitlement::Drive, Entitlement::Jira])
        );
        let renewed = store.set_license("acme", LicensePlan::Enterprise, 500, None, &[], "bob").await.unwrap();
        assert_eq!(
            (renewed.plan, renewed.updated_by.as_str()),
            (LicensePlan::Enterprise, "bob"),
            "setting it again replaces it"
        );
        assert_eq!(store.license("acme").await.unwrap(), Some(renewed));

        assert!(matches!(
            store.set_license("acme", LicensePlan::Standard, 0, None, &[], "max").await,
            Err(LicenseError::Invalid(_))
        ));
        assert!(
            matches!(
                store.set_license("globex", LicensePlan::Standard, 10, None, &[], "max").await,
                Err(LicenseError::Invalid(_))
            ),
            "only a client with an install"
        );
    }
}
