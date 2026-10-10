//! Larkspur's everyday activity (demo plan step 5b): who opens what, as tool calls on
//! larkspur-drive. Each day is generated from the seed, so a run can be repeated exactly.
//!
//! People mostly open what their open work links and their team's documents, in working hours,
//! with a little ordinary noise: a document from another team, a late night. Agents act for
//! their owners on the same kind of material (SalesGenie for Nora on her accounts), or on their
//! own authority when nobody is there (FinanceSummarizer on the month's reports).
//!
//! Each access carries why the generator chose it ([`Why`]), the ground truth that detection is
//! later scored against: `Noise` is ordinary but out of pattern, and nothing here is an attack.
//! The scenarios add those ([`crate::scenarios`]).

use std::collections::{BTreeMap, HashMap};

use crate::{
    org::{Company, Profile},
    rng::Rng,
    world::{person_documents, work_items, Resource},
};

/// The two larkspur-drive tools (ADR 0050).
pub const READ: &str = "read_document";
pub const LIST: &str = "list_folder";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Why {
    /// Linked from the accessor's open work: an account, a ticket, a statement of work.
    Assigned,
    /// A document of the accessor's own team.
    Team,
    /// Ordinary but out of pattern: another team's document.
    Noise,
    /// Part of a demo plan scenario, named ([`crate::scenarios`]).
    Scenario(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Access {
    /// Minutes after midnight, Larkspur time.
    pub minute: u32,
    /// A person's userName, or a service account's ID.
    pub subject: String,
    /// The person an agent acts for.
    pub on_behalf_of: Option<String>,
    /// Whether a person is there to answer a challenge.
    pub human_present: bool,
    /// The subject's team (a person's own, or the person it acts for), sent as the token's groups.
    pub team: &'static str,
    pub tool: &'static str,
    pub resource: String,
    pub why: Why,
}

/// How many documents a person opens on an ordinary day.
const READS_PER_PERSON: usize = 12;
/// How many tool calls an agent makes in a day.
const CALLS_PER_AGENT: usize = 25;
/// Out of 100: another team's document; a late-night access.
const NOISE: usize = 5;
const LATE: usize = 3;

/// Every access on day `day` of the company's activity, in time order.
pub fn day(company: &Company, profile: Profile, day: u32) -> Vec<Access> {
    let seed = company.seed;
    let mut rng = Rng::new(seed ^ 0xAC71_0000 ^ (u64::from(day) << 32));
    let mut docs = Documents { company, profile, seed, cache: HashMap::new() };

    let mut assigned: HashMap<&str, Vec<String>> = HashMap::new();
    let work = work_items(company, profile, seed);
    for w in &work {
        for a in &w.assignees {
            assigned.entry(a.as_str()).or_default().extend(w.resources.iter().cloned());
        }
    }
    // Ordered, so picking another team is the same on every run.
    let mut by_team: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, p) in company.people.iter().enumerate() {
        by_team.entry(p.team).or_default().push(i);
    }
    let index_of: HashMap<&str, usize> =
        company.people.iter().enumerate().map(|(i, p)| (p.user_name.as_str(), i)).collect();

    let mut out = Vec::new();
    for (i, p) in company.people.iter().enumerate() {
        let mine = assigned.get(p.user_name.as_str()).map(Vec::as_slice).unwrap_or_default();
        for _ in 0..READS_PER_PERSON - 2 + rng.below(5) {
            let (resource, why) = pick(&mut rng, &mut docs, mine, &by_team[p.team], i, &by_team);
            out.push(Access {
                minute: minute(&mut rng),
                subject: p.user_name.clone(),
                on_behalf_of: None,
                human_present: true,
                team: p.team,
                tool: tool_for(&resource),
                resource,
                why,
            });
        }
    }

    for sa in &company.service_accounts {
        let Some(&owner) = index_of.get(sa.owner.as_str()) else { continue };
        let person = &company.people[owner];
        // Scheduled agents work alone; the others act for their owner, who is there.
        let unattended = !sa.acts_for_owner();
        let mine = assigned.get(person.user_name.as_str()).map(Vec::as_slice).unwrap_or_default();
        for _ in 0..CALLS_PER_AGENT {
            let (resource, why) = pick(&mut rng, &mut docs, mine, &by_team[person.team], owner, &by_team);
            out.push(Access {
                minute: if unattended { 6 * 60 + rng.below(60) as u32 } else { minute(&mut rng) },
                subject: sa.id.clone(),
                on_behalf_of: (!unattended).then(|| person.user_name.clone()),
                human_present: !unattended,
                team: person.team,
                tool: tool_for(&resource),
                resource,
                why,
            });
        }
    }
    out.sort_by_key(|a| a.minute);
    out
}

/// One resource for someone at `me` (an index into the company's people): mostly their work's,
/// else their team's, sometimes another team's.
fn pick(
    rng: &mut Rng,
    docs: &mut Documents,
    assigned: &[String],
    team: &[usize],
    me: usize,
    by_team: &BTreeMap<&str, Vec<usize>>,
) -> (String, Why) {
    let roll = rng.below(100);
    if roll < NOISE {
        let teams: Vec<&Vec<usize>> = by_team.values().filter(|t| !t.contains(&me)).collect();
        let other = rng_pick(rng, &teams);
        if let Some(d) = docs.random(rng, other) {
            return (d, Why::Noise);
        }
    }
    if !assigned.is_empty() && roll < 60 {
        return (assigned[rng.below(assigned.len())].clone(), Why::Assigned);
    }
    match docs.random(rng, team) {
        Some(d) => (d, Why::Team),
        // A team with no documents: its own folder.
        None => (format!("synthetic:folder:{}", docs.company.people[me].team), Why::Team),
    }
}

fn rng_pick<'a, T>(rng: &mut Rng, items: &'a [T]) -> &'a T {
    &items[rng.below(items.len())]
}

