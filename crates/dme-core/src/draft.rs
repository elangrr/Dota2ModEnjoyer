use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const NUMBERS_URL: &str = "https://raw.githubusercontent.com/elangrr/Dota2ModEnjoyer/data/draft.json";

pub const NUMBERS_LIMIT: usize = 4 * 1024 * 1024;

pub const FORMAT: u32 = 1;

const WHAT: &str = "the draft helper's numbers";

const PLAYED_SHARE: f64 = 0.10;
const DOUBT: f64 = 400.0;
const ENOUGH_IN_A_ROLE: u64 = 100;
const PERCENT_TO_A_STEP: f64 = 25.0;
const PARTNER_WEIGHT: f32 = 0.5;
const SUPPORT_WEIGHT: f32 = 0.5;
const TREND_LEAST: f32 = 1.0;
const TREND_SURE: f32 = 2.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    AllPick,
    CaptainsMode,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::AllPick, Mode::CaptainsMode];

    pub fn name(self) -> &'static str {
        match self {
            Mode::AllPick => "All Pick",
            Mode::CaptainsMode => "Captain's Mode",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Mode::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Ban,
    Pick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CmStep {
    pub first: bool,
    pub action: Action,
    pub phase: usize,
    pub slot: usize,
}

pub const CAPTAINS_STEPS: [CmStep; 24] = [
    CmStep { first: true, action: Action::Ban, phase: 0, slot: 0 },
    CmStep { first: true, action: Action::Ban, phase: 0, slot: 1 },
    CmStep { first: false, action: Action::Ban, phase: 0, slot: 0 },
    CmStep { first: false, action: Action::Ban, phase: 0, slot: 1 },
    CmStep { first: true, action: Action::Ban, phase: 0, slot: 2 },
    CmStep { first: false, action: Action::Ban, phase: 0, slot: 2 },
    CmStep { first: false, action: Action::Ban, phase: 0, slot: 3 },
    CmStep { first: true, action: Action::Pick, phase: 0, slot: 0 },
    CmStep { first: false, action: Action::Pick, phase: 0, slot: 0 },
    CmStep { first: true, action: Action::Ban, phase: 1, slot: 3 },
    CmStep { first: true, action: Action::Ban, phase: 1, slot: 4 },
    CmStep { first: false, action: Action::Ban, phase: 1, slot: 4 },
    CmStep { first: false, action: Action::Pick, phase: 1, slot: 1 },
    CmStep { first: true, action: Action::Pick, phase: 1, slot: 1 },
    CmStep { first: true, action: Action::Pick, phase: 1, slot: 2 },
    CmStep { first: false, action: Action::Pick, phase: 1, slot: 2 },
    CmStep { first: false, action: Action::Pick, phase: 1, slot: 3 },
    CmStep { first: true, action: Action::Pick, phase: 1, slot: 3 },
    CmStep { first: true, action: Action::Ban, phase: 2, slot: 5 },
    CmStep { first: false, action: Action::Ban, phase: 2, slot: 5 },
    CmStep { first: true, action: Action::Ban, phase: 2, slot: 6 },
    CmStep { first: false, action: Action::Ban, phase: 2, slot: 6 },
    CmStep { first: true, action: Action::Pick, phase: 2, slot: 4 },
    CmStep { first: false, action: Action::Pick, phase: 2, slot: 4 },
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    #[default]
    Team,
    Lane,
}

impl Style {
    pub const ALL: [Style; 2] = [Style::Team, Style::Lane];

    pub fn name(self) -> &'static str {
        match self {
            Style::Team => "Team composition",
            Style::Lane => "Lane winning",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Style::default()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rank {
    Herald,
    Guardian,
    Crusader,
    #[default]
    Archon,
    Legend,
    Ancient,
    Divine,
    Immortal,
}

impl Rank {
    pub const ALL: [Rank; 8] = [Rank::Herald, Rank::Guardian, Rank::Crusader, Rank::Archon, Rank::Legend, Rank::Ancient, Rank::Divine, Rank::Immortal];

    pub fn name(self) -> &'static str {
        match self {
            Rank::Herald => "Herald",
            Rank::Guardian => "Guardian",
            Rank::Crusader => "Crusader",
            Rank::Archon => "Archon",
            Rank::Legend => "Legend",
            Rank::Ancient => "Ancient",
            Rank::Divine => "Divine",
            Rank::Immortal => "Immortal",
        }
    }

    pub fn is_default(&self) -> bool {
        *self == Rank::default()
    }

    pub fn place(self) -> usize {
        self as usize
    }

    fn pair(self) -> usize {
        self.place() / 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Carry,
    Mid,
    Offlane,
    SoftSupport,
    HardSupport,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Carry, Role::Mid, Role::Offlane, Role::SoftSupport, Role::HardSupport];

    pub fn name(self) -> &'static str {
        match self {
            Role::Carry => "Carry",
            Role::Mid => "Mid",
            Role::Offlane => "Offlane",
            Role::SoftSupport => "Soft support",
            Role::HardSupport => "Hard support",
        }
    }

    pub fn as_played(self) -> &'static str {
        match self {
            Role::Carry => "as carry",
            Role::Mid => "in mid",
            Role::Offlane => "in the offlane",
            Role::SoftSupport => "as soft support",
            Role::HardSupport => "as hard support",
        }
    }

    pub fn place(self) -> usize {
        self as usize
    }

    pub fn lane_opponents(self) -> &'static [Role] {
        match self {
            Role::Carry | Role::HardSupport => &[Role::Offlane, Role::SoftSupport],
            Role::Mid => &[Role::Mid],
            Role::Offlane | Role::SoftSupport => &[Role::Carry, Role::HardSupport],
        }
    }

    pub fn lane_weight(self, enemy: Role) -> f32 {
        let core = matches!(self, Role::Carry | Role::Offlane);
        let support = matches!(enemy, Role::SoftSupport | Role::HardSupport);
        if core && support { SUPPORT_WEIGHT } else { 1.0 }
    }

    pub fn lane_partner(self) -> Option<Role> {
        match self {
            Role::Carry => Some(Role::HardSupport),
            Role::HardSupport => Some(Role::Carry),
            Role::Offlane => Some(Role::SoftSupport),
            Role::SoftSupport => Some(Role::Offlane),
            Role::Mid => None,
        }
    }
}

#[derive(Deserialize)]
struct Header {
    format: u32,
}

