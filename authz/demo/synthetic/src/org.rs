//! Larkspur Capital: the people, groups, and AI service accounts of the synthetic company
//! (docs/plans/demo-plan.md). Generated from a seed; the same seed and profile always give the same
//! company.

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::{names, rng::Rng};

/// People's email domain. `.example` is reserved, so no real mail goes anywhere.
pub const DOMAIN: &str = "larkspur.example";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    /// About 200 people: the demo install and local development.
    Demo,
    /// About 10,000 people: the scale tests.
    Scale,
}

impl Profile {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "demo" => Some(Self::Demo),
            "scale" => Some(Self::Scale),
            _ => None,
        }
    }

    fn multiplier(self) -> usize {
        match self {
            Self::Demo => 1,
            Self::Scale => 51,
        }
    }

    fn service_accounts(self) -> usize {
        match self {
            Self::Demo => 4,
            Self::Scale => 200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Employee,
    Contractor,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Human {
    /// The person's email, which is also their ID in awzy (SCIM `userName`).
    pub user_name: String,
    pub given_name: String,
    pub family_name: String,
    pub title: String,
    pub team: &'static str,
    pub department: &'static str,
    /// The manager's `user_name`.
    pub manager: Option<String>,
    pub employee_number: String,
    pub kind: Kind,
    /// Contractors' end date, as the IdP would carry it.
    pub contract_end: Option<&'static str>,
}

impl Human {
    pub fn display_name(&self) -> String {
        format!("{} {}", self.given_name, self.family_name)
    }
}

/// An IdP group. Members are people (by `user_name`) and other groups (by name).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Group {
    pub name: String,
    pub humans: Vec<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ServiceAccount {
    pub id: String,
    /// The owner's `user_name`.
    pub owner: String,
    pub ai: bool,
    pub groups: Vec<String>,
    pub description: String,
}

impl ServiceAccount {
    /// Whether it works for its owner, who is there to answer for it. A scheduled agent works
    /// alone, on its own authority.
    pub fn acts_for_owner(&self) -> bool {
        self.id != "finance-summarizer"
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Company {
    pub seed: u64,
    /// Managers come before the people who report to them.
    pub humans: Vec<Human>,
    /// Groups come after the groups they contain.
    pub groups: Vec<Group>,
    pub service_accounts: Vec<ServiceAccount>,
}

struct Team {
    slug: &'static str,
    department: &'static str,
    /// People in the demo profile; the scale profile multiplies it, except where `fixed`.
    size: usize,
    fixed: bool,
    title: &'static str,
    lead_title: &'static str,
    contractors: bool,
}

const fn team(
    slug: &'static str,
    department: &'static str,
    size: usize,
    title: &'static str,
    lead: &'static str,
) -> Team {
    Team { slug, department, size, fixed: false, title, lead_title: lead, contractors: false }
}

/// The executive team comes first: its first member is the CEO, and team leads report to it.
const TEAMS: &[Team] = &[
    Team {
        slug: "executive",
        department: "executive",
        size: 5,
        fixed: true,
        title: "Managing Director",
        lead_title: "Chief Executive Officer",
        contractors: false,
    },
    team("sales-northeast-finserv", "sales", 18, "Account Executive", "Head of Sales, Northeast Financial Services"),
    team("sales-west-tech", "sales", 16, "Account Executive", "Head of Sales, West Coast Technology"),
    team("sales-emea", "sales", 14, "Account Executive", "Head of Sales, EMEA"),
    team("strategy", "strategy", 8, "Strategy Associate", "Head of Strategy"),
    team("finance", "finance", 16, "Financial Analyst", "Head of Finance"),
    team("engineering-platform", "engineering", 28, "Software Engineer", "Engineering Manager, Platform"),
    team("engineering-client-apps", "engineering", 26, "Software Engineer", "Engineering Manager, Client Apps"),
    team("it-admin", "it", 8, "Systems Administrator", "IT Manager"),
    team("security", "security", 8, "Security Engineer", "Head of Security"),
    team("support", "support", 22, "Support Specialist", "Head of Support"),
    team("legal", "legal", 6, "Counsel", "General Counsel"),
    team("hr", "hr", 8, "People Partner", "Head of People"),
    Team {
        slug: "contractors-quarry-labs",
        department: "engineering",
        size: 15,
        fixed: false,
        title: "Contract Software Engineer",
        lead_title: "Engagement Lead, Quarry Labs",
        contractors: true,
    },
];

const CONTRACT_END: &str = "2027-03-31";

/// The people the demo's scenarios are about, at fixed places: (team, index in team, name).
const CAST: &[(&str, usize, &str, &str)] = &[
    // Prompts SalesGenie (the rogue agent).
    ("sales-northeast-finserv", 1, "Nora", "Patel"),
    // Whose credentials are stolen (the curious contractor).
    ("contractors-quarry-labs", 1, "Sam", "Okafor"),
    // The suddenly foolish administrator.
    ("it-admin", 1, "Lee", "Kowalski"),
    // Opens far more files than the peer group (the peer outlier).
    ("finance", 1, "Riley", "Chen"),
];

/// The `user_name` of the person at a cast position, e.g. `cast("finance")`.
pub fn cast(team: &str) -> Option<String> {
    CAST.iter().find(|(t, ..)| *t == team).map(|(_, _, given, family)| user_name(given, family))
}

pub fn user_name(given: &str, family: &str) -> String {
    format!("{}.{}@{DOMAIN}", given.to_lowercase(), family.to_lowercase())
}

pub fn generate(profile: Profile, seed: u64) -> Company {
    let mut rng = Rng::new(seed);
    let (given, family) = (names::given(), names::family());
    let mut taken: HashSet<String> = CAST.iter().map(|(_, _, g, f)| user_name(g, f)).collect();
    let mut humans: Vec<Human> = Vec::new();
    let mut leads: HashMap<&'static str, String> = HashMap::new();
    let executives = TEAMS[0].size;

    for (t, team) in TEAMS.iter().enumerate() {
        let size = if team.fixed { team.size } else { team.size * profile.multiplier() };
        for i in 0..size {
            let (given_name, family_name) = match CAST.iter().find(|(s, n, ..)| *s == team.slug && *n == i) {
                Some((_, _, g, f)) => (g.to_string(), f.to_string()),
                None => unique_name(&mut rng, &given, &family, &mut taken),
            };
            let manager = if t == 0 && i == 0 {
                None
            } else if t == 0 {
                Some(humans[0].user_name.clone())
            } else if i == 0 {
                // Team leads report to one of the executives other than the CEO.
                Some(humans[1 + (t - 1) % (executives - 1)].user_name.clone())
            } else {
                Some(leads[team.slug].clone())
            };
            let human = Human {
                user_name: user_name(&given_name, &family_name),
                given_name,
                family_name,
                title: if i == 0 { team.lead_title } else { team.title }.to_string(),
                team: team.slug,
                department: team.department,
                manager,
                employee_number: format!("LC-{:05}", humans.len() + 1),
                kind: if team.contractors { Kind::Contractor } else { Kind::Employee },
                contract_end: team.contractors.then_some(CONTRACT_END),
            };
            if i == 0 {
                leads.insert(team.slug, human.user_name.clone());
            }
            humans.push(human);
        }
    }

    let groups = groups(&humans);
    let service_accounts = service_accounts(profile, &mut rng, &humans, &leads);
    Company { seed, humans, groups, service_accounts }
}

fn unique_name(rng: &mut Rng, given: &[&str], family: &[&str], taken: &mut HashSet<String>) -> (String, String) {
    for attempt in 0.. {
        let (g, f) = (rng.pick(given).to_string(), rng.pick(family).to_string());
        // After a few collisions, a number keeps names unique in the scale profile.
        let f = if attempt < 8 { f } else { format!("{f}{attempt}") };
        if taken.insert(user_name(&g, &f)) {
            return (g, f);
        }
    }
    unreachable!("the loop only ends by returning")
}

/// One group per team; a group per department holding its teams (when the department has
/// teams of another name); `contractors`, `managers`, and `everyone`.
fn groups(humans: &[Human]) -> Vec<Group> {
    let mut groups: Vec<Group> = TEAMS
        .iter()
        .map(|t| Group {
            name: t.slug.to_string(),
            humans: humans.iter().filter(|p| p.team == t.slug).map(|p| p.user_name.clone()).collect(),
            groups: vec![],
        })
        .collect();

    let mut top: Vec<String> = Vec::new();
    for team in TEAMS {
        if top.contains(&team.department.to_string()) {
            continue;
        }
        let members: Vec<String> = TEAMS
            .iter()
            .filter(|t| t.department == team.department && t.slug != t.department)
            .map(|t| t.slug.to_string())
            .collect();
        if !members.is_empty() {
            groups.push(Group { name: team.department.to_string(), humans: vec![], groups: members });
        }
        top.push(team.department.to_string());
    }

    let contractor_teams = TEAMS.iter().filter(|t| t.contractors).map(|t| t.slug.to_string()).collect();
    groups.push(Group { name: "contractors".into(), humans: vec![], groups: contractor_teams });
    let managers: HashSet<&str> = humans.iter().filter_map(|p| p.manager.as_deref()).collect();
    let managers = humans.iter().filter(|p| managers.contains(p.user_name.as_str())).map(|p| p.user_name.clone());
    groups.push(Group { name: "managers".into(), humans: managers.collect(), groups: vec![] });
    groups.push(Group { name: "everyone".into(), humans: vec![], groups: top });
    groups
}

fn service_accounts(
    profile: Profile,
    rng: &mut Rng,
    humans: &[Human],
    leads: &HashMap<&'static str, String>,
) -> Vec<ServiceAccount> {
    let sa = |id: &str, owner: String, team: &str, description: &str| ServiceAccount {
        id: id.to_string(),
        owner,
        ai: true,
        groups: vec![team.to_string()],
        description: description.to_string(),
    };
    let mut accounts = vec![
        sa(
            "sales-genie",
            cast("sales-northeast-finserv").expect("cast"),
            "sales-northeast-finserv",
            "Reads sales reports for the rep who prompts it",
        ),
        sa(
            "code-helper",
            leads["engineering-platform"].clone(),
            "engineering-platform",
            "Engineering's coding agent: reads repositories and tickets",
        ),
        sa("support-bot", leads["support"].clone(), "support", "Answers customer questions from support articles"),
        sa(
            "finance-summarizer",
            leads["finance"].clone(),
            "finance",
            "Summarizes the month's finance reports on a schedule, with no human present",
        ),
    ];
    // The scale profile adds agents spread across the teams, each owned by one of its people.
    let teams: Vec<&Team> = TEAMS.iter().filter(|t| !t.fixed).collect();
    for n in accounts.len()..profile.service_accounts() {
        let team = teams[n % teams.len()];
        let members: Vec<&Human> = humans.iter().filter(|p| p.team == team.slug).collect();
        let owner = rng.pick(&members).user_name.clone();
        accounts.push(sa(&format!("agent-{n:03}"), owner, team.slug, "A team's AI assistant (scale profile)"));
    }
    accounts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_company() {
        assert_eq!(generate(Profile::Demo, 1), generate(Profile::Demo, 1));
        assert_ne!(generate(Profile::Demo, 1).humans, generate(Profile::Demo, 2).humans);
    }

    #[test]
    fn profiles_have_the_planned_sizes() {
        let demo = generate(Profile::Demo, 1);
        assert!((190..=210).contains(&demo.humans.len()), "{}", demo.humans.len());
        assert_eq!(demo.service_accounts.len(), 4);
        let scale = generate(Profile::Scale, 1);
        assert!((9_500..=10_500).contains(&scale.humans.len()), "{}", scale.humans.len());
        assert_eq!(scale.service_accounts.len(), 200);
    }

    #[test]
    fn the_org_is_consistent() {
        for profile in [Profile::Demo, Profile::Scale] {
            let c = generate(profile, 7);
            let index: HashMap<&str, &Human> = c.humans.iter().map(|p| (p.user_name.as_str(), p)).collect();
            assert_eq!(index.len(), c.humans.len(), "user names are unique");
            assert_eq!(c.humans.iter().filter(|p| p.manager.is_none()).count(), 1, "one CEO");
            for (i, p) in c.humans.iter().enumerate() {
                assert!(p.user_name.ends_with(DOMAIN) && p.user_name.is_ascii());
                if let Some(m) = &p.manager {
                    let pos = c.humans.iter().position(|q| &q.user_name == m).expect("manager exists");
                    assert!(pos < i, "managers come first");
                }
                assert_eq!(p.kind == Kind::Contractor, p.contract_end.is_some());
            }
            let names: HashSet<&str> = c.groups.iter().map(|g| g.name.as_str()).collect();
            assert_eq!(names.len(), c.groups.len(), "group names are unique");
            for (i, g) in c.groups.iter().enumerate() {
                assert!(g.humans.iter().all(|p| index.contains_key(p.as_str())));
                for member in &g.groups {
                    let pos = c.groups.iter().position(|h| &h.name == member).expect("member group exists");
                    assert!(pos < i, "groups come after the groups they contain");
                }
            }
            for sa in &c.service_accounts {
                assert!(index.contains_key(sa.owner.as_str()), "{} has a real owner", sa.id);
                assert!(sa.groups.iter().all(|g| names.contains(g.as_str())));
            }
        }
    }

    #[test]
    fn the_cast_is_where_the_scenarios_expect() {
        let c = generate(Profile::Demo, 3);
        let find = |team: &str| c.humans.iter().find(|p| Some(&p.user_name) == cast(team).as_ref()).unwrap();
        assert_eq!(find("sales-northeast-finserv").user_name, "nora.patel@larkspur.example");
        assert_eq!(find("contractors-quarry-labs").kind, Kind::Contractor);
        assert_eq!(find("it-admin").title, "Systems Administrator");
        assert_eq!(find("finance").team, "finance");
        let genie = c.service_accounts.iter().find(|s| s.id == "sales-genie").unwrap();
        assert_eq!(genie.owner, "nora.patel@larkspur.example");
        let contractors = c.groups.iter().find(|g| g.name == "contractors").unwrap();
        assert_eq!(contractors.groups, vec!["contractors-quarry-labs".to_string()]);
    }
}
