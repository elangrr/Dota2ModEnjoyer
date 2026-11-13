use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;

use serde::{Deserialize, Serialize};

use crate::effects;
use crate::error::{Error, Result};
use crate::items::{self, AttachedEffect, Item, ItemsGame};
use crate::kv::{Block, Pair, Value};

pub const LOOKS: &[&str] = &[
    "model_player",
    "model_player1",
    "model_player2",
    "model_player3",
    "visuals",
    "portraits",
    "particle_folder",
    "particle_snapshot",
    "styles",
    "match_cycle_to_parent",
    "override_attack_attachments",
    "disable_hero_portrait_override",
    "set_parent_skin",
    "courier_slots",
    "image_inventory",
    "item_name",
    "item_description",
    "item_type_name",
];

pub const NOT_COPIED: &[&str] = &[
    "name",
    "prefab",
    "used_by_heroes",
    "item_slot",
    "baseitem",
    "item_class",
    "min_ilevel",
    "max_ilevel",
    "player_loadout",
    "is_dedicated_showcase_background",
    "item_quality",
    "item_rarity",
    "forced_item_quality",
    "static_attributes",
    "capabilities",
    "price_info",
    "tool",
    "bundle",
    "tags",
    "associated_item",
    "equip_all_items_in_set",
    "has_store_custom_item_details_panel",
    "hide_in_store",
    "hide_tradecraftdelete",
    "hide_in_purchase_popup",
    "shards_purchase_price",
    "treasure_display_override",
    "plus_auto_grant_yearly_pattern_start",
    "plus_auto_grant_yearly_pattern_end",
    "creation_date",
    "expiration_date",
    "event_id",
    "branding",
    "developer",
];

const FILE_ENDINGS: &[&str] = &[".vmdl", ".vpcf", ".vmap"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hero: Option<String>,
    pub slot: String,
    pub item: String,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub style: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<u32>,
}

