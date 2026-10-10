//! awzy-classify: metadata-first sensitivity classification (demo plan step 4, ADR 0047).
//!
//! Decides a resource's tier from what a connector sends, in order of how much it can be
//! trusted: a sensitivity label, then markings in the content excerpt, then keywords in the name
//! and folder path. A label is never lowered, only raised by content marked higher, since a
//! mislabeled document is far more often under-labeled than over. Folders are placed only by a
//! label: a folder called "Board" holds board papers, but listing it isn't reading them.
//!
//! What none of this places is left unclassified for the model step, which runs within the
//! client's LLM budget once a provider is configured.

use std::fmt;

/// Sensitivity tiers, lowest to highest, as the Cedar schema names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Public,
    Internal,
    Confidential,
    CrownJewels,
}

impl Tier {
    pub const ALL: [Tier; 4] = [Tier::Public, Tier::Internal, Tier::Confidential, Tier::CrownJewels];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::CrownJewels => "crown_jewels",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What decided a classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The connector's sensitivity, or a label naming a tier.
    Label,
    /// Markings in the text excerpt.
    Content,
    /// Keywords in the name or folder path.
    Metadata,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Content => "content",
            Self::Metadata => "metadata",
        }
    }
}

/// What the classifier reads about one resource.
#[derive(Debug, Clone, Default)]
pub struct Facts<'a> {
    /// document | folder | repository | ...
    pub kind: &'a str,
    pub name: &'a str,
    /// The tier the connector sent (a Purview or Drive label it already mapped), if any.
    pub sensitivity: Option<Tier>,
    /// Labels as the source shows them, e.g. "Highly Confidential".
    pub labels: &'a [String],
    /// The names of the folders above it, nearest first.
    pub folders: &'a [&'a str],
    /// The connector's text excerpt, if it could read one.
    pub text: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classification {
    pub tier: Tier,
    pub source: Source,
    /// One line on why, for the client portal and audits.
    pub reason: String,
}

/// Places `facts` in a tier, or `None` when nothing here decides it.
pub fn classify(facts: &Facts) -> Option<Classification> {
    let label = facts
        .sensitivity
        .map(|t| (t, "the connector's sensitivity".to_string()))
        .or_else(|| facts.labels.iter().find_map(|l| label_tier(l).map(|t| (t, format!("labeled \"{l}\"")))));
    let evidence = if facts.kind == "folder" {
        None
    } else {
        let content = facts
            .text
            .and_then(|t| matched(MARKINGS, t))
            .map(|(t, phrase)| (t, Source::Content, format!("content marked \"{phrase}\"")));
        content.or_else(|| metadata(facts))
    };
    match (label, evidence) {
        (Some((label, why)), Some((tier, source, reason))) if tier > label => {
            Some(Classification { tier, source, reason: format!("{reason}, above {why} ({label})") })
        }
        (Some((tier, why)), _) => Some(Classification { tier, source: Source::Label, reason: why }),
        (None, Some((tier, source, reason))) => Some(Classification { tier, source, reason }),
        (None, None) => None,
    }
}

/// A source's label text as a tier: "Highly Confidential", "Internal Only", "Public".
pub fn label_tier(label: &str) -> Option<Tier> {
    let l = normalize(label);
    if ["highly confidential", "restricted", "secret", "crown jewels"].iter().any(|k| has(&l, k)) {
        Some(Tier::CrownJewels)
    } else if has(&l, "confidential") {
        Some(Tier::Confidential)
    } else if has(&l, "internal") || has(&l, "general") {
        Some(Tier::Internal)
    } else if has(&l, "public") {
        Some(Tier::Public)
    } else {
        None
    }
}

/// Phrases that mark a document's sensitivity in its text, highest tier first: the first tier
/// with a match wins, so "strictly confidential" is never read as just "confidential".
const MARKINGS: &[(Tier, &[&str])] = &[
    (
        Tier::CrownJewels,
        &[
            "strictly confidential",
            "highly confidential",
            "material non public information",
            "board and executive committee only",
            "attorney client privileged",
        ],
    ),
    (Tier::Confidential, &["confidential", "do not forward", "limited distribution", "need to know"]),
    (Tier::Public, &["approved for external distribution", "for public release", "public document"]),
    (Tier::Internal, &["internal use only", "for internal use", "employees only", "employees"]),
];

/// Keywords in a name or folder that say what a document is, highest tier first. Only kinds of
/// document that are nearly always in one tier are listed: "account plan" isn't, so it's left to
/// the content or the model. Nothing here says "public": a name like "case study" is often an
/// internal draft, and calling it public is the costliest mistake, so that takes a label or a
/// marking.
const KEYWORDS: &[(Tier, &[&str])] = &[
    (
        Tier::CrownJewels,
        &[
            "board",
            "strategic plan",
            "acquisition",
            "merger",
            "m a",
            "executive compensation",
            "payroll register",
            "investigation",
        ],
    ),
    (
        Tier::Confidential,
        &[
            "payroll",
            "compensation",
            "contract",
            "nda",
            "litigation",
            "audit",
            "incident report",
            "privileged accounts",
            "performance review",
            "offer letter",
            "cash flow",
        ],
    ),
];

