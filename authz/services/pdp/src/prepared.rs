//! What a [`crate::Pdp`] works out once per bundle so each check doesn't: the entities it has
//! parsed and assembled, each policy's ID and challenge, and each action's policies, with and
//! without the challenges.
//!
//! Profiling a PDP under load found most of a check's CPU outside policy evaluation: in Cedar's
//! JSON parser, for entities that arrive check after check unchanged (the same principals,
//! groups, and resources) and for a context of a few keys, and in validating entities again.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use awzy_model::ChallengeKind;
use cedar_policy::{
    ActionConstraint, Context, Entities, Entity, EntityUid, PolicyId, PolicySet, RestrictedExpression, Schema,
};

/// Parsed entities kept before the cache starts over: enough for a working set of principals
/// and resources, a few megabytes.
const CAPACITY: usize = 10_000;

/// Cedar entities by their JSON, parsed and validated against one schema.
pub(crate) struct ParsedEntities {
    parsed: RwLock<HashMap<String, Arc<Entity>>>,
    /// Requests' whole entity sets, by their entities' JSON in order, with Cedar's transitive
    /// closure computed: the same principal and resource come back check after check.
    assembled: RwLock<HashMap<String, Entities>>,
    /// The schema's action entities when it groups actions, for `action in Action::"group"`.
    /// Without groups they say nothing: Cedar treats an action it has no entity for as one with
    /// no ancestors, and adding them to each request's entities is a third of the work.
    actions: Vec<Entity>,
}

impl ParsedEntities {
    pub(crate) fn new(schema: &Schema) -> anyhow::Result<Self> {
        let actions =
            if groups_actions(schema)? { schema.action_entities()?.into_iter().collect() } else { Vec::new() };
        Ok(Self { parsed: RwLock::default(), assembled: RwLock::default(), actions })
    }

    /// `json` (an array of Cedar entity JSON) as Cedar entities, validated against `schema`
    /// with its action entities added: what `Entities::from_json_value` gives, from entities
    /// parsed once each.
    pub(crate) fn entities(&self, schema: &Schema, json: Vec<serde_json::Value>) -> anyhow::Result<Entities> {
        // The same text is the same entity; an entity written another way is only a miss.
        let keys: Vec<String> = json.iter().map(|v| v.to_string()).collect();
        let whole = keys.join("\n");
        if let Some(entities) = self.assembled.read().unwrap_or_else(|e| e.into_inner()).get(&whole) {
            return Ok(entities.clone());
        }
        let entities = self.assemble(schema, json, keys)?;
        let mut assembled = self.assembled.write().unwrap_or_else(|e| e.into_inner());
        if assembled.len() >= CAPACITY {
            assembled.clear();
        }
        assembled.insert(whole, entities.clone());
        Ok(entities)
    }

    fn assemble(&self, schema: &Schema, json: Vec<serde_json::Value>, keys: Vec<String>) -> anyhow::Result<Entities> {
        let mut parsed = Vec::with_capacity(json.len());
        for (value, key) in json.into_iter().zip(keys) {
            let cached = self.parsed.read().unwrap_or_else(|e| e.into_inner()).get(&key).cloned();
            let entity = match cached {
                Some(entity) => entity,
                None => {
                    let entity = Arc::new(Entity::from_json_value(value, Some(schema))?);
                    let mut cache = self.parsed.write().unwrap_or_else(|e| e.into_inner());
                    if cache.len() >= CAPACITY {
                        cache.clear();
                    }
                    cache.insert(key, entity.clone());
                    entity
                }
            };
            parsed.push(Entity::clone(&entity));
        }
        // An action entity from the request must match the schema's, which only Cedar checks.
        if parsed.iter().any(|e| e.uid().type_name().basename() == "Action") {
            return Ok(Entities::from_entities(parsed, Some(schema))?);
        }
        // Each was validated against the schema as it was parsed.
        Ok(Entities::from_entities(parsed.into_iter().chain(self.actions.iter().cloned()), None)?)
    }
}