#[derive(Deserialize)]
struct File {
    made: u64,
    #[serde(default)]
    source: String,
    #[serde(default)]
    patch: String,
    #[serde(default)]
    weeks: u32,
    heroes: Vec<String>,
    ranks: Vec<Played>,
    pairs: Vec<Pairs>,
    #[serde(default)]
    lanes: Vec<Lanes>,
    #[serde(default)]
    weekly: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Weekly {
    ranks: Vec<[Played; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trend {
    pub now: f32,
    pub before: f32,
    pub way: Way,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Rising,
    Falling,
    Level,
}

#[derive(Deserialize)]
struct Played {
    played: Vec<Vec<u64>>,
    won: Vec<Vec<u64>>,
}

#[derive(Deserialize)]
struct Pairs {
    against: Vec<Vec<i16>>,
    with: Vec<Vec<i16>>,
}

#[derive(Deserialize)]
struct Lanes {
    played: Vec<Vec<u64>>,
    won: Vec<Vec<u64>>,
    drawn: Vec<Vec<u64>>,
    against: Vec<Vec<Vec<i16>>>,
    with: Vec<Vec<Vec<i16>>>,
}

#[derive(Debug)]
pub struct Numbers {
    pub made: u64,
    pub source: String,
    pub patch: String,
    pub weeks: u32,
    heroes: Vec<String>,
    places: HashMap<String, usize>,
    ranks: Vec<Played>,
    pairs: Vec<Pairs>,
    lanes: Vec<Lanes>,
    weekly: Vec<[Played; 2]>,
}

impl std::fmt::Debug for Lanes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the lanes of a pair of ranks")
    }
}

impl std::fmt::Debug for Played {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the matches of a rank")
    }
}

impl std::fmt::Debug for Pairs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the tables of {} heroes", self.against.len())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub hero: String,
    pub points: f32,
    pub win_rate: f32,
    pub matches: u64,
    pub pick_rate: f32,
    pub reasons: Vec<Reason>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reason {
    pub hero: String,
    pub beside: bool,
    pub points: f32,
}

impl Numbers {
    pub fn parse(bytes: &[u8]) -> Result<Numbers> {
        let wrong = |detail: &str| Error::format(WHAT, detail);
        let header: Header = serde_json::from_slice(bytes).map_err(|error| wrong(&error.to_string()))?;
        if header.format != FORMAT {
            return Err(wrong("they are of a shape this version of the app does not read"));
        }
        let file: File = serde_json::from_slice(bytes).map_err(|error| wrong(&error.to_string()))?;
        let heroes = file.heroes.len();
        if heroes == 0 || heroes > 1000 {
            return Err(wrong("the list of heroes is not one"));
        }
        if file.ranks.len() != Rank::ALL.len() {
            return Err(wrong("there are not eight ranks"));
        }
        if file.pairs.len() != Rank::ALL.len() / 2 {
            return Err(wrong("there are not four pairs of ranks"));
        }
        let by_role = |table: &[Vec<u64>]| table.len() == Role::ALL.len() && table.iter().all(|row| row.len() == heroes);
        let by_hero = |table: &[Vec<i16>]| table.len() == heroes && table.iter().all(|row| row.len() == heroes);
        let fits = file.ranks.iter().all(|rank| by_role(&rank.played) && by_role(&rank.won)) && file.pairs.iter().all(|pair| by_hero(&pair.against) && by_hero(&pair.with));
        if !fits {
            return Err(wrong("a table does not fit the list of heroes"));
        }
        if !file.lanes.is_empty() {
            if file.lanes.len() != Rank::ALL.len() / 2 {
                return Err(wrong("the lanes are not of four pairs of ranks"));
            }
            let by_role_and_hero = |tables: &[Vec<Vec<i16>>]| tables.len() == Role::ALL.len() && tables.iter().all(|table| by_hero(table));
            let lanes_fit = file.lanes.iter().all(|lanes| by_role(&lanes.played) && by_role(&lanes.won) && by_role(&lanes.drawn) && by_role_and_hero(&lanes.against) && by_role_and_hero(&lanes.with));
            if !lanes_fit {
                return Err(wrong("a table of the lanes does not fit the list of heroes"));
            }
        }
        let weekly = file.weekly.and_then(|weekly| serde_json::from_value::<Weekly>(weekly).ok()).map(|weekly| weekly.ranks).filter(|ranks| ranks.len() == Rank::ALL.len() && ranks.iter().flatten().all(|week| by_role(&week.played) && by_role(&week.won))).unwrap_or_default();
        let places = file.heroes.iter().enumerate().map(|(place, hero)| (hero.clone(), place)).collect();
        Ok(Numbers { made: file.made, source: file.source, patch: file.patch, weeks: file.weeks, heroes: file.heroes, places, ranks: file.ranks, pairs: file.pairs, lanes: file.lanes, weekly })
    }

    pub fn has_weeks(&self) -> bool {
        !self.weekly.is_empty()
    }

    pub fn trend(&self, rank: Rank, role: Role, hero: &str) -> Option<Trend> {
        let [newest, before] = self.weekly.get(rank.place())?;
        let hero = *self.places.get(hero)?;
        let at = role.place();
        let week = |week: &Played| -> Option<(f64, f64)> {
            let (matches, wins) = (week.played[at][hero], week.won[at][hero]);
            (matches >= ENOUGH_IN_A_ROLE && wins <= matches).then(|| (wins as f64 / matches as f64, matches as f64))
        };
        let ((now, n_now), (then, n_then)) = (week(newest)?, week(before)?);
        let change = ((now - then) * 100.0) as f32;
        let middle = (now * n_now + then * n_then) / (n_now + n_then);
        let luck = ((middle * (1.0 - middle) * (1.0 / n_now + 1.0 / n_then)).sqrt() * 100.0) as f32;
        let way = match change {
            change if change >= TREND_LEAST && change >= TREND_SURE * luck => Way::Rising,
            change if change <= -TREND_LEAST && -change >= TREND_SURE * luck => Way::Falling,
            _ => Way::Level,
        };
        Some(Trend { now: (now * 100.0) as f32, before: (then * 100.0) as f32, way })
    }

