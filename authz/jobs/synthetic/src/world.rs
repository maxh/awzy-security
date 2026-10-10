//! Larkspur's resources and work items (demo plan step 2): team folders, documents with text and
//! a true sensitivity, engineering repositories, the scenarios' documents, and the work that ties
//! people and agents to resources (sales accounts, tickets, a contractor's statement of work).
//!
//! Every person's documents come from a seed derived from theirs, so the same seed always gives
//! the same world and the scale profile's million documents can be generated and sent a person at
//! a time instead of all at once.
//!
//! Only what a source would carry is sent. A document's tier reaches awzy only when it has a
//! sensitivity label (as Purview or Drive labels do); the rest arrive unclassified, for the
//! classifier to place. The true tier ([`Resource::truth`]) and the scenario a resource belongs to
//! stay here, to measure classification and detection against.

use serde::Serialize;
use serde_json::{json, Value};

use crate::{
    org::{cast, Company, Person, Profile},
    rng::Rng,
};

/// The `source` every synthetic resource and work item carries.
pub const SOURCE: &str = "synthetic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    Public,
    Internal,
    Confidential,
    CrownJewels,
}

pub const TIERS: [Tier; 4] = [Tier::Public, Tier::Internal, Tier::Confidential, Tier::CrownJewels];

impl Tier {
    /// As awzy-context and the Cedar schema name it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::CrownJewels => "crown_jewels",
        }
    }

    /// The sensitivity label a source would show.
    fn label(self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Internal => "Internal",
            Self::Confidential => "Confidential",
            Self::CrownJewels => "Highly Confidential",
        }
    }

    fn lower(self) -> Self {
        match self {
            Self::CrownJewels => Self::Confidential,
            Self::Confidential => Self::Internal,
            t => t,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Share {
    /// person | group | anyone
    pub kind: &'static str,
    pub id: String,
    /// reader | writer
    pub role: &'static str,
}

fn group(id: &str, role: &'static str) -> Share {
    Share { kind: "group", id: id.into(), role }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Resource {
    pub id: String,
    /// folder | document | repository
    pub kind: &'static str,
    pub name: String,
    pub parent: Option<String>,
    pub owner: String,
    pub labels: Vec<String>,
    pub sharing: Vec<Share>,
    pub text: String,
    /// The tier a careful reviewer would give it: the ground truth. Not sent.
    pub truth: Tier,
    /// The tier its sensitivity label carries, sent as `sensitivity`; `None` when unlabeled. Can be
    /// lower than `truth`: some documents are mislabeled, as in real drives.
    pub labeled: Option<Tier>,
    /// The demo plan scenario it's part of. Not sent.
    pub scenario: Option<&'static str>,
}

impl Resource {
    /// The body for awzy-context's `/v1/resources`.
    pub fn to_input(&self) -> Value {
        json!({
            "id": self.id,
            "source": SOURCE,
            "kind": self.kind,
            "name": self.name,
            "parent": self.parent,
            "owner": self.owner,
            "sensitivity": self.labeled.map(Tier::as_str),
            "labels": self.labels,
            "sharing": self.sharing,
            "text": self.text,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WorkItem {
    pub id: String,
    /// account | ticket | project | statement_of_work
    pub kind: &'static str,
    pub title: String,
    pub assignees: Vec<String>,
    pub resources: Vec<String>,
}

impl WorkItem {
    /// The body for awzy-context's `/v1/work-items`.
    pub fn to_input(&self) -> Value {
        json!({
            "id": self.id,
            "source": SOURCE,
            "kind": self.kind,
            "title": self.title,
            "assignees": self.assignees,
            "resources": self.resources,
        })
    }
}

/// A kind of document a team keeps in one folder, and how sensitive those documents truly are:
/// weights for public, internal, confidential, and crown jewels.
struct Topic {
    folder: &'static str,
    titles: &'static [&'static str],
    weights: [u32; 4],
}

const fn topic(folder: &'static str, titles: &'static [&'static str], weights: [u32; 4]) -> Topic {
    Topic { folder, titles, weights }
}

const SALES: &[Topic] = &[
    topic("Accounts", &["Account plan", "Meeting notes", "Renewal forecast"], [0, 60, 40, 0]),
    topic("Reports", &["Weekly pipeline report", "Win/loss review", "Territory report"], [0, 70, 30, 0]),
    topic("Collateral", &["Product one-pager", "Case study", "Pricing overview"], [60, 40, 0, 0]),
];
const ENGINEERING: &[Topic] = &[
    topic("Design docs", &["Design doc", "RFC", "Architecture review"], [0, 85, 15, 0]),
    topic("Runbooks", &["Runbook", "Incident postmortem", "On-call notes"], [0, 90, 10, 0]),
];

const EXECUTIVE: &[Topic] = &[
    topic("Board", &["Board meeting minutes", "Board deck", "Executive compensation review"], [0, 10, 40, 50]),
    topic("Leadership", &["Leadership offsite notes", "Org planning", "Quarterly priorities"], [0, 40, 50, 10]),
];
const STRATEGY: &[Topic] = &[
    topic("Planning", &["Market analysis", "Competitive landscape", "Scenario model"], [0, 40, 50, 10]),
    topic("Corporate development", &["Acquisition screen", "Partnership memo"], [0, 10, 60, 30]),
];
const FINANCE: &[Topic] = &[
    topic("Reports", &["Monthly close", "Budget vs. actuals", "Cash flow forecast"], [0, 30, 60, 10]),
    topic("Audit", &["Audit workpapers", "Controls testing"], [0, 40, 60, 0]),
    topic("Payroll", &["Payroll register", "Compensation bands"], [0, 0, 70, 30]),
];
const IT_ADMIN: &[Topic] = &[
    topic("Runbooks", &["Admin runbook", "Backup procedure", "Network diagram"], [0, 60, 40, 0]),
    topic("Access", &["Access review", "Privileged accounts list"], [0, 20, 70, 10]),
];
const SECURITY: &[Topic] = &[
    topic("Incidents", &["Incident report", "Threat assessment"], [0, 30, 60, 10]),
    topic("Policies", &["Security policy", "Acceptable use policy"], [30, 60, 10, 0]),
];
const SUPPORT: &[Topic] = &[
    topic("Articles", &["How-to article", "FAQ", "Troubleshooting guide"], [70, 30, 0, 0]),
    topic("Escalations", &["Escalation notes", "Customer case summary"], [0, 60, 40, 0]),
];
const LEGAL: &[Topic] = &[
    topic("Contracts", &["Customer contract", "Vendor agreement", "NDA"], [0, 20, 80, 0]),
    topic("Matters", &["Litigation memo", "Regulatory filing draft"], [0, 10, 60, 30]),
];
const HR: &[Topic] = &[
    topic("People", &["Performance review", "Offer letter", "Investigation notes"], [0, 10, 80, 10]),
    topic("Policies", &["Employee handbook", "Benefits overview", "Holiday calendar"], [40, 60, 0, 0]),
];
const QUARRY_LABS: &[Topic] =
    &[topic("Deliverables", &["Sprint report", "Delivery plan", "Test results"], [0, 90, 10, 0])];

/// Each team's folders and documents.
fn topics(team: &str) -> &'static [Topic] {
    match team {
        "executive" => EXECUTIVE,
        "strategy" => STRATEGY,
        "finance" => FINANCE,
        "it-admin" => IT_ADMIN,
        "security" => SECURITY,
        "support" => SUPPORT,
        "legal" => LEGAL,
        "hr" => HR,
        "contractors-quarry-labs" => QUARRY_LABS,
        t if t.starts_with("sales-") => SALES,
        t if t.starts_with("engineering-") => ENGINEERING,
        _ => &[],
    }
}

/// Customer accounts per sales team, for account plans and the reps' work items.
fn accounts(team: &str) -> &'static [&'static str] {
    match team {
        "sales-northeast-finserv" => &[
            "Harborline Bank",
            "Granite Trust",
            "Bayview Credit Union",
            "Kestrel Asset Management",
            "Old Mill Insurance",
            "Northgate Securities",
        ],
        "sales-west-tech" => {
            &["Redwood Analytics", "Cobalt Systems", "Sierra Robotics", "Pacific Cloudworks", "Lumen Labs"]
        }
        "sales-emea" => {
            &["Albion Partners", "Rhein Capital", "Nordlys Energy", "Iberia Retail Group", "Adriatic Shipping"]
        }
        _ => &[],
    }
}

/// Repositories per engineering team.
fn repositories(team: &str) -> &'static [(&'static str, Tier)] {
    match team {
        "engineering-platform" => &[
            ("payments-service", Tier::Confidential),
            ("ledger", Tier::Confidential),
            ("identity-service", Tier::Internal),
            ("infra-terraform", Tier::Confidential),
            ("observability", Tier::Internal),
        ],
        "engineering-client-apps" => &[
            ("client-portal-web", Tier::Internal),
            ("mobile-app", Tier::Internal),
            ("reporting-ui", Tier::Internal),
            ("design-system", Tier::Public),
        ],
        _ => &[],
    }
}

fn docs_per_person(profile: Profile) -> usize {
    match profile {
        Profile::Demo => 20,
        Profile::Scale => 95,
    }
}

fn team_name(team: &str) -> String {
    match team {
        "sales-northeast-finserv" => "Sales, Northeast Financial Services".into(),
        "sales-west-tech" => "Sales, West Coast Technology".into(),
        "sales-emea" => "Sales, EMEA".into(),
        "engineering-platform" => "Engineering, Platform".into(),
        "engineering-client-apps" => "Engineering, Client Apps".into(),
        "it-admin" => "IT Administration".into(),
        "hr" => "People (HR)".into(),
        "contractors-quarry-labs" => "Quarry Labs (contractor)".into(),
        t => {
            let mut c = t.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        }
    }
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

fn team_folder(team: &str) -> String {
    format!("synthetic:folder:{team}")
}

fn topic_folder(team: &str, folder: &str) -> String {
    format!("synthetic:folder:{team}/{}", slug(folder))
}

fn weighted(rng: &mut Rng, weights: [u32; 4]) -> Tier {
    let total: u32 = weights.iter().sum();
    let mut n = (rng.next_u64() % u64::from(total)) as u32;
    for (tier, w) in TIERS.into_iter().zip(weights) {
        if n < w {
            return tier;
        }
        n -= w;
    }
    Tier::Internal
}

/// What a document's text says about its sensitivity, as a reader would see it.
fn marking(tier: Tier) -> &'static str {
    match tier {
        Tier::Public => "Approved for external distribution.",
        Tier::Internal => "For Larkspur Capital employees.",
        Tier::Confidential => "Confidential: Larkspur internal, limited distribution. Do not forward outside the team.",
        Tier::CrownJewels => {
            "Strictly confidential. Board and executive committee only. Contains material non-public \
             information; trading on it is prohibited."
        }
    }
}

const FILLER: &[&str] = &[
    "Open questions are listed at the end; comments are welcome until Friday.",
    "Figures are as of the last close and will be refreshed next month.",
    "Owners and next steps are in the table below.",
    "This replaces the previous version; earlier drafts are archived in the same folder.",
    "Decisions made in the review are marked as such.",
];

const QUARTERS: [&str; 5] = ["Q1 2026", "Q2 2026", "Q3 2026", "Q4 2026", "FY27"];

fn quarter(rng: &mut Rng) -> &'static str {
    QUARTERS[rng.below(QUARTERS.len())]
}

/// Team folders, each topic's folder, and engineering repositories.
pub fn folders_and_repositories(company: &Company) -> Vec<Resource> {
    let mut out = Vec::new();
    let teams = teams(company);
    for (team, lead) in &teams {
        out.push(Resource {
            id: team_folder(team),
            kind: "folder",
            name: team_name(team),
            parent: None,
            owner: lead.clone(),
            labels: vec![],
            sharing: vec![group(team, "writer")],
            text: String::new(),
            truth: Tier::Internal,
            labeled: None,
            scenario: None,
        });
        for t in topics(team) {
            out.push(Resource {
                id: topic_folder(team, t.folder),
                kind: "folder",
                name: t.folder.into(),
                parent: Some(team_folder(team)),
                owner: lead.clone(),
                labels: vec![],
                sharing: vec![group(team, "writer")],
                text: String::new(),
                truth: Tier::Internal,
                labeled: None,
                scenario: None,
            });
        }
        for (repo, tier) in repositories(team) {
            out.push(Resource {
                id: format!("synthetic:repo:{repo}"),
                kind: "repository",
                name: (*repo).into(),
                parent: None,
                owner: lead.clone(),
                labels: vec![team.to_string()],
                sharing: vec![group(team, "writer"), group("engineering", "reader")],
                text: format!("Source for {repo}, maintained by {}.", team_name(team)),
                truth: *tier,
                labeled: (*tier >= Tier::Confidential).then_some(*tier),
                scenario: None,
            });
        }
    }
    out
}

/// Teams in company order, with their leads (each team's first person).
fn teams(company: &Company) -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for p in &company.people {
        if !out.iter().any(|(t, _)| *t == p.team) {
            out.push((p.team, p.user_name.clone()));
        }
    }
    out
}

fn lead(company: &Company, team: &str) -> String {
    company.people.iter().find(|p| p.team == team).map(|p| p.user_name.clone()).unwrap_or_default()
}

/// The demo plan's scenario documents, at fixed IDs.
pub fn scenario_resources(company: &Company) -> Vec<Resource> {
    let doc = |id: &str, team: &str, folder: &str, name: &str, owner: String, truth: Tier, text: &str| Resource {
        id: format!("synthetic:doc:{id}"),
        kind: "document",
        name: name.into(),
        parent: Some(topic_folder(team, folder)),
        owner,
        labels: vec![truth.label().into()],
        sharing: vec![group(team, "writer")],
        text: format!("{text} {}", marking(truth)),
        truth,
        labeled: Some(truth),
        scenario: None,
    };
    let nora = cast("sales-northeast-finserv").expect("cast");

    let mut plan = doc(
        "strategic-plan-fy27",
        "strategy",
        "Planning",
        "FY27 Strategic Plan (board draft)",
        lead(company, "strategy"),
        Tier::CrownJewels,
        "Company-level strategy for FY27: target markets, planned acquisitions, and the earnings outlook \
         shared with the board ahead of the investor update.",
    );
    plan.sharing.push(group("finance", "reader"));
    plan.sharing.push(group("executive", "reader"));
    // Overshared to all of sales for the investor update: the static check lets SalesGenie and
    // its person through, as in the paper, and Beyond Zero is what catches it.
    plan.sharing.push(group("sales", "reader"));
    plan.scenario = Some("rogue-agent");

    let mut report = doc(
        "northeast-finserv-sales-q3",
        "sales-northeast-finserv",
        "Reports",
        "Q3 2026 sales report, Northeast Financial Services",
        nora,
        Tier::Internal,
        "Quarter-to-date bookings, pipeline by account, and renewals at risk for the Northeast financial \
         services team.",
    );
    report.sharing.push(group("sales", "reader"));
    report.scenario = Some("rogue-agent");

    let mut payments = doc(
        "payments-architecture",
        "engineering-platform",
        "Design docs",
        "Payments platform architecture",
        lead(company, "engineering-platform"),
        Tier::Confidential,
        "How client money moves through the payments service and ledger: settlement windows, \
         reconciliation, and the keys that sign transfers.",
    );
    // Shared with all of engineering, Quarry Labs' contractors included, though their statement of
    // work doesn't cover it: what the curious contractor finds open.
    payments.sharing.push(group("engineering", "reader"));
    payments.scenario = Some("curious-contractor");

    let mut export = doc(
        "client-holdings-export",
        "finance",
        "Reports",
        "Client holdings export, September 2026",
        lead(company, "finance"),
        Tier::Confidential,
        "Positions and balances for every client account as of September 30, exported for the \
         quarter-end review.",
    );
    export.scenario = Some("curious-contractor");

    let mut runbook = doc(
        "production-admin-runbook",
        "it-admin",
        "Runbooks",
        "Production administration runbook",
        lead(company, "it-admin"),
        Tier::Confidential,
        "Break-glass access to production, rotating the domain admin credentials, and restoring the \
         directory from backup.",
    );
    runbook.scenario = Some("foolish-admin");

    vec![plan, report, payments, export, runbook]
}

/// The documents person `index` (in `company.people`) owns. Deterministic per person, so they can
/// be generated a person at a time.
pub fn person_documents(company: &Company, profile: Profile, seed: u64, index: usize) -> Vec<Resource> {
    let p = &company.people[index];
    let topics = topics(p.team);
    if topics.is_empty() {
        return vec![];
    }
    let mut rng = Rng::new(seed ^ 0x5EED_D0C5 ^ ((index as u64) << 20));
    let accounts = accounts(p.team);
    (0..docs_per_person(profile))
        .map(|k| {
            let t = rng.pick(topics);
            let truth = weighted(&mut rng, t.weights);
            let stem = *rng.pick(t.titles);
            let name = if t.folder == "Accounts" && !accounts.is_empty() {
                format!("{stem}: {}", account_of(index, &mut rng, accounts))
            } else {
                format!("{stem}, {}", quarter(&mut rng))
            };
            document(p, index, k, t, truth, name, &mut rng)
        })
        .collect()
}

/// One of the rep's own accounts.
fn account_of(index: usize, rng: &mut Rng, accounts: &[&'static str]) -> &'static str {
    let mine = rep_accounts(index, accounts);
    mine[rng.below(mine.len())]
}

/// The three accounts the rep at `index` covers.
fn rep_accounts(index: usize, accounts: &[&'static str]) -> Vec<&'static str> {
    (0..3).map(|i| accounts[(index + i) % accounts.len()]).collect()
}

fn document(p: &Person, index: usize, k: usize, t: &Topic, truth: Tier, name: String, rng: &mut Rng) -> Resource {
    // Sensitivity labels: most crown jewels, some confidential, few others. A few labels are one
    // tier too low, as in real drives.
    let labeled_share = match truth {
        Tier::CrownJewels => 90,
        Tier::Confidential => 40,
        _ => 10,
    };
    let labeled = (rng.below(100) < labeled_share).then(|| if rng.below(100) < 5 { truth.lower() } else { truth });
    let mut sharing = vec![group(p.team, "writer")];
    // Oversharing: a few confidential and crown-jewel documents are open to anyone with the link.
    if truth >= Tier::Confidential && rng.below(100) < 3 {
        sharing.push(Share { kind: "anyone", id: String::new(), role: "reader" });
    } else if rng.below(100) < 10 {
        sharing.push(group("everyone", "reader"));
    }
    let text = format!(
        "{name}. Prepared by {} for {}. {} {}",
        p.display_name(),
        team_name(p.team),
        rng.pick(FILLER),
        marking(truth)
    );
    Resource {
        id: format!("synthetic:doc:{index:05}-{k:03}"),
        kind: "document",
        name,
        parent: Some(topic_folder(p.team, t.folder)),
        owner: p.user_name.clone(),
        labels: labeled.map(|l| vec![l.label().to_string()]).unwrap_or_default(),
        sharing,
        text,
        truth,
        labeled,
        scenario: None,
    }
}

/// Who works on what: each sales rep's accounts (with SalesGenie on Nora's), engineering tickets,
/// Quarry Labs' statement of work, and the FY27 strategic plan.
pub fn work_items(company: &Company, profile: Profile, seed: u64) -> Vec<WorkItem> {
    let mut items = people_work(company, profile, seed);
    // An agent working for its owner is on its owner's work too (SalesGenie on Nora's accounts),
    // so its own assignment check passes where hers does (ADR 0046).
    for item in &mut items {
        let agents: Vec<String> = company
            .service_accounts
            .iter()
            .filter(|sa| sa.acts_for_owner() && item.assignees.contains(&sa.owner))
            .map(|sa| sa.id.clone())
            .collect();
        item.assignees.extend(agents);
    }
    items
}

/// People's work items: sales accounts, engineering tickets, the contractor's statement of work,
/// and the strategy team's plan.
fn people_work(company: &Company, profile: Profile, seed: u64) -> Vec<WorkItem> {
    let mut items = Vec::new();
    for (index, p) in company.people.iter().enumerate() {
        let team_accounts = accounts(p.team);
        if !team_accounts.is_empty() {
            let docs = person_documents(company, profile, seed, index);
            for account in rep_accounts(index, team_accounts) {
                let assignees = vec![p.user_name.clone()];
                items.push(WorkItem {
                    id: format!("synthetic:account:{}-{index:05}", slug(account)),
                    kind: "account",
                    title: format!("{account} ({})", team_name(p.team)),
                    assignees,
                    resources: docs.iter().filter(|d| d.name.ends_with(account)).map(|d| d.id.clone()).collect(),
                });
            }
        }
        if p.team.starts_with("engineering-") {
            let docs = person_documents(company, profile, seed, index);
            let repos = repositories(p.team);
            for n in 0..2 {
                let (repo, _) = repos[(index + n) % repos.len()];
                let mut resources = vec![format!("synthetic:repo:{repo}")];
                resources.extend(
                    docs.iter()
                        .filter(|d| d.parent.as_deref().unwrap_or("").ends_with("/design-docs"))
                        .take(1)
                        .map(|d| d.id.clone()),
                );
                items.push(WorkItem {
                    id: format!("synthetic:ticket:ENG-{}", index * 2 + n + 100),
                    kind: "ticket",
                    title: format!("{} work on {repo}", if n == 0 { "Feature" } else { "Maintenance" }),
                    assignees: vec![p.user_name.clone()],
                    resources,
                });
            }
        }
    }

    // The contractor's scope: the client apps and their own deliverables, not the platform.
    let contractors: Vec<String> =
        company.people.iter().filter(|p| p.team == "contractors-quarry-labs").map(|p| p.user_name.clone()).collect();
    let mut sow = vec![topic_folder("contractors-quarry-labs", "Deliverables")];
    sow.extend(repositories("engineering-client-apps").iter().map(|(r, _)| format!("synthetic:repo:{r}")));
    items.push(WorkItem {
        id: "synthetic:sow:quarry-labs-2026".into(),
        kind: "statement_of_work",
        title: "Quarry Labs: client apps development, 2026".into(),
        assignees: contractors,
        resources: sow,
    });

    let mut planners: Vec<String> =
        company.people.iter().filter(|p| p.team == "strategy").map(|p| p.user_name.clone()).collect();
    planners.push(lead(company, "finance"));
    items.push(WorkItem {
        id: "synthetic:project:fy27-strategic-plan".into(),
        kind: "project",
        title: "FY27 strategic plan".into(),
        assignees: planners,
        resources: vec!["synthetic:doc:strategic-plan-fy27".into()],
    });
    items
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::*;
    use crate::org::generate;

    fn world(seed: u64) -> (Company, Vec<Resource>, Vec<WorkItem>) {
        let c = generate(Profile::Demo, seed);
        let mut all = folders_and_repositories(&c);
        all.extend(scenario_resources(&c));
        for i in 0..c.people.len() {
            all.extend(person_documents(&c, Profile::Demo, seed, i));
        }
        let work = work_items(&c, Profile::Demo, seed);
        (c, all, work)
    }

    #[test]
    fn the_same_seed_gives_the_same_world() {
        let (_, a, wa) = world(1);
        let (_, b, wb) = world(1);
        assert_eq!(a, b);
        assert_eq!(wa, wb);
        assert_ne!(a, world(2).1);
    }

    #[test]
    fn the_demo_world_has_the_planned_size_and_mix() {
        let (_, all, work) = world(1);
        let docs: Vec<&Resource> = all.iter().filter(|r| r.kind == "document").collect();
        assert!((3_500..=5_000).contains(&docs.len()), "{}", docs.len());
        let share = |t: Tier| docs.iter().filter(|d| d.truth == t).count() * 100 / docs.len();
        assert!((2..=15).contains(&share(Tier::Public)), "public {}%", share(Tier::Public));
        assert!((45..=80).contains(&share(Tier::Internal)), "internal {}%", share(Tier::Internal));
        assert!((12..=40).contains(&share(Tier::Confidential)), "confidential {}%", share(Tier::Confidential));
        assert!((1..=10).contains(&share(Tier::CrownJewels)), "crown jewels {}%", share(Tier::CrownJewels));
        let unlabeled = docs.iter().filter(|d| d.labeled.is_none()).count() * 100 / docs.len();
        assert!(unlabeled > 50, "most documents arrive unclassified: {unlabeled}%");
        assert!(docs.iter().any(|d| d.labeled.is_some_and(|l| l < d.truth)), "some are mislabeled");
        assert!(docs.iter().any(|d| d.sharing.iter().any(|s| s.kind == "anyone")), "some are overshared");
        assert!(work.len() > 100, "{}", work.len());
    }

    #[test]
    fn everything_refers_to_things_that_exist() {
        let (c, all, work) = world(3);
        let ids: HashSet<&str> = all.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), all.len(), "IDs are unique");
        let people: HashSet<&str> = c.people.iter().map(|p| p.user_name.as_str()).collect();
        let accounts: HashSet<&str> = c.service_accounts.iter().map(|s| s.id.as_str()).collect();
        for r in &all {
            assert!(r.parent.as_deref().is_none_or(|p| ids.contains(p)), "{} has a real parent", r.id);
            assert!(people.contains(r.owner.as_str()), "{} has a real owner", r.id);
        }
        for w in &work {
            assert!(w.resources.iter().all(|r| ids.contains(r.as_str())), "{} links real resources", w.id);
            assert!(w.assignees.iter().all(|a| people.contains(a.as_str()) || accounts.contains(a.as_str())));
        }
        let mut seen: HashMap<&str, usize> = HashMap::new();
        for w in &work {
            *seen.entry(w.id.as_str()).or_default() += 1;
        }
        assert!(seen.values().all(|n| *n == 1), "work item IDs are unique");
    }

    /// How awzy-classify does on the generated documents against the true tiers, with each
    /// document's text and with metadata only. Printed for `bazel test --test_output=all`.
    #[test]
    fn the_classifier_against_ground_truth() {
        use awzy_classify::{classify, Facts, Tier as Classified};

        let (_, all, _) = world(1);
        let by_id: HashMap<&str, &Resource> = all.iter().map(|r| (r.id.as_str(), r)).collect();
        let docs: Vec<&Resource> = all.iter().filter(|r| r.kind == "document").collect();
        let tier = |t: Tier| Classified::parse(t.as_str()).unwrap();

        for with_text in [true, false] {
            let (mut exact, mut under, mut over, mut undecided, mut leaked) = (0, 0, 0, 0, 0);
            for d in &docs {
                let mut folders = Vec::new();
                let mut parent = d.parent.as_deref();
                while let Some(f) = parent.and_then(|id| by_id.get(id)) {
                    folders.push(f.name.as_str());
                    parent = f.parent.as_deref();
                }
                let facts = Facts {
                    kind: d.kind,
                    name: &d.name,
                    sensitivity: d.labeled.map(tier),
                    labels: &d.labels,
                    folders: &folders,
                    text: with_text.then_some(d.text.as_str()),
                };
                let truth = tier(d.truth);
                match classify(&facts).map(|c| c.tier) {
                    None => undecided += 1,
                    Some(t) if t == truth => exact += 1,
                    Some(t) if t < truth => {
                        under += 1;
                        if t == Classified::Public {
                            leaked += 1;
                        }
                    }
                    Some(_) => over += 1,
                }
            }
            let pct = |n: usize| n * 100 / docs.len();
            println!(
                "classifier, {}: {} documents; exact {}%, under {}%, over {}%, undecided {}%",
                if with_text { "with text" } else { "metadata only" },
                docs.len(),
                pct(exact),
                pct(under),
                pct(over),
                pct(undecided)
            );
            assert_eq!(leaked, 0, "nothing more sensitive than public is ever called public");
            if with_text {
                // Generated text always carries its marking, so this measures the rules, not
                // how messy real documents are (demo plan, "What synthetic data won't tell us").
                assert_eq!((under, undecided), (0, 0), "mislabeled documents are raised to their truth");
                assert!(exact * 100 >= docs.len() * 99, "exact {exact} of {}", docs.len());
            } else {
                assert!(exact + over > 0 && undecided < docs.len(), "metadata places some documents");
            }
        }
    }

    #[test]
    fn the_scenarios_are_set_up_as_the_plan_describes() {
        let (c, all, work) = world(1);
        let find = |id: &str| all.iter().find(|r| r.id == id).unwrap();
        let plan = find("synthetic:doc:strategic-plan-fy27");
        assert_eq!((plan.truth, plan.labeled), (Tier::CrownJewels, Some(Tier::CrownJewels)));
        assert!(
            plan.sharing.iter().any(|s| s.id == "sales"),
            "overshared to sales, as the paper's static check allows"
        );
        assert_eq!(plan.to_input()["sensitivity"], "crown_jewels");

        let nora = cast("sales-northeast-finserv").unwrap();
        let noras: Vec<&WorkItem> = work.iter().filter(|w| w.assignees.contains(&nora)).collect();
        assert_eq!(noras.len(), 3, "Nora has three accounts");
        assert!(noras.iter().all(|w| w.assignees.contains(&"sales-genie".to_string())), "with SalesGenie on them");
        assert!(noras.iter().all(|w| !w.resources.contains(&plan.id)), "none of which needs the plan");

        let sam = cast("contractors-quarry-labs").unwrap();
        let sow = work.iter().find(|w| w.kind == "statement_of_work").unwrap();
        assert!(sow.assignees.contains(&sam));
        assert!(
            !sow.resources.contains(&"synthetic:doc:payments-architecture".to_string()),
            "outside the contractors' scope"
        );
        assert!(!sow.resources.contains(&"synthetic:repo:payments-service".to_string()));
        assert!(c.people.iter().any(|p| p.user_name == sam));

        let unlabeled = find("synthetic:doc:00010-000");
        assert!(unlabeled.to_input().get("truth").is_none(), "the truth isn't sent");
    }
}