/// `context` as Cedar's context for `action`. Built from its JSON values directly first, which
/// costs a fraction of Cedar's parser: when that validates against the schema, the
/// schema-directed parse would have given the same values. A context that leaves an entity
/// reference or extension value implicit (`"1.2.3.4"` for an `ipaddr`), or uses a form this
/// doesn't read (`__extn`), is parsed by Cedar with the schema as before.
pub(crate) fn context(
    schema: &Schema,
    action: &EntityUid,
    context: serde_json::Map<String, serde_json::Value>,
) -> anyhow::Result<Context> {
    let direct: Option<Vec<(String, RestrictedExpression)>> =
        context.iter().map(|(k, v)| Some((k.clone(), restricted(v)?))).collect();
    if let Some(plain) = direct.and_then(|pairs| Context::from_pairs(pairs).ok()) {
        if plain.validate(schema, action).is_ok() {
            return Ok(plain);
        }
    }
    Ok(Context::from_json_value(serde_json::Value::Object(context), Some((schema, action)))?)
}

/// A JSON value as the Cedar value Cedar's parser reads it as without a schema, for the forms
/// that are unambiguous; `None` for the rest.
fn restricted(value: &serde_json::Value) -> Option<RestrictedExpression> {
    use serde_json::Value;
    Some(match value {
        Value::Bool(b) => RestrictedExpression::new_bool(*b),
        Value::Number(n) => RestrictedExpression::new_long(n.as_i64()?),
        Value::String(s) => RestrictedExpression::new_string(s.clone()),
        Value::Array(items) => RestrictedExpression::new_set(items.iter().map(restricted).collect::<Option<Vec<_>>>()?),
        Value::Object(fields) if fields.keys().any(|k| k.starts_with("__")) => {
            let (Some(Value::Object(uid)), 1) = (fields.get("__entity"), fields.len()) else { return None };
            let (Some(Value::String(kind)), Some(Value::String(id)), 2) = (uid.get("type"), uid.get("id"), uid.len())
            else {
                return None;
            };
            RestrictedExpression::new_entity_uid(EntityUid::from_type_name_and_id(
                kind.parse().ok()?,
                cedar_policy::EntityId::new(id),
            ))
        }
        Value::Object(fields) => RestrictedExpression::new_record(
            fields.iter().map(|(k, v)| Some((k.clone(), restricted(v)?))).collect::<Option<Vec<_>>>()?,
        )
        .ok()?,
        Value::Null => return None,
    })
}

/// Whether the schema puts any action in a group, for `action in Action::"group"`.
fn groups_actions(schema: &Schema) -> anyhow::Result<bool> {
    let actions = schema.action_entities()?;
    let grouped = actions.iter().any(|a| actions.ancestors(&a.uid()).is_some_and(|mut up| up.next().is_some()));
    Ok(grouped)
}

/// The policies a check evaluates, and the same without the `@challenge` ones: whether a
/// request is allowed once its challenges are set aside. (A set that has set aside some passed
/// challenges gives the same set without the rest.)
pub(crate) struct Policies {
    pub(crate) all: PolicySet,
    pub(crate) without_challenges: PolicySet,
}

impl Policies {
    fn new(policies: Vec<cedar_policy::Policy>) -> anyhow::Result<Self> {
        let rest = policies.iter().filter(|p| p.annotation("challenge").is_none()).cloned();
        Ok(Self { without_challenges: PolicySet::from_policies(rest)?, all: PolicySet::from_policies(policies)? })
    }
}

/// Each policy's ID as decisions report it and the challenge it asks for, and each action's
/// policies.
pub(crate) struct PreparedPolicies {
    ids: HashMap<PolicyId, String>,
    challenges: HashMap<PolicyId, ChallengeKind>,
    /// Every policy, for an action the schema doesn't name.
    every: Policies,
    /// By action ID: only the policies whose scope can match it. Cedar builds and evaluates each
    /// policy's condition for every request, and most are scoped to other actions. Empty when
    /// the schema groups actions, where scope matching needs the action's ancestors.
    by_action: HashMap<String, Policies>,
}