    pub fn to_ban(&self, rank: Rank, role: Option<Role>, mine: &[&str], left_out: &[&str]) -> Vec<Suggestion> {
        let (ranked, paired) = (&self.ranks[rank.place()], &self.pairs[rank.pair()]);
        let placed = |heroes: &[&str]| -> Vec<usize> { heroes.iter().filter_map(|hero| self.places.get(*hero).copied()).collect() };
        let (mine, left_out) = (placed(mine), placed(left_out));
        let targets: Vec<(usize, f64)> = match (mine.is_empty(), role) {
            (false, _) => mine.iter().map(|&hero| (hero, 1.0)).collect(),
            (true, Some(role)) => (0..self.heroes.len()).filter_map(|hero| Some((hero, ranked.record(hero, Some(role))?.0 as f64))).collect(),
            (true, None) => Vec::new(),
        };

        let mut suggestions: Vec<Suggestion> = (0..self.heroes.len())
            .filter(|hero| !mine.contains(hero) && !left_out.contains(hero))
            .filter_map(|hero| {
                let (matches, wins) = ranked.record(hero, None)?;
                let share = wins as f64 / matches as f64;
                let own = (share - 0.5) * 100.0 * matches as f64 / (matches as f64 + DOUBT);
                let against = |target: usize| f64::from(paired.against[hero][target]) / 10.0;
                let others = targets.iter().filter(|(target, _)| *target != hero);
                let (sum, weight) = others.fold((0.0, 0.0), |(sum, all), &(target, weight)| (sum + against(target) * weight, all + weight));
                let threat = if weight > 0.0 { sum / weight } else { 0.0 };
                let reasons = mine.iter().map(|&target| Reason { hero: self.heroes[target].clone(), beside: false, points: against(target) as f32 }).collect();
                let mut suggestion = Suggestion::of(self.heroes[hero].clone(), own, share, matches, reasons);
                suggestion.points = (own + threat) as f32;
                Some(suggestion)
            })
            .collect();
        best_first(&mut suggestions);
        suggestions
    }

    pub fn to_ban_in_draft(&self, rank: Rank, role: Option<Role>, mine: &[&str], team: &[&str], enemies: &[&str], left_out: &[&str]) -> Vec<Suggestion> {
        if team.is_empty() && enemies.is_empty() {
            return self.to_ban(rank, role, mine, left_out);
        }
        let (ranked, paired) = (&self.ranks[rank.place()], &self.pairs[rank.pair()]);
        let placed = |heroes: &[&str]| -> Vec<usize> { heroes.iter().filter_map(|hero| self.places.get(*hero).copied()).collect() };
        let (mine, team, enemies, left_out) = (placed(mine), placed(team), placed(enemies), placed(left_out));
        let targets = if team.is_empty() { &mine } else { &team };

        let mut suggestions: Vec<Suggestion> = (0..self.heroes.len())
            .filter(|hero| !mine.contains(hero) && !team.contains(hero) && !enemies.contains(hero) && !left_out.contains(hero))
            .filter_map(|hero| {
                let (matches, wins) = ranked.record(hero, None)?;
                let share = wins as f64 / matches as f64;
                let own = (share - 0.5) * 100.0 * matches as f64 / (matches as f64 + DOUBT);
                let against_sum: f64 = targets.iter().map(|&ally| f64::from(paired.against[hero][ally]) / 10.0).sum();
                let with_sum: f64 = enemies.iter().map(|&enemy| f64::from(paired.with[hero][enemy]) / 10.0).sum();
                let threat = if targets.is_empty() { 0.0 } else { against_sum / targets.len() as f64 };
                let synergy = if enemies.is_empty() { 0.0 } else { with_sum / enemies.len() as f64 };
                let reasons = targets
                    .iter()
                    .map(|&ally| Reason { hero: self.heroes[ally].clone(), beside: false, points: f32::from(paired.against[hero][ally]) / 10.0 })
                    .chain(enemies.iter().map(|&enemy| Reason { hero: self.heroes[enemy].clone(), beside: true, points: f32::from(paired.with[hero][enemy]) / 10.0 }))
                    .collect();
                let mut suggestion = Suggestion::of(self.heroes[hero].clone(), own, share, matches, reasons);
                suggestion.points = (own + threat + synergy) as f32;
                Some(suggestion)
            })
            .collect();
        best_first(&mut suggestions);
        suggestions
    }

    pub fn suggest_popular(&self, rank: Rank, role: Role, picked: &[&str]) -> Vec<Suggestion> {
        let ranked = &self.ranks[rank.place()];
        let picked: Vec<usize> = picked.iter().filter_map(|hero| self.places.get(*hero).copied()).collect();
        let total_role_picks: u64 = ranked.played[role.place()].iter().sum();
        let total_matches = (total_role_picks as f64 / 2.0).max(1.0);
        let mut suggestions: Vec<Suggestion> = (0..self.heroes.len())
            .filter(|hero| !picked.contains(hero))
            .filter_map(|hero| {
                let (matches, wins) = ranked.record(hero, Some(role))?;
                let share = wins as f64 / matches as f64;
                let own = (share - 0.5) * 100.0 * matches as f64 / (matches as f64 + DOUBT);
                let mut suggestion = Suggestion::of(self.heroes[hero].clone(), own, share, matches, Vec::new());
                suggestion.pick_rate = ((matches as f64 / total_matches) * 100.0).min(100.0) as f32;
                Some(suggestion)
            })
            .collect();
        suggestions.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| b.win_rate.total_cmp(&a.win_rate)));
        suggestions
    }

    pub fn knows(&self, hero: &str) -> bool {
        self.places.contains_key(hero)
    }

    pub fn is_played_in(&self, rank: Rank, role: Role, hero: &str) -> bool {
        self.places.get(hero).is_some_and(|&hero| self.ranks[rank.place()].record(hero, Some(role)).is_some())
    }

    pub fn has_lanes(&self) -> bool {
        !self.lanes.is_empty()
    }

    pub fn age_in_days(&self, now: u64) -> u64 {
        now.saturating_sub(self.made) / (24 * 3600)
    }

    pub fn suggest(&self, rank: Rank, role: Option<Role>, team: &[&str], enemies: &[&str]) -> Vec<Suggestion> {
        let (ranked, paired) = (&self.ranks[rank.place()], &self.pairs[rank.pair()]);
        let placed = |heroes: &[&str]| -> Vec<usize> { heroes.iter().filter_map(|hero| self.places.get(*hero).copied()).collect() };
        let (team, enemies) = (placed(team), placed(enemies));

        let mut suggestions: Vec<Suggestion> = (0..self.heroes.len())
            .filter(|hero| !team.contains(hero) && !enemies.contains(hero))
            .filter_map(|hero| {
                let (matches, wins) = ranked.record(hero, role)?;
                let share = wins as f64 / matches as f64;
                let own = (share - 0.5) * 100.0 * matches as f64 / (matches as f64 + DOUBT);
                let reason = |other: usize, beside: bool| {
                    let table = if beside { &paired.with } else { &paired.against };
                    Reason { hero: self.heroes[other].clone(), beside, points: f32::from(table[hero][other]) / 10.0 }
                };
                let reasons = enemies.iter().map(|&enemy| reason(enemy, false)).chain(team.iter().map(|&friend| reason(friend, true))).collect();
                Some(Suggestion::of(self.heroes[hero].clone(), own, share, matches, reasons))
            })
            .collect();
        best_first(&mut suggestions);
        suggestions
    }

    pub fn suggest_in_lane(&self, rank: Rank, role: Role, partner: &[&str], opponents: &[(&str, Role)], picked: &[&str]) -> Vec<Suggestion> {
        let Some(lanes) = self.lanes.get(rank.pair()) else { return Vec::new() };
        let ranked = &self.ranks[rank.place()];
        let at = role.place();
        let placed = |heroes: &[&str]| -> Vec<usize> { heroes.iter().filter_map(|hero| self.places.get(*hero).copied()).collect() };
        let (partner, picked) = (placed(partner), placed(picked));
        let opponents: Vec<(usize, f32)> = opponents.iter().filter_map(|(hero, enemy)| Some((*self.places.get(*hero)?, role.lane_weight(*enemy)))).collect();
        let share = |hero: usize| -> Option<(f64, u64)> {
            let (met, won, drawn) = (lanes.played[at][hero], lanes.won[at][hero], lanes.drawn[at][hero]);
            (met > 0 && won + drawn <= met).then(|| ((won as f64 + drawn as f64 / 2.0) / met as f64, met))
        };
        let counted = (0..self.heroes.len()).filter(|hero| share(*hero).is_some());
        let (all, won, drawn) = counted.fold((0, 0, 0), |(all, won, drawn), hero| (all + lanes.played[at][hero], won + lanes.won[at][hero], drawn + lanes.drawn[at][hero]));
        if all == 0 {
            return Vec::new();
        }
        let usual = (won as f64 + drawn as f64 / 2.0) / all as f64;

        let mut suggestions: Vec<Suggestion> = (0..self.heroes.len())
            .filter(|hero| !picked.contains(hero) && ranked.record(*hero, Some(role)).is_some())
            .filter_map(|hero| {
                let (share, met) = share(hero)?;
                let own = (share - usual) * 100.0 * met as f64 / (met as f64 + DOUBT);
                let reason = |other: usize, beside: bool, weight: f32| {
                    let table = if beside { &lanes.with[at] } else { &lanes.against[at] };
                    Reason { hero: self.heroes[other].clone(), beside, points: f32::from(table[hero][other]) / 10.0 * weight }
                };
                let reasons = opponents.iter().map(|&(enemy, weight)| reason(enemy, false, weight)).chain(partner.iter().map(|&friend| reason(friend, true, PARTNER_WEIGHT))).collect();
                Some(Suggestion::of(self.heroes[hero].clone(), own, share, met, reasons))
            })
            .collect();
        best_first(&mut suggestions);
        suggestions
    }
}