fn is_zero(number: &u32) -> bool {
    *number == 0
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectsFor {
    Every,
    #[default]
    Picked,
    These(Vec<String>),
}

impl EffectsFor {
    pub fn is_every(&self) -> bool {
        *self == EffectsFor::Every
    }

    pub fn is_usual(&self) -> bool {
        *self == EffectsFor::default()
    }

    pub fn reaches(&self, hero: &str, picks: &[Pick]) -> bool {
        match self {
            EffectsFor::Every => true,
            EffectsFor::Picked => picks.iter().any(|pick| pick.hero.as_deref().is_some_and(|owner| owner.eq_ignore_ascii_case(hero))),
            EffectsFor::These(heroes) => heroes.iter().any(|chosen| chosen.eq_ignore_ascii_case(hero)),
        }
    }

    pub fn reaches_anyone(&self, picks: &[Pick]) -> bool {
        match self {
            EffectsFor::Every => true,
            EffectsFor::Picked => picks.iter().any(|pick| pick.hero.is_some()),
            EffectsFor::These(heroes) => !heroes.is_empty(),
        }
    }
}

pub fn is_for_heroes(pick: &Pick) -> bool {
    effects::is_emblem_pick(pick) || (pick.hero.is_none() && effects::kind_of(&pick.slot).is_some())
}

pub fn reach_nobody(picks: &[Pick], effects_for: &EffectsFor) -> bool {
    picks.iter().any(is_for_heroes) && !effects_for.reaches_anyone(picks)
}

impl Pick {
    pub fn hero(hero: &str, slot: &str, item: &str) -> Pick {
        Pick { hero: Some(hero.to_string()), slot: slot.to_string(), item: item.to_string(), style: 0, effect: None }
    }

    pub fn world(slot: &str, item: &str) -> Pick {
        Pick { hero: None, slot: slot.to_string(), item: item.to_string(), style: 0, effect: None }
    }

    pub fn styled(mut self, style: u32) -> Pick {
        self.style = style;
        self
    }

    pub fn with_effect(mut self, effect: Option<u32>) -> Pick {
        self.effect = effect;
        self
    }

    pub fn place(&self) -> String {
        match &self.hero {
            Some(hero) => format!("{}'s {} slot", items::hero_short_name(hero), self.slot),
            None => format!("the {} slot", self.slot),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub hero: Option<String>,
    pub slot: String,
    pub default_id: String,
    pub item_id: String,
    pub item_name: String,
    pub style: u32,
    pub model: Option<String>,
    pub files: Vec<String>,
}

#[derive(Debug)]
pub struct Patched {
    pub bytes: Vec<u8>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub item_name: String,
    pub reason: String,
}

#[derive(Debug)]
pub struct SetPicks {
    pub picks: Vec<Pick>,
    pub skipped: Vec<Skipped>,
}

pub fn set_picks(game: &ItemsGame<'_>, hero: &str, set_key: &str, style: u32) -> Result<SetPicks> {
    if game.hero_slots(hero).is_empty() {
        return Err(Error::NotFound(format!("there is no hero called {hero}")));
    }
    let set = game
        .set(set_key)
        .ok_or_else(|| Error::NotFound(format!("there is no set called {set_key}")))?;
    if !game.set_is_for(set, hero) {
        return Err(Error::NotFound(format!("{set_key} is not a set for {hero}")));
    }

    let mut picks = Vec::new();
    let mut skipped = Vec::new();
    let mut slots_filled = HashSet::new();
    for item in game.set_items(set).into_iter().flatten() {
        let name = item.name.unwrap_or(item.id);
        let mut skip = |reason: String| skipped.push(Skipped { item_name: name.to_string(), reason });
        let Some(slot) = game.slot(item) else {
            skip("it has no slot".to_string());
            continue;
        };
        if game.default_item(Some(hero), &slot).is_none() {
            skip(if item.has_prefab("wearable") {
                format!("the hero has no built-in item in the {slot} slot")
            } else {
                format!("it is a {}, not something the hero wears", item.prefab.unwrap_or("special item"))
            });
            continue;
        }
        if !game.fits(item, Some(hero), &slot) {
            skip("it cannot take the place of a built-in item".to_string());
            continue;
        }
        if !slots_filled.insert(slot.clone()) {
            skip(format!("another item of the set already fills the {slot} slot"));
            continue;
        }
        let style = if (item.styles_to_choose().len() as u32) > style { style } else { 0 };
        picks.push(Pick::hero(hero, &slot, item.id).styled(style));
    }
    Ok(SetPicks { picks, skipped })
}

pub fn patch_items_game(vanilla: &[u8], picks: &[Pick]) -> Result<Patched> {
    patch(&ItemsGame::parse(vanilla)?, vanilla, picks)
}

pub fn patch<'a>(game: &ItemsGame<'a>, vanilla: &'a [u8], picks: &[Pick]) -> Result<Patched> {
    patch_with(game, vanilla, picks, &|_, _| true)
}

pub fn patch_with<'a>(game: &ItemsGame<'a>, vanilla: &'a [u8], picks: &[Pick], live: &dyn Fn(&str, &str) -> bool) -> Result<Patched> {
    patch_for(game, vanilla, picks, live, &EffectsFor::Every)
}

pub fn patch_for<'a>(game: &ItemsGame<'a>, vanilla: &'a [u8], picks: &[Pick], live: &dyn Fn(&str, &str) -> bool, effects_for: &EffectsFor) -> Result<Patched> {
    let reaches = |hero: &str| effects_for.reaches(hero, picks);
    let reaches_all = |default: &Item<'_>| default.heroes.iter().all(|hero| reaches(hero));
    let mut plans: Vec<Plan<'_, 'a>> = Vec::new();
    let mut effects_taken = HashSet::new();
    let mut for_all: Vec<ForAll<'_, 'a>> = Vec::new();
    let mut emblem: Option<Emblem<'_, 'a>> = None;
    let personas = personas(game, picks, live)?;
    let persona_of_hero = |hero: Option<&str>| hero.and_then(|hero| personas.iter().find(|persona| persona.hero.eq_ignore_ascii_case(hero)));

    for (index, pick) in picks.iter().enumerate() {
        let hero = pick.hero.as_deref();
        if let Some(hero) = hero
            && game.hero_slots(hero).is_empty()
        {
            return Err(Error::NotFound(format!("there is no hero called {hero}")));
        }
        let place = pick.place();
        if let Some(persona) = persona_of_hero(hero) {
            if persona.takes(pick) {
                continue;
            }
            if persona_takes_the_place_of(&pick.slot) {
                let name = persona.item.name.unwrap_or(persona.item.id);
                return Err(Error::Refused(format!("{place} cannot have a pick while the hero has a persona ({name}), which takes its place")));
            }
        }
        let find_item = || game.item(&pick.item).ok_or_else(|| Error::NotFound(format!("there is no item with id {}", pick.item)));

        if effects::is_emblem_pick(pick) {
            let item = find_item()?;
            let blocks = effects::emblem_blocks(game, item);
            if blocks.is_empty() {
                return Err(Error::NotFound(format!("{} is no emblem that shows on a hero", item.name.unwrap_or(item.id))));
            }
            if emblem.is_some() {
                return Err(Error::Refused(format!("{place} is picked more than once")));
            }
            emblem = Some(Emblem { index, item, blocks });
            continue;
        }

        if let Some(kind) = effects::kind_of(&pick.slot).filter(|_| hero.is_none()) {
            let item = find_item()?;
            let levels = if effects::is_pack(game, item) { effects::variants(item, kind) } else { Vec::new() };
            let Some(level) = levels.into_iter().nth(pick.style as usize) else {
                return Err(Error::NotFound(format!("{} has no such effect for {place}", item.name.unwrap_or(item.id))));
            };
            if for_all.iter().any(|known| known.slot == kind.slot()) {
                return Err(Error::Refused(format!("{place} is picked more than once")));
            }
            for_all.push(ForAll { index, slot: kind.slot(), item, style: pick.style, blocks: level.blocks });
            continue;
        }
        let effects = hero.and_then(|hero| Some((hero, items::effects_slot(&pick.slot)?)));

        let (default, item) = match effects {
            Some((hero, _)) => {
                let item = find_item()?;
                let name = item.name.unwrap_or(item.id);
                let carrier = game.carrier(hero, item).filter(|_| game.fits(item, Some(hero), &pick.slot));
                let carrier = carrier.ok_or_else(|| Error::NotFound(format!("{name} has no ability effects for {place}")))?;
                let carrier = persona_of_hero(Some(hero)).and_then(|persona| persona.moved(carrier)).unwrap_or(carrier);
                if pick.style > 0 && !item.styles().iter().any(|style| style.index == pick.style) {
                    return Err(Error::NotFound(format!("{name} has no style {}", pick.style)));
                }
                (carrier, item)
            }
            None => {
                let default = game.default_item(hero, &pick.slot).ok_or_else(|| {
                    Error::NotFound(format!("the game has no default item for {place}, so nothing can be shown there"))
                })?;
                let item = find_item()?;
                if !game.fits(item, hero, &pick.slot) {
                    return Err(Error::NotFound(format!("{} does not go in {place}", item.name.unwrap_or(item.id))));
                }
                (default, item)
            }
        };
        let twice = || Error::Refused(format!("{place} is picked more than once"));
        let plan = plan_for(&mut plans, default, hero);
        match effects {
            Some((hero, slot)) => {
                if !effects_taken.insert((hero.to_ascii_lowercase(), pick.slot.to_ascii_lowercase())) {
                    return Err(twice());
                }
                let style = if item.switches_styles_itself() { 0 } else { pick.style };
                plan.effects.push(Effects { index, slot: pick.slot.to_ascii_lowercase(), number: slot.number, item, style });
            }
            None if plan.look.is_some() && plan.hero.as_deref().is_some_and(|other| !hero.is_some_and(|hero| hero.eq_ignore_ascii_case(other))) => {
                let other = plan.hero.as_deref().map(items::hero_short_name).unwrap_or_default();
                return Err(Error::Refused(format!("{place} and {other}'s have one default item between them, so only one of the two can have a pick")));
            }
            None if plan.look.is_some() => return Err(twice()),
            None => {
                let slot = match game.world_slot_name(&pick.slot).filter(|_| hero.is_none()) {
                    Some(name) => name.to_string(),
                    None => pick.slot.to_ascii_lowercase(),
                };
                let style = if item.switches_styles_itself() { 0 } else { pick.style };
                let effect = match pick.effect {
                    Some(id) => Some(game.attached_effect(id).ok_or_else(|| Error::NotFound(format!("there is no effect numbered {id}")))?),
                    None => None,
                };
                plan.look = Some(Look { index, slot, item, style, effect, reported: true });
            }
        }
    }
    for persona in &personas {
        let base = plan_for(&mut plans, persona.base, Some(&persona.hero));
        base.look = Some(Look { index: persona.index, slot: persona.base_slot.to_string(), item: persona.item, style: persona.style, effect: None, reported: true });
        base.persona_base = true;
        for part in &persona.parts {
            let plan = plan_for(&mut plans, part.default, Some(&persona.hero));
            match &part.worn {
                Some(worn) => {
                    let effect = match worn.effect {
                        Some(id) => Some(game.attached_effect(id).ok_or_else(|| Error::NotFound(format!("there is no effect numbered {id}")))?),
                        None => None,
                    };
                    let reported = worn.index.is_some();
                    let index = worn.index.unwrap_or(persona.index);
                    plan.look = Some(Look { index, slot: part.slot.to_string(), item: worn.item, style: worn.style, effect, reported });
                }
                None => plan.bare = true,
            }
        }
    }

    if !for_all.is_empty() {
        for (hero, default) in game.hero_default_items().into_iter().filter(|(_, default)| reaches_all(default)) {
            plan_for(&mut plans, default, Some(hero));
        }
    }
    for_all.sort_by(|one, other| one.slot.cmp(&other.slot));
    if emblem.is_some() {
        for carrier in emblem_carriers(game, picks, live).into_iter().filter(|carrier| reaches(carrier.hero)) {
            plan_for(&mut plans, carrier.default, Some(carrier.hero)).emblem = true;
        }
    }

    let mut edits: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
    let mut changes: Vec<(usize, Change)> = Vec::new();
    let mut expected = Vec::new();
    for plan in &mut plans {
        let default = plan.default;
        let mut rewritten = match &plan.look {
            Some(look) => with_looks(vanilla, default.block, look.item, look.style, &look.slot)?,
            None if plan.bare => without_looks(vanilla, default),
            None => as_written(vanilla, default),
        };
        let layout = Layout::of(vanilla, default.block);
        let mut sockets = None;
        if let Some(look) = &plan.look {
            let held: Vec<Vec<u8>> = look.item.static_sockets().iter().map(|pair| vanilla[pair.start..pair.end].to_vec()).collect();
            sockets = Some(put_in_sockets(vanilla, default.block, &mut rewritten.parts, &held));
        } else if plan.bare {
            sockets = Some(put_in_sockets(vanilla, default.block, &mut rewritten.parts, &[]));
        }
        plan.effects.sort_by_key(|effects| effects.number);
        let gem = plan.look.as_ref().and_then(|look| look.effect);
        let carries = plan.hero.is_some() && reaches_all(default) && !for_all.is_empty();
        let emblem = emblem.as_ref().filter(|_| plan.emblem);
        let mut added = Vec::new();
        let mut blocks = None;
        if !plan.effects.is_empty() || gem.is_some() || carries || emblem.is_some() || plan.persona_base {
            let mut visuals = Visuals::of(&rewritten.parts)?;
            if plan.persona_base {
                visuals.shown.retain(|visual| visual.kind.as_deref() != Some(PERSONA_TYPE));
            }
            added = add_effects(vanilla, &layout, &mut visuals, &plan.effects)?;
            if let Some(effect) = gem {
                for block in gem_blocks(&layout, &visuals, effect) {
                    visuals.add(block);
                }
            }
            if carries {
                for pair in for_all.iter().flat_map(|event| &event.blocks) {
                    visuals.add(Visual::new(pair, for_everybody(vanilla, pair)));
                }
            }
            for pair in emblem.iter().flat_map(|emblem| &emblem.blocks) {
                visuals.add(Visual::new(pair, vanilla[pair.start..pair.end].to_vec()));
            }
            for (effects, added) in plan.effects.iter().zip(&mut added) {
                added.blocks = visuals.shown.iter().filter(|visual| visual.mark == Some(effects.number)).count();
            }
            blocks = Some(visuals.put_back(vanilla, default.block, &mut rewritten.parts));
        }

        let texts: Vec<Cow<'_, [u8]>> = rewritten.parts.iter().map(|(_, text)| Cow::Borrowed(text.as_ref())).collect();
        let content = write_block(vanilla, default.block, &texts);
        let model = rewritten.model.map(|model| String::from_utf8_lossy(model).into_owned());
        let change = |slot: &str, item: &Item<'_>, style: u32, model: Option<String>, files: Vec<String>| Change {
            hero: plan.hero.clone(),
            slot: slot.to_string(),
            default_id: default.id.to_string(),
            item_id: item.id.to_string(),
            item_name: item.name.unwrap_or(item.id).to_string(),
            style,
            model,
            files,
        };
        if let Some(look) = plan.look.as_ref().filter(|look| look.reported) {
            changes.push((look.index, change(&look.slot, look.item, look.style, model.clone(), referenced_files(&content))));
        }
        for (effects, added) in plan.effects.iter().zip(&added) {
            changes.push((effects.index, change(&effects.slot, effects.item, effects.style, None, referenced_files(&added.text))));
        }
        expected.push(Expected {
            id: default.id.to_string(),
            hero: plan.hero.clone(),
            slot: match &plan.look {
                Some(look) => look.slot.clone(),
                None => game.slot(default).unwrap_or_default(),
            },
            model,
            marked: plan.effects.iter().zip(&added).map(|(effects, added)| (effects.number, added.blocks)).collect(),
            blocks,
            sockets,
        });
        edits.push((default.block.start()..default.block.end(), content));
    }
    let for_every_hero = |slot: &str, item: &Item<'_>, style: u32, texts: Vec<Vec<u8>>| Change {
        hero: None,
        slot: slot.to_string(),
        default_id: String::new(),
        item_id: item.id.to_string(),
        item_name: item.name.unwrap_or(item.id).to_string(),
        style,
        model: None,
        files: referenced_files(&texts.join(b"\n".as_slice())),
    };
    for event in &for_all {
        let texts = event.blocks.iter().map(|pair| for_everybody(vanilla, pair)).collect();
        changes.push((event.index, for_every_hero(&event.slot, event.item, event.style, texts)));
    }
    if let Some(emblem) = &emblem {
        let texts = emblem.blocks.iter().map(|pair| vanilla[pair.start..pair.end].to_vec()).collect();
        changes.push((emblem.index, for_every_hero(effects::EMBLEM_SLOT, emblem.item, 0, texts)));
    }
    changes.sort_by_key(|(index, _)| *index);
    let changes: Vec<Change> = changes.into_iter().map(|(_, change)| change).collect();

    let bytes = splice(vanilla, edits)?;
    check(game, &bytes, &expected)?;
    Ok(Patched { bytes, changes })
}