impl PreparedPolicies {
    pub(crate) fn new(policies: &PolicySet, schema: &Schema) -> anyhow::Result<Self> {
        let ids = policies.policies().map(|p| (p.id().clone(), crate::policy_id(p))).collect();
        let challenges = policies
            .policies()
            .filter_map(|p| Some((p.id().clone(), p.annotation("challenge").and_then(ChallengeKind::parse)?)))
            .collect();
        let mut by_action = HashMap::new();
        if !groups_actions(schema)? {
            // The Action type the PDP's requests use; a namespaced schema's actions keep every policy.
            for action in schema.actions().filter(|a| a.type_name().to_string() == "Action") {
                let applies = |p: &&cedar_policy::Policy| match p.action_constraint() {
                    ActionConstraint::Any => true,
                    ActionConstraint::Eq(a) => a == *action,
                    // Without groups, `in` an action is that action.
                    ActionConstraint::In(actions) => actions.contains(action),
                };
                let scoped = Policies::new(policies.policies().filter(applies).cloned().collect())?;
                by_action.insert(action.id().unescaped().to_string(), scoped);
            }
        }
        Ok(Self { ids, challenges, every: Policies::new(policies.policies().cloned().collect())?, by_action })
    }

    /// The policies a check of `action` evaluates.
    pub(crate) fn for_action(&self, action: &str) -> &Policies {
        self.by_action.get(action).unwrap_or(&self.every)
    }

    /// The ID decisions report for the policy Cedar calls `id`.
    pub(crate) fn id(&self, id: &PolicyId) -> String {
        self.ids.get(id).cloned().unwrap_or_else(|| id.to_string())
    }