/// Folders are listed; everything else is read.
fn tool_for(resource: &str) -> &'static str {
    if resource.starts_with("synthetic:folder:") {
        LIST
    } else {
        READ
    }
}

/// Working hours, 8:00 to 18:00, with the odd late night.
fn minute(rng: &mut Rng) -> u32 {
    if rng.below(100) < LATE {
        (22 * 60 + rng.below(3 * 60)) as u32 % (24 * 60)
    } else {
        (8 * 60 + rng.below(10 * 60)) as u32
    }
}

/// People's documents, generated once each as they're needed.
struct Documents<'a> {
    company: &'a Company,
    profile: Profile,
    seed: u64,
    cache: HashMap<usize, Vec<Resource>>,
}

impl Documents<'_> {
    /// A document owned by someone in `people`, if they have any.
    fn random(&mut self, rng: &mut Rng, people: &[usize]) -> Option<String> {
        let owner = *rng_pick(rng, people);
        let (company, profile, seed) = (self.company, self.profile, self.seed);
        let docs = self.cache.entry(owner).or_insert_with(|| person_documents(company, profile, seed, owner));
        (!docs.is_empty()).then(|| docs[rng.below(docs.len())].id.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{
        org::{cast, generate},
        world::{folders_and_repositories, scenario_resources},
    };

    fn all_ids(c: &Company) -> HashSet<String> {
        let mut ids: HashSet<String> = folders_and_repositories(c).into_iter().map(|r| r.id).collect();
        ids.extend(scenario_resources(c).into_iter().map(|r| r.id));
        for i in 0..c.people.len() {
            ids.extend(person_documents(c, Profile::Demo, c.seed, i).into_iter().map(|r| r.id));
        }
        ids
    }

    #[test]
    fn a_day_is_the_same_every_time_and_differs_from_the_next() {
        let c = generate(Profile::Demo, 1);
        assert_eq!(day(&c, Profile::Demo, 0), day(&c, Profile::Demo, 0));
        assert_ne!(day(&c, Profile::Demo, 0), day(&c, Profile::Demo, 1));
    }

    #[test]
    fn an_ordinary_day_looks_ordinary() {
        let c = generate(Profile::Demo, 1);
        let accesses = day(&c, Profile::Demo, 0);
        assert!((2_000..=4_000).contains(&accesses.len()), "{}", accesses.len());
        assert!(accesses.windows(2).all(|w| w[0].minute <= w[1].minute), "in time order");

        let ids = all_ids(&c);
        assert!(accesses.iter().all(|a| ids.contains(&a.resource)), "every access is to something that exists");
        let share = |why: Why| accesses.iter().filter(|a| a.why == why).count() * 100 / accesses.len();
        assert!((2..=10).contains(&share(Why::Noise)), "noise {}%", share(Why::Noise));
        assert!(share(Why::Assigned) > 20, "assigned {}%", share(Why::Assigned));
        let working = accesses.iter().filter(|a| (8 * 60..18 * 60).contains(&a.minute)).count();
        assert!(working * 100 / accesses.len() > 90, "mostly in working hours");
        assert!(accesses.iter().any(|a| a.tool == LIST) && accesses.iter().any(|a| a.tool == READ));
    }

    #[test]
    fn agents_act_for_their_owners_or_alone_when_scheduled() {
        let c = generate(Profile::Demo, 1);
        let accesses = day(&c, Profile::Demo, 0);
        let nora = cast("sales-northeast-finserv").unwrap();
        let genie: Vec<&Access> = accesses.iter().filter(|a| a.subject == "sales-genie").collect();
        assert_eq!(genie.len(), CALLS_PER_AGENT);
        assert!(genie.iter().all(|a| a.on_behalf_of.as_deref() == Some(nora.as_str()) && a.human_present));
        assert!(!genie.iter().any(|a| a.resource == "synthetic:doc:strategic-plan-fy27"), "an ordinary day");

        let summarizer: Vec<&Access> = accesses.iter().filter(|a| a.subject == "finance-summarizer").collect();
        assert!(summarizer.iter().all(|a| a.on_behalf_of.is_none() && !a.human_present && a.team == "finance"));
    }
}