struct Plan<'g, 'a> {
    default: &'g Item<'a>,
    hero: Option<String>,
    look: Option<Look<'g, 'a>>,
    effects: Vec<Effects<'g, 'a>>,
    emblem: bool,
    bare: bool,
    persona_base: bool,
}

fn plan_for<'p, 'g, 'a>(plans: &'p mut Vec<Plan<'g, 'a>>, default: &'g Item<'a>, hero: Option<&str>) -> &'p mut Plan<'g, 'a> {
    let at = match plans.iter().position(|plan| plan.default.id == default.id) {
        Some(at) => at,
        None => {
            plans.push(Plan { default, hero: hero.map(str::to_string), look: None, effects: Vec::new(), emblem: false, bare: false, persona_base: false });
            plans.len() - 1
        }
    };
    &mut plans[at]
}

struct Emblem<'g, 'a> {
    index: usize,
    item: &'g Item<'a>,
    blocks: Vec<Pair<'a>>,
}

#[derive(Debug)]
pub struct EmblemCarrier<'g, 'a> {
    pub hero: &'a str,
    pub slot: &'g str,
    pub default: &'g Item<'a>,
    pub others: usize,
}

impl EmblemCarrier<'_, '_> {
    pub fn always_worn(&self) -> bool {
        self.others == 0
    }
}