impl Suggestion {
    fn of(hero: String, own: f64, share: f64, matches: u64, mut reasons: Vec<Reason>) -> Suggestion {
        let points = own as f32 + reasons.iter().map(|reason| reason.points).sum::<f32>();
        reasons.retain(|reason| reason.points != 0.0);
        reasons.sort_by(|a, b| b.points.abs().total_cmp(&a.points.abs()));
        Suggestion { hero, points, win_rate: (share * 100.0) as f32, matches, pick_rate: 0.0, reasons }
    }
}

fn best_first(suggestions: &mut [Suggestion]) {
    suggestions.sort_by(|a, b| b.points.total_cmp(&a.points));
}

impl Numbers {
    pub fn chance(&self, rank: Rank, team: &[(&str, Role)], enemies: &[(&str, Role)]) -> f32 {
        let (ranked, paired) = (&self.ranks[rank.place()], &self.pairs[rank.pair()]);
        let placed = |heroes: &[(&str, Role)]| -> Vec<(usize, Role)> { heroes.iter().filter_map(|(hero, role)| Some((*self.places.get(*hero)?, *role))).collect() };
        let (team, enemies) = (placed(team), placed(enemies));
        let own = |side: &[(usize, Role)]| -> f64 { side.iter().map(|(hero, role)| ranked.edge(*hero, *role)).sum() };
        let beside = |side: &[(usize, Role)]| -> f64 {
            let pairs = side.iter().enumerate().flat_map(|(place, (hero, _))| side[place + 1..].iter().map(move |(other, _)| (*hero, *other)));
            pairs.map(|(hero, other)| f64::from(paired.with[hero][other]) / 10.0).sum()
        };
        let against: f64 = team.iter().flat_map(|(hero, _)| enemies.iter().map(move |(enemy, _)| f64::from(paired.against[*hero][*enemy]) / 10.0)).sum();
        let ahead = own(&team) - own(&enemies) + beside(&team) - beside(&enemies) + against;
        (100.0 / (1.0 + (-ahead / PERCENT_TO_A_STEP).exp())) as f32
    }
}

impl Played {
    fn edge(&self, hero: usize, role: Role) -> f64 {
        let sum = |table: &[Vec<u64>]| -> u64 { table.iter().map(|row| row[hero]).sum() };
        let there = (self.played[role.place()][hero], self.won[role.place()][hero]);
        let (matches, wins) = if there.0 >= ENOUGH_IN_A_ROLE { there } else { (sum(&self.played), sum(&self.won)) };
        if matches == 0 || wins > matches {
            return 0.0;
        }
        (wins as f64 / matches as f64 - 0.5) * 100.0 * matches as f64 / (matches as f64 + DOUBT)
    }

