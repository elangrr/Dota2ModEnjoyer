use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::draft::{Rank, Role};
use crate::error::{Error, Result};
use crate::heroes::HERO_FILES;
use crate::kv::{self, Block, Value};
use crate::names::Names;
use crate::vpk::Archive;

pub const BUILDS_FILE: &str = "builds.json";

pub const BUILDS_LIMIT: usize = 8 * 1024 * 1024;

pub const FORMAT: u32 = 2;

const WHAT: &str = "the builds";

const ITEMS_FILE: &str = "scripts/npc/items.txt";
const SKILLS_FILE: &str = "scripts/npc/npc_abilities.txt";
const NEUTRALS_FILE: &str = "scripts/npc/neutral_items.txt";

pub const STARTING_GOLD: u32 = 600;

const GOES_ON_SHARE: f64 = 0.6;

const CORE_SHARE: f64 = 0.35;

const MOST_SHARE: f32 = 99.0;

const PRO_CORE_SHARE: f32 = 50.0;

pub const POINT_LEVELS: [u32; 23] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 18, 20, 25, 27, 28, 29, 30];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bought {
    pub item: String,
    pub copies: u32,
    pub minute: u32,
    pub matches: u64,
    pub wins: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Order {
    pub role: Role,
    pub matches: u64,
    pub skills: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Other<'b> {
    pub item: &'b Bought,
    pub share: f32,
    pub against: Vec<(String, f32)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProItem {
    pub item: String,
    pub share: f32,
    pub minute: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProBuild {
    pub role: Role,
    pub players: u64,
    pub items: Vec<ProItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeutralBuild {
    pub role: Option<Role>,
    pub players: u64,
    pub tiers: Vec<NeutralTaken>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeutralTaken {
    pub tier: u32,
    pub reached: u64,
    pub kept: u64,
    pub trinkets: Vec<(String, u64)>,
    pub enhancements: Vec<(String, u64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NeutralTier<'b> {
    pub tier: u32,
    pub minute: u32,
    pub reached: u64,
    pub kept: u64,
    pub trinkets: Vec<(&'b str, f32)>,
    pub enhancements: Vec<(&'b str, f32)>,
}

impl NeutralTaken {
    pub fn share(&self, count: u64) -> f32 {
        if self.reached == 0 { 0.0 } else { (count as f64 / self.reached as f64 * 100.0).min(100.0) as f32 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Taken {
    pub talent: String,
    pub matches: u64,
    pub wins: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build {
    pub role: Role,
    pair: usize,
    pub matches: u64,
    pub start: Vec<Bought>,
    pub items: Vec<Bought>,
    pub talents: Vec<Taken>,
}

impl Build {
    pub fn share(&self, matches: u64) -> f32 {
        if self.matches == 0 { 0.0 } else { (matches as f64 / self.matches as f64 * 100.0).min(100.0) as f32 }
    }

    pub fn starting(&self, texts: &GameTexts) -> Vec<Bought> {
        let mut left = STARTING_GOLD;
        let mut bought = Vec::new();
        for item in self.start.iter().filter(|item| texts.can_be_bought(&item.item)) {
            let cost = texts.item_cost(&item.item).unwrap_or(0);
            let copies = left.checked_div(cost).map_or(item.copies, |pays_for| item.copies.min(pays_for));
            if copies > 0 {
                left -= copies * cost;
                bought.push(Bought { copies, ..item.clone() });
            }
        }
        bought
    }

    pub fn starting_cost(bought: &[Bought], texts: &GameTexts) -> u32 {
        bought.iter().map(|item| item.copies * texts.item_cost(&item.item).unwrap_or(0)).sum()
    }

    pub fn whole_items(&self, texts: &GameTexts) -> Vec<&Bought> {
        self.items.iter().filter(|item| texts.can_be_bought(&item.item) && !self.goes_on(item, texts)).collect()
    }

    pub fn core(&self, texts: &GameTexts) -> Vec<&Bought> {
        self.whole_items(texts).into_iter().filter(|item| self.is_core(item)).collect()
    }

    fn is_core(&self, item: &Bought) -> bool {
        item.matches as f64 >= CORE_SHARE * self.matches as f64
    }

    fn goes_on(&self, part: &Bought, texts: &GameTexts) -> bool {
        self.items.iter().any(|whole| whole.matches as f64 >= GOES_ON_SHARE * part.matches as f64 && texts.is_part_of(&part.item, &whole.item))
    }

    pub fn talent(&self, talent: &str) -> Option<&Taken> {
        self.talents.iter().find(|taken| taken.talent.eq_ignore_ascii_case(talent))
    }

    pub fn is_new_talent(&self, own: &[Talent], talent: &str) -> bool {
        self.talent(talent).is_none_or(|taken| taken.matches == 0) && self.talents.iter().any(|taken| taken.matches > 0 && !own.iter().any(|talent| talent.id.eq_ignore_ascii_case(&taken.talent)))
    }
}

pub fn win_rate(matches: u64, wins: u64) -> Option<f32> {
    (matches > 0).then(|| (wins as f64 / matches as f64 * 100.0) as f32)
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
    sampled: u64,
    items: Vec<String>,
    skills: Vec<String>,
    heroes: HashMap<String, Vec<BuildInFile>>,
    #[serde(default)]
    orders: HashMap<String, Vec<(usize, u64, Vec<usize>)>>,
    #[serde(default)]
    against: HashMap<String, Vec<(usize, u32)>>,
    #[serde(default)]
    pros: ProsInFile,
    #[serde(default)]
    neutrals: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct NeutralsInFile {
    #[serde(default)]
    source: String,
    #[serde(default)]
    patch: String,
    #[serde(default)]
    matches: u64,
    names: Vec<String>,
    heroes: HashMap<String, Vec<(usize, u64, Vec<NeutralTierInFile>)>>,
}

type NeutralTierInFile = (u32, u64, u64, Vec<(usize, u64)>, Vec<(usize, u64)>);

const ANY_ROLE: usize = 5;

type NeutralsRead = (String, String, u64, HashMap<String, Vec<NeutralBuild>>);

fn neutrals_from(value: serde_json::Value) -> Option<NeutralsRead> {
    let file: NeutralsInFile = serde_json::from_value(value).ok()?;
    let named = |entries: &[(usize, u64)]| entries.iter().map(|(place, count)| Some((file.names.get(*place)?.clone(), *count))).collect::<Option<Vec<_>>>();
    let mut heroes = HashMap::new();
    for (hero, in_file) in &file.heroes {
        let mut of_hero = Vec::new();
        for (role, players, tiers) in in_file {
            let role = if *role == ANY_ROLE { None } else { Some(*Role::ALL.get(*role)?) };
            let tiers = tiers.iter().map(|(tier, reached, kept, trinkets, enhancements)| Some(NeutralTaken { tier: *tier, reached: *reached, kept: *kept, trinkets: named(trinkets)?, enhancements: named(enhancements)? })).collect::<Option<Vec<_>>>()?;
            of_hero.push(NeutralBuild { role, players: *players, tiers });
        }
        heroes.insert(hero.to_ascii_lowercase(), of_hero);
    }
    Some((file.source, file.patch, file.matches, heroes))
}

#[derive(Deserialize, Default)]
struct ProsInFile {
    #[serde(default)]
    source: String,
    #[serde(default)]
    matches: u64,
    #[serde(default)]
    heroes: HashMap<String, Vec<(usize, u64, Vec<ProBoughtInFile>)>>,
}

type ProBoughtInFile = (usize, u64, u32);

#[derive(Deserialize)]
struct BuildInFile {
    role: usize,
    pair: usize,
    matches: u64,
    #[serde(default)]
    start: Vec<Vec<u64>>,
    #[serde(default)]
    items: Vec<Vec<u64>>,
    #[serde(default)]
    talents: Vec<Vec<u64>>,
}

#[derive(Debug)]
pub struct Builds {
    pub made: u64,
    pub source: String,
    pub patch: String,
    pub sampled: u64,
    heroes: HashMap<String, Vec<Build>>,
    orders: HashMap<String, Vec<Order>>,
    against: HashMap<String, HashMap<String, f32>>,
    pub pro_source: String,
    pub pro_matches: u64,
    pros: HashMap<String, Vec<ProBuild>>,
    pub neutral_source: String,
    pub neutral_patch: String,
    pub neutral_matches: u64,
    neutrals: HashMap<String, Vec<NeutralBuild>>,
}

impl Builds {
    pub fn parse(bytes: &[u8]) -> Result<Builds> {
        let wrong = |detail: &str| Error::format(WHAT, detail);
        let header: Header = serde_json::from_slice(bytes).map_err(|error| wrong(&error.to_string()))?;
        if header.format != FORMAT {
            return Err(wrong("they are of a shape this version of the app does not read"));
        }
        let file: File = serde_json::from_slice(bytes).map_err(|error| wrong(&error.to_string()))?;
        let named = |names: &[String], entry: &[u64], numbers: usize| -> Result<String> {
            let name = (entry.len() == numbers + 1).then(|| names.get(entry[0] as usize)).flatten();
            name.cloned().ok_or_else(|| wrong("a build names something its lists do not have"))
        };
        let bought = |entries: &[Vec<u64>], at_start: bool| -> Result<Vec<Bought>> {
            let read = |entry: &Vec<u64>| {
                let item = named(&file.items, entry, 3)?;
                let (copies, minute) = if at_start { (entry[1].clamp(1, 99) as u32, 0) } else { (1, entry[1].min(999) as u32) };
                Ok(Bought { item, copies, minute, matches: entry[2], wins: entry[3] })
            };
            entries.iter().map(read).collect()
        };
        let taken = |entries: &[Vec<u64>]| -> Result<Vec<Taken>> {
            entries.iter().map(|entry| Ok(Taken { talent: named(&file.skills, entry, 2)?, matches: entry[1], wins: entry[2] })).collect()
        };
        let mut heroes = HashMap::new();
        for (hero, in_file) in &file.heroes {
            let mut builds = Vec::new();
            for build in in_file {
                let role = Role::ALL.get(build.role).copied().ok_or_else(|| wrong("a build is for a role there is none of"))?;
                if build.pair >= Rank::ALL.len() / 2 {
                    return Err(wrong("a build is for a pair of ranks there is none of"));
                }
                builds.push(Build {
                    role,
                    pair: build.pair,
                    matches: build.matches,
                    start: bought(&build.start, true)?,
                    items: bought(&build.items, false)?,
                    talents: taken(&build.talents)?,
                });
            }
            heroes.insert(hero.to_ascii_lowercase(), builds);
        }
        let listed = |names: &[String], place: usize| names.get(place).cloned().ok_or_else(|| wrong("a build names something its lists do not have"));
        let mut orders = HashMap::new();
        for (hero, in_file) in &file.orders {
            let mut of_hero = Vec::new();
            for (role, matches, points) in in_file {
                let role = Role::ALL.get(*role).copied().ok_or_else(|| wrong("an order of skills is for a role there is none of"))?;
                let skills = points.iter().map(|point| listed(&file.skills, *point)).collect::<Result<Vec<String>>>()?;
                of_hero.push(Order { role, matches: *matches, skills });
            }
            orders.insert(hero.to_ascii_lowercase(), of_hero);
        }
        let mut against = HashMap::new();
        for (enemy, in_file) in &file.against {
            let mut items = HashMap::new();
            for (item, hundredths) in in_file {
                items.insert(listed(&file.items, *item)?, *hundredths as f32 / 100.0);
            }
            against.insert(enemy.to_ascii_lowercase(), items);
        }
        let mut pros = HashMap::new();
        for (hero, in_file) in &file.pros.heroes {
            let mut of_hero = Vec::new();
            for (role, players, bought) in in_file {
                let role = Role::ALL.get(*role).copied().ok_or_else(|| wrong("what pros buy is for a role there is none of"))?;
                let share = |count: u64| if *players == 0 { 0.0 } else { (count as f64 / *players as f64 * 100.0).min(100.0) as f32 };
                let items = bought.iter().map(|(item, count, minute)| Ok(ProItem { item: listed(&file.items, *item)?, share: share(*count), minute: (*minute).min(999) })).collect::<Result<Vec<ProItem>>>()?;
                of_hero.push(ProBuild { role, players: *players, items });
            }
            pros.insert(hero.to_ascii_lowercase(), of_hero);
        }
        let (pro_source, pro_matches) = (file.pros.source, file.pros.matches);
        let (neutral_source, neutral_patch, neutral_matches, neutrals) = file.neutrals.and_then(neutrals_from).unwrap_or_default();
        Ok(Builds { made: file.made, source: file.source, patch: file.patch, sampled: file.sampled, heroes, orders, against, pro_source, pro_matches, pros, neutral_source, neutral_patch, neutral_matches, neutrals })
    }

    pub fn age_in_days(&self, now: u64) -> u64 {
        now.saturating_sub(self.made) / (24 * 3600)
    }

    fn in_rank<'b>(&'b self, hero: &str, rank: Rank) -> impl Iterator<Item = &'b Build> {
        let pair = rank.place() / 2;
        self.heroes.get(&hero.to_ascii_lowercase()).into_iter().flatten().filter(move |build| build.pair == pair)
    }

    pub fn roles(&self, hero: &str, rank: Rank) -> Vec<Role> {
        let mut builds: Vec<&Build> = self.in_rank(hero, rank).collect();
        builds.sort_by_key(|build| std::cmp::Reverse(build.matches));
        builds.into_iter().map(|build| build.role).collect()
    }

    pub fn of(&self, hero: &str, rank: Rank, role: Role) -> Option<&Build> {
        self.in_rank(hero, rank).find(|build| build.role == role)
    }

    pub fn order(&self, hero: &str, role: Role) -> Option<&Order> {
        self.orders.get(&hero.to_ascii_lowercase())?.iter().find(|order| order.role == role)
    }

    pub fn times_against(&self, enemy: &str, item: &str) -> Option<f32> {
        self.against.get(&enemy.to_ascii_lowercase())?.get(item).copied().filter(|times| *times > 1.0)
    }

    pub fn bought_against(&self, item: &str) -> Vec<(&str, f32)> {
        let mut against: Vec<(&str, f32)> = self.against.iter().filter_map(|(enemy, items)| Some((enemy.as_str(), *items.get(item)?))).filter(|(_, times)| *times > 1.0).collect();
        against.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
        against
    }

    pub fn pro_build(&self, hero: &str, role: Role) -> Option<&ProBuild> {
        self.pros.get(&hero.to_ascii_lowercase())?.iter().find(|pro| pro.role == role)
    }

    pub fn pro_core(&self, hero: &str, build: &Build, texts: &GameTexts) -> Vec<&ProItem> {
        let Some(pro) = self.pro_build(hero, build.role) else { return Vec::new() };
        let core = build.core(texts);
        let most: Vec<&ProItem> = pro.items.iter().filter(|pro| pro.share >= PRO_CORE_SHARE && texts.can_be_bought(&pro.item)).collect();
        let in_core = |pro: &ProItem| core.iter().any(|item| item.item.eq_ignore_ascii_case(&pro.item) || texts.is_part_of(&pro.item, &item.item));
        let goes_on = |pro: &ProItem| most.iter().any(|whole| whole.share >= GOES_ON_SHARE as f32 * pro.share && texts.is_part_of(&pro.item, &whole.item));
        most.iter().copied().filter(|pro| !in_core(pro) && !goes_on(pro)).collect()
    }

    pub fn others<'b>(&self, hero: &str, build: &'b Build, texts: &GameTexts, enemies: &[&str]) -> Vec<Other<'b>> {
        let other = |item: &'b Bought| {
            let mut against: Vec<(String, f32)> = enemies.iter().filter_map(|enemy| Some((enemy.to_string(), self.times_against(enemy, &item.item)?))).collect();
            against.sort_by(|a, b| b.1.total_cmp(&a.1));
            let usual = build.share(item.matches);
            let more: f32 = against.iter().map(|(_, times)| times).product();
            let share = if against.is_empty() || usual >= MOST_SHARE {
                usual
            } else {
                let odds = usual / (100.0 - usual) * more;
                (100.0 * odds / (1.0 + odds)).min(MOST_SHARE)
            };
            Other { item, share, against }
        };
        let pros = self.pro_core(hero, build, texts);
        let by_pros = |item: &Bought| pros.iter().any(|pro| pro.item.eq_ignore_ascii_case(&item.item));
        let mut others: Vec<Other<'b>> = build.whole_items(texts).into_iter().filter(|item| !build.is_core(item) && !by_pros(item)).map(other).collect();
        others.sort_by(|a, b| b.share.total_cmp(&a.share).then(b.item.matches.cmp(&a.item.matches)));
        others
    }

    pub fn all(&self) -> impl Iterator<Item = (&str, &[Build])> {
        self.heroes.iter().map(|(hero, builds)| (hero.as_str(), builds.as_slice()))
    }

    pub fn all_orders(&self) -> impl Iterator<Item = (&str, &Order)> {
        self.orders.iter().flat_map(|(hero, orders)| orders.iter().map(move |order| (hero.as_str(), order)))
    }

    pub fn all_against(&self) -> impl Iterator<Item = (&str, &str, f32)> {
        self.against.iter().flat_map(|(enemy, items)| items.iter().map(move |(item, times)| (enemy.as_str(), item.as_str(), *times)))
    }

    pub fn all_pro_builds(&self) -> impl Iterator<Item = (&str, &ProBuild)> {
        self.pros.iter().flat_map(|(hero, builds)| builds.iter().map(move |pro| (hero.as_str(), pro)))
    }

    pub fn neutral_build(&self, hero: &str, role: Role) -> Option<&NeutralBuild> {
        let of_hero = self.neutrals.get(&hero.to_ascii_lowercase())?;
        of_hero.iter().find(|neutral| neutral.role == Some(role)).or_else(|| of_hero.iter().find(|neutral| neutral.role.is_none()))
    }

    pub fn neutral_tiers<'b>(&'b self, hero: &str, role: Role, texts: &GameTexts) -> Option<(&'b NeutralBuild, Vec<NeutralTier<'b>>)> {
        let neutral = self.neutral_build(hero, role)?;
        let shown: Vec<NeutralTier<'b>> = neutral
            .tiers
            .iter()
            .filter_map(|taken| {
                let game = texts.neutral_tiers.iter().find(|tier| tier.tier == taken.tier)?;
                let kept = |entries: &'b [(String, u64)], known: &[String]| -> Vec<(&'b str, f32)> { entries.iter().filter(|(name, _)| known.iter().any(|known| known.eq_ignore_ascii_case(name))).map(|(name, count)| (name.as_str(), taken.share(*count))).collect() };
                let (trinkets, enhancements) = (kept(&taken.trinkets, &game.trinkets), kept(&taken.enhancements, &game.enhancements));
                (!trinkets.is_empty() || !enhancements.is_empty()).then_some(NeutralTier { tier: taken.tier, minute: game.minute, reached: taken.reached, kept: taken.kept, trinkets, enhancements })
            })
            .collect();
        (!shown.is_empty()).then_some((neutral, shown))
    }

    pub fn all_neutral_builds(&self) -> impl Iterator<Item = (&str, &NeutralBuild)> {
        self.neutrals.iter().flat_map(|(hero, builds)| builds.iter().map(move |neutral| (hero.as_str(), neutral)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Talent {
    pub id: String,
    pub text: String,
}

#[derive(Default)]
pub struct GameTexts {
    names: Names,
    talents: HashMap<String, Vec<Talent>>,
    parts: HashMap<String, Vec<String>>,
    costs: HashMap<String, u32>,
    unsold: HashSet<String>,
    neutral_tiers: Vec<GameNeutralTier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameNeutralTier {
    pub tier: u32,
    pub minute: u32,
    pub trinkets: Vec<String>,
    pub enhancements: Vec<String>,
}

pub fn neutral_tiers(file: &[u8]) -> Vec<GameNeutralTier> {
    if file.is_empty() || kv::validate(file, "a file of the game's").is_err() {
        return Vec::new();
    }
    let Some(listed) = Block::document(file).block("neutral_items").and_then(|items| items.block("neutral_tiers")) else { return Vec::new() };
    let names = |block: Option<Block<'_>>| -> Vec<String> {
        let mut names: Vec<String> = block.into_iter().flat_map(|block| block.pairs()).filter_map(|pair| pair.value.as_block()).flat_map(|inner| inner.pairs()).map(|pair| String::from_utf8_lossy(pair.key).trim().to_ascii_lowercase()).collect();
        names.sort();
        names.dedup();
        names
    };
    let mut tiers: Vec<GameNeutralTier> = listed
        .pairs()
        .filter_map(|pair| {
            let tier = std::str::from_utf8(pair.key).ok()?.trim().parse().ok()?;
            let body = pair.value.as_block()?;
            let start = String::from_utf8_lossy(body.str("start_time").unwrap_or(b"0:00")).to_string();
            let minute = start.split(':').next()?.trim().parse().ok()?;
            let pools = body.block("item_pools");
            let mut trinkets: Vec<String> = pools.into_iter().flat_map(|pools| pools.pairs()).filter_map(|pool| pool.value.as_block()?.block("items")).flat_map(|items| items.pairs()).map(|item| String::from_utf8_lossy(item.key).trim().to_ascii_lowercase()).collect();
            trinkets.sort();
            trinkets.dedup();
            Some(GameNeutralTier { tier, minute, trinkets, enhancements: names(body.block("enhancements")) })
        })
        .collect();
    tiers.sort_by_key(|tier| tier.tier);
    tiers
}

impl std::fmt::Debug for GameTexts {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "the game's texts for {} heroes", self.talents.len())
    }
}

impl GameTexts {
    pub fn read(archive: &Archive) -> GameTexts {
        GameTexts::read_in(archive, "english")
    }

    pub fn read_in(archive: &Archive, language: &str) -> GameTexts {
        let file = |path: &str| archive.read(path).ok().flatten().unwrap_or_default();
        let mut names = Names::parse(&file(&crate::catalog::names_file("abilities", language)));
        if language != "english" {
            names.extend(Names::parse(&file(&crate::catalog::names_file("abilities", "english"))));
        }
        let mut paths: Vec<String> = archive.entries().iter().map(|entry| archive.path_of(entry)).filter(|path| path.to_ascii_lowercase().starts_with(HERO_FILES)).collect();
        paths.sort();
        let heroes: Vec<Vec<u8>> = paths.iter().map(|path| file(path)).collect();
        GameTexts { neutral_tiers: neutral_tiers(&file(NEUTRALS_FILE)), ..GameTexts::from_files(names, &heroes, &file(ITEMS_FILE), &file(SKILLS_FILE)) }
    }

    pub fn from_files(names: Names, heroes: &[Vec<u8>], items: &[u8], skills: &[u8]) -> GameTexts {
        let readable = |file: &[u8]| !file.is_empty() && kv::validate(file, "a file of the game's").is_ok();
        let mut shared = TalentValues::default();
        if readable(skills) {
            shared.take_in(Block::document(skills), "", None);
        }
        let mut talents = HashMap::new();
        for file in heroes.iter().filter(|file| readable(file)) {
            let Some(listed) = Block::document(file).block("DOTAHeroes") else { continue };
            for hero in listed.pairs() {
                let Some(body) = hero.value.as_block() else { continue };
                let mut values = TalentValues::default();
                values.take_in(body, "", None);
                let mut numbered: Vec<(u32, String)> = body
                    .pairs()
                    .filter_map(|pair| {
                        let place = std::str::from_utf8(pair.key).ok()?.to_ascii_lowercase().strip_prefix("ability")?.parse().ok()?;
                        let id = String::from_utf8_lossy(pair.value.as_str()?).trim().to_string();
                        id.to_ascii_lowercase().starts_with("special_bonus").then_some((place, id))
                    })
                    .collect();
                numbered.sort();
                let of_hero: Vec<Talent> = numbered.into_iter().map(|(_, id)| Talent { text: talent_text(&names, &id, &values, &shared), id }).collect();
                if !of_hero.is_empty() {
                    talents.insert(String::from_utf8_lossy(hero.key).to_ascii_lowercase(), of_hero);
                }
            }
        }
        let (mut parts, mut costs, mut unsold) = (HashMap::new(), HashMap::new(), HashSet::new());
        if readable(items)
            && let Some(listed) = Block::document(items).block("DOTAAbilities")
        {
            for item in listed.pairs() {
                let Some(body) = item.value.as_block() else { continue };
                let name = String::from_utf8_lossy(item.key).trim().to_ascii_lowercase();
                let number = |key: &str| body.str(key).and_then(|value| std::str::from_utf8(value).ok()?.trim().parse::<u32>().ok());
                if let Some(cost) = number("ItemCost") {
                    costs.insert(name.clone(), cost);
                }
                if number("ItemPurchasable") == Some(0) || number("IsObsolete") == Some(1) {
                    unsold.insert(name);
                }
            }
            for recipe in listed.pairs().filter_map(|pair| pair.value.as_block()) {
                let text = |key: &str| recipe.str(key).map(|value| String::from_utf8_lossy(value).trim().to_ascii_lowercase());
                let (Some(whole), Some(needed)) = (text("ItemResult").filter(|whole| !whole.is_empty()), recipe.block("ItemRequirements")) else { continue };
                let Some(first) = needed.pairs().find_map(|pair| pair.value.as_str()) else { continue };
                let of_it: Vec<String> = String::from_utf8_lossy(first).split(';').map(|part| part.trim().trim_end_matches('*').to_ascii_lowercase()).filter(|part| !part.is_empty()).collect();
                parts.insert(whole, of_it);
            }
        }
        GameTexts { names, talents, parts, costs, unsold, neutral_tiers: Vec::new() }
    }

    pub fn can_be_bought(&self, item: &str) -> bool {
        !self.unsold.contains(&item.to_ascii_lowercase())
    }

    pub fn item_cost(&self, item: &str) -> Option<u32> {
        self.costs.get(&item.to_ascii_lowercase()).copied()
    }

    pub fn item_name(&self, item: &str) -> String {
        self.named(item).map_or_else(|| in_words(item.strip_prefix("item_").unwrap_or(item)), str::to_string)
    }

    pub fn skill_name(&self, skill: &str) -> String {
        self.named(skill).map_or_else(|| in_words(skill), str::to_string)
    }

    pub fn names(&self, item_or_skill: &str) -> bool {
        self.named(item_or_skill).is_some()
    }

    fn named(&self, item_or_skill: &str) -> Option<&str> {
        let token = format!("DOTA_Tooltip_ability_{item_or_skill}");
        self.names.get(&token).or_else(|| self.names.get(&format!("{token}:n")))
    }

    pub fn neutral_tiers(&self) -> &[GameNeutralTier] {
        &self.neutral_tiers
    }

    pub fn talents(&self, hero: &str) -> &[Talent] {
        self.talents.get(&hero.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }

    pub fn is_part_of(&self, part: &str, whole: &str) -> bool {
        self.is_part_within(&part.to_ascii_lowercase(), &whole.to_ascii_lowercase(), 6)
    }

    fn is_part_within(&self, part: &str, whole: &str, steps: u8) -> bool {
        let Some(parts) = self.parts.get(whole).filter(|_| steps > 0) else { return false };
        parts.iter().any(|of_it| of_it == part || self.is_part_within(part, of_it, steps - 1))
    }
}

pub fn item_picture(item: &str) -> String {
    format!("panorama/images/items/{}_png.vtex_c", item.strip_prefix("item_").unwrap_or(item))
}

pub fn skill_picture(skill: &str) -> String {
    format!("panorama/images/spellicons/{skill}_png.vtex_c")
}

fn in_words(name: &str) -> String {
    let word = |word: &str| {
        let mut letters = word.chars();
        letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
    };
    name.split('_').filter(|part| !part.is_empty()).map(word).collect::<Vec<_>>().join(" ")
}

#[derive(Default)]
struct TalentValues {
    by_talent: HashMap<String, Vec<(String, String)>>,
}

impl TalentValues {
    fn take_in(&mut self, block: Block<'_>, name: &str, owner: Option<&str>) {
        for pair in block.pairs() {
            let key = String::from_utf8_lossy(pair.key).to_ascii_lowercase();
            let is_talent = key.starts_with("special_bonus");
            match pair.value {
                Value::Str(value) => {
                    let value = String::from_utf8_lossy(value).trim().to_string();
                    if is_talent {
                        self.by_talent.entry(key).or_default().push((name.to_string(), value));
                    } else if let Some(owner) = owner {
                        let named = if key == "value" && name != "abilityvalues" { name.to_string() } else { key };
                        self.by_talent.entry(owner.to_string()).or_default().push((named, value));
                    }
                }
                Value::Block(inner) => self.take_in(inner, &key, if is_talent { Some(&key) } else { owner }),
            }
        }
    }

    fn get(&self, talent: &str, name: &str) -> Option<&str> {
        self.by_talent.get(talent)?.iter().find(|(named, _)| named == name).map(|(_, value)| value.as_str())
    }

    fn only(&self, talent: &str) -> Option<&str> {
        let values = self.by_talent.get(talent)?;
        let first = values.first()?;
        values.iter().all(|(_, value)| *value == first.1).then_some(first.1.as_str())
    }
}

fn talent_text(names: &Names, talent: &str, of_hero: &TalentValues, shared: &TalentValues) -> String {
    let Some(text) = names.get(&format!("DOTA_Tooltip_ability_{talent}")) else { return in_words(talent) };
    let id = talent.to_ascii_lowercase();
    let amount = |name: &str| -> Option<String> {
        let name = name.to_ascii_lowercase();
        let known = |values: &TalentValues| values.get(&id, &name).or_else(|| values.get(&id, name.strip_prefix("bonus_")?)).map(str::to_string);
        let in_its_name = || id.rsplit('_').next().filter(|last| name == "value" && last.parse::<f64>().is_ok()).map(str::to_string);
        known(of_hero).or_else(|| known(shared)).or_else(in_its_name).or_else(|| of_hero.only(&id).or_else(|| shared.only(&id)).map(str::to_string))
    };
    let mut said = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("{s:") {
        let Some(length) = rest[start..].find('}') else { break };
        said.push_str(&rest[..start]);
        said.push_str(&amount(&rest[start + 3..start + length]).map_or_else(|| "?".to_string(), |written| plain_amount(&written)));
        rest = &rest[start + length + 1..];
    }
    said.push_str(rest);
    said.replace("%%", "%")
}

fn plain_amount(written: &str) -> String {
    let first = written.split_whitespace().next().unwrap_or_default().trim_start_matches(['+', '-', '=']);
    match first.parse::<f64>() {
        Ok(number) => format!("{number}"),
        Err(_) => first.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const PA: &str = "npc_dota_hero_phantom_assassin";

    fn file() -> serde_json::Value {
        let carry = json!({
            "role": 0, "pair": 1, "matches": 1000,
            "start": [[0, 2, 900, 500], [1, 1, 400, 210]],
            "items": [[7, 3, 300, 150], [2, 9, 950, 520], [3, 11, 900, 500], [4, 16, 880, 495], [5, 30, 300, 180], [6, 38, 150, 100], [8, 33, 100, 60]],
            "talents": [[2, 600, 330], [3, 350, 160]],
        });
        let mid = json!({ "role": 1, "pair": 1, "matches": 300, "items": [[9, 2, 24, 12], [5, 20, 150, 70]] });
        let low = json!({ "role": 0, "pair": 0, "matches": 4000 });
        json!({
            "format": 2, "made": 1_000_000, "source": "STRATZ", "patch": "7.41f", "sampled": 900,
            "items": ["item_branches", "item_tango", "item_power_treads", "item_pers", "item_bfury", "item_basher", "item_abyssal_blade", "item_boots", "item_monkey_king_bar", "item_bottle"],
            "skills": ["phantom_assassin_stifling_dagger", "phantom_assassin_phantom_strike", "special_bonus_unique_phantom_assassin_5", "special_bonus_hp_250"],
            "heroes": { PA: [carry, mid, low] },
            "orders": { PA: [[0, 600, [0, 1, 0, 0, 2]], [1, 300, [1, 0]]] },
            "against": { "npc_dota_hero_windrunner": [[8, 300], [5, 150]], "npc_dota_hero_axe": [[5, 200]] },
            "pros": { "source": "OpenDota", "matches": 1227, "heroes": { PA: [[1, 40, [[9, 38, 2], [7, 40, 5], [2, 36, 9], [5, 30, 18], [8, 12, 25]]], [0, 20, [[9, 12, 3], [7, 20, 4]]]] } },
        })
    }

    fn read(file: &serde_json::Value) -> Result<Builds> {
        Builds::parse(file.to_string().as_bytes())
    }

    #[test]
    fn a_hero_has_a_build_for_each_role_it_is_played_in_by_rank() {
        let builds = read(&file()).unwrap();
        assert_eq!((builds.source.as_str(), builds.patch.as_str()), ("STRATZ", "7.41f"));
        assert_eq!(builds.age_in_days(1_000_000 + 2 * 24 * 3600 + 9), 2);

        assert_eq!(builds.roles(PA, Rank::Archon), [Role::Carry, Role::Mid], "the most played first");
        assert_eq!(builds.roles(PA, Rank::Crusader), [Role::Carry, Role::Mid]);
        assert_eq!(builds.roles(PA, Rank::Herald), [Role::Carry]);
        assert!(builds.roles(PA, Rank::Immortal).is_empty());
        assert!(builds.roles("npc_dota_hero_axe", Rank::Archon).is_empty());

        let build = builds.of(PA, Rank::Archon, Role::Carry).unwrap();
        assert_eq!(build.matches, 1000);
        assert_eq!(build.start[0], Bought { item: "item_branches".to_string(), copies: 2, minute: 0, matches: 900, wins: 500 });
        assert_eq!(build.items[1], Bought { item: "item_power_treads".to_string(), copies: 1, minute: 9, matches: 950, wins: 520 });
        assert_eq!(build.items.iter().map(|item| (item.item.as_str(), item.minute)).collect::<Vec<_>>()[2..5], [("item_pers", 11), ("item_bfury", 16), ("item_basher", 30)]);
        assert_eq!(build.talent("special_bonus_unique_phantom_assassin_5").map(|taken| (taken.matches, taken.wins)), Some((600, 330)));
        assert!(build.talent("special_bonus_never_taken").is_none());
        assert_eq!((build.share(900), build.share(0), build.share(5000)), (90.0, 0.0, 100.0));
        assert_eq!((win_rate(600, 330), win_rate(0, 0)), (Some(55.0), None));

        assert!(builds.of(PA, Rank::Archon, Role::Offlane).is_none());
        assert_eq!(builds.all().map(|(hero, of_hero)| (hero, of_hero.len())).collect::<Vec<_>>(), [(PA, 3)]);
        assert!(builds.of("NPC_DOTA_HERO_PHANTOM_ASSASSIN", Rank::Guardian, Role::Carry).is_some(), "a hero is found whatever its letters are");
    }

    #[test]
    fn a_file_of_another_shape_is_refused() {
        let refused = |change: &dyn Fn(&mut serde_json::Value)| {
            let mut file = file();
            change(&mut file);
            read(&file).unwrap_err().to_string()
        };
        assert!(refused(&|file| file["format"] = json!(1)).contains("this version of the app does not read"));
        assert!(refused(&|file| file["heroes"][PA][0]["role"] = json!(5)).contains("a role there is none of"));
        assert!(refused(&|file| file["heroes"][PA][0]["pair"] = json!(4)).contains("a pair of ranks there is none of"));
        assert!(refused(&|file| file["heroes"][PA][0]["items"][0][0] = json!(99)).contains("its lists do not have"));
        assert!(refused(&|file| file["heroes"][PA][0]["talents"][0][0] = json!(4)).contains("its lists do not have"));
        assert!(refused(&|file| file["heroes"][PA][0]["items"][0] = json!([2, 9, 950])).contains("its lists do not have"));
        assert!(refused(&|file| file["heroes"][PA][0]["start"][0] = json!([0, 1, 950, 7, 7])).contains("its lists do not have"));
        assert!(refused(&|file| file["orders"][PA][0][2][1] = json!(4)).contains("its lists do not have"));
        assert!(refused(&|file| file["orders"][PA][0][0] = json!(7)).contains("a role there is none of"));
        assert!(refused(&|file| file["against"]["npc_dota_hero_axe"][0][0] = json!(10)).contains("its lists do not have"));
        assert!(refused(&|file| file["pros"]["heroes"][PA][0][2][0][0] = json!(10)).contains("its lists do not have"));
        assert!(refused(&|file| file["pros"]["heroes"][PA][0][0] = json!(7)).contains("a role there is none of"));
        let mut without = file();
        without.as_object_mut().unwrap().remove("pros");
        let builds = read(&without).unwrap();
        assert!(builds.pro_build(PA, Role::Mid).is_none() && builds.pro_source.is_empty() && builds.pro_matches == 0);
        assert!(Builds::parse(b"nonsense").is_err());
        assert!(Builds::parse(b"{}").is_err());
    }

    const NAMES: &str = r##""lang" { "Language" "English" "Tokens" {
        "DOTA_Tooltip_Ability_item_blink" "Blink Dagger"
        "DOTA_Tooltip_ability_phantom_assassin_stifling_dagger" "Stifling Dagger"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin_5" "+{s:bonus_attack_factor}%% Stifling Dagger Damage"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin_7" "-{s:bonus_AbilityCooldown}s Stifling Dagger Cooldown"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin_4" "+{s:bonus_duration}s Phantom Strike Duration"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin" "Triple Strike Stifling Dagger"
        "DOTA_Tooltip_ability_special_bonus_hp_250" "+{s:value} Health"
        "DOTA_Tooltip_ability_special_bonus_attack_damage_20" "+{s:value} Damage"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin_8" "+{s:value} Range and {s:nothing_known} Daggers"
        "DOTA_Tooltip_ability_special_bonus_unique_phantom_assassin_9" "{s:charges} Phantom Strike Charges"
    } }"##;

    const HERO: &str = r##""DOTAHeroes" { "npc_dota_hero_phantom_assassin" {
        "Ability1" "phantom_assassin_stifling_dagger"
        "Ability10" "special_bonus_unique_phantom_assassin_4"
        "Ability11" "special_bonus_unique_phantom_assassin_7"
        "Ability12" "special_bonus_hp_250"
        "Ability13" "special_bonus_unique_phantom_assassin_5"
        "Ability14" "special_bonus_attack_damage_20"
        "Ability15" "special_bonus_unique_phantom_assassin_8"
        "Ability16" "special_bonus_unique_phantom_assassin_9"
        "Ability17" "special_bonus_unique_phantom_assassin"
        "AbilityDefinitions" {
            "phantom_assassin_stifling_dagger" { "AbilityValues" {
                "AbilityCooldown" { "value" "6" "special_bonus_unique_phantom_assassin_7" "-2.0" }
                "attack_factor" { "value" "-70 -55 -40 -25" "special_bonus_unique_phantom_assassin_5" "+15" }
                "attack_factor_tooltip" { "value" "30 45 60 75" "special_bonus_unique_phantom_assassin_5" "+15" }
                "extra_daggers" { "special_bonus_unique_phantom_assassin" "2" }
            } }
            "phantom_assassin_phantom_strike" { "AbilityValues" {
                "duration" { "value" "3.0" "special_bonus_unique_phantom_assassin_4" "+0.8" }
            } }
            "special_bonus_unique_phantom_assassin_8" { "AbilityValues" { "value" "600" "vision" "800" } }
            "special_bonus_unique_phantom_assassin_9" { "AbilityValues" { "charges" { "value" "2 3" } } }
        }
    } }"##;

    const SKILLS: &str = r##""DOTAAbilities" { "Version" "1"
        "special_bonus_hp_250" { "AbilityType" "DOTA_ABILITY_TYPE_ATTRIBUTES" "AbilityValues" { "value" "250" } }
    }"##;

    const ITEMS: &str = r##""DOTAAbilities" {
        "item_pers" { "ID" "69" }
        "item_recipe_bfury" { "ItemRecipe" "1" "ItemResult" "item_bfury" "ItemRequirements" { "01" "item_broadsword;item_claymore;item_pers;item_quelling_blade*" } }
        "item_recipe_pers" { "ItemRecipe" "1" "ItemResult" "item_pers" "ItemRequirements" { "01" "item_ring_of_health;item_void_stone" "02" "item_other" } }
        "item_recipe_abyssal_blade" { "ItemRecipe" "1" "ItemResult" "item_abyssal_blade" "ItemRequirements" { "01" "item_basher;item_vanguard" } }
        "item_recipe_power_treads" { "ItemRecipe" "1" "ItemResult" "item_power_treads" "ItemRequirements" { "01" "item_boots;item_gloves;item_belt_of_strength" } }
    }"##;

    fn texts() -> GameTexts {
        GameTexts::from_files(Names::parse(NAMES.as_bytes()), &[HERO.as_bytes().to_vec()], ITEMS.as_bytes(), SKILLS.as_bytes())
    }

    #[test]
    fn items_and_skills_go_by_the_games_own_names_and_pictures() {
        let texts = texts();
        assert_eq!(texts.item_name("item_blink"), "Blink Dagger");
        assert_eq!(texts.skill_name("phantom_assassin_stifling_dagger"), "Stifling Dagger");
        assert_eq!(texts.item_name("item_power_treads"), "Power Treads");
        assert!(texts.names("item_blink") && !texts.names("item_power_treads"));
        assert_eq!(texts.skill_name("phantom_assassin_phantom_strike"), "Phantom Assassin Phantom Strike");
        assert_eq!(item_picture("item_blink"), "panorama/images/items/blink_png.vtex_c");
        assert_eq!(skill_picture("phantom_assassin_blur"), "panorama/images/spellicons/phantom_assassin_blur_png.vtex_c");
    }

    #[test]
    fn a_talent_says_what_it_does_with_the_amounts_of_the_games_files() {
        let texts = texts();
        let said: Vec<&str> = texts.talents(PA).iter().map(|talent| talent.text.as_str()).collect();
        assert_eq!(
            said,
            [
                "+0.8s Phantom Strike Duration",
                "-2s Stifling Dagger Cooldown",
                "+250 Health",
                "+15% Stifling Dagger Damage",
                "+20 Damage",
                "+600 Range and ? Daggers",
                "2 Phantom Strike Charges",
                "Triple Strike Stifling Dagger",
            ]
        );
        assert_eq!(texts.talents(PA)[3].id, "special_bonus_unique_phantom_assassin_5");
        assert_eq!(texts.talents("NPC_DOTA_HERO_PHANTOM_ASSASSIN").len(), 8);

        let invoker = br#""DOTAHeroes" { "npc_dota_hero_invoker" {
            "Ability1" "invoker_quas" "Ability16" "invoker_deafening_blast" "AbilityTalentStart" "17"
            "Ability18" "special_bonus_hp_250" "Ability17" "special_bonus_attack_damage_20" "Ability2" "invoker_wex"
        } }"#;
        let texts = GameTexts::from_files(Names::parse(NAMES.as_bytes()), &[invoker.to_vec()], b"", b"");
        assert_eq!(texts.talents("npc_dota_hero_invoker").iter().map(|talent| talent.text.as_str()).collect::<Vec<_>>(), ["+20 Damage", "+250 Health"]);
        assert!(texts.talents("npc_dota_hero_axe").is_empty());

        assert_eq!((plain_amount("+15"), plain_amount("-2.0"), plain_amount("=0"), plain_amount("0.8"), plain_amount("10 20 30"), plain_amount("x")), ("15".into(), "2".into(), "0".into(), "0.8".into(), "10".into(), "x".into()));
        let none = GameTexts::from_files(Names::default(), &[HERO.as_bytes().to_vec()], b"", b"");
        assert_eq!(none.talents(PA)[2].text, "Special Bonus Hp 250");
    }

    #[test]
    fn an_item_that_is_only_a_part_of_another_is_not_an_item_of_its_own() {
        let texts = texts();
        assert!(texts.is_part_of("item_pers", "item_bfury"));
        assert!(texts.is_part_of("item_quelling_blade", "item_bfury"), "a part that is named with a star");
        assert!(texts.is_part_of("item_void_stone", "item_bfury"), "a part of a part");
        assert!(texts.is_part_of("ITEM_BASHER", "item_abyssal_blade"));
        assert!(!texts.is_part_of("item_bfury", "item_pers") && !texts.is_part_of("item_basher", "item_bfury") && !texts.is_part_of("item_other", "item_pers"));

        let builds = read(&file()).unwrap();
        let build = builds.of(PA, Rank::Archon, Role::Carry).unwrap();
        let whole: Vec<&str> = build.whole_items(&texts).into_iter().map(|item| item.item.as_str()).collect();
        assert_eq!(whole, ["item_power_treads", "item_bfury", "item_basher", "item_abyssal_blade", "item_monkey_king_bar"]);
        assert_eq!(build.whole_items(&GameTexts::default()).len(), 7);
    }

    #[test]
    fn the_start_is_what_one_player_can_buy_with_the_gold_a_hero_starts_with() {
        let prices = br#""DOTAAbilities" {
            "item_tango" { "ItemCost" "90" } "item_blood_grenade" { "ItemCost" "50" } "item_branches" { "ItemCost" "55" }
            "item_magic_stick" { "ItemCost" "200" } "item_boots" { "ItemCost" "500" } "item_wind_lace" { "ItemCost" "225" }
            "item_ward_observer" { "ItemCost" "0" } "item_circlet" { "ItemCost" " 155 " }
            "item_famango" { "ItemCost" "0" "ItemPurchasable" "0" } "item_great_famango" { "ItemPurchasable" "0" } "item_bfury" { "ItemPurchasable" "1" }
            "item_javelin" { "ItemCost" "900" "IsObsolete" "1" }
        }"#;
        let texts = GameTexts::from_files(Names::default(), &[], prices, b"");
        assert_eq!((texts.item_cost("ITEM_TANGO"), texts.item_cost("item_circlet"), texts.item_cost("item_other")), (Some(90), Some(155), None));
        let one = |item: &str, copies: u32, matches: u64| Bought { item: item.to_string(), copies, minute: 0, matches, wins: matches / 2 };
        let mut build = read(&file()).unwrap().of(PA, Rank::Archon, Role::Carry).unwrap().clone();
        build.start = vec![one("item_tango", 1, 820), one("item_blood_grenade", 1, 710), one("item_branches", 3, 510), one("item_ward_observer", 2, 450), one("item_magic_stick", 1, 320), one("item_wind_lace", 1, 290), one("item_boots", 1, 160), one("item_circlet", 1, 150), one("item_other", 1, 150)];
        let start = build.starting(&texts);
        let said: Vec<(&str, u32)> = start.iter().map(|bought| (bought.item.as_str(), bought.copies)).collect();
        assert_eq!(said, [("item_tango", 1), ("item_blood_grenade", 1), ("item_branches", 3), ("item_ward_observer", 2), ("item_magic_stick", 1), ("item_other", 1)]);
        assert_eq!(Build::starting_cost(&start, &texts), 505);
        build.start = vec![one("item_boots", 1, 900), one("item_branches", 3, 800), one("item_tango", 1, 700)];
        let said: Vec<(String, u32)> = build.starting(&texts).into_iter().map(|bought| (bought.item, bought.copies)).collect();
        assert_eq!(said, [("item_boots".to_string(), 1), ("item_branches".to_string(), 1)]);
        build.start = vec![one("item_wind_lace", 1, 900), one("item_magic_stick", 1, 800), one("item_tango", 1, 700), one("item_branches", 2, 600)];
        assert_eq!(Build::starting_cost(&build.starting(&texts), &texts), 225 + 200 + 90 + 55);
        assert_eq!(build.starting(&GameTexts::default()), build.start);

        assert!(!texts.can_be_bought("ITEM_FAMANGO") && texts.can_be_bought("item_bfury") && texts.can_be_bought("item_tango"));
        assert!(!texts.can_be_bought("item_javelin"));
        build.start = vec![one("item_famango", 1, 900), one("item_tango", 1, 700)];
        assert_eq!(build.starting(&texts).len(), 1);
        build.items = vec![Bought { minute: 25, ..one("item_great_famango", 1, 300) }, Bought { minute: 16, ..one("item_bfury", 1, 800) }];
        assert_eq!(build.whole_items(&texts).iter().map(|item| item.item.as_str()).collect::<Vec<_>>(), ["item_bfury"]);
        assert_eq!(build.whole_items(&GameTexts::default()).len(), 2);
    }

    #[test]
    fn a_build_has_a_core_and_other_items_that_go_by_the_enemy() {
        let texts = texts();
        let builds = read(&file()).unwrap();
        let build = builds.of(PA, Rank::Archon, Role::Carry).unwrap();
        assert_eq!(build.core(&texts).iter().map(|item| item.item.as_str()).collect::<Vec<_>>(), ["item_power_treads", "item_bfury"]);

        let said = |enemies: &[&str]| -> Vec<(String, f32, usize)> { builds.others(PA, build, &texts, enemies).iter().map(|other| (other.item.item.clone(), (other.share * 10.0).round() / 10.0, other.against.len())).collect() };
        assert_eq!(said(&[]), [("item_basher".to_string(), 30.0, 0), ("item_abyssal_blade".to_string(), 15.0, 0), ("item_monkey_king_bar".to_string(), 10.0, 0)]);
        assert_eq!(said(&["npc_dota_hero_windrunner", "npc_dota_hero_lina"]), [("item_basher".to_string(), 39.1, 1), ("item_monkey_king_bar".to_string(), 25.0, 1), ("item_abyssal_blade".to_string(), 15.0, 0)]);
        let both = builds.others(PA, build, &texts, &["npc_dota_hero_windrunner", "NPC_DOTA_HERO_AXE"]);
        assert_eq!((both[0].item.item.as_str(), both[0].share.round()), ("item_basher", 56.0));
        assert_eq!(both[0].against, [("NPC_DOTA_HERO_AXE".to_string(), 2.0), ("npc_dota_hero_windrunner".to_string(), 1.5)]);
        let mut sure = build.clone();
        sure.items[6].matches = 340;
        let nearly_all = HashMap::from([("item_monkey_king_bar".to_string(), 900.0)]);
        let lopsided = Builds { against: HashMap::from([("x".to_string(), nearly_all)]), ..read(&file()).unwrap() };
        assert_eq!(lopsided.others(PA, &sure, &texts, &["x"])[0].share, 99.0);
        assert_eq!((builds.times_against("npc_dota_hero_axe", "item_basher"), builds.times_against("npc_dota_hero_axe", "item_bfury"), builds.times_against("npc_dota_hero_lina", "item_basher")), (Some(2.0), None, None));
        assert_eq!(builds.all_against().count(), 3);
        assert_eq!(builds.bought_against("item_basher"), [("npc_dota_hero_axe", 2.0), ("npc_dota_hero_windrunner", 1.5)]);
        assert!(builds.bought_against("item_bfury").is_empty());
    }

    #[test]
    fn a_build_has_what_most_pros_buy_in_the_role_in_its_core() {
        let texts = texts();
        let builds = read(&file()).unwrap();
        assert_eq!((builds.pro_source.as_str(), builds.pro_matches), ("OpenDota", 1227));
        let mid = builds.pro_build(PA, Role::Mid).unwrap();
        assert_eq!((mid.role, mid.players, mid.items.len()), (Role::Mid, 40, 5));
        assert_eq!(mid.items[0], ProItem { item: "item_bottle".to_string(), share: 95.0, minute: 2 });
        assert_eq!(builds.all_pro_builds().count(), 2);

        let build = builds.of(PA, Rank::Archon, Role::Mid).unwrap();
        let pros: Vec<&str> = builds.pro_core(PA, build, &texts).iter().map(|pro| pro.item.as_str()).collect();
        assert_eq!(pros, ["item_bottle", "item_power_treads"]);
        assert_eq!(build.core(&texts).iter().map(|item| item.item.as_str()).collect::<Vec<_>>(), ["item_basher"]);
        assert_eq!(build.whole_items(&texts).len(), 2);
        assert!(builds.others(PA, build, &texts, &[]).is_empty());

        let carry = builds.of(PA, Rank::Archon, Role::Carry).unwrap();
        assert_eq!(builds.pro_core(PA, carry, &texts).iter().map(|pro| (pro.item.as_str(), pro.share)).collect::<Vec<_>>(), [("item_bottle", 60.0)]);
        assert!(builds.pro_core("npc_dota_hero_axe", build, &texts).is_empty());
        assert_eq!(builds.pro_core(PA, build, &GameTexts::default()).len(), 3);
    }

    #[test]
    fn the_order_of_skills_is_point_by_point_and_the_same_for_every_rank() {
        let builds = read(&file()).unwrap();
        let order = builds.order(PA, Role::Carry).unwrap();
        assert_eq!((order.matches, order.skills.len(), builds.sampled), (600, 5, 900));
        assert_eq!(order.skills[..2], ["phantom_assassin_stifling_dagger".to_string(), "phantom_assassin_phantom_strike".to_string()]);
        assert_eq!(order.skills[4], "special_bonus_unique_phantom_assassin_5");
        assert_eq!(builds.order("NPC_DOTA_HERO_PHANTOM_ASSASSIN", Role::Mid).map(|order| order.skills.len()), Some(2));
        assert!(builds.order(PA, Role::Offlane).is_none() && builds.order("npc_dota_hero_axe", Role::Carry).is_none());
        assert_eq!(builds.all_orders().count(), 2);
        assert_eq!((POINT_LEVELS[0], POINT_LEVELS[15], POINT_LEVELS[16], POINT_LEVELS[17], POINT_LEVELS[18]), (1, 16, 18, 20, 25));
    }

    #[test]
    fn files_that_cannot_be_read_add_nothing() {
        assert!(neutral_tiers(b"").is_empty() && neutral_tiers(b"{{{").is_empty() && neutral_tiers(b"\"neutral_items\" { }").is_empty());
        let texts = GameTexts::from_files(Names::default(), &[b"\"DOTAHeroes\" { \"broken".to_vec(), Vec::new()], b"{{{", b"nonsense {");
        assert!(texts.talents(PA).is_empty());
        assert!(!texts.is_part_of("item_pers", "item_bfury"));
        assert_eq!(texts.item_name("item_blink"), "Blink");
    }

    const NEUTRALS: &str = r##""neutral_items" { "madstone_limits" { "madstone_no_limit_time" "65:00" } "neutral_tiers" {
        "1" { "start_time" "0:00" "item_pools" { "pool_a" { "options" "1" "items" { "item_pogo_stick" "1" "item_chipped_vest" "1" } } }
            "enhancements" { "global" { "item_enhancement_quickened" "1" } "agility" { "item_enhancement_alert" "1" } "universal" { "item_enhancement_alert" "1" } } }
        "2" { "start_time" "15:00" "item_pools" { "pool_a" { "items" { "item_brick" "1" } } "pool_b" { "items" { "item_Defiant_Shell" "1" } } }
            "enhancements" { "global" { "item_enhancement_quickened" "2" } } }
    } }"##;

    fn with_neutrals() -> serde_json::Value {
        let mut file = file();
        file["neutrals"] = json!({
            "source": "OpenDota", "patch": "7.42", "matches": 40,
            "names": ["item_pogo_stick", "item_enhancement_alert", "item_old_trinket", "item_brick", "item_defiant_shell"],
            "heroes": { PA: [
                [0, 10, [[1, 10, 0, [[0, 7], [2, 2]], [[1, 9]]], [2, 8, 2, [[3, 4], [4, 2]], []], [3, 5, 0, [[0, 5]], []]]],
                [5, 13, [[1, 13, 0, [[0, 10]], [[1, 13]]]]],
            ] },
        });
        file
    }

    #[test]
    fn the_neutral_items_pros_take_are_those_the_game_has_in_each_tier() {
        let tiers = neutral_tiers(NEUTRALS.as_bytes());
        assert_eq!(tiers.iter().map(|tier| (tier.tier, tier.minute)).collect::<Vec<_>>(), [(1, 0), (2, 15)]);
        assert_eq!(tiers[0].trinkets, ["item_chipped_vest", "item_pogo_stick"]);
        assert_eq!(tiers[0].enhancements, ["item_enhancement_alert", "item_enhancement_quickened"], "of every attribute, each once");
        assert_eq!(tiers[1].trinkets, ["item_brick", "item_defiant_shell"], "of every pool");

        let builds = read(&with_neutrals()).unwrap();
        assert_eq!((builds.neutral_source.as_str(), builds.neutral_patch.as_str(), builds.neutral_matches), ("OpenDota", "7.42", 40));
        assert_eq!(builds.neutral_build(PA, Role::Carry).map(|neutral| (neutral.role, neutral.players)), Some((Some(Role::Carry), 10)));
        assert_eq!(builds.neutral_build("NPC_DOTA_HERO_PHANTOM_ASSASSIN", Role::Mid).map(|neutral| (neutral.role, neutral.players)), Some((None, 13)));
        assert!(builds.neutral_build("npc_dota_hero_axe", Role::Carry).is_none());
        assert_eq!(builds.all_neutral_builds().count(), 2);

        let texts = GameTexts { neutral_tiers: tiers, ..texts() };
        let (neutral, shown) = builds.neutral_tiers(PA, Role::Carry, &texts).unwrap();
        assert_eq!(neutral.tiers.len(), 3);
        assert_eq!(
            shown,
            [
                NeutralTier { tier: 1, minute: 0, reached: 10, kept: 0, trinkets: vec![("item_pogo_stick", 70.0)], enhancements: vec![("item_enhancement_alert", 90.0)] },
                NeutralTier { tier: 2, minute: 15, reached: 8, kept: 2, trinkets: vec![("item_brick", 50.0), ("item_defiant_shell", 25.0)], enhancements: vec![] },
            ]
        );
        assert!(builds.neutral_tiers(PA, Role::Carry, &self::texts()).is_none());
        assert!(read(&file()).unwrap().neutral_build(PA, Role::Carry).is_none());
    }

    #[test]
    fn neutral_items_that_are_not_what_this_version_reads_leave_the_rest_of_the_file_whole() {
        for broken in [json!("nonsense"), json!({ "names": [], "heroes": { PA: [[0, 10, [[1, 10, 0, [[0, 7]], []]]]] } }), json!({ "names": ["item_x"], "heroes": { PA: [[7, 10, []]] } })] {
            let mut file = with_neutrals();
            file["neutrals"] = broken;
            let builds = read(&file).unwrap();
            assert!(builds.neutral_build(PA, Role::Carry).is_none() && builds.neutral_matches == 0);
            assert!(builds.of(PA, Rank::Archon, Role::Carry).is_some());
        }
    }

    #[test]
    fn an_item_whose_name_the_game_keeps_under_n_has_it() {
        let names = r##""lang" { "Tokens" { "dota_tooltip_ability_item_pogo_stick:n" "Tumbler's Toy" "DOTA_Tooltip_Ability_item_blink" "Blink Dagger" } }"##;
        let texts = GameTexts::from_files(Names::parse(names.as_bytes()), &[], b"", b"");
        assert_eq!((texts.item_name("item_pogo_stick"), texts.item_name("item_blink")), ("Tumbler's Toy".to_string(), "Blink Dagger".to_string()));
        assert!(texts.names("item_pogo_stick"));
    }

    #[test]
    fn a_talent_the_numbers_do_not_count_is_new_where_they_count_talents_the_hero_no_longer_has() {
        let own = |ids: &[&str]| ids.iter().map(|id| Talent { id: id.to_string(), text: String::new() }).collect::<Vec<_>>();
        let taken = |talent: &str, matches: u64| Taken { talent: talent.to_string(), matches, wins: matches / 2 };
        let mut build = read(&file()).unwrap().of(PA, Rank::Archon, Role::Carry).unwrap().clone();
        build.talents = vec![taken("special_bonus_a", 600), taken("special_bonus_old", 400), taken("special_bonus_none", 0)];
        let now = own(&["special_bonus_a", "special_bonus_new", "special_bonus_none"]);
        assert!(build.is_new_talent(&now, "special_bonus_new") && build.is_new_talent(&now, "SPECIAL_BONUS_NONE"));
        assert!(!build.is_new_talent(&now, "special_bonus_a"));
        let before = own(&["special_bonus_a", "special_bonus_old", "special_bonus_none", "special_bonus_new"]);
        assert!(!build.is_new_talent(&before, "special_bonus_new") && !build.is_new_talent(&before, "special_bonus_none"));
    }
}
