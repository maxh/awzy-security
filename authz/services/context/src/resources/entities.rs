//! Resources as the Cedar entities the PDP evaluates (ADR 0061).

use awzy_model::entity_type;
use serde_json::{json, Value};

use super::{Resource, Share, DEFAULT_SENSITIVITY};

/// A resource as the Cedar entity the PDP evaluates, with only its own sharing.
pub fn entity(r: &Resource) -> Value {
    entity_sharing(r, r.sharing.iter())
}

/// A resource and its ancestors (nearest first) as Cedar entities. Each one's sharing includes
/// its folders', as in Drive: a document in a folder shared with a group is shared with it too.
pub fn chain_entities(chain: &[Resource]) -> Vec<Value> {
    (0..chain.len()).map(|i| entity_sharing(&chain[i], chain[i..].iter().flat_map(|r| r.sharing.iter()))).collect()
}

/// The entity: its sensitivity (the default tier until it is classified), its owner, who it's
/// shared with (ADR 0061), and its parent folder.
fn entity_sharing<'a>(r: &Resource, sharing: impl Iterator<Item = &'a Share>) -> Value {
    use std::collections::BTreeSet;
    let parents: Vec<Value> = r.parent.iter().map(|p| json!({ "type": entity_type::RESOURCE, "id": p })).collect();
    // Sorted and deduplicated, so an entity is the same however its shares were written.
    let (mut humans, mut groups, mut writer_humans, mut writer_groups) =
        (BTreeSet::new(), BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut everyone = false;
    for share in sharing {
        let writes = matches!(share.role.as_str(), "writer" | "owner");
        match share.kind.as_str() {
            "human" => {
                humans.insert(share.id.as_str());
                if writes {
                    writer_humans.insert(share.id.as_str());
                }
            }
            "group" => {
                groups.insert(share.id.as_str());
                if writes {
                    writer_groups.insert(share.id.as_str());
                }
            }
            // A single-tenant install's domain is the client's own: everyone there.
            "anyone" | "domain" => everyone = true,
            _ => {}
        }
    }
    let refs = |kind: &str, ids: BTreeSet<&str>| -> Vec<Value> {
        ids.into_iter().map(|id| json!({ "__entity": { "type": kind, "id": id } })).collect()
    };
    let mut attrs = json!({
        "sensitivity": r.effective_sensitivity().unwrap_or(DEFAULT_SENSITIVITY),
        "unclassified": r.effective_sensitivity().is_none(),
        "shared_humans": refs(entity_type::HUMAN, humans),
        "shared_groups": refs(entity_type::GROUP, groups),
        "writer_humans": refs(entity_type::HUMAN, writer_humans),
        "writer_groups": refs(entity_type::GROUP, writer_groups),
        "shared_with_everyone": everyone,
    });
    if let Some(owner) = &r.owner {
        attrs["owner"] = json!({ "__entity": { "type": entity_type::HUMAN, "id": owner } });
    }
    json!({ "uid": { "type": entity_type::RESOURCE, "id": r.id }, "attrs": attrs, "parents": parents })
}
