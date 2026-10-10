//! Postgres storage for people, groups, and service accounts, and the snapshot the PDPs load
//! (ADR 0031, ADR 0033).

use std::collections::{BTreeMap, HashMap};

use awzy_model::entity_type;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

fn conflict(e: sqlx::Error, what: String) -> StoreError {
    match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => StoreError::Conflict(what),
        e => StoreError::Db(e),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UserInput {
    pub user_name: String,
    pub external_id: Option<String>,
    pub display_name: Option<String>,
    pub active: bool,
    /// The SCIM resource to keep and return, without `password`.
    pub resource: Value,
}

#[derive(Debug, Clone)]
pub struct User {
    pub scim_id: Uuid,
    pub input: UserInput,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupInput {
    pub display_name: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Member {
    pub id: Uuid,
    /// The member's userName or displayName.
    pub display: String,
    /// "User" or "Group".
    pub kind: &'static str,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub scim_id: Uuid,
    pub input: GroupInput,
    pub members: Vec<Member>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

fn user_from_row(row: &PgRow) -> Result<User, sqlx::Error> {
    Ok(User {
        scim_id: row.try_get("scim_id")?,
        input: UserInput {
            user_name: row.try_get("user_name")?,
            external_id: row.try_get("external_id")?,
            display_name: row.try_get("display_name")?,
            active: row.try_get("active")?,
            resource: row.try_get::<sqlx::types::Json<Value>, _>("resource")?.0,
        },
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

/// A service account to register: an agent, a backend service, or a scheduled job.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ServiceAccountInput {
    /// From the URL path; a body needn't repeat it.
    #[serde(default)]
    pub id: String,
    pub owner: String,
    pub ai: bool,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub description: String,
}

impl ServiceAccountInput {
    fn validate(&self) -> Result<(), StoreError> {
        let ok_id = !self.id.is_empty()
            && self.id.len() <= 128
            && self.id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@:-".contains(&b));
        if !ok_id {
            return Err(StoreError::Invalid("the ID must be 1 to 128 letters, digits, and . _ @ : -".into()));
        }
        if self.groups.iter().any(|g| g.trim().is_empty()) {
            return Err(StoreError::Invalid("group names can't be empty".into()));
        }
        if self.description.len() > 500 {
            return Err(StoreError::Invalid("the description is at most 500 characters".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ServiceAccount {
    pub id: String,
    pub owner: String,
    pub ai: bool,
    pub groups: Vec<String>,
    pub description: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

const SA_COLUMNS: &str = "id, owner, ai, groups, description, created_by, created_at, updated_by, updated_at";

fn service_account_from_row(row: &PgRow) -> Result<ServiceAccount, sqlx::Error> {
    Ok(ServiceAccount {
        id: row.try_get("id")?,
        owner: row.try_get("owner")?,
        ai: row.try_get("ai")?,
        groups: row.try_get("groups")?,
        description: row.try_get("description")?,
        created_by: row.try_get("created_by")?,
        created_at: row.try_get("created_at")?,
        updated_by: row.try_get("updated_by")?,
        updated_at: row.try_get("updated_at")?,
    })
}

const USER_COLUMNS: &str = "scim_id, user_name, external_id, display_name, active, resource, created_at, updated_at";

/// userName, display name, active, updated at, and the groups the person is directly in.
type DirectoryPersonRow = (String, Option<String>, bool, DateTime<Utc>, Vec<String>);
/// Name, updated at, people and groups directly in it, and the groups it's in.
type DirectoryGroupRow = (String, DateTime<Utc>, i64, i64, Vec<String>);

#[derive(Clone)]
pub struct ContextStore {
    pool: PgPool,
}

pub(crate) async fn bump(tx: &mut Transaction<'_, Postgres>) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE snapshot_version SET version = version + 1").execute(&mut **tx).await?;
    Ok(())
}

impl ContextStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// For the other stores in this crate ([`crate::resources`]).
    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    // ---- People

    pub async fn create_user(&self, input: &UserInput) -> Result<User, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(&format!(
            "INSERT INTO humans (scim_id, user_name, external_id, display_name, active, resource) VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING {USER_COLUMNS}"
        ))
        .bind(Uuid::now_v7())
        .bind(&input.user_name)
        .bind(&input.external_id)
        .bind(&input.display_name)
        .bind(input.active)
        .bind(sqlx::types::Json(&input.resource))
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| conflict(e, format!("userName {:?} is already taken", input.user_name)))?;
        bump(&mut tx).await?;
        tx.commit().await?;
        Ok(user_from_row(&row)?)
    }

    pub async fn get_user(&self, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query(&format!("SELECT {USER_COLUMNS} FROM humans WHERE scim_id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(user_from_row)
            .transpose()
    }

    /// Ordered by creation; `user_name` filters to an exact (case-insensitive) match.
    pub async fn find_users(
        &self,
        user_name: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<(i64, Vec<User>), sqlx::Error> {
        let filter = "($1::text IS NULL OR lower(user_name) = lower($1))";
        let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM humans WHERE {filter}"))
            .bind(user_name)
            .fetch_one(&self.pool)
            .await?;
        let rows = sqlx::query(&format!(
            "SELECT {USER_COLUMNS} FROM humans WHERE {filter} ORDER BY scim_id OFFSET $2 LIMIT $3"
        ))
        .bind(user_name)
        .bind(offset)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok((total, rows.iter().map(user_from_row).collect::<Result<_, _>>()?))
    }

    pub async fn replace_user(&self, id: Uuid, input: &UserInput) -> Result<Option<User>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(&format!(
            "UPDATE humans SET user_name = $2, external_id = $3, display_name = $4, active = $5, resource = $6, updated_at = now()
             WHERE scim_id = $1 RETURNING {USER_COLUMNS}"
        ))
        .bind(id)
        .bind(&input.user_name)
        .bind(&input.external_id)
        .bind(&input.display_name)
        .bind(input.active)
        .bind(sqlx::types::Json(&input.resource))
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| conflict(e, format!("userName {:?} is already taken", input.user_name)))?;
        if row.is_some() {
            bump(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(row.as_ref().map(user_from_row).transpose()?)
    }

    pub async fn delete_user(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let deleted =
            sqlx::query("DELETE FROM humans WHERE scim_id = $1").bind(id).execute(&mut *tx).await?.rows_affected() > 0;
        sqlx::query("DELETE FROM group_members WHERE member_id = $1").bind(id).execute(&mut *tx).await?;
        if deleted {
            bump(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(deleted)
    }

    // ---- Groups

    async fn group_from_row(&self, row: &PgRow) -> Result<Group, sqlx::Error> {
        let scim_id: Uuid = row.try_get("scim_id")?;
        let members = sqlx::query(
            "SELECT m.member_id, h.user_name, g.display_name FROM group_members m
             LEFT JOIN humans h ON h.scim_id = m.member_id LEFT JOIN groups g ON g.scim_id = m.member_id
             WHERE m.group_id = $1 ORDER BY m.member_id",
        )
        .bind(scim_id)
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .filter_map(|r| {
            let id: Uuid = r.get(0);
            match (r.get::<Option<String>, _>(1), r.get::<Option<String>, _>(2)) {
                (Some(u), _) => Some(Member { id, display: u, kind: "User" }),
                (None, Some(g)) => Some(Member { id, display: g, kind: "Group" }),
                (None, None) => None,
            }
        })
        .collect();
        Ok(Group {
            scim_id,
            input: GroupInput { display_name: row.try_get("display_name")?, external_id: row.try_get("external_id")? },
            members,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    /// Members that aren't a known person or group are ignored.
    async fn insert_members(
        tx: &mut Transaction<'_, Postgres>,
        group: Uuid,
        members: &[Uuid],
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO group_members (group_id, member_id)
             SELECT $1, m FROM unnest($2::uuid[]) AS m
             WHERE m <> $1 AND (EXISTS (SELECT 1 FROM humans WHERE scim_id = m) OR EXISTS (SELECT 1 FROM groups WHERE scim_id = m))
             ON CONFLICT DO NOTHING",
        )
        .bind(group)
        .bind(members)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    pub async fn create_group(&self, input: &GroupInput, members: &[Uuid]) -> Result<Group, StoreError> {
        let mut tx = self.pool.begin().await?;
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO groups (scim_id, display_name, external_id) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(&input.display_name)
            .bind(&input.external_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| conflict(e, format!("a group called {:?} already exists", input.display_name)))?;
        Self::insert_members(&mut tx, id, members).await?;
        bump(&mut tx).await?;
        tx.commit().await?;
        Ok(self.get_group(id).await?.expect("just created"))
    }

    pub async fn get_group(&self, id: Uuid) -> Result<Option<Group>, sqlx::Error> {
        match sqlx::query("SELECT * FROM groups WHERE scim_id = $1").bind(id).fetch_optional(&self.pool).await? {
            Some(row) => Ok(Some(self.group_from_row(&row).await?)),
            None => Ok(None),
        }
    }

    pub async fn find_groups(
        &self,
        display_name: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<(i64, Vec<Group>), sqlx::Error> {
        let filter = "($1::text IS NULL OR lower(display_name) = lower($1))";
        let total: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM groups WHERE {filter}"))
            .bind(display_name)
            .fetch_one(&self.pool)
            .await?;
        let rows = sqlx::query(&format!("SELECT * FROM groups WHERE {filter} ORDER BY scim_id OFFSET $2 LIMIT $3"))
            .bind(display_name)
            .bind(offset)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;
        let mut groups = Vec::with_capacity(rows.len());
        for row in &rows {
            groups.push(self.group_from_row(row).await?);
        }
        Ok((total, groups))
    }

    /// Changes a group: its name and ID, and its members (`set` replaces them all; then `add`
    /// and `remove` apply).
    pub async fn update_group(
        &self,
        id: Uuid,
        input: Option<&GroupInput>,
        set: Option<&[Uuid]>,
        add: &[Uuid],
        remove: &[Uuid],
    ) -> Result<Option<Group>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM groups WHERE scim_id = $1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            return Ok(None);
        }
        if let Some(input) = input {
            sqlx::query("UPDATE groups SET display_name = $2, external_id = $3, updated_at = now() WHERE scim_id = $1")
                .bind(id)
                .bind(&input.display_name)
                .bind(&input.external_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| conflict(e, format!("a group called {:?} already exists", input.display_name)))?;
        }
        if let Some(set) = set {
            sqlx::query("DELETE FROM group_members WHERE group_id = $1").bind(id).execute(&mut *tx).await?;
            Self::insert_members(&mut tx, id, set).await?;
        }
        Self::insert_members(&mut tx, id, add).await?;
        sqlx::query("DELETE FROM group_members WHERE group_id = $1 AND member_id = ANY($2)")
            .bind(id)
            .bind(remove)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE groups SET updated_at = now() WHERE scim_id = $1").bind(id).execute(&mut *tx).await?;
        bump(&mut tx).await?;
        tx.commit().await?;
        Ok(self.get_group(id).await?)
    }

    pub async fn delete_group(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let deleted =
            sqlx::query("DELETE FROM groups WHERE scim_id = $1").bind(id).execute(&mut *tx).await?.rows_affected() > 0;
        sqlx::query("DELETE FROM group_members WHERE member_id = $1").bind(id).execute(&mut *tx).await?;
        if deleted {
            bump(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(deleted)
    }

    // ---- Service accounts

    pub async fn service_accounts(&self) -> Result<Vec<ServiceAccount>, sqlx::Error> {
        let rows = sqlx::query(&format!("SELECT {SA_COLUMNS} FROM service_accounts ORDER BY id"))
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(service_account_from_row).collect()
    }

    pub async fn service_account(&self, id: &str) -> Result<Option<ServiceAccount>, sqlx::Error> {
        sqlx::query(&format!("SELECT {SA_COLUMNS} FROM service_accounts WHERE id = $1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .as_ref()
            .map(service_account_from_row)
            .transpose()
    }

    /// Registers a service account or changes one. The owner must be a person the IdP has
    /// provisioned; it is stored as their userName exactly.
    pub async fn put_service_account(
        &self,
        input: &ServiceAccountInput,
        by: &str,
    ) -> Result<ServiceAccount, StoreError> {
        input.validate()?;
        let mut tx = self.pool.begin().await?;
        let owner: Option<String> =
            sqlx::query_scalar("SELECT user_name FROM humans WHERE lower(user_name) = lower($1)")
                .bind(&input.owner)
                .fetch_optional(&mut *tx)
                .await?;
        let Some(owner) = owner else {
            return Err(StoreError::Invalid(format!(
                "the owner {:?} is not a person the IdP has provisioned",
                input.owner
            )));
        };
        let row = sqlx::query(&format!(
            "INSERT INTO service_accounts (id, owner, ai, groups, description, created_by, updated_by) VALUES ($1, $2, $3, $4, $5, $6, $6)
             ON CONFLICT (id) DO UPDATE SET owner = $2, ai = $3, groups = $4, description = $5, updated_by = $6, updated_at = now()
             RETURNING {SA_COLUMNS}"
        ))
        .bind(&input.id)
        .bind(&owner)
        .bind(input.ai)
        .bind(&input.groups)
        .bind(&input.description)
        .bind(by)
        .fetch_one(&mut *tx)
        .await?;
        bump(&mut tx).await?;
        tx.commit().await?;
        Ok(service_account_from_row(&row)?)
    }

    pub async fn delete_service_account(&self, id: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let deleted =
            sqlx::query("DELETE FROM service_accounts WHERE id = $1").bind(id).execute(&mut *tx).await?.rows_affected()
                > 0;
        if deleted {
            bump(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(deleted)
    }

    // ---- The directory, for the client portal

    /// People ordered by userName, optionally those whose userName or display name contains
    /// `search`, with the groups they're directly in.
    pub async fn directory_people(&self, search: Option<&str>, limit: i64) -> Result<Vec<Value>, sqlx::Error> {
        let rows: Vec<DirectoryPersonRow> = sqlx::query_as(
            "SELECT h.user_name, h.display_name, h.active, h.updated_at,
                    COALESCE(array_agg(g.display_name ORDER BY g.display_name) FILTER (WHERE g.display_name IS NOT NULL), '{}')
             FROM humans h
             LEFT JOIN group_members m ON m.member_id = h.scim_id
             LEFT JOIN groups g ON g.scim_id = m.group_id
             WHERE $1::text IS NULL OR h.user_name ILIKE '%' || $1 || '%' OR h.display_name ILIKE '%' || $1 || '%'
             GROUP BY h.scim_id ORDER BY lower(h.user_name) LIMIT $2",
        )
        .bind(search)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(user_name, display_name, active, updated_at, groups)| {
                json!({ "user_name": user_name, "display_name": display_name, "active": active, "groups": groups, "updated_at": updated_at })
            })
            .collect())
    }

    /// Groups ordered by name, with how many people and groups they directly contain and the
    /// groups they're in.
    pub async fn directory_groups(&self) -> Result<Vec<Value>, sqlx::Error> {
        let rows: Vec<DirectoryGroupRow> = sqlx::query_as(
            "SELECT g.display_name, g.updated_at,
                    (SELECT count(*) FROM group_members m JOIN humans h ON h.scim_id = m.member_id WHERE m.group_id = g.scim_id),
                    (SELECT count(*) FROM group_members m JOIN groups c ON c.scim_id = m.member_id WHERE m.group_id = g.scim_id),
                    COALESCE((SELECT array_agg(p.display_name ORDER BY p.display_name) FROM group_members m JOIN groups p ON p.scim_id = m.group_id
                              WHERE m.member_id = g.scim_id), '{}')
             FROM groups g ORDER BY lower(g.display_name)",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(name, updated_at, people, groups, parents)| {
                json!({ "name": name, "people": people, "groups": groups, "parents": parents, "updated_at": updated_at })
            })
            .collect())
    }

    /// Totals, and when the IdP last changed anything.
    pub async fn directory_summary(&self) -> Result<Value, sqlx::Error> {
        let (people, active, groups, accounts, last): (i64, i64, i64, i64, Option<DateTime<Utc>>) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM humans), (SELECT count(*) FROM humans WHERE active), (SELECT count(*) FROM groups),
                    (SELECT count(*) FROM service_accounts),
                    GREATEST((SELECT max(updated_at) FROM humans), (SELECT max(updated_at) FROM groups))",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(
            json!({ "people": people, "active_people": active, "groups": groups, "service_accounts": accounts, "last_provisioned_at": last }),
        )
    }

    // ---- The PDPs' snapshot

    pub async fn version(&self) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar("SELECT version FROM snapshot_version").fetch_one(&self.pool).await
    }

    /// Every person and group as Cedar entity JSON: people with `active` and their groups as
    /// parents, groups with the groups they belong to. Read in one transaction, so the version
    /// matches the entities.
    pub async fn snapshot(&self) -> Result<(i64, Vec<Value>), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ").execute(&mut *tx).await?;
        let version: i64 = sqlx::query_scalar("SELECT version FROM snapshot_version").fetch_one(&mut *tx).await?;
        let groups: HashMap<Uuid, String> =
            sqlx::query_as("SELECT scim_id, display_name FROM groups").fetch_all(&mut *tx).await?.into_iter().collect();
        let mut parents: HashMap<Uuid, Vec<&str>> = HashMap::new();
        let memberships: Vec<(Uuid, Uuid)> =
            sqlx::query_as("SELECT group_id, member_id FROM group_members").fetch_all(&mut *tx).await?;
        for (group, member) in &memberships {
            if let Some(name) = groups.get(group) {
                parents.entry(*member).or_default().push(name);
            }
        }
        let parent_refs = |id: &Uuid| -> Vec<Value> {
            let mut names = parents.get(id).cloned().unwrap_or_default();
            names.sort();
            names.into_iter().map(|g| json!({ "type": entity_type::GROUP, "id": g })).collect()
        };
        let mut entities = Vec::new();
        let humans: Vec<(Uuid, String, bool)> =
            sqlx::query_as("SELECT scim_id, user_name, active FROM humans").fetch_all(&mut *tx).await?;
        for (id, user_name, active) in &humans {
            entities.push(json!({
                "uid": { "type": entity_type::HUMAN, "id": user_name },
                "attrs": { "active": active },
                // A deactivated person keeps no group access.
                "parents": if *active { parent_refs(id) } else { vec![] },
            }));
        }
        let accounts: Vec<(String, String, bool, Vec<String>)> =
            sqlx::query_as("SELECT id, owner, ai, groups FROM service_accounts ORDER BY id")
                .fetch_all(&mut *tx)
                .await?;
        for (id, owner, ai, groups) in accounts {
            let parents: Vec<Value> = groups.iter().map(|g| json!({ "type": entity_type::GROUP, "id": g })).collect();
            entities.push(json!({
                "uid": { "type": entity_type::SERVICE_ACCOUNT, "id": id },
                "attrs": { "owner": { "__entity": { "type": entity_type::HUMAN, "id": owner } }, "ai": ai },
                "parents": parents,
            }));
        }
        let ordered: BTreeMap<&String, &Uuid> = groups.iter().map(|(id, name)| (name, id)).collect();
        for (name, id) in ordered {
            entities.push(
                json!({ "uid": { "type": entity_type::GROUP, "id": name }, "attrs": {}, "parents": parent_refs(id) }),
            );
        }
        tx.commit().await?;
        Ok((version, entities))
    }
}

/// Integration tests against Postgres; skipped without AWZY_TEST_POSTGRES_URL.
#[cfg(test)]
pub(crate) mod tests {
    use std::str::FromStr;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    use super::*;

    pub(crate) async fn test_store() -> Option<ContextStore> {
        let Ok(url) = std::env::var("AWZY_TEST_POSTGRES_URL") else {
            eprintln!("skipping: AWZY_TEST_POSTGRES_URL is not set");
            return None;
        };
        let schema = format!("test_{}", Uuid::now_v7().simple());
        let admin = PgPool::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&admin).await.unwrap();
        let options = PgConnectOptions::from_str(&url).unwrap().options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new().max_connections(2).connect_with(options).await.unwrap();
        let store = ContextStore::new(pool);
        store.migrate().await.unwrap();
        Some(store)
    }

    fn user(name: &str) -> UserInput {
        UserInput {
            user_name: name.into(),
            external_id: None,
            display_name: None,
            active: true,
            resource: json!({ "userName": name }),
        }
    }

    #[tokio::test]
    async fn snapshot_has_people_groups_and_nesting() {
        let Some(store) = test_store().await else { return };
        let alice = store.create_user(&user("alice@acme.com")).await.unwrap();
        let bob = store.create_user(&user("bob@acme.com")).await.unwrap();
        let appsec = store
            .create_group(&GroupInput { display_name: "appsec".into(), external_id: None }, &[alice.scim_id])
            .await
            .unwrap();
        let security = store
            .create_group(&GroupInput { display_name: "security".into(), external_id: None }, &[appsec.scim_id])
            .await
            .unwrap();
        store
            .create_group(
                &GroupInput { display_name: "everyone".into(), external_id: None },
                &[alice.scim_id, bob.scim_id, Uuid::now_v7()],
            )
            .await
            .unwrap();

        let (v1, entities) = store.snapshot().await.unwrap();
        let find = |id: &str| entities.iter().find(|e| e["uid"]["id"] == id).unwrap().clone();
        assert_eq!(
            find("alice@acme.com")["parents"],
            json!([{ "type": "Group", "id": "appsec" }, { "type": "Group", "id": "everyone" }])
        );
        assert_eq!(find("appsec")["parents"], json!([{ "type": "Group", "id": "security" }]), "nested group");
        assert_eq!(store.get_group(security.scim_id).await.unwrap().unwrap().members[0].display, "appsec");

        let mut deactivated = user("alice@acme.com");
        deactivated.active = false;
        store.replace_user(alice.scim_id, &deactivated).await.unwrap();
        let (v2, entities) = store.snapshot().await.unwrap();
        assert!(v2 > v1, "every change moves the version");
        let alice = entities.iter().find(|e| e["uid"]["id"] == "alice@acme.com").unwrap();
        assert_eq!((alice["attrs"]["active"].clone(), alice["parents"].clone()), (json!(false), json!([])));

        assert!(
            matches!(store.create_user(&user("BOB@acme.com")).await, Err(StoreError::Conflict(_))),
            "userName is case-insensitive"
        );
    }

    #[tokio::test]
    async fn service_accounts_need_a_provisioned_owner_and_reach_the_snapshot() {
        let Some(store) = test_store().await else { return };
        store.create_user(&user("bob@acme.com")).await.unwrap();
        let input = |owner: &str| ServiceAccountInput {
            id: "sales-genie".into(),
            owner: owner.into(),
            ai: true,
            groups: vec!["engineering".into()],
            description: "Drafts follow-up emails".into(),
        };
        assert!(matches!(
            store.put_service_account(&input("mallory@acme.com"), "alice").await,
            Err(StoreError::Invalid(_))
        ));
        let mut bad = input("bob@acme.com");
        bad.id = "has spaces".into();
        assert!(matches!(store.put_service_account(&bad, "alice").await, Err(StoreError::Invalid(_))));

        let sa = store.put_service_account(&input("BOB@acme.com"), "alice").await.unwrap();
        assert_eq!(
            (sa.owner.as_str(), sa.created_by.as_str()),
            ("bob@acme.com", "alice"),
            "the owner is stored as provisioned"
        );
        let (v1, entities) = store.snapshot().await.unwrap();
        let genie = entities.iter().find(|e| e["uid"]["type"] == "ServiceAccount").unwrap();
        assert_eq!(
            genie,
            &json!({ "uid": { "type": "ServiceAccount", "id": "sales-genie" },
                     "attrs": { "owner": { "__entity": { "type": "Human", "id": "bob@acme.com" } }, "ai": true },
                     "parents": [{ "type": "Group", "id": "engineering" }] })
        );

        let mut changed = input("bob@acme.com");
        changed.ai = false;
        assert!(!store.put_service_account(&changed, "carol").await.unwrap().ai);
        assert!(store.delete_service_account("sales-genie").await.unwrap());
        let (v2, entities) = store.snapshot().await.unwrap();
        assert!(v2 > v1 && !entities.iter().any(|e| e["uid"]["type"] == "ServiceAccount"));
    }
}