const AMBIENT_SLOT: &str = "ambient_effects";

fn carries_an_emblem(slot: &str) -> bool {
    const NOT: [&str; 7] = ["summon", "voice", "costume", "hero_base", "persona_selector", "shapeshift", "taunt"];
    !slot.starts_with("ability") && !NOT.contains(&slot) && items::persona_of(slot).is_empty()
}

pub fn emblem_carriers<'g, 'a>(game: &'g ItemsGame<'a>, picks: &[Pick], live: &dyn Fn(&str, &str) -> bool) -> Vec<EmblemCarrier<'g, 'a>> {
    let to_pick = game.items_to_pick();
    let mut carriers = Vec::new();
    for hero in game.heroes() {
        let slots: Vec<(&'g str, &'g Item<'a>)> = game.hero_slots(hero).into_iter().filter(|(slot, _)| carries_an_emblem(slot) && live(hero, slot)).collect();
        let others = |slot: &str| to_pick.get(&(hero.to_ascii_lowercase(), slot.to_string())).copied().unwrap_or(0);
        let picked_for = |slot: &str| picks.iter().any(|pick| pick.slot.eq_ignore_ascii_case(slot) && pick.hero.as_deref().is_some_and(|owner| owner.eq_ignore_ascii_case(hero)));
        let first = |fits: &dyn Fn(&str) -> bool| {
            let fitting = || slots.iter().filter(|(slot, _)| fits(slot));
            fitting().find(|(slot, _)| *slot == AMBIENT_SLOT).or_else(|| fitting().next()).copied()
        };
        let always_worn = first(&|slot| others(slot) == 0);
        let fewest_others = || slots.iter().min_by_key(|(slot, _)| others(slot)).copied();
        if let Some((slot, default)) = always_worn.or_else(|| first(&picked_for)).or_else(fewest_others) {
            carriers.push(EmblemCarrier { hero, slot, default, others: others(slot) });
        }
    }
    carriers
}

const HERO_BASE: &str = "hero_base";
pub const PERSONA_SELECTOR: &str = "persona_selector";
const PERSONA_TYPE: &[u8] = b"persona";

pub fn persona_takes_the_place_of(slot: &str) -> bool {
    let slot = slot.to_ascii_lowercase();
    slot == HERO_BASE || carries_an_emblem(&slot)
}

struct Persona<'g, 'a> {
    index: usize,
    hero: String,
    item: &'g Item<'a>,
    style: u32,
    base: &'g Item<'a>,
    base_slot: &'static str,
    parts: Vec<PersonaSlot<'g, 'a>>,
}

struct PersonaSlot<'g, 'a> {
    slot: &'g str,
    default: &'g Item<'a>,
    worn: Option<Worn<'g, 'a>>,
}

struct Worn<'g, 'a> {
    slot: &'g str,
    default: &'g Item<'a>,
    item: &'g Item<'a>,
    style: u32,
    effect: Option<u32>,
    index: Option<usize>,
}

impl<'g, 'a> Persona<'g, 'a> {
    fn takes(&self, pick: &Pick) -> bool {
        pick.slot.eq_ignore_ascii_case(PERSONA_SELECTOR) || self.parts.iter().filter_map(|part| part.worn.as_ref()).any(|worn| worn.slot.eq_ignore_ascii_case(&pick.slot))
    }

    fn moved(&self, persona_default: &Item<'a>) -> Option<&'g Item<'a>> {
        let worn_in = |part: &&PersonaSlot<'g, 'a>| part.worn.as_ref().is_some_and(|worn| worn.default.id == persona_default.id);
        self.parts.iter().find(worn_in).map(|part| part.default)
    }
}

fn persona_number(item: &Item<'_>) -> Option<u32> {
    let visuals = item.block.block("visuals")?;
    visuals.pairs().filter_map(|pair| pair.value.as_block()).find(|block| block.str("type").is_some_and(|kind| kind.eq_ignore_ascii_case(PERSONA_TYPE)))?.str("persona").and_then(parse_number)
}

fn personas<'g, 'a>(game: &'g ItemsGame<'a>, picks: &[Pick], live: &dyn Fn(&str, &str) -> bool) -> Result<Vec<Persona<'g, 'a>>> {
    let mut found: Vec<Persona<'g, 'a>> = Vec::new();
    for (index, pick) in picks.iter().enumerate() {
        let Some(hero) = pick.hero.as_deref().filter(|_| pick.slot.eq_ignore_ascii_case(PERSONA_SELECTOR)) else { continue };
        let Some(item) = game.item(&pick.item).filter(|item| game.fits(item, Some(hero), PERSONA_SELECTOR)) else { continue };
        let Some(number) = persona_number(item) else { continue };
        if found.iter().any(|persona| persona.hero.eq_ignore_ascii_case(hero)) {
            return Err(Error::Refused(format!("{} is picked more than once", pick.place())));
        }
        let place = pick.place();
        let (base, base_slot) = match game.default_item(Some(hero), HERO_BASE) {
            Some(base) => (base, HERO_BASE),
            None => (game.default_item(Some(hero), PERSONA_SELECTOR).ok_or_else(|| Error::NotFound(format!("the game has no default item for {place}")))?, PERSONA_SELECTOR),
        };
        let suffix = format!("_persona_{number}");
        let slots = game.hero_slots(hero);
        let worn_by_hero: Vec<(&'g str, &'g Item<'a>)> = slots.iter().copied().filter(|(slot, _)| items::persona_of(slot).is_empty() && carries_an_emblem(slot) && live(hero, slot)).collect();
        let mut parts: Vec<Worn<'g, 'a>> = Vec::new();
        for (slot, default) in slots.iter().copied() {
            let Some(stands_for) = slot.to_ascii_lowercase().strip_suffix(&suffix).map(str::to_string) else { continue };
            if !carries_an_emblem(&stands_for) || !live(hero, slot) {
                continue;
            }
            let picked = picks.iter().enumerate().find(|(_, other)| other.hero.as_deref().is_some_and(|owner| owner.eq_ignore_ascii_case(hero)) && other.slot.eq_ignore_ascii_case(slot));
            let worn = match picked {
                Some((at, other)) => {
                    let part = game.item(&other.item).filter(|part| game.fits(part, Some(hero), slot));
                    let part = part.ok_or_else(|| Error::NotFound(format!("{} does not go in {}", other.item, other.place())))?;
                    Worn { slot, default, item: part, style: if part.switches_styles_itself() { 0 } else { other.style }, effect: other.effect, index: Some(at) }
                }
                None => Worn { slot, default, item: default, style: 0, effect: None, index: None },
            };
            parts.push(worn);
        }
        let mut placed: Vec<Option<Worn<'g, 'a>>> = worn_by_hero.iter().map(|_| None).collect();
        let mut homeless = Vec::new();
        for part in parts {
            let stands_for = part.slot[..part.slot.len() - suffix.len()].to_ascii_lowercase();
            match worn_by_hero.iter().position(|(slot, _)| slot.eq_ignore_ascii_case(&stands_for)) {
                Some(at) => placed[at] = Some(part),
                None => homeless.push(part),
            }
        }
        for part in homeless {
            let free = placed.iter().zip(&worn_by_hero).position(|(taken, (slot, _))| taken.is_none() && !parts_stand_for(slots.as_slice(), slot, &suffix));
            match free {
                Some(at) => placed[at] = Some(part),
                None => return Err(Error::Refused(format!("{place}: the hero has no slot left for the persona's {}", part.slot))),
            }
        }
        let parts = worn_by_hero.iter().zip(placed).map(|((slot, default), worn)| PersonaSlot { slot, default, worn }).collect();
        found.push(Persona { index, hero: hero.to_string(), item, style: pick.style, base, base_slot, parts });
    }
    Ok(found)
}

fn parts_stand_for(slots: &[(&str, &Item<'_>)], slot: &str, suffix: &str) -> bool {
    slots.iter().any(|(other, _)| other.eq_ignore_ascii_case(&format!("{slot}{suffix}")))
}

struct Look<'g, 'a> {
    index: usize,
    slot: String,
    item: &'g Item<'a>,
    style: u32,
    effect: Option<&'g AttachedEffect<'a>>,
    reported: bool,
}

struct ForAll<'g, 'a> {
    index: usize,
    slot: String,
    item: &'g Item<'a>,
    style: u32,
    blocks: Vec<Pair<'a>>,
}

struct Effects<'g, 'a> {
    index: usize,
    slot: String,
    number: u32,
    item: &'g Item<'a>,
    style: u32,
}

type Part<'a> = (&'a [u8], Cow<'a, [u8]>);

struct Rewritten<'a> {
    parts: Vec<Part<'a>>,
    model: Option<&'a [u8]>,
}

fn as_written<'a>(src: &'a [u8], default: &Item<'a>) -> Rewritten<'a> {
    let parts = default.block.pairs().map(|pair| (pair.key, Cow::Borrowed(&src[pair.start..pair.end]))).collect();
    Rewritten { parts, model: default.model.map(str::as_bytes) }
}

fn without_looks<'a>(src: &'a [u8], default: &Item<'a>) -> Rewritten<'a> {
    let parts = default.block.pairs().filter(|pair| !listed(LOOKS, pair.key)).map(|pair| (pair.key, Cow::Borrowed(&src[pair.start..pair.end]))).collect();
    Rewritten { parts, model: None }
}

fn with_looks<'a>(src: &'a [u8], default: Block<'a>, item: &Item<'a>, style: u32, slot: &str) -> Result<Rewritten<'a>> {
    let baked = if style == 0 { None } else { Some(bake_style(src, item, style)?) };

    let mut copied: Vec<Part<'a>> = Vec::new();
    for pair in item.block.pairs().filter(|pair| !listed(NOT_COPIED, pair.key)) {
        let text = match &baked {
            Some(baked) if is(pair.key, "visuals") => Cow::Owned(baked.visuals.clone()),
            Some(Baked { model: Some(model), .. }) if is(pair.key, "model_player") => {
                Cow::Owned(string_pair("model_player", model))
            }
            Some(Baked { icon: Some(icon), .. }) if is(pair.key, "image_inventory") => {
                Cow::Owned(string_pair("image_inventory", icon))
            }
            _ => Cow::Borrowed(&src[pair.start..pair.end]),
        };
        copied.push((pair.key, text));
    }
    let style_model = baked.as_ref().and_then(|baked| baked.model);
    if let Some(model) = style_model
        && !copied.iter().any(|(key, _)| is(key, "model_player"))
    {
        copied.push((b"model_player".as_slice(), Cow::Owned(string_pair("model_player", model))));
    }

    let kept = default
        .pairs()
        .filter(|pair| !listed(LOOKS, pair.key))
        .filter(|pair| !copied.iter().any(|(key, _)| key.eq_ignore_ascii_case(pair.key)));
    let mut parts: Vec<Part<'a>> = kept
        .map(|pair| match pair.value {
            Value::Str(named) if is(pair.key, "item_slot") && !named.eq_ignore_ascii_case(slot.as_bytes()) => {
                (pair.key, Cow::Owned(string_pair("item_slot", slot.as_bytes())))
            }
            _ => (pair.key, Cow::Borrowed(&src[pair.start..pair.end])),
        })
        .collect();
    parts.extend(copied);

    Ok(Rewritten { parts, model: style_model.or(item.model.map(str::as_bytes)) })
}

struct Visual {
    text: Vec<u8>,
    mark: Option<u32>,
    replaces: Option<(Vec<u8>, Vec<u8>)>,
    system: Option<Vec<u8>>,
    creates: bool,
    kind: Option<Vec<u8>>,
}

impl Visual {
    fn new(pair: &Pair<'_>, text: Vec<u8>) -> Visual {
        let block = pair.value.as_block();
        let value = |key: &str| block.and_then(|block| block.str(key));
        let kind = value("type").map(<[u8]>::to_ascii_lowercase);
        let replaces = match (&kind, value("asset")) {
            (Some(kind), Some(asset)) if !asset.is_empty() => {
                let mut what = asset.to_ascii_lowercase();
                if let Some(number) = value("control_point_number") {
                    what.push(b'#');
                    what.extend_from_slice(number.trim_ascii());
                }
                Some((kind.clone(), what))
            }
            _ => None,
        };
        let creates = kind.as_ref().is_some_and(|kind| kind == CREATES);
        let brings = creates || kind.as_ref().is_some_and(|kind| kind == b"particle");
        let system = value("modifier").filter(|_| brings).map(<[u8]>::to_vec);
        Visual { text, mark: value(items::EFFECTS_MARK).and_then(parse_number), replaces, system, creates, kind }
    }

    fn made(text: Vec<u8>) -> Visual {
        let read = Block::document(&text).pairs().next().map(|pair| Visual::new(&pair, Vec::new()));
        let read = read.unwrap_or(Visual { text: Vec::new(), mark: None, replaces: None, system: None, creates: false, kind: None });
        Visual { text, ..read }
    }
}

const CREATES: &[u8] = b"particle_create";

struct Visuals {
    at: Option<usize>,
    old: Option<Vec<u8>>,
    shown: Vec<Visual>,
}

impl Visuals {
    fn of(parts: &[Part<'_>]) -> Result<Visuals> {
        let at = parts.iter().position(|(key, _)| is(key, "visuals"));
        let old: Option<Vec<u8>> = at.map(|at| parts[at].1.to_vec());
        let shown = match &old {
            Some(text) => {
                let block = Block::document(text).pairs().next().and_then(|pair| pair.value.as_block());
                let block = block.ok_or_else(|| Error::format("the item database", "an item's visuals are not a block"))?;
                block.pairs().map(|inner| Visual::new(&inner, text[inner.start..inner.end].to_vec())).collect()
            }
            None => Vec::new(),
        };
        Ok(Visuals { at, old, shown })
    }

    fn add(&mut self, new: Visual) {
        if new.replaces.is_some() {
            self.shown.retain(|old| old.replaces != new.replaces);
        }
        self.shown.push(new);
    }

    fn put_back<'a>(self, src: &'a [u8], default: Block<'a>, parts: &mut Vec<Part<'a>>) -> usize {
        if self.at.is_none() && self.shown.is_empty() {
            return 0;
        }
        let texts: Vec<Cow<'_, [u8]>> = self.shown.iter().map(|visual| Cow::Borrowed(visual.text.as_slice())).collect();
        let was = self.old.as_deref().and_then(|text| {
            let pair = Block::document(text).pairs().next()?;
            Some((text, pair, pair.value.as_block()?))
        });
        let visuals = match was {
            Some((text, pair, block)) => rewrite_pair(text, &pair, block, &texts),
            None => new_block_pair(src, default, "visuals", &texts),
        };
        match self.at {
            Some(at) => parts[at].1 = Cow::Owned(visuals),
            None => parts.push((b"visuals".as_slice(), Cow::Owned(visuals))),
        }
        self.shown.len()
    }
}

struct Added {
    text: Vec<u8>,
    blocks: usize,
}

fn add_effects(src: &[u8], layout: &Layout<'_>, visuals: &mut Visuals, effects: &[Effects<'_, '_>]) -> Result<Vec<Added>> {
    let mut added = Vec::new();
    for one in effects {
        let (incoming, states) = if one.item.switches_styles_itself() {
            effects_of_states(src, layout, one.item, one.number, &visuals.shown)
        } else {
            let blocks = one.item.effects_in_style(one.number, one.style);
            let incoming = blocks.iter().map(|pair| {
                let of_a_later_style = pair.value.as_block().filter(|block| block.str("style").and_then(parse_number).is_some_and(|style| style > 0));
                let text = match of_a_later_style {
                    Some(block) => without_key(src, pair, block, "style"),
                    None => src[pair.start..pair.end].to_vec(),
                };
                Visual::new(pair, text)
            });
            (incoming.collect(), None)
        };
        if incoming.is_empty() {
            let name = one.item.name.unwrap_or(one.item.id);
            return Err(Error::NotFound(format!("{name} has no ability effects for the {} slot in style {}", one.slot, one.style)));
        }
        visuals.shown.retain(|old| old.mark != Some(one.number) && !incoming.iter().any(|new| new.replaces.is_some() && new.replaces == old.replaces));
        let texts: Vec<&[u8]> = incoming.iter().map(|new| new.text.as_slice()).collect();
        added.push(Added { text: texts.join(b"\n".as_slice()), blocks: 0 });
        visuals.shown.extend(incoming);
        visuals.shown.extend(states);
    }
    Ok(added)
}

fn effects_of_states(src: &[u8], layout: &Layout<'_>, item: &Item<'_>, number: u32, shown: &[Visual]) -> (Vec<Visual>, Option<Visual>) {
    let blocks = item.effects_for(number);
    let state_of = |pair: &Pair<'_>| pair.value.as_block().and_then(|block| block.str("style")).and_then(parse_number);
    let what = |pair: &Pair<'_>| {
        let fields = pair.value.as_block().into_iter().flat_map(|block| block.pairs()).filter(|inner| !is(inner.key, "style"));
        let mut fields: Vec<(Vec<u8>, Vec<u8>)> = fields
            .map(|inner| {
                let value = match inner.value {
                    Value::Str(text) => text.trim_ascii().to_ascii_lowercase(),
                    Value::Block(_) => src[inner.start..inner.end].to_vec(),
                };
                (inner.key.to_ascii_lowercase(), value)
            })
            .collect();
        fields.sort();
        fields
    };
    let replaces = |pair: &Pair<'_>| Visual::new(pair, Vec::new()).replaces;
    let states: Vec<u32> = item.styles().iter().map(|style| style.index).collect();
    let in_every_state = |pair: &Pair<'_>| {
        let mine = what(pair);
        states.iter().all(|&state| blocks.iter().any(|other| state_of(other).is_none_or(|marked| marked == state) && what(other) == mine))
    };
    let alone = |pair: &Pair<'_>| replaces(pair).is_some() && blocks.iter().all(|other| replaces(other) != replaces(pair) || what(other) == what(pair));
    let keeps_its_mark = |pair: &Pair<'_>| state_of(pair).is_some() && !in_every_state(pair) && !alone(pair);

    let theirs = shown.iter().find_map(|visual| rules_of_states(&visual.text));
    let ours = rules_of(item);
    let go_through = blocks.iter().any(keeps_its_mark) && theirs.as_ref().is_none_or(|theirs| *theirs == ours);
    let first = states.first().copied().unwrap_or(0);
    let (mut incoming, mut taken) = (Vec::new(), Vec::new());
    for pair in &blocks {
        if keeps_its_mark(pair) {
            if go_through {
                incoming.push(Visual::new(pair, src[pair.start..pair.end].to_vec()));
                continue;
            }
            if state_of(pair) != Some(first) {
                continue;
            }
        }
        let mine = what(pair);
        if taken.contains(&mine) {
            continue;
        }
        taken.push(mine);
        let text = match pair.value.as_block().filter(|block| block.get("style").is_some()) {
            Some(block) => without_key(src, pair, block, "style"),
            None => src[pair.start..pair.end].to_vec(),
        };
        incoming.push(Visual::new(pair, text));
    }
    let states = (go_through && theirs.is_none()).then(|| states_pair(layout, item));
    (incoming, states)
}

