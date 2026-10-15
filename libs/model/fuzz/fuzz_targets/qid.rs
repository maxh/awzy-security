//! Any text, read as each kind of QID: a QID reads back as itself, and has one spelling, so
//! comparing QIDs as text is comparing IDs.
#![no_main]

use awzy_model::{
    CeremonyId, ChallengeId, ContainmentId, DecisionId, EventId, InstallId, InvestigationId, PlanId, PolicyBundleId,
    QidEntity, SecurityKeyId, SupportSessionId, TypedQid,
};
use libfuzzer_sys::fuzz_target;

fn check<T: TypedQid + PartialEq + std::fmt::Debug>(s: &str) {
    if let Ok(qid) = s.parse::<T>() {
        assert_eq!(qid.to_string(), s, "a QID has one spelling");
        assert_eq!(qid.to_string().parse::<T>().ok(), Some(qid));
        assert_eq!(QidEntity::of(s), Some(T::ENTITY));
    }
}

fuzz_target!(|s: &str| {
    check::<DecisionId>(s);
    check::<PolicyBundleId>(s);
    check::<InstallId>(s);
    check::<EventId>(s);
    check::<ChallengeId>(s);
    check::<ContainmentId>(s);
    check::<SecurityKeyId>(s);
    check::<CeremonyId>(s);
    check::<InvestigationId>(s);
    check::<SupportSessionId>(s);
    check::<PlanId>(s);
    if let Some(entity) = QidEntity::of(s) {
        assert!(s.starts_with(entity.prefix()));
    }
});
