//! How awzy-reasoner's containments hold up on human review (#140): the measure that decides
//! when automated containments can leave monitor.

use async_graphql::SimpleObject;
use awzy_authz_proto::challenge::v1::Containment as WireContainment;
use awzy_portal::UtcTimestamp;
use chrono::{DateTime, Utc};

use crate::challenges::time;

/// How awzy-reasoner's containments held up on human review (#140): of those it made in a
/// period, how many a person lifted, or dismissed when only recorded. The share lifted is what
/// decides when automated containments can leave monitor.
#[derive(SimpleObject, Debug, Clone, PartialEq)]
pub struct ContainmentReview {
    /// awzy-reasoner's containments since `since`, applied or only recorded.
    pub automated: i32,
    /// Those a person has since lifted or dismissed.
    pub lifted_on_review: i32,
    pub since: UtcTimestamp,
}

/// Counts `list`'s automated containments made since `since`, and those a person lifted.
pub(crate) fn review(list: &[WireContainment], since: DateTime<Utc>) -> ContainmentReview {
    let reasoner = awzy_model::Actor::awzy("awzy-reasoner").to_string();
    let automated: Vec<&WireContainment> =
        list.iter().filter(|c| c.created_by == reasoner && time(&c.created_at).is_some_and(|t| t.0 >= since)).collect();
    // A person lifts it; awzy's own services never count as a review.
    let service = format!("{}::", awzy_model::actor_type::AWZY_SERVICE);
    let lifted = automated.iter().filter(|c| !c.lifted_by.is_empty() && !c.lifted_by.starts_with(&service)).count();
    let count = | n : usize |   i32::try_from(n).unwrap_or(i32::MAX);
    ContainmentReview { automated: count(automated.len()), lifted_on_review: count(lifted), since: UtcTimestamp(since) }
}

#[cfg(test)]
mod tests {
    use awzy_authz_proto::prost_types::Timestamp;

    use super::*;

    /// Only awzy-reasoner's containments in the period count, and only a person's lift.
    #[test]
    fn the_review_counts_what_people_lifted_of_the_reasoners_containments() {
        let at = |days_ago: i64| {
            let t = Utc::now() - chrono::Duration::days(days_ago);
            Some(Timestamp { seconds: t.timestamp(), nanos: 0 })
        };
        let c = |by: &str, days_ago: i64, lifted_by: &str| WireContainment {
            created_by: by.into(),
            created_at: at(days_ago),
            lifted_by: lifted_by.into(),
            ..Default::default()
        };
        let list = [
            c("AwzyService::awzy-reasoner", 1, "Human::priya"),
            c("AwzyService::awzy-reasoner", 2, ""),
            c("AwzyService::awzy-reasoner", 3, "AwzyService::awzy-challenge"),
            c("AwzyService::awzy-reasoner", 40, "Human::priya"),
            c("Human::priya", 1, "Human::priya"),
        ];
        let r = review(&list, Utc::now() - chrono::Duration::days(30));
        assert_eq!((r.automated, r.lifted_on_review), (3, 1));
    }
}