fn metadata(facts: &Facts) -> Option<(Tier, Source, String)> {
    if let Some((tier, k)) = matched(KEYWORDS, facts.name) {
        return Some((tier, Source::Metadata, format!("the name mentions \"{k}\"")));
    }
    facts.folders.iter().find_map(|f| {
        matched(KEYWORDS, f).map(|(tier, k)| (tier, Source::Metadata, format!("in a folder about \"{k}\" ({f})")))
    })
}

/// The first tier, in table order, with a phrase in `text`, and the phrase.
fn matched(table: &[(Tier, &[&'static str])], text: &str) -> Option<(Tier, &'static str)> {
    let t = normalize(text);
    table.iter().find_map(|(tier, phrases)| phrases.iter().find(|p| has(&t, p)).map(|p| (*tier, *p)))
}

/// Lowercase words separated by single spaces, padded, so phrases match whole words only:
/// "M&A" becomes " m a ", "one-pager" " one pager ".
fn normalize(s: &str) -> String {
    let mut out = String::from(" ");
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.ends_with(' ') {
            out.push(' ');
        }
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn has(normalized: &str, phrase: &str) -> bool {
    normalized.contains(&format!(" {phrase} "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc<'a>(name: &'a str, text: Option<&'a str>) -> Facts<'a> {
        Facts { kind: "document", name, text, ..Default::default() }
    }

    fn tier(f: &Facts) -> Option<Tier> {
        classify(f).map(|c| c.tier)
    }

    #[test]
    fn a_label_decides_and_content_can_only_raise_it() {
        let labels = vec!["Highly Confidential".to_string()];
        let f = Facts { labels: &labels, ..doc("Q3 notes", Some("For Larkspur employees.")) };
        let c = classify(&f).unwrap();
        assert_eq!((c.tier, c.source), (Tier::CrownJewels, Source::Label), "{}", c.reason);

        let mislabeled = Facts { sensitivity: Some(Tier::Internal), ..doc("Q3 notes", Some("Strictly confidential.")) };
        let c = classify(&mislabeled).unwrap();
        assert_eq!((c.tier, c.source), (Tier::CrownJewels, Source::Content));
        assert!(c.reason.contains("above the connector's sensitivity (internal)"), "{}", c.reason);
    }

    #[test]
    fn content_markings_decide_before_names() {
        assert_eq!(tier(&doc("Board deck", Some("Approved for external distribution."))), Some(Tier::Public));
        assert_eq!(tier(&doc("Notes", Some("Confidential: limited distribution."))), Some(Tier::Confidential));
        assert_eq!(
            tier(&doc("Notes", Some("Strictly confidential. Contains material non-public information."))),
            Some(Tier::CrownJewels),
            "the higher marking wins over the word confidential"
        );
        assert_eq!(tier(&doc("Notes", Some("For Larkspur Capital employees."))), Some(Tier::Internal));
    }

    #[test]
    fn names_and_folders_decide_when_nothing_else_does() {
        assert_eq!(tier(&doc("M&A pipeline", None)), Some(Tier::CrownJewels));
        assert_eq!(tier(&doc("Vendor contract renewal", None)), Some(Tier::Confidential));
        assert_eq!(tier(&doc("Customer case study", None)), None, "public takes a label or a marking");
        assert_eq!(tier(&doc("Dashboard ideas", None)), None, "whole words only: dashboard isn't board");

        let folders = ["Payroll", "Finance"];
        assert_eq!(tier(&Facts { folders: &folders, ..doc("March", None) }), Some(Tier::Confidential));
        let public = ["Case study archive"];
        assert_eq!(tier(&Facts { folders: &public, ..doc("Draft", None) }), None);
    }

    #[test]
    fn folders_are_placed_only_by_labels() {
        assert_eq!(tier(&Facts { kind: "folder", ..doc("Board", Some("Strictly confidential")) }), None);
        let labeled = Facts { kind: "folder", sensitivity: Some(Tier::Confidential), ..doc("Board", None) };
        assert_eq!(tier(&labeled), Some(Tier::Confidential));
    }

    #[test]
    fn label_text_maps_to_tiers() {
        assert_eq!(label_tier("Highly Confidential"), Some(Tier::CrownJewels));
        assert_eq!(label_tier("Confidential \\ All Employees"), Some(Tier::Confidential));
        assert_eq!(label_tier("General"), Some(Tier::Internal));
        assert_eq!(label_tier("Public"), Some(Tier::Public));
        assert_eq!(label_tier("Q3"), None);
        assert_eq!(Tier::parse("crown_jewels"), Some(Tier::CrownJewels));
    }
}