fn rules_of(item: &Item<'_>) -> Vec<Vec<u8>> {
    item.styles().iter().map(|style| rule_text(style.block.str(items::AUTO_STYLE_RULE))).collect()
}

fn rules_of_states(text: &[u8]) -> Option<Vec<Vec<u8>>> {
    let pair = Block::document(text).pairs().next().filter(|pair| is(pair.key, "styles"))?;
    let styles = pair.value.as_block()?;
    Some(styles.pairs().map(|style| rule_text(style.value.as_block().and_then(|block| block.str(items::AUTO_STYLE_RULE)))).collect())
}

fn rule_text(rule: Option<&[u8]>) -> Vec<u8> {
    rule.unwrap_or_default().iter().filter(|byte| !byte.is_ascii_whitespace()).map(u8::to_ascii_lowercase).collect()
}

fn states_pair(layout: &Layout<'_>, item: &Item<'_>) -> Visual {
    let line = [layout.newline, &layout.pair_indent, b"\t".as_slice()].concat();
    let inner = [line.as_slice(), b"\t"].concat();
    let mut text = b"\"styles\"".to_vec();
    text.extend_from_slice(&line);
    text.push(b'{');
    for style in item.styles() {
        text.extend_from_slice(&inner);
        text.extend_from_slice(format!("\"{}\"", style.index).as_bytes());
        text.extend_from_slice(&inner);
        text.push(b'{');
        if let Some(rule) = style.block.str(items::AUTO_STYLE_RULE) {
            text.extend_from_slice(&inner);
            text.push(b'\t');
            text.extend_from_slice(&string_pair(items::AUTO_STYLE_RULE, rule));
        }
        text.extend_from_slice(&inner);
        text.push(b'}');
    }
    text.extend_from_slice(&line);
    text.push(b'}');
    Visual::made(text)
}