    /// The challenge the policy asks for, if it's a `@challenge` policy.
    pub(crate) fn challenge(&self, id: &PolicyId) -> Option<ChallengeKind> {
        self.challenges.get(id).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Schema {
        Schema::from_cedarschema_str(crate::DEFAULT_SCHEMA).unwrap().0
    }

    #[test]
    fn cached_entities_decide_as_freshly_parsed_ones() {
        let schema = schema();
        let json = json!([
            { "uid": { "type": "Human", "id": "alice" }, "attrs": {}, "parents": [{ "type": "Group", "id": "security" }] },
            { "uid": { "type": "Group", "id": "security" }, "attrs": {}, "parents": [] },
            { "uid": { "type": "Resource", "id": "doc" }, "attrs": { "sensitivity": "internal" }, "parents": [] },
        ]);
        let cache = ParsedEntities::new(&schema).unwrap();
        let first = cache.entities(&schema, json.as_array().unwrap().clone()).unwrap();
        let again = cache.entities(&schema, json.as_array().unwrap().clone()).unwrap();
        let fresh = Entities::from_json_value(json, Some(&schema)).unwrap();
        let without_actions = |e: &Entities| {
            Entities::from_entities(e.iter().filter(|e| e.uid().type_name().basename() != "Action").cloned(), None)
                .unwrap()
        };
        assert!(without_actions(&first).deep_eq(&without_actions(&fresh)));
        assert!(again.deep_eq(&first));
    }

    #[test]
    fn a_schema_that_groups_actions_keeps_them_for_action_in_group() {
        let grouped = Schema::from_cedarschema_str(
            "entity Human; entity Doc; action reading; action read in [reading] appliesTo { principal: Human, resource: Doc };",
        )
        .unwrap()
        .0;
        let cache = ParsedEntities::new(&grouped).unwrap();
        let entities = cache
            .entities(&grouped, vec![json!({ "uid": { "type": "Human", "id": "a" }, "attrs": {}, "parents": [] })])
            .unwrap();
        let read: EntityUid = r#"Action::"read""#.parse().unwrap();
        let up: Vec<String> = entities.ancestors(&read).unwrap().map(|u| u.to_string()).collect();
        assert_eq!(up, vec![r#"Action::"reading""#]);

        assert!(ParsedEntities::new(&schema()).unwrap().actions.is_empty(), "awzy's schema doesn't group actions");
    }

    #[test]
    fn each_action_evaluates_the_policies_whose_scope_can_match_it() {
        let policies: PolicySet = crate::DEFAULT_POLICIES.parse().unwrap();
        let prepared = PreparedPolicies::new(&policies, &schema()).unwrap();
        let read: EntityUid = r#"Action::"read""#.parse().unwrap();
        let can_read = |p: &cedar_policy::Policy| match p.action_constraint() {
            ActionConstraint::Any => true,
            ActionConstraint::Eq(a) => a == read,
            ActionConstraint::In(actions) => actions.contains(&read),
        };
        let scoped: Vec<_> = prepared.for_action("read").all.policies().map(|p| p.id().clone()).collect();
        let expected: Vec<_> = policies.policies().filter(|p| can_read(p)).map(|p| p.id().clone()).collect();
        assert_eq!(scoped.len(), expected.len());
        assert!(expected.iter().all(|id| scoped.contains(id)));
        assert!(scoped.len() < policies.policies().count(), "other actions' policies are left out");

        assert_eq!(prepared.for_action("no-such-action").all.policies().count(), policies.policies().count());
    }

    #[test]
    fn an_entity_the_schema_rejects_is_rejected_from_the_cache_too() {
        let schema = schema();
        let bad = json!([{ "uid": { "type": "Resource", "id": "doc" }, "attrs": { "sensitivity": 3 }, "parents": [] }]);
        let cache = ParsedEntities::new(&schema).unwrap();
        assert!(cache.entities(&schema, bad.as_array().unwrap().clone()).is_err());
        assert!(cache.entities(&schema, bad.as_array().unwrap().clone()).is_err());
    }

    #[test]
    fn json_values_become_the_cedar_values_cedars_parser_makes() {
        let json = json!({ "n": -3, "yes": true, "s": "text", "set": ["a", "b"], "nested": { "deep": [1, 2] },
            "who": { "__entity": { "type": "Human", "id": "bob" } }, "empty": [] });
        let pairs: Vec<_> =
            json.as_object().unwrap().iter().map(|(k, v)| (k.clone(), restricted(v).unwrap())).collect();
        let direct = Context::from_pairs(pairs).unwrap();
        let parsed = Context::from_json_value(json, None).unwrap();
        assert_eq!(direct.to_json_value().unwrap(), parsed.to_json_value().unwrap());

        for unread in [
            json!(1.5),
            json!(null),
            json!({ "__extn": { "fn": "ip", "arg": "10.0.0.1" } }),
            json!({ "__entity": { "type": "Human", "id": "bob" }, "extra": 1 }),
        ] {
            assert!(restricted(&unread).is_none(), "{unread} is left to Cedar's parser");
        }
    }

    #[test]
    fn a_context_parses_as_the_schema_would_parse_it() {
        let schema = schema();
        let read: EntityUid = r#"Action::"read""#.parse().unwrap();
        let explicit = json!({ "risk_score": 7, "human_present": true,
            "on_behalf_of": { "__entity": { "type": "Human", "id": "bob" } } });
        let fast = context(&schema, &read, explicit.as_object().unwrap().clone()).unwrap();
        let directed = Context::from_json_value(explicit, Some((&schema, &read))).unwrap();
        assert_eq!(format!("{:?}", fast.get("on_behalf_of")), format!("{:?}", directed.get("on_behalf_of")));
        assert_eq!(format!("{:?}", fast.get("risk_score")), format!("{:?}", directed.get("risk_score")));

        // An entity reference without `__entity` only the schema can read.
        let implicit =
            json!({ "risk_score": 7, "human_present": true, "on_behalf_of": { "type": "Human", "id": "bob" } });
        let fallback = context(&schema, &read, implicit.as_object().unwrap().clone()).unwrap();
        assert_eq!(format!("{:?}", fallback.get("on_behalf_of")), format!("{:?}", directed.get("on_behalf_of")));

        // A wrongly typed value still fails where it always did: `Request::new` checks the schema.
        let wrong = json!({ "risk_score": "high", "human_present": true });
        let parsed = context(&schema, &read, wrong.as_object().unwrap().clone()).unwrap();
        let (alice, doc) = (r#"Human::"alice""#.parse().unwrap(), r#"Resource::"doc""#.parse().unwrap());
        assert!(cedar_policy::Request::new(alice, read, doc, parsed, Some(&schema)).is_err());
    }
}