    fn record(&self, hero: usize, role: Option<Role>) -> Option<(u64, u64)> {
        let sum = |table: &[Vec<u64>]| -> u64 { table.iter().map(|row| row[hero]).sum() };
        let all = sum(&self.played);
        let (matches, wins) = match role {
            Some(role) => (self.played[role.place()][hero], self.won[role.place()][hero]),
            None => (all, sum(&self.won)),
        };
        let played_there = matches > 0 && matches as f64 >= PLAYED_SHARE * all as f64;
        (played_there && wins <= matches).then_some((matches, wins))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const AXE: &str = "npc_dota_hero_axe";
    const LINA: &str = "npc_dota_hero_lina";
    const PUDGE: &str = "npc_dota_hero_pudge";
    const SVEN: &str = "npc_dota_hero_sven";

    fn file() -> serde_json::Value {
        let played = json!([[0, 0, 2000, 8000], [0, 9000, 2000, 0], [10000, 0, 2000, 0], [0, 1000, 2000, 0], [0, 0, 2000, 0]]);
        let won = json!([[0, 0, 1000, 4000], [0, 4500, 1000, 0], [5000, 0, 1000, 0], [0, 500, 1000, 0], [0, 0, 1000, 0]]);
        let against = json!([[0, -30, 5, 0], [30, 0, -20, 0], [-5, 20, 0, 0], [0, 0, 0, 0]]);
        let with = json!([[0, 0, 0, 0], [0, 0, 0, 15], [0, 0, 0, 0], [0, 15, 0, 0]]);
        let rank = json!({ "played": played, "won": won });
        let pair = json!({ "against": against, "with": with });
        let ranks = json!([rank, rank, rank, rank, rank, rank, rank, rank]);
        json!({ "format": 1, "made": 1_000_000, "source": "STRATZ", "patch": "7.41f", "weeks": 4, "heroes": [AXE, LINA, PUDGE, SVEN], "ranks": ranks, "pairs": [pair, pair, pair, pair] })
    }

    fn read(file: &serde_json::Value) -> Result<Numbers> {
        Numbers::parse(file.to_string().as_bytes())
    }

    fn names(suggestions: &[Suggestion]) -> Vec<&str> {
        suggestions.iter().map(|suggestion| suggestion.hero.as_str()).collect()
    }

    #[test]
    fn the_file_says_whose_numbers_it_has_and_how_old_they_are() {
        let numbers = read(&file()).unwrap();
        assert_eq!((numbers.source.as_str(), numbers.patch.as_str(), numbers.weeks), ("STRATZ", "7.41f", 4));
        assert!(numbers.knows(AXE) && !numbers.knows("npc_dota_hero_new"));
        let day = 24 * 3600;
        assert_eq!(numbers.age_in_days(1_000_000 + 3 * day + 5), 3);
        assert_eq!(numbers.age_in_days(1_000_000 + day - 1), 0);
        assert_eq!(numbers.age_in_days(5), 0, "a clock that is behind makes them new, not older than time");
    }

    #[test]
    fn a_file_of_another_shape_is_refused() {
        let refused = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut file = file();
            change(&mut file);
            read(&file).unwrap_err().to_string()
        };
        assert!(refused(&|file| file["format"] = json!(2)).contains("this version of the app does not read"));
        assert!(refused(&|file| file["heroes"] = json!([])).contains("list of heroes"));
        assert!(refused(&|file| file["ranks"].as_array_mut().unwrap().truncate(7)).contains("eight ranks"));
        assert!(refused(&|file| file["pairs"].as_array_mut().unwrap().truncate(3)).contains("four pairs"));
        assert!(refused(&|file| file["pairs"][2]["against"][1] = json!([30, 0, -20])).contains("does not fit"));
        assert!(refused(&|file| file["pairs"][0]["with"].as_array_mut().unwrap().truncate(3)).contains("does not fit"));
        assert!(refused(&|file| file["ranks"][1]["played"].as_array_mut().unwrap().truncate(4)).contains("does not fit"));
        assert!(refused(&|file| file["ranks"][3]["won"][0] = json!([1])).contains("does not fit"));
        assert!(Numbers::parse(b"nonsense").is_err());
        assert!(Numbers::parse(b"{}").is_err());
    }

    #[test]
    fn with_nothing_picked_every_hero_is_suggested_by_its_own_share_of_wins() {
        let mut file = file();
        file["ranks"][1]["won"][0][3] = json!(4800);
        let numbers = read(&file).unwrap();
        let suggestions = numbers.suggest(Rank::Guardian, None, &[], &[]);
        assert_eq!(names(&suggestions), [SVEN, AXE, LINA, PUDGE], "the best first, the rest in the order of the file");
        assert_eq!((suggestions[0].win_rate, suggestions[0].matches), (60.0, 8000));
        assert!(suggestions[0].points > 9.0 && suggestions[0].points < 10.0, "a little less than the 10% it shows");
        assert_eq!((suggestions[2].win_rate, suggestions[2].matches), (50.0, 10000));
        assert!(suggestions[0].reasons.is_empty());
        assert_eq!(names(&numbers.suggest(Rank::Immortal, None, &[], &[])), [AXE, LINA, PUDGE, SVEN]);
        assert_eq!(names(&numbers.suggest(Rank::Herald, None, &[], &[])), [AXE, LINA, PUDGE, SVEN]);
    }

    #[test]
    fn heroes_that_do_well_against_the_enemy_come_first() {
        let numbers = read(&file()).unwrap();
        let suggestions = numbers.suggest(Rank::Legend, None, &[], &[AXE]);
        assert_eq!(names(&suggestions), [LINA, SVEN, PUDGE], "the enemy's hero is not there to pick");
        assert_eq!(suggestions[0].points, 3.0);
        assert_eq!(suggestions[0].reasons, [Reason { hero: AXE.to_string(), beside: false, points: 3.0 }]);
        assert_eq!(suggestions[2].points, -0.5);
        assert!(suggestions[1].reasons.is_empty(), "a hero that changes nothing is no reason");

        let lina = &numbers.suggest(Rank::Legend, None, &[], &[PUDGE, AXE])[0];
        assert_eq!((lina.hero.as_str(), lina.points), (LINA, 1.0));
        let said: Vec<(&str, f32)> = lina.reasons.iter().map(|reason| (reason.hero.as_str(), reason.points)).collect();
        assert_eq!(said, [(AXE, 3.0), (PUDGE, -2.0)]);
    }

    #[test]
    fn heroes_that_do_well_beside_the_team_come_first() {
        let numbers = read(&file()).unwrap();
        let suggestions = numbers.suggest(Rank::Herald, None, &[LINA], &[]);
        assert_eq!(names(&suggestions), [SVEN, AXE, PUDGE]);
        assert_eq!(suggestions[0].reasons, [Reason { hero: LINA.to_string(), beside: true, points: 1.5 }]);
        assert_eq!(numbers.suggest(Rank::Herald, None, &[LINA, "npc_dota_hero_new"], &["npc_dota_hero_newer"]), suggestions);
    }

    #[test]
    fn a_role_shows_the_heroes_that_are_played_in_it() {
        let numbers = read(&file()).unwrap();
        let in_role = |role: Role| numbers.suggest(Rank::Guardian, Some(role), &[], &[]);
        assert_eq!(names(&in_role(Role::Carry)), [PUDGE, SVEN]);
        assert_eq!(names(&in_role(Role::Mid)), [LINA, PUDGE]);
        assert_eq!(names(&in_role(Role::Offlane)), [AXE, PUDGE]);
        assert_eq!(names(&in_role(Role::SoftSupport)), [LINA, PUDGE]);
        assert_eq!(names(&in_role(Role::HardSupport)), [PUDGE]);
        assert_eq!(in_role(Role::Carry)[1].matches, 8000, "the matches in that role");
    }