fn gem_blocks(layout: &Layout<'_>, visuals: &Visuals, effect: &AttachedEffect<'_>) -> Vec<Visual> {
    let comes_with_it = |visual: &Visual| visual.creates && visual.system.as_deref().is_some_and(|system| system.eq_ignore_ascii_case(effect.system.as_bytes()));
    if visuals.shown.iter().any(comes_with_it) {
        return Vec::new();
    }
    vec![made_block(layout, &[("type", CREATES), ("modifier", effect.system.as_bytes())])]
}

fn for_everybody(src: &[u8], pair: &Pair<'_>) -> Vec<u8> {
    match pair.value.as_block().filter(|block| block.get("criteria").is_some()) {
        Some(block) => without_key(src, pair, block, "criteria"),
        None => src[pair.start..pair.end].to_vec(),
    }
}

fn made_block(layout: &Layout<'_>, fields: &[(&str, &[u8])]) -> Visual {
    Visual::made(made_pair(layout, "asset_modifier", fields))
}

fn made_pair(layout: &Layout<'_>, key: &str, fields: &[(&str, &[u8])]) -> Vec<u8> {
    let line = [layout.newline, &layout.pair_indent, b"\t".as_slice()].concat();
    let mut text = [b"\"", key.as_bytes(), b"\""].concat();
    text.extend_from_slice(&line);
    text.push(b'{');
    for (key, value) in fields {
        text.extend_from_slice(&line);
        text.push(b'\t');
        text.extend_from_slice(&string_pair(key, value));
    }
    text.extend_from_slice(&line);
    text.push(b'}');
    text
}