    #[test]
    fn a_hero_with_few_matches_in_a_role_is_not_believed_much() {
        let mut file = file();
        (file["ranks"][0]["played"][0][3], file["ranks"][0]["won"][0][3]) = (json!(100), json!(60));
        (file["ranks"][1]["played"][0][3], file["ranks"][1]["won"][0][3]) = (json!(10000), json!(6000));
        let numbers = read(&file).unwrap();
        let sven = |rank: Rank| numbers.suggest(rank, Some(Role::Carry), &[], &[]).into_iter().find(|suggestion| suggestion.hero == SVEN).unwrap();
        let (few, many) = (sven(Rank::Herald), sven(Rank::Guardian));
        assert_eq!((few.win_rate, many.win_rate), (60.0, 60.0));
        assert_eq!(few.points, 2.0, "10% more than half, of which a fifth is believed");
        assert!(many.points > 9.5 && many.points < 10.0);
    }

    #[test]
    fn a_hero_the_file_is_wrong_about_is_left_out() {
        let mut file = file();
        file["ranks"][1]["won"][2][0] = json!(99_999);
        let numbers = read(&file).unwrap();
        assert_eq!(names(&numbers.suggest(Rank::Guardian, Some(Role::Offlane), &[], &[])), [PUDGE]);
        for role in 0..5 {
            file["ranks"][1]["played"][role][2] = json!(0);
            file["ranks"][1]["won"][role][2] = json!(0);
        }
        file["ranks"][1]["won"][2][0] = json!(5000);
        let numbers = read(&file).unwrap();
        assert_eq!(names(&numbers.suggest(Rank::Guardian, None, &[], &[])), [AXE, LINA, SVEN]);
    }

    #[test]
    fn a_whole_draft_is_weighed_one_team_against_the_other() {
        let numbers = read(&file()).unwrap();
        let rank = Rank::Guardian;
        let chance = |team: &[(&str, Role)], enemies: &[(&str, Role)]| numbers.chance(rank, team, enemies);
        assert_eq!(chance(&[(AXE, Role::Offlane)], &[(SVEN, Role::Carry)]), 50.0);
        assert_eq!(chance(&[], &[]), 50.0);

        let lina = chance(&[(LINA, Role::Mid)], &[(AXE, Role::Offlane)]);
        assert!(lina > 52.9 && lina < 53.1, "{lina}");
        let axe = chance(&[(AXE, Role::Offlane)], &[(LINA, Role::Mid)]);
        assert!((lina + axe - 100.0).abs() < 0.001, "what one team has, the other lacks");

        let with_sven = chance(&[(LINA, Role::Mid), (SVEN, Role::Carry)], &[(AXE, Role::Offlane)]);
        assert!(with_sven > lina + 1.3 && with_sven < lina + 1.6, "{with_sven}");
        let against_pudge = chance(&[(LINA, Role::Mid), (SVEN, Role::Carry)], &[(AXE, Role::Offlane), (PUDGE, Role::Mid)]);
        assert!(against_pudge < with_sven - 1.9 && against_pudge > 50.0, "{against_pudge}");

        assert_eq!(chance(&[(LINA, Role::Mid), ("npc_dota_hero_new", Role::Carry)], &[(AXE, Role::Offlane)]), lina);
        let mut file = file();
        file["pairs"][0]["against"][1][0] = json!(3000);
        let sure = read(&file).unwrap().chance(rank, &[(LINA, Role::Mid)], &[(AXE, Role::Offlane)]);
        assert!(sure > 99.0 && sure < 100.0, "{sure}");
    }

    #[test]
    fn a_hero_counts_by_how_it_does_in_the_role_it_is_played_in() {
        let mut file = file();
        file["ranks"][1]["won"][3][1] = json!(600);
        let numbers = read(&file).unwrap();
        let chance = |role: Role| numbers.chance(Rank::Guardian, &[(LINA, role)], &[]);
        assert_eq!(chance(Role::Mid), 50.0);
        let support = chance(Role::SoftSupport);
        assert!(support > 57.0 && support < 57.2, "10% more than half, of which 1000 matches make 7% believed: {support}");
        let carry = chance(Role::Carry);
        assert!(carry > 50.9 && carry < 51.0, "{carry}");
    }

    #[test]
    fn the_two_ranks_of_a_pair_share_what_heroes_do_to_each_other() {
        let mut file = file();
        file["pairs"][2]["against"][1][0] = json!(80);
        let numbers = read(&file).unwrap();
        let lina = |rank: Rank| numbers.suggest(rank, None, &[], &[AXE])[0].points;
        assert_eq!((lina(Rank::Legend), lina(Rank::Ancient)), (8.0, 8.0));
        assert_eq!((lina(Rank::Archon), lina(Rank::Divine)), (3.0, 3.0));
        let chance = |rank: Rank| numbers.chance(rank, &[(LINA, Role::Mid)], &[(AXE, Role::Offlane)]);
        assert_eq!(chance(Rank::Legend), chance(Rank::Ancient));
        assert!(chance(Rank::Legend) > chance(Rank::Archon));
    }