fn put_in_sockets<'a>(src: &'a [u8], default: Block<'a>, parts: &mut Vec<Part<'a>>, held: &[Vec<u8>]) -> usize {
    const KEY: &str = "static_attributes";
    let new = held.iter().map(|socket| Cow::Borrowed(socket.as_slice()));
    let Some(at) = parts.iter().position(|(key, _)| is(key, KEY)) else {
        if !held.is_empty() {
            parts.push((KEY.as_bytes(), Cow::Owned(new_block_pair(src, default, KEY, &new.collect::<Vec<_>>()))));
        }
        return held.len();
    };
    let text = parts[at].1.to_vec();
    let read = Block::document(&text).pairs().next().and_then(|pair| Some((pair, pair.value.as_block()?)));
    let Some((pair, block)) = read else { return 0 };
    let is_a_socket = |inner: &Pair<'_>| items::socket_number(inner.key).is_some();
    if held.is_empty() && !block.pairs().any(|inner| is_a_socket(&inner)) {
        return 0;
    }
    let kept = block.pairs().filter(|inner| !is_a_socket(inner)).map(|inner| Cow::Borrowed(&text[inner.start..inner.end]));
    let texts: Vec<Cow<'_, [u8]>> = kept.chain(new).collect();
    parts[at].1 = Cow::Owned(rewrite_pair(&text, &pair, block, &texts));
    held.len()
}

struct Baked<'a> {
    visuals: Vec<u8>,
    model: Option<&'a [u8]>,
    icon: Option<&'a [u8]>,
}

fn bake_style<'a>(src: &'a [u8], item: &Item<'a>, style: u32) -> Result<Baked<'a>> {
    let name = item.name.unwrap_or(item.id);
    let no_style = || Error::NotFound(format!("{name} has no style {style}"));
    let chosen = item.styles().into_iter().find(|s| s.index == style).ok_or_else(no_style)?.block;
    let (visuals_pair, visuals) = item
        .block
        .pairs()
        .find_map(|pair| Some((pair, pair.value.as_block().filter(|_| is(pair.key, "visuals"))?)))
        .ok_or_else(no_style)?;

    let skin = chosen.str("skin");
    let mut parts: Vec<Cow<'_, [u8]>> = Vec::new();
    let mut skin_written = false;
    for pair in visuals.pairs() {
        let as_written = Cow::Borrowed(&src[pair.start..pair.end]);
        match pair.value {
            Value::Block(_) if is(pair.key, "styles") => {}
            Value::Block(effect) => match effect.str("style").and_then(parse_number) {
                None => parts.push(as_written),
                Some(for_style) if for_style == style => parts.push(Cow::Owned(without_key(src, &pair, effect, "style"))),
                Some(_) => {}
            },
            Value::Str(_) => match skin.filter(|_| is(pair.key, "skin")) {
                Some(skin) => {
                    parts.push(Cow::Owned(string_pair("skin", skin)));
                    skin_written = true;
                }
                None => parts.push(as_written),
            },
        }
    }
    if let Some(skin) = skin
        && !skin_written
    {
        parts.insert(0, Cow::Owned(string_pair("skin", skin)));
    }

    let icon = chosen.str("alternate_icon").and_then(|index| {
        let index = std::str::from_utf8(index).ok()?;
        visuals.block("alternate_icons")?.block(index)?.str("icon_path")
    });
    Ok(Baked {
        visuals: rewrite_pair(src, &visuals_pair, visuals, &parts),
        model: chosen.str("model_player"),
        icon,
    })
}