    fn file_with_lanes() -> serde_json::Value {
        let mut file = file();
        let none = json!([[0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]]);
        let lanes = json!({
            "played": [[0, 0, 1000, 1000], [0, 1000, 1000, 0], [1000, 0, 1000, 0], [0, 1000, 1000, 0], [0, 0, 1000, 0]],
            "won": [[0, 0, 500, 600], [0, 400, 500, 0], [400, 0, 500, 0], [0, 500, 500, 0], [0, 0, 500, 0]],
            "drawn": [[0, 0, 0, 0], [0, 200, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
            "against": [none, none, none, none, none],
            "with": [none, none, none, none, none],
        });
        file["lanes"] = json!([lanes, lanes, lanes, lanes]);
        file
    }

    #[test]
    fn a_file_without_lanes_is_read_and_says_nothing_of_them() {
        let numbers = read(&file()).unwrap();
        assert!(!numbers.has_lanes());
        assert!(numbers.suggest_in_lane(Rank::Legend, Role::Carry, &[], &[], &[]).is_empty());
        assert!(numbers.suggest(Rank::Legend, Some(Role::Carry), &[], &[]).len() == 2, "the rest of the page is as before");
        assert!(read(&file_with_lanes()).unwrap().has_lanes());

        let refused = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut file = file_with_lanes();
            change(&mut file);
            read(&file).unwrap_err().to_string()
        };
        assert!(refused(&|file| file["lanes"].as_array_mut().unwrap().truncate(3)).contains("four pairs"));
        assert!(refused(&|file| file["lanes"][1]["drawn"].as_array_mut().unwrap().truncate(4)).contains("lanes does not fit"));
        assert!(refused(&|file| file["lanes"][2]["against"][4][3] = json!([0, 0])).contains("lanes does not fit"));
        assert!(refused(&|file| file["lanes"][0]["with"].as_array_mut().unwrap().truncate(4)).contains("lanes does not fit"));
    }

    #[test]
    fn in_a_lane_a_hero_is_weighed_by_how_its_lanes_go() {
        let mut file = file_with_lanes();
        let numbers = read(&file).unwrap();
        let carries = numbers.suggest_in_lane(Rank::Legend, Role::Carry, &[], &[], &[]);
        assert_eq!(names(&carries), [SVEN, PUDGE], "only the heroes played in the role");
        assert_eq!(carries[0].win_rate, 60.0);
        assert!((carries[0].points - 5.0 * 1000.0 / 1400.0).abs() < 0.001, "{}", carries[0].points);
        assert!((carries[1].points + carries[0].points).abs() < 0.001, "Pudge loses as much against the usual");
        let mids = numbers.suggest_in_lane(Rank::Legend, Role::Mid, &[], &[], &[]);
        assert_eq!((names(&mids), mids[0].win_rate, mids[0].points), (vec![LINA, PUDGE], 50.0, 0.0));

        file["lanes"][2]["against"][0][2][0] = json!(80);
        file["lanes"][2]["with"][0][3][1] = json!(30);
        let numbers = read(&file).unwrap();
        let carries = numbers.suggest_in_lane(Rank::Ancient, Role::Carry, &[LINA], &[(AXE, Role::Offlane)], &[LINA, AXE]);
        assert_eq!(names(&carries), [SVEN, PUDGE]);
        assert_eq!(carries[0].reasons, [Reason { hero: LINA.to_string(), beside: true, points: 1.5 }]);
        assert_eq!(carries[1].reasons, [Reason { hero: AXE.to_string(), beside: false, points: 8.0 }]);
        assert!((carries[1].points - (8.0 - 5.0 * 1000.0 / 1400.0)).abs() < 0.001);
        assert_eq!(names(&numbers.suggest_in_lane(Rank::Ancient, Role::Carry, &[], &[(AXE, Role::Offlane)], &[AXE])), [PUDGE, SVEN]);
        let half = &numbers.suggest_in_lane(Rank::Ancient, Role::Carry, &[], &[(AXE, Role::SoftSupport)], &[AXE])[1];
        assert_eq!(half.reasons, [Reason { hero: AXE.to_string(), beside: false, points: 4.0 }]);
        assert_eq!(Role::HardSupport.lane_weight(Role::SoftSupport), 1.0);
        assert_eq!(names(&numbers.suggest_in_lane(Rank::Herald, Role::Carry, &[], &[(AXE, Role::Offlane)], &[AXE, SVEN])), [PUDGE]);
        assert!(numbers.suggest_in_lane(Rank::Herald, Role::Carry, &[], &[(AXE, Role::Offlane)], &[])[1].reasons.is_empty(), "nothing in that pair");
    }

    #[test]
    fn each_role_meets_the_roles_of_its_lane() {
        assert_eq!(Role::Mid.lane_opponents(), [Role::Mid]);
        assert_eq!(Role::Carry.lane_opponents(), [Role::Offlane, Role::SoftSupport]);
        assert_eq!(Role::Offlane.lane_opponents(), [Role::Carry, Role::HardSupport]);
        assert_eq!(Role::Mid.lane_partner(), None);
        assert_eq!((Role::Carry.lane_weight(Role::Offlane), Role::Carry.lane_weight(Role::SoftSupport)), (1.0, 0.5));
        assert_eq!((Role::Offlane.lane_weight(Role::Carry), Role::Offlane.lane_weight(Role::HardSupport)), (1.0, 0.5));
        assert_eq!((Role::Mid.lane_weight(Role::Mid), Role::SoftSupport.lane_weight(Role::HardSupport)), (1.0, 1.0));
        for role in Role::ALL {
            if let Some(partner) = role.lane_partner() {
                assert_eq!(partner.lane_partner(), Some(role));
                assert_eq!(partner.lane_opponents(), role.lane_opponents());
                assert!(role.lane_opponents().iter().all(|enemy| enemy.lane_opponents().contains(&role)), "what {role:?} meets meets it");
            }
        }
        assert_eq!(Style::default(), Style::Team);
        assert!(Style::Team.is_default() && !Style::Lane.is_default());
        assert_eq!(serde_json::to_string(&Style::ALL).unwrap(), r#"["team","lane"]"#);
    }

    fn file_with_weeks() -> serde_json::Value {
        let mut file = file();
        let newest = json!({
            "played": [[0, 0, 2000, 150], [0, 2000, 0, 0], [2000, 0, 2000, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
            "won": [[0, 0, 1000, 81], [0, 1000, 0, 0], [1100, 0, 960, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
        });
        let before = json!({
            "played": [[0, 0, 2000, 150], [0, 2000, 0, 0], [2000, 0, 2000, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
            "won": [[0, 0, 1000, 75], [0, 1000, 0, 0], [1000, 0, 1040, 0], [0, 0, 0, 0], [0, 0, 0, 0]],
        });
        let week = json!([newest, before]);
        file["weekly"] = json!({ "weeks": [1_000_000, 395_200], "ranks": [week, week, week, week, week, week, week, week] });
        file
    }

    #[test]
    fn the_newest_week_and_the_one_before_say_who_is_rising_and_who_is_falling() {
        let numbers = read(&file_with_weeks()).unwrap();
        assert!(numbers.has_weeks());
        let axe = numbers.trend(Rank::Legend, Role::Offlane, AXE).unwrap();
        assert_eq!((axe.now, axe.before, axe.way), (55.0, 50.0, Way::Rising));
        assert_eq!(numbers.trend(Rank::Herald, Role::Offlane, PUDGE).unwrap().way, Way::Falling);
        assert_eq!(numbers.trend(Rank::Legend, Role::Mid, LINA).unwrap().way, Way::Level);
        let sven = numbers.trend(Rank::Legend, Role::Carry, SVEN).unwrap();
        assert_eq!((sven.now, sven.way), (54.0, Way::Level));
        assert_eq!(numbers.trend(Rank::Legend, Role::Carry, LINA), None);
        assert_eq!(numbers.trend(Rank::Legend, Role::Carry, "npc_dota_hero_new"), None);
    }

    #[test]
    fn a_file_without_the_weeks_apart_or_with_them_wrong_is_read_as_ever() {
        let numbers = read(&file()).unwrap();
        assert!(!numbers.has_weeks());
        assert_eq!(numbers.trend(Rank::Legend, Role::Offlane, AXE), None);
        for wrong in [json!("weeks"), json!({ "ranks": [] }), json!({ "ranks": [[{ "played": [[1]], "won": [[1]] }]] })] {
            let mut file = file_with_weeks();
            file["weekly"] = wrong;
            let numbers = read(&file).expect("the rest of the file stands");
            assert!(!numbers.has_weeks());
            assert_eq!(names(&numbers.suggest(Rank::Legend, None, &[], &[])), [AXE, LINA, PUDGE, SVEN]);
        }
        let mut file = file_with_weeks();
        file["weekly"]["ranks"][7][1]["won"].as_array_mut().unwrap().truncate(4);
        assert!(!read(&file).unwrap().has_weeks(), "one rank that does not fit, and none are taken");
    }

    #[test]
    fn heroes_to_ban_are_those_that_win_most_and_do_best_against_the_players_heroes() {
        let mut file = file();
        for role in 0..5 {
            file["ranks"][4]["won"][role][2] = json!(1100);
        }
        let numbers = read(&file).unwrap();
        let bans = numbers.to_ban(Rank::Legend, None, &[], &[]);
        assert_eq!(names(&bans)[0], PUDGE);
        assert!(bans.iter().all(|ban| ban.reasons.is_empty()));
        let bans = numbers.to_ban(Rank::Legend, None, &[LINA], &[]);
        assert_eq!(names(&bans), [PUDGE, SVEN, AXE], "never the player's own hero");
        assert_eq!(bans[0].reasons, [Reason { hero: LINA.to_string(), beside: false, points: 2.0 }]);
        assert!((bans[0].points - (bans[1].points + 2.0 + 5.0 * 10000.0 / 10400.0)).abs() < 0.01, "{}", bans[0].points);
        let lina_and_sven = numbers.to_ban(Rank::Legend, None, &[LINA, SVEN], &[]);
        assert_eq!(names(&lina_and_sven), [PUDGE, AXE]);
        assert!((lina_and_sven[1].points + 1.5).abs() < 0.01, "Axe is weak against Lina, and does nothing to Sven");
        let mid = numbers.to_ban(Rank::Legend, Some(Role::Mid), &[], &[]);
        let axe = mid.iter().find(|ban| ban.hero == AXE).unwrap();
        assert!((axe.points - (-3.0 * 9000.0 + 0.5 * 2000.0) / 11000.0).abs() < 0.01, "{}", axe.points);
        assert_eq!(names(&numbers.to_ban(Rank::Legend, None, &[LINA], &[PUDGE])), [SVEN, AXE]);
        assert!(numbers.is_played_in(Rank::Legend, Role::Mid, LINA) && !numbers.is_played_in(Rank::Legend, Role::Carry, LINA));
    }

    #[test]
    fn ranks_and_roles_have_names_and_keep_theirs_when_saved() {
        assert_eq!(Rank::ALL.map(Rank::place), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(Rank::ALL.map(Rank::pair), [0, 0, 1, 1, 2, 2, 3, 3], "two ranks to a pair");
        assert_eq!(Role::ALL.map(Role::place), [0, 1, 2, 3, 4]);
        assert_eq!(Rank::default().name(), "Archon");
        assert!(Rank::Archon.is_default() && !Rank::Immortal.is_default());
        assert_eq!(serde_json::to_string(&(Rank::Ancient, Role::SoftSupport)).unwrap(), r#"["ancient","soft_support"]"#);
        assert_eq!(Role::Mid.as_played(), "in mid");
        assert_eq!(Mode::default(), Mode::AllPick);
        assert!(Mode::AllPick.is_default() && !Mode::CaptainsMode.is_default());
        assert_eq!(serde_json::to_string(&Mode::ALL).unwrap(), r#"["all_pick","captains_mode"]"#);
    }

    #[test]
    fn captains_mode_steps_match_patch_740_24_step_order() {
        assert_eq!(CAPTAINS_STEPS.len(), 24);
        let first_bans: Vec<(usize, usize)> = CAPTAINS_STEPS.iter().filter(|s| s.first && s.action == Action::Ban).map(|s| (s.phase, s.slot)).collect();
        let second_bans: Vec<(usize, usize)> = CAPTAINS_STEPS.iter().filter(|s| !s.first && s.action == Action::Ban).map(|s| (s.phase, s.slot)).collect();
        let first_picks: Vec<(usize, usize)> = CAPTAINS_STEPS.iter().filter(|s| s.first && s.action == Action::Pick).map(|s| (s.phase, s.slot)).collect();
        let second_picks: Vec<(usize, usize)> = CAPTAINS_STEPS.iter().filter(|s| !s.first && s.action == Action::Pick).map(|s| (s.phase, s.slot)).collect();
        assert_eq!(first_bans, [(0, 0), (0, 1), (0, 2), (1, 3), (1, 4), (2, 5), (2, 6)]);
        assert_eq!(second_bans, [(0, 0), (0, 1), (0, 2), (0, 3), (1, 4), (2, 5), (2, 6)]);
        assert_eq!(first_picks, [(0, 0), (1, 1), (1, 2), (1, 3), (2, 4)]);
        assert_eq!(second_picks, [(0, 0), (1, 1), (1, 2), (1, 3), (2, 4)]);
    }

    #[test]
    fn popular_supports_rank_by_pick_count_and_report_pick_rate() {
        let numbers = read(&file()).unwrap();
        let popular = numbers.suggest_popular(Rank::Archon, Role::SoftSupport, &[]);
        assert_eq!(names(&popular), [PUDGE, LINA]);
        assert_eq!((popular[0].matches, popular[1].matches), (2000, 1000));
        assert!((popular[0].pick_rate - 100.0).abs() < 0.01);
        assert!((popular[1].pick_rate - 66.67).abs() < 0.1);
        assert_eq!(names(&numbers.suggest_popular(Rank::Archon, Role::SoftSupport, &[PUDGE])), [LINA]);
    }

    #[test]
    fn mid_draft_bans_weigh_counters_against_team_and_synergies_with_enemy() {
        let numbers = read(&file()).unwrap();
        assert_eq!(numbers.to_ban_in_draft(Rank::Legend, None, &[LINA], &[], &[], &[]), numbers.to_ban(Rank::Legend, None, &[LINA], &[]));
        let mid = numbers.to_ban_in_draft(Rank::Legend, None, &[], &[LINA], &[AXE], &[]);
        assert_eq!(names(&mid), [PUDGE, SVEN]);
        assert!(mid[0].points > mid[1].points);
        let against_axe_with_sven = numbers.to_ban_in_draft(Rank::Legend, None, &[], &[AXE], &[SVEN], &[]);
        assert_eq!(names(&against_axe_with_sven), [LINA, PUDGE]);
        assert!((against_axe_with_sven[0].points - 4.5).abs() < 0.01, "{}", against_axe_with_sven[0].points);
    }
}