fn without_key(src: &[u8], pair: &Pair<'_>, effect: Block<'_>, key: &str) -> Vec<u8> {
    let parts: Vec<Cow<'_, [u8]>> = effect
        .pairs()
        .filter(|inner| !is(inner.key, key))
        .map(|inner| Cow::Borrowed(&src[inner.start..inner.end]))
        .collect();
    rewrite_pair(src, pair, effect, &parts)
}

fn rewrite_pair(src: &[u8], pair: &Pair<'_>, block: Block<'_>, parts: &[Cow<'_, [u8]>]) -> Vec<u8> {
    let mut text = src[pair.start..block.start()].to_vec();
    text.extend_from_slice(&write_block(src, block, parts));
    text.push(b'}');
    text
}

struct Layout<'s> {
    newline: &'static [u8],
    pair_indent: Vec<u8>,
    closing_indent: &'s [u8],
}

impl<'s> Layout<'s> {
    fn of(src: &'s [u8], block: Block<'_>) -> Layout<'s> {
        let closing_indent = indent_before(src, block.end());
        Layout {
            newline: if src[block.start()..block.end()].contains(&b'\r') { b"\r\n" } else { b"\n" },
            pair_indent: match block.pairs().next() {
                Some(first) => indent_before(src, first.start).to_vec(),
                None => [closing_indent, b"\t".as_slice()].concat(),
            },
            closing_indent,
        }
    }
}

fn write_block(src: &[u8], block: Block<'_>, parts: &[Cow<'_, [u8]>]) -> Vec<u8> {
    let layout = Layout::of(src, block);
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(layout.newline);
        out.extend_from_slice(&layout.pair_indent);
        out.extend_from_slice(part);
    }
    out.extend_from_slice(layout.newline);
    out.extend_from_slice(layout.closing_indent);
    out
}

fn new_block_pair(src: &[u8], within: Block<'_>, key: &str, parts: &[Cow<'_, [u8]>]) -> Vec<u8> {
    let layout = Layout::of(src, within);
    let line = [layout.newline, &layout.pair_indent].concat();
    let mut text = [b"\"", key.as_bytes(), b"\"", &line, b"{"].concat();
    for part in parts {
        text.extend_from_slice(&line);
        text.push(b'\t');
        text.extend_from_slice(part);
    }
    text.extend_from_slice(&line);
    text.push(b'}');
    text
}

fn string_pair(key: &str, value: &[u8]) -> Vec<u8> {
    [b"\"", key.as_bytes(), b"\"\t\t\"", value, b"\""].concat()
}

fn is(key: &[u8], name: &str) -> bool {
    key.eq_ignore_ascii_case(name.as_bytes())
}

fn listed(list: &[&str], key: &[u8]) -> bool {
    list.iter().any(|known| is(key, known))
}

fn parse_number(text: &[u8]) -> Option<u32> {
    std::str::from_utf8(text).ok()?.trim().parse().ok()
}

fn indent_before(src: &[u8], at: usize) -> &[u8] {
    let line_start = src[..at].iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let indent = &src[line_start..at];
    if indent.iter().all(|&b| b == b' ' || b == b'\t') { indent } else { &[] }
}

fn referenced_files(content: &[u8]) -> Vec<String> {
    let mut files = Vec::new();
    Block::document(content).for_each_string(&mut |_, value| {
        let lower = value.to_ascii_lowercase();
        if FILE_ENDINGS.iter().any(|ending| lower.ends_with(ending.as_bytes()))
            && let Ok(path) = std::str::from_utf8(value)
        {
            files.push(path.to_string());
        }
    });
    files.sort();
    files.dedup();
    files
}

fn splice(src: &[u8], mut edits: Vec<(Range<usize>, Vec<u8>)>) -> Result<Vec<u8>> {
    edits.sort_by_key(|(range, _)| range.start);
    let mut out = Vec::with_capacity(src.len() + edits.iter().map(|(_, new)| new.len()).sum::<usize>());
    let mut at = 0;
    for (range, new) in &edits {
        if range.start < at || range.end > src.len() {
            return Err(Error::Refused("two picks would rewrite the same item".to_string()));
        }
        out.extend_from_slice(&src[at..range.start]);
        out.extend_from_slice(new);
        at = range.end;
    }
    out.extend_from_slice(&src[at..]);
    Ok(out)
}

struct Expected {
    id: String,
    hero: Option<String>,
    slot: String,
    model: Option<String>,
    marked: Vec<(u32, usize)>,
    blocks: Option<usize>,
    sockets: Option<usize>,
}

fn check(before: &ItemsGame<'_>, bytes: &[u8], expected: &[Expected]) -> Result<()> {
    let wrong = |detail: String| Error::format("the edited item database", detail);
    let after = ItemsGame::parse(bytes)?;
    if after.items().len() != before.items().len() || after.sets().len() != before.sets().len() {
        return Err(wrong("the number of items or sets changed".to_string()));
    }
    for expected in expected {
        let id = &expected.id;
        let (Some(item), Some(original)) = (after.item(id), before.item(id)) else {
            return Err(wrong(format!("item {id} is gone")));
        };
        let still_the_default = after
            .default_item(expected.hero.as_deref(), &expected.slot)
            .is_some_and(|default| default.id == *id);
        let intact = still_the_default
            && item.name == original.name
            && item.prefab == original.prefab
            && item.heroes == original.heroes
            && item.model == expected.model.as_deref()
            && expected.marked.iter().all(|&(number, blocks)| item.effects_for(number).len() == blocks)
            && expected.blocks.is_none_or(|blocks| item.block.block("visuals").map_or(0, |visuals| visuals.pairs().count()) == blocks)
            && expected.sockets.is_none_or(|sockets| item.static_sockets().len() == sockets);
        if !intact {
            return Err(wrong(format!("item {id} did not come out as intended")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
