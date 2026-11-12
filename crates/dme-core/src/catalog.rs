use std::collections::{HashMap, HashSet};

use crate::apply::{self, Pick};
use crate::effects;
use crate::engine::ITEMS_GAME;
use crate::error::{Error, Result};
use crate::heroes::HeroFiles;
use crate::items::{self, AttachedEffect, Item, ItemSet, ItemsGame, Style};
use crate::kv;
use crate::map_effect;
use crate::names::Names;
use crate::pictures;
use crate::resource::Wholeness;
use crate::trees;
use crate::vpk::Archive;

#[derive(Debug, Default)]
pub struct Catalog {
    pub heroes: Vec<Hero>,
    pub world: Vec<Slot>,
    pub rarities: Vec<Rarity>,
    pub lost_files: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Effect {
    pub id: u32,
    pub name: String,
    pub for_couriers: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gap {
    Files,
    ArcanaLevel,
    Checkered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rarity {
    pub id: String,
    pub name: String,
    pub color: [u8; 3],
}

pub const NO_COLOR: [u8; 3] = [0xb0, 0xb0, 0xb0];

#[derive(Debug)]
pub struct Hero {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub picture: Option<String>,
    pub sets: Vec<Set>,
    pub slots: Vec<Slot>,
}

impl Hero {
    pub fn fits(&self, wanted: &str) -> Option<u8> {
        if wanted.is_empty() {
            return Some(0);
        }
        let name = self.name.to_lowercase();
        let aliases = || self.aliases.iter().map(|alias| alias.to_lowercase());
        let starts_a_word = |text: &str| text.split([' ', '-', '\'']).any(|word| word.starts_with(wanted));
        if name.starts_with(wanted) || aliases().any(|alias| alias == wanted) {
            Some(0)
        } else if starts_a_word(&name) || aliases().any(|alias| starts_a_word(&alias)) {
            Some(1)
        } else if name.contains(wanted) || aliases().any(|alias| alias.contains(wanted)) {
            Some(2)
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub struct Set {
    pub key: String,
    pub name: String,
    pub picture: Option<String>,
    pub pieces: Vec<Pick>,
    pub rarity: Option<usize>,
}

#[derive(Debug)]
pub struct Slot {
    pub id: String,
    pub name: String,
    pub default: Entry,
    pub items: Vec<Entry>,
    pub effects: Vec<Effect>,
    pub shared_with: Vec<Sharer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sharer {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub picture: Option<String>,
    pub styles: Vec<StyleEntry>,
    pub rarity: Option<usize>,
    pub worn_in: Option<String>,
    pub gaps: Vec<Gap>,
}

#[derive(Clone, Debug)]
pub struct StyleEntry {
    pub index: u32,
    pub name: String,
    pub picture: Option<String>,
}

pub const NAME_FILES: [&str; 3] = [
    "resource/localization/items_english.txt",
    "resource/localization/dota_english.txt",
    "resource/localization/abilities_english.txt",
];

pub fn read_names(archive: &Archive) -> Names {
    let mut names = Names::default();
    for file in NAME_FILES {
        if let Ok(Some(bytes)) = archive.read(file) {
            names.extend(Names::parse(&bytes));
        }
    }
    names
}

impl Catalog {
    pub fn load(archive: &Archive) -> Result<Catalog> {
        let bytes = archive
            .read(ITEMS_GAME)?
            .ok_or_else(|| Error::format("the game's archive", format!("{ITEMS_GAME} is missing")))?;
        let game = ItemsGame::parse(&bytes)?;
        Ok(Catalog::build(&game, &read_names(archive), archive))
    }

    pub fn build(game: &ItemsGame<'_>, names: &Names, archive: &Archive) -> Catalog {
        let rarities: Vec<Rarity> = game
            .rarities()
            .iter()
            .map(|rarity| Rarity {
                id: rarity.id.to_ascii_lowercase(),
                name: match rarity.name_token.and_then(|token| names.get(token)) {
                    Some(name) => plain(name).to_string(),
                    None => title(rarity.id),
                },
                color: rarity.color.unwrap_or(NO_COLOR),
            })
            .collect();
        let rarity_places = rarities.iter().enumerate().map(|(place, rarity)| (rarity.id.clone(), place)).collect();

        let named = |effect: &AttachedEffect<'_>| {
            let name = names.get(&format!("Attrib_Particle{}", effect.id))?;
            let shown = archive.contains(&format!("{}_c", effect.system));
            shown.then(|| Effect { id: effect.id, name: plain(name).to_string(), for_couriers: effect.is_for_couriers() })
        };
        let mut for_couriers: Vec<Effect> = game.attached_effects().iter().filter_map(named).collect();
        for_couriers.sort_by_cached_key(|effect| (effect.name.to_lowercase(), effect.id));
        let mut for_heroes = for_couriers.clone();
        for_heroes.sort_by_key(|effect| effect.for_couriers);

        let wholeness = Wholeness::new(archive);
        let reader = Reader { game, names, archive, rarity_places, hero_files: HeroFiles::read(archive), for_heroes, for_couriers, wholeness };
        let mut heroes: Vec<Hero> = game.heroes().into_iter().filter_map(|hero| reader.hero(hero)).collect();
        heroes.sort_by_cached_key(|hero| hero.name.to_lowercase());
        let mut world: Vec<Slot> = game.world_slots().iter().filter_map(|slot| reader.slot(None, slot)).collect();
        world.extend(reader.emblem_slot());
        let events: Vec<&Item<'_>> = game.items().iter().filter(|item| effects::is_pack(game, item)).collect();
        let pictures = reader.event_pictures(&events);
        world.extend(effects::KINDS.iter().filter_map(|kind| reader.event_slot(kind, &events, &pictures)));
        world.extend(reader.trees_slot());
        world.extend(reader.map_slot());
        let lost_files = reader.wholeness.gone();
        drop(reader);
        Catalog { heroes, world, rarities, lost_files }
    }

    pub fn hero(&self, id: &str) -> Option<&Hero> {
        self.heroes.iter().find(|hero| hero.id.eq_ignore_ascii_case(id))
    }

    pub fn slots(&self, hero: Option<&str>) -> &[Slot] {
        match hero {
            Some(hero) => self.hero(hero).map_or(&[][..], |hero| hero.slots.as_slice()),
            None => &self.world,
        }
    }

    pub fn slot(&self, hero: Option<&str>, slot: &str) -> Option<&Slot> {
        self.slots(hero).iter().find(|known| known.id.eq_ignore_ascii_case(slot))
    }

    pub fn entry(&self, pick: &Pick) -> Option<&Entry> {
        self.slot(pick.hero.as_deref(), &pick.slot)?.entry(&pick.item)
    }

    pub fn settled(&self, pick: &Pick) -> Option<Pick> {
        let slot = self.slot(pick.hero.as_deref(), &pick.slot)?;
        let entry = slot.entry(&pick.item)?;
        let style = if entry.has_style(pick.style) { pick.style } else { entry.first_style() };
        let effect = pick.effect.and_then(|id| slot.effect(id));
        Some(Pick { style, effect: effect.map(|effect| effect.id), ..pick.clone() })
    }
}

impl Slot {
    pub fn entry(&self, item: &str) -> Option<&Entry> {
        self.items.iter().find(|entry| entry.id == item)
    }

    pub fn effect(&self, id: u32) -> Option<&Effect> {
        self.effects.iter().find(|effect| effect.id == id)
    }

    pub fn is_for_an_event(&self) -> bool {
        effects::kind_of(&self.id).is_some()
    }

    pub fn is_for_an_emblem(&self) -> bool {
        self.id == effects::EMBLEM_SLOT
    }

    pub fn is_for_trees(&self) -> bool {
        self.id == trees::SLOT
    }

    pub fn is_for_the_map(&self) -> bool {
        self.id == map_effect::SLOT
    }

    pub fn is_for_effects(&self) -> bool {
        items::effects_slot(&self.id).is_some()
    }

    pub fn group(&self) -> Group {
        Group::of(&self.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Worn,
    Effects,
    Persona,
    PersonaEffects,
    Events,
    Experiments,
}

impl Group {
    fn of(slot: &str) -> Group {
        if effects::kind_of(slot).is_some() || slot == effects::EMBLEM_SLOT {
            return Group::Events;
        }
        if slot == trees::SLOT || slot == map_effect::SLOT {
            return Group::Experiments;
        }
        let persona = !items::persona_of(slot).is_empty() || slot.eq_ignore_ascii_case(PERSONA_SELECTOR);
        match (persona, items::effects_slot(slot).is_some()) {
            (false, false) => Group::Worn,
            (false, true) => Group::Effects,
            (true, false) => Group::Persona,
            (true, true) => Group::PersonaEffects,
        }
    }
}

const PERSONA_SELECTOR: &str = "persona_selector";
const COURIER: &str = "courier";
const ARCANA_LEVEL: &str = "required_arcana_level";
const NO_PICTURE: &str = "econ/testitem_slot_empty";
const NORMAL_MAP_PICTURE: &str = "econ/terrain/default_terrain";
const EVENT_PASS: &[u8] = b"compendium";
const EVENT_KEEPSAKES: [&str; 4] = ["emblem", "treasure_chest", "key", "bundle"];

impl Entry {
    pub fn has_style(&self, index: u32) -> bool {
        if self.styles.is_empty() { index == 0 } else { self.styles.iter().any(|style| style.index == index) }
    }

    pub fn first_style(&self) -> u32 {
        self.styles.first().map_or(0, |style| style.index)
    }

    pub fn in_style(&self, index: u32) -> (Option<&str>, Option<&str>) {
        match self.styles.iter().find(|style| style.index == index) {
            Some(style) => (Some(&style.name), style.picture.as_deref().or(self.picture.as_deref())),
            None => (None, self.picture.as_deref()),
        }
    }
}

struct Reader<'g, 'a> {
    game: &'g ItemsGame<'a>,
    names: &'g Names,
    archive: &'g Archive,
    rarity_places: HashMap<String, usize>,
    hero_files: HeroFiles,
    for_heroes: Vec<Effect>,
    for_couriers: Vec<Effect>,
    wholeness: Wholeness<'g>,
}

impl Reader<'_, '_> {
    fn hero(&self, id: &str) -> Option<Hero> {
        let worn = self.game.hero_slots(id).into_iter().filter(|(slot, _)| self.hero_files.has_slot(id, slot));
        let mut slots: Vec<Slot> = worn.filter_map(|(slot, _)| self.slot(Some(id), slot)).collect();
        slots.extend(self.hero_files.ability_slots(id).iter().filter_map(|known| self.effects_slot(id, known.slot, known.ability)));
        slots.sort_by_key(|slot| (list_place(&slot.id), self.hero_files.place(id, &slot.id).unwrap_or(usize::MAX)));
        self.tell_apart(&mut slots);
        let mut sets: Vec<Set> = self.game.sets_for(id).into_iter().filter_map(|set| self.set(id, set)).collect();
        sets.sort_by_cached_key(|set| set.name.to_lowercase());
        if slots.is_empty() && sets.is_empty() {
            return None;
        }
        let name = self.hero_name(id);
        let mut aliases = self.hero_files.aliases(id).to_vec();
        let short = items::hero_short_name(id).replace('_', " ");
        if !short.eq_ignore_ascii_case(&name) && !aliases.iter().any(|alias| alias.eq_ignore_ascii_case(&short)) {
            aliases.push(short);
        }
        Some(Hero { id: id.to_string(), name, aliases, picture: self.picture(&format!("heroes/{id}")), sets, slots })
    }

    fn hero_name(&self, id: &str) -> String {
        match self.names.get(&format!("{id}:n")) {
            Some(name) => plain(name).to_string(),
            None => title(items::hero_short_name(id)),
        }
    }

    fn slot(&self, hero: Option<&str>, slot: &str) -> Option<Slot> {
        let default = self.game.default_item(hero, slot)?;
        let shares = |other: &&str| {
            hero.is_some_and(|hero| !other.eq_ignore_ascii_case(hero))
                && self.hero_files.has_slot(other, slot)
                && self.game.default_item(Some(other), slot).is_some_and(|theirs| theirs.id == default.id)
        };
        let mut shared_with: Vec<Sharer> = default.heroes.iter().copied().filter(shares).map(|other| Sharer { id: other.to_ascii_lowercase(), name: self.hero_name(other) }).collect();
        shared_with.sort_by_cached_key(|sharer| sharer.name.to_lowercase());
        let mut items: Vec<Entry> = self.game.items_for(hero, slot).into_iter().filter_map(|item| self.entry(item)).collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.parse::<u64>().unwrap_or(u64::MAX)));
        let default = Entry {
            id: default.id.to_string(),
            name: self.item_name(default),
            picture: self.item_picture(default),
            styles: Vec::new(),
            rarity: None,
            worn_in: None,
            gaps: Vec::new(),
        };
        let effects = match hero {
            Some(_) if slot.eq_ignore_ascii_case(PERSONA_SELECTOR) => Vec::new(),
            Some(_) => self.for_heroes.clone(),
            None if slot.eq_ignore_ascii_case(COURIER) => self.for_couriers.clone(),
            None => Vec::new(),
        };
        Some(Slot { id: slot.to_ascii_lowercase(), name: self.slot_name(hero, slot), default, items, effects, shared_with })
    }

    fn event_slot(&self, kind: &effects::Kind, events: &[&Item<'_>], pictures: &HashMap<String, String>) -> Option<Slot> {
        let picture = kind.item.and_then(|item| self.picture(&format!("items/{}", item.strip_prefix("item_").unwrap_or(item))));
        let picture_of = |event: &Item<'_>| pictures.get(event.id).or(picture.as_ref()).cloned();
        let mut items: Vec<Entry> = events.iter().filter_map(|event| self.event_entry(event, kind, picture_of(event))).collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.parse::<u64>().unwrap_or(u64::MAX)));
        Some(Slot { id: kind.slot(), name: kind.name.to_string(), default: self.nothing(kind.name, picture), items, effects: Vec::new(), shared_with: Vec::new() })
    }

    fn nothing(&self, name: &str, picture: Option<String>) -> Entry {
        Entry { id: String::new(), name: name.to_string(), picture, styles: Vec::new(), rarity: None, worn_in: None, gaps: Vec::new() }
    }

    fn plain_entry(&self, item: &Item<'_>) -> Entry {
        Entry {
            id: item.id.to_string(),
            name: self.item_name(item),
            picture: self.item_picture(item),
            styles: Vec::new(),
            rarity: self.rarity_of(item),
            worn_in: None,
            gaps: Vec::new(),
        }
    }

    fn emblem_slot(&self) -> Option<Slot> {
        let shows = |item: &&Item<'_>| {
            let blocks = effects::emblem_blocks(self.game, item);
            !blocks.is_empty() && blocks.iter().filter_map(file_of).all(|file| self.archive.contains(&format!("{file}_c")))
        };
        let mut items: Vec<Entry> = self.game.items().iter().filter(shows).map(|item| self.plain_entry(item)).collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.parse::<u64>().unwrap_or(u64::MAX)));
        let name = self.general_name(effects::EMBLEM_SLOT).unwrap_or_else(|| title(effects::EMBLEM_SLOT));
        Some(Slot { id: effects::EMBLEM_SLOT.to_string(), name, default: self.nothing("No emblem", None), items, effects: Vec::new(), shared_with: Vec::new() })
    }

    fn trees_slot(&self) -> Option<Slot> {
        let entry = |kind: &trees::Kind| Entry { id: kind.id.to_string(), ..self.nothing(kind.name, self.picture(kind.picture)) };
        let items: Vec<Entry> = trees::KINDS.iter().filter(|kind| kind.is_there(self.archive)).map(entry).collect();
        let default = self.nothing("The map's own trees", self.picture(NORMAL_MAP_PICTURE));
        (!items.is_empty()).then(|| Slot { id: trees::SLOT.to_string(), name: "Trees".to_string(), default, items, effects: Vec::new(), shared_with: Vec::new() })
    }

    fn map_slot(&self) -> Option<Slot> {
        let shows = |item: &&Item<'_>| map_effect::is_there(self.game, self.archive, item);
        let entry = |item: &Item<'_>| {
            let mut entry = self.plain_entry(item);
            if map_effect::shows_checkered(self.game, item) {
                entry.gaps.push(Gap::Checkered);
            }
            entry
        };
        let mut items: Vec<Entry> = self.game.items().iter().filter(shows).map(entry).collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.parse::<u64>().unwrap_or(u64::MAX)));
        let name = self.general_name(map_effect::SLOT).unwrap_or_else(|| title(map_effect::SLOT));
        let default = self.nothing("The map as it is", self.picture(NORMAL_MAP_PICTURE));
        Some(Slot { id: map_effect::SLOT.to_string(), name, default, items, effects: Vec::new(), shared_with: Vec::new() })
    }

    fn event_pictures(&self, events: &[&Item<'_>]) -> HashMap<String, String> {
        let event_of = |item: &Item<'_>| item.block.str("event_id").and_then(text).filter(|id| !id.is_empty()).map(str::to_ascii_lowercase);
        let wanted: HashSet<String> = events.iter().filter_map(|event| event_of(event)).collect();
        let mut by_event: HashMap<String, Vec<&Item<'_>>> = HashMap::new();
        for item in self.game.items() {
            if let Some(id) = event_of(item).filter(|id| wanted.contains(id)) {
                by_event.entry(id).or_default().push(item);
            }
        }
        let number = |item: &Item<'_>| item.id.parse::<u64>().unwrap_or(u64::MAX);
        let is_a_pass = |item: &&&Item<'_>| item.block.block("tool").and_then(|tool| tool.str("type")).is_some_and(|kind| kind.eq_ignore_ascii_case(EVENT_PASS));
        let of_the_event = |event: &Item<'_>| {
            let others = by_event.get(&event_of(event)?)?;
            let mut passes: Vec<&&Item<'_>> = others.iter().filter(is_a_pass).collect();
            passes.sort_by_key(|pass| (pass.name.map_or(0, str::len), number(pass)));
            let pass = passes.into_iter().find_map(|pass| self.item_picture(pass));
            pass.or_else(|| {
                EVENT_KEEPSAKES.iter().find_map(|prefab| {
                    let mut kept: Vec<&&Item<'_>> = others.iter().filter(|item| item.has_prefab(prefab)).collect();
                    kept.sort_by_key(|item| number(item));
                    kept.into_iter().find_map(|item| self.item_picture(item))
                })
            })
        };
        fn files<'a>(event: &Item<'a>) -> Vec<&'a str> {
            event.block.block("visuals").into_iter().flat_map(|visuals| visuals.pairs()).filter_map(|pair| file_of(&pair)).collect()
        }
        let team = |event: &Item<'_>| files(event).into_iter().filter_map(team_number).find_map(|team| self.picture(&format!("teams/{team}")));
        let brand = |event: &Item<'_>| {
            let names = files(event).into_iter().flat_map(brand_names);
            names.filter(|_| event_of(event).is_none()).find_map(|name| self.picture(&format!("econ/brands/brand_{name}")))
        };
        let own = |event: &Item<'_>| self.item_picture(event).filter(|_| !event.block.str("image_inventory").is_some_and(|image| image.eq_ignore_ascii_case(NO_PICTURE.as_bytes())));
        let picture = |event: &Item<'_>| own(event).or_else(|| team(event)).or_else(|| of_the_event(event)).or_else(|| brand(event));
        events.iter().filter_map(|event| Some((event.id.to_string(), picture(event)?))).collect()
    }

    fn event_entry(&self, event: &Item<'_>, kind: &effects::Kind, picture: Option<String>) -> Option<Entry> {
        let has_its_files = |level: &effects::Variant<'_>| level.blocks.iter().filter_map(file_of).all(|file| self.archive.contains(&format!("{file}_c")));
        let levels = effects::variants(event, kind);
        let shown = levels.iter().enumerate().filter(|(_, level)| has_its_files(level));
        let shown: Vec<(usize, &effects::Variant<'_>)> = shown.collect();
        let styles: Vec<StyleEntry> = shown.iter().map(|(place, _)| StyleEntry { index: *place as u32, name: format!("Level {}", place + 1), picture: None }).collect();
        let first = styles.first()?.index;
        let lacking = shown.iter().any(|(_, level)| !self.is_whole(&level.blocks));
        Some(Entry {
            id: event.id.to_string(),
            styles: if styles.len() == 1 && first == 0 { Vec::new() } else { styles },
            gaps: if lacking { vec![Gap::Files] } else { Vec::new() },
            ..self.nothing(&self.item_name(event), picture)
        })
    }

    fn slot_name(&self, hero: Option<&str>, slot: &str) -> String {
        let listed = hero.and_then(|hero| self.hero_files.slot(hero, slot));
        let own = listed.and_then(|listed| listed.name_token()).and_then(|token| self.names.get(token)).map(|name| plain(name).to_string());
        match persona_base(slot) {
            Some(base) => {
                let of_the_hero = self.slot_name(hero, base);
                match own {
                    Some(name) if name != of_the_hero => name,
                    _ => format!("Persona {}", self.general_name(slot).unwrap_or(of_the_hero)),
                }
            }
            None => own.or_else(|| self.general_name(slot)).unwrap_or_else(|| title(slot)),
        }
    }

    fn general_name(&self, slot: &str) -> Option<String> {
        self.names.get(&format!("LoadoutSlot_{slot}")).map(|name| plain(name).to_string())
    }

    fn tell_apart(&self, slots: &mut [Slot]) {
        for index in 0..slots.len() {
            let name = &slots[index].name;
            let shared = slots.iter().filter(|other| other.name == *name).count() > 1;
            let Some(general) = self.general_name(&slots[index].id).filter(|_| shared && !slots[index].is_for_effects()) else { continue };
            if !slots.iter().any(|other| other.name == general) {
                slots[index].name = general;
            }
        }
    }

    fn effects_slot(&self, hero: &str, slot: &str, ability: &str) -> Option<Slot> {
        let effects = items::effects_slot(slot)?;
        let mut items: Vec<Entry> =
            self.game.items_for(Some(hero), slot).into_iter().filter_map(|item| self.effects_entry(hero, item, effects.number)).collect();
        if items.is_empty() {
            return None;
        }
        items.sort_by_cached_key(|entry| (entry.name.to_lowercase(), entry.id.parse::<u64>().unwrap_or(u64::MAX)));
        let ability_name = match self.names.get(&format!("DOTA_Tooltip_ability_{ability}")) {
            Some(name) => plain(name).to_string(),
            None => title(ability.strip_prefix(items::hero_short_name(hero)).unwrap_or(ability)),
        };
        let whose = if effects.whose.is_empty() { "" } else { "Persona " };
        let default = Entry {
            id: String::new(),
            name: ability_name.clone(),
            picture: self.picture(&format!("spellicons/{ability}")),
            styles: Vec::new(),
            rarity: None,
            worn_in: None,
            gaps: Vec::new(),
        };
        Some(Slot { id: slot.to_ascii_lowercase(), name: format!("{whose}{ability_name} effects"), default, items, effects: Vec::new(), shared_with: Vec::new() })
    }

    fn effects_entry(&self, hero: &str, item: &Item<'_>, number: u32) -> Option<Entry> {
        if item.effects_in_style(number, item.first_effects_style(number)).is_empty() {
            return None;
        }
        let picture = self.item_picture(item);
        let styles: Vec<StyleEntry> = item.effects_styles(number).iter().map(|style| self.style_look(item, style, picture.as_deref())).collect();
        let styles = if styles.len() == 1 && styles[0].index == 0 { Vec::new() } else { styles };
        let indexes: Vec<u32> = if styles.is_empty() { vec![item.first_effects_style(number)] } else { styles.iter().map(|style| style.index).collect() };
        let blocks: Vec<kv::Pair<'_>> = indexes.into_iter().flat_map(|index| item.effects_in_style(number, index)).collect();
        Some(Entry {
            id: item.id.to_string(),
            name: self.item_name(item),
            picture,
            styles,
            rarity: self.rarity_of(item),
            worn_in: self.game.slot(item).map(|slot| self.slot_name(Some(hero), &slot)),
            gaps: self.gaps(&blocks),
        })
    }

    fn set(&self, hero: &str, set: &ItemSet<'_>) -> Option<Set> {
        let found = apply::set_picks(self.game, hero, set.key, 0).ok()?;
        let can_show = |pick: &Pick| {
            self.hero_files.has_slot(hero, &pick.slot) && self.game.item(&pick.item).is_some_and(|item| self.has_model_of(item))
        };
        let pieces: Vec<Pick> = found.picks.into_iter().filter(can_show).collect();
        let first = self.game.item(&pieces.first()?.item)?;

        let name = match set.name_token.and_then(|token| self.names.get(token)) {
            Some(name) => plain(name).to_string(),
            None => title(set.key),
        };
        let own_picture = set.portrait.and_then(|image| self.picture(&format!("{image}_small")).or_else(|| self.picture(image)));
        let picture = own_picture.or_else(|| self.item_picture(first));
        let bundle = set.bundle.and_then(|name| self.game.item_named(name));
        let rarest_piece = || pieces.iter().filter_map(|piece| self.rarity_of(self.game.item(&piece.item)?)).max();
        let rarity = bundle.and_then(|bundle| self.rarity_of(bundle)).or_else(rarest_piece);
        Some(Set { key: set.key.to_string(), name, picture, pieces, rarity })
    }

    fn rarity_of(&self, item: &Item<'_>) -> Option<usize> {
        self.rarity_places.get(&self.game.rarity(item)?.to_ascii_lowercase()).copied()
    }

    fn entry(&self, item: &Item<'_>) -> Option<Entry> {
        if !self.has_model_of(item) {
            return None;
        }
        let picture = self.item_picture(item);
        let styles: Vec<StyleEntry> =
            item.styles_to_choose().iter().filter_map(|style| self.style(item, style, picture.as_deref())).collect();
        let blocks: Vec<kv::Pair<'_>> = item.block.block("visuals").into_iter().flat_map(|visuals| visuals.pairs()).collect();
        Some(Entry {
            id: item.id.to_string(),
            name: self.item_name(item),
            picture,
            styles: if styles.len() > 1 { styles } else { Vec::new() },
            rarity: self.rarity_of(item),
            worn_in: None,
            gaps: self.gaps(&blocks),
        })
    }

    fn gaps(&self, blocks: &[kv::Pair<'_>]) -> Vec<Gap> {
        let mut gaps = Vec::new();
        if !self.is_whole(blocks) {
            gaps.push(Gap::Files);
        }
        let for_a_level = |pair: &kv::Pair<'_>| {
            let level = pair.value.as_block().and_then(|block| block.str(ARCANA_LEVEL)).and_then(text);
            level.and_then(|level| level.trim().parse::<u32>().ok()).is_some_and(|level| level > 0)
        };
        if blocks.iter().any(for_a_level) {
            gaps.push(Gap::ArcanaLevel);
        }
        gaps
    }

    fn is_whole(&self, blocks: &[kv::Pair<'_>]) -> bool {
        blocks.iter().filter_map(file_of).all(|file| self.wholeness.is_whole(file))
    }

    fn style(&self, item: &Item<'_>, style: &Style<'_>, item_picture: Option<&str>) -> Option<StyleEntry> {
        let own_model = style.block.str("model_player").and_then(text);
        if style.index > 0 && own_model.is_some_and(|model| !self.has_model(model)) {
            return None;
        }
        Some(self.style_look(item, style, item_picture))
    }

    fn style_look(&self, item: &Item<'_>, style: &Style<'_>, item_picture: Option<&str>) -> StyleEntry {
        let icon = style.block.str("alternate_icon").and_then(text).and_then(|index| {
            let icons = item.block.block("visuals")?.block("alternate_icons")?;
            icons.block(index)?.str("icon_path").and_then(text)
        });
        let name = match style.name_token.and_then(|token| self.names.get(token)) {
            Some(name) => plain(name).to_string(),
            None => format!("Style {}", style.index + 1),
        };
        let picture = icon.and_then(|image| self.picture(image)).or_else(|| item_picture.map(str::to_string));
        StyleEntry { index: style.index, name, picture }
    }

    fn item_name(&self, item: &Item<'_>) -> String {
        let shown = item.block.str("item_name").and_then(text).and_then(|token| self.names.get(token));
        match shown {
            Some(name) => plain(name).to_string(),
            None => kv::unescape(item.name.unwrap_or(item.id).as_bytes()),
        }
    }

    fn item_picture(&self, item: &Item<'_>) -> Option<String> {
        self.picture(item.block.str("image_inventory").and_then(text)?)
    }

    fn picture(&self, image: &str) -> Option<String> {
        let path = pictures::archive_path(image);
        self.archive.contains(&path).then_some(path)
    }

    fn has_model_of(&self, item: &Item<'_>) -> bool {
        item.model.is_none_or(|model| self.has_model(model))
    }

    fn has_model(&self, model: &str) -> bool {
        self.archive.contains(&format!("{model}_c"))
    }
}

fn text(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

fn file_of<'a>(pair: &kv::Pair<'a>) -> Option<&'a str> {
    pair.value.as_block().and_then(|block| block.str("modifier")).and_then(text)
}

fn team_number(file: &str) -> Option<u64> {
    const MARK: &str = "team";
    let lower = file.to_ascii_lowercase();
    let mut after = lower.match_indices(MARK).map(|(at, _)| &lower[at + MARK.len()..]);
    after.find_map(|rest| rest[..rest.bytes().take_while(u8::is_ascii_digit).count()].parse().ok())
}

fn brand_names(file: &str) -> Vec<String> {
    let name = file.rsplit('/').next().unwrap_or(file);
    let name = name.rsplit_once('.').map_or(name, |(name, _)| name).to_ascii_lowercase();
    let words: Vec<&str> = name.split('_').collect();
    let Some(numbered) = words.iter().rposition(|word| word.bytes().any(|letter| letter.is_ascii_digit())) else { return Vec::new() };
    let team = &words[numbered + 1..];
    let spelled = (1..=team.len()).rev().flat_map(|count| [team[..count].join("_"), team[..count].concat()]);
    spelled.filter(|name| name.len() > 2).collect()
}

fn persona_base(slot: &str) -> Option<&str> {
    let (base, number) = slot.rsplit_once("_persona_")?;
    let numbered = !number.is_empty() && number.bytes().all(|digit| digit.is_ascii_digit());
    (numbered && !base.is_empty()).then_some(base)
}

fn list_place(slot: &str) -> (Group, bool) {
    (Group::of(slot), !slot.eq_ignore_ascii_case(PERSONA_SELECTOR))
}

fn plain(name: &str) -> &str {
    name.strip_prefix("#|").and_then(|rest| rest.split_once("|#")).map_or(name, |(_, name)| name).trim()
}

fn title(id: &str) -> String {
    let words = id.split(['_', ' ']).filter(|word| !word.is_empty()).map(|word| {
        let mut letters = word.chars();
        let first = letters.next().map(|letter| letter.to_uppercase().collect::<String>()).unwrap_or_default();
        first + letters.as_str()
    });
    words.collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::effects::tests::PACKS;
    use crate::heroes::tests::JUGGERNAUT_FILE;
    use crate::items::tests::{SAMPLE, effects_sample};
    use crate::vpk::{self, PackFile};

    const AXE: &str = "npc_dota_hero_axe";
    const MODELS: [&str; 5] = [
        "models/items/axe/vanguard_weapon/vanguard_weapon.vmdl_c",
        "models/items/axe/vanguard_head/vanguard_head.vmdl_c",
        "models/items/axe/vanguard_misc/vanguard_misc.vmdl_c",
        "models/items/axe/crested/crested.vmdl_c",
        "models/items/axe/crested/crested_gold.vmdl_c",
    ];
    const PICTURES: [&str; 6] = [
        "panorama/images/heroes/npc_dota_hero_axe_png.vtex_c",
        "panorama/images/econ/heroes/axe/axe_weapon_png.vtex_c",
        "panorama/images/econ/items/axe/vanguard_weapon_png.vtex_c",
        "panorama/images/econ/items/axe/crested_gold_png.vtex_c",
        "panorama/images/econ/sets/v2/armor_of_the_vanguard_png.vtex_c",
        "panorama/images/econ/sets/v2/armor_of_the_vanguard_small_png.vtex_c",
    ];

    fn archive(files: &[&str]) -> Archive {
        let files: Vec<PackFile> = files.iter().map(|&path| PackFile { path, data: b"x" }).collect();
        Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&files).unwrap()).unwrap()
    }

    fn names() -> Names {
        let tokens = [
            ("npc_dota_hero_axe:n", "Axe"),
            ("LoadoutSlot_weapon", "Weapon"),
            ("LoadoutSlot_courier", "#|m|#Courier"),
            ("DOTA_Item_Axe_of_the_Vanguard", "Vanguard Axe"),
            ("DOTA_Item_Axes_Axe", "Axe's Own Axe"),
            ("DOTA_Set_Armor_of_the_Vanguard", "Armor of the Vanguard"),
            ("Style_Gold", "Gold"),
            ("Rarity_Mythical", "Mythical"),
            ("Rarity_Legendary", "#|f|#Legendary"),
        ];
        let lines: String = tokens.iter().map(|(token, name)| format!("\"{token}\" \"{name}\"\n")).collect();
        Names::parse(format!("\"lang\" {{ \"Tokens\" {{ {lines} }} }}").as_bytes())
    }

    fn catalog(files: &[&str]) -> Catalog {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        Catalog::build(&game, &names(), &archive(files))
    }

    fn everything() -> Vec<&'static str> {
        [MODELS.as_slice(), PICTURES.as_slice()].concat()
    }

    fn ids(entries: &[Entry]) -> Vec<&str> {
        entries.iter().map(|entry| entry.id.as_str()).collect()
    }

    #[test]
    fn lists_heroes_with_their_slots_and_sets() {
        let catalog = catalog(&everything());
        assert_eq!(catalog.heroes.len(), 1);
        let axe = &catalog.heroes[0];
        assert_eq!((axe.id.as_str(), axe.name.as_str()), (AXE, "Axe"));
        assert_eq!(axe.picture.as_deref(), Some(PICTURES[0]));

        let slots: Vec<(&str, &str)> = axe.slots.iter().map(|slot| (slot.id.as_str(), slot.name.as_str())).collect();
        assert_eq!(slots, [("weapon", "Weapon"), ("head", "Head"), ("misc", "Misc")]);

        let weapon = &axe.slots[0];
        assert_eq!(weapon.default.name, "Axe's Own Axe");
        assert_eq!(weapon.default.picture.as_deref(), Some(PICTURES[1]));
        assert_eq!(ids(&weapon.items), ["9040"]);
        assert_eq!(weapon.items[0].name, "Vanguard Axe", "the name the game shows");
        assert_eq!(weapon.items[0].picture.as_deref(), Some(PICTURES[2]));

        let head = &axe.slots[1];
        assert_eq!(ids(&head.items), ["9500", "8910"]);
        assert_eq!(head.items[1].name, "Helm of the \"Vanguard\"");
        assert_eq!(head.items[1].picture, None);

        let sets: Vec<(&str, &str, usize)> =
            axe.sets.iter().map(|set| (set.key.as_str(), set.name.as_str(), set.pieces.len())).collect();
        assert_eq!(sets, [("armor_of_the_vanguard", "Armor of the Vanguard", 3), ("crested_set", "Crested Set", 2)]);
        assert_eq!(axe.sets[0].picture.as_deref(), Some(PICTURES[5]), "the small picture is preferred");
        assert_eq!(axe.sets[1].picture, None, "no picture of its own, and none for its first piece");
        assert_eq!(axe.sets[0].pieces[0], Pick::hero(AXE, "weapon", "9040"));
    }

    #[test]
    fn lists_styles_with_their_names_and_pictures() {
        let catalog = catalog(&everything());
        let crested = catalog.entry(&Pick::hero(AXE, "head", "9500")).unwrap();
        let styles: Vec<(u32, &str, Option<&str>)> =
            crested.styles.iter().map(|style| (style.index, style.name.as_str(), style.picture.as_deref())).collect();
        assert_eq!(styles, [(0, "Style 1", None), (1, "Gold", Some(PICTURES[3]))]);
        assert!(crested.has_style(0) && crested.has_style(1) && !crested.has_style(2));
        assert_eq!(crested.in_style(1), (Some("Gold"), Some(PICTURES[3])));
        assert_eq!(crested.in_style(0), (Some("Style 1"), None));

        let plain = catalog.entry(&Pick::hero(AXE, "weapon", "9040")).unwrap();
        assert!(plain.styles.is_empty());
        assert_eq!(plain.in_style(0), (None, Some(PICTURES[2])));
        assert!(plain.has_style(0) && !plain.has_style(1));
    }

    #[test]
    fn lists_the_world_slots_that_have_something_to_pick() {
        let catalog = catalog(&everything());
        let world: Vec<(&str, &str, &str, Vec<&str>)> = catalog
            .world
            .iter()
            .map(|slot| (slot.id.as_str(), slot.name.as_str(), slot.default.name.as_str(), ids(&slot.items)))
            .collect();
        assert_eq!(
            world,
            [
                ("courier", "Courier", "Default Courier", vec!["10003"]),
                ("cursor_pack", "Cursor Pack", "Default Cursor Pack", vec!["203"]),
                ("weather", "Weather", "Default Weather", vec!["556"]),
            ],
            "terrain has no default item, so it is not listed"
        );
        assert_eq!(catalog.slots(None).len(), 3);
        assert!(catalog.slot(None, "COURIER").is_some());
        assert!(catalog.slot(Some(AXE), "courier").is_none());
        assert!(catalog.slots(Some("npc_dota_hero_nobody")).is_empty());
    }

    #[test]
    fn what_the_game_cannot_show_is_left_out() {
        let without_gold: Vec<&str> = everything().into_iter().filter(|file| !file.contains("crested_gold.vmdl")).collect();
        let catalog_a = catalog(&without_gold);
        assert!(catalog_a.entry(&Pick::hero(AXE, "head", "9500")).unwrap().styles.is_empty());

        let without_misc: Vec<&str> = everything().into_iter().filter(|file| !file.contains("vanguard_misc")).collect();
        let catalog_b = catalog(&without_misc);
        let axe = catalog_b.hero(AXE).unwrap();
        assert!(axe.slots.iter().all(|slot| slot.id != "misc"));
        assert_eq!(axe.sets.iter().find(|set| set.key == "armor_of_the_vanguard").unwrap().pieces.len(), 2);

        let bare = catalog(&[]);
        assert!(bare.heroes.is_empty());
        assert_eq!(bare.world.len(), 3);
    }

    #[test]
    fn picks_are_settled_to_what_there_is() {
        let catalog = catalog(&everything());
        let settled = |pick: Pick| catalog.settled(&pick);
        let same = |pick: Pick| settled(pick.clone()) == Some(pick);
        assert!(same(Pick::hero(AXE, "weapon", "9040")));
        assert!(same(Pick::hero(AXE, "head", "9500").styled(1)));
        assert!(same(Pick::world("courier", "10003")));
        assert_eq!(settled(Pick::hero(AXE, "head", "9500").styled(5)), Some(Pick::hero(AXE, "head", "9500")));
        assert_eq!(settled(Pick::hero(AXE, "weapon", "8910")), None);
        assert_eq!(settled(Pick::world("courier", "1")), None);
        assert_eq!(settled(Pick::world("terrain", "12000")), None);
        let gem = Pick::world("courier", "10003").with_effect(Some(15));
        assert_eq!(settled(gem), Some(Pick::world("courier", "10003")));
    }

    const GEM_FILES: [&str; 16] = [
        "models/items/axe/red_axe.vmdl_c",
        "models/items/axe/glowing_axe.vmdl_c",
        "particles/econ/courier/courier_trail_01/courier_trail_01.vpcf_c",
        "particles/econ/courier/courier_trail_03/courier_trail_03.vpcf_c",
        "particles/econ/events/fall_2021/frostbloom.vpcf_c",
        "particles/events/t19/teleport_start.vpcf_c",
        "particles/events/t19/teleport_end.vpcf_c",
        "particles/events/t19/teleport_start_lvl2.vpcf_c",
        "particles/events/t19/teleport_end_lvl2.vpcf_c",
        "particles/events/t19/blink_start.vpcf_c",
        "particles/teams/team_teleport_start.vpcf_c",
        "particles/teams/team_teleport_end.vpcf_c",
        "particles/championship/teleport_start.vpcf_c",
        "panorama/images/items/tpscroll_png.vtex_c",
        "panorama/images/econ/tools/championship_tp_png.vtex_c",
        "panorama/images/econ/testitem_slot_empty_png.vtex_c",
    ];

    fn gem_catalog(without: &[&str], tokens: &[(&str, &str)]) -> Catalog {
        let game = ItemsGame::parse(PACKS.as_bytes()).unwrap();
        let files: Vec<&str> = GEM_FILES.into_iter().filter(|file| !without.iter().any(|part| file.contains(part))).collect();
        let lines: String = tokens.iter().map(|(token, name)| format!("\"{token}\" \"{name}\"\n")).collect();
        Catalog::build(&game, &Names::parse(format!("\"lang\" {{ \"Tokens\" {{ {lines} }} }}").as_bytes()), &archive(&files))
    }

    const GEM_NAMES: [(&str, &str); 6] = [
        ("Attrib_Particle15", "Ethereal Flame"),
        ("Attrib_Particle21", "Burning Animus"),
        ("Attrib_Particle826", "#|n|#Frostbloom"),
        ("UnusualRed", "Red"),
        ("DOTA_Item_Tournament_2019_Effects", "The Tournament 2019"),
        ("LoadoutSlot_courier", "Courier"),
    ];

    #[test]
    fn lists_the_effects_that_gems_give() {
        let catalog = gem_catalog(&[], &GEM_NAMES);
        let effect = |id: u32, name: &str, for_couriers: bool| Effect { id, name: name.to_string(), for_couriers };
        let (animus, flame, bloom) = (effect(21, "Burning Animus", true), effect(15, "Ethereal Flame", true), effect(826, "Frostbloom", false));

        let courier = catalog.slot(None, "courier").unwrap();
        assert_eq!(courier.effects, [animus.clone(), flame.clone(), bloom.clone()]);
        let weapon = catalog.slot(Some(AXE), "weapon").unwrap();
        assert_eq!(weapon.effects, [bloom.clone(), animus.clone(), flame.clone()]);
        assert_eq!(courier.effect(21), Some(&animus));
        assert_eq!(weapon.effect(21), Some(&animus));

        let settled = |pick: Pick| catalog.settled(&pick).unwrap();
        let drodo = || Pick::world("courier", "30");
        assert_eq!(settled(drodo().with_effect(Some(21))), drodo().with_effect(Some(21)));
        assert_eq!(settled(drodo().with_effect(Some(999))), drodo(), "no such effect");
        let glowing = || Pick::hero(AXE, "weapon", "21");
        assert_eq!(settled(glowing().with_effect(Some(15))), glowing().with_effect(Some(15)), "an effect for couriers, as an experiment");
        assert_eq!(settled(glowing().with_effect(Some(826))), glowing().with_effect(Some(826)));

        let without_file = gem_catalog(&["courier_trail_03"], &GEM_NAMES);
        assert_eq!(without_file.slot(None, "courier").unwrap().effects, [flame.clone(), bloom.clone()]);
        let without_name = gem_catalog(&[], &GEM_NAMES[1..]);
        assert_eq!(without_name.slot(None, "courier").unwrap().effects, [animus, bloom]);
        assert!(catalog.world.iter().filter(|slot| slot.id != "courier").all(|slot| slot.effects.is_empty()));
    }

    #[test]
    fn lists_a_slot_for_each_kind_of_effect_that_an_event_has_one_for() {
        const SCROLL: &str = "panorama/images/items/tpscroll_png.vtex_c";
        let catalog = gem_catalog(&[], &GEM_NAMES);
        let world: Vec<(&str, &str, Vec<&str>)> = catalog.world.iter().map(|slot| (slot.id.as_str(), slot.name.as_str(), ids(&slot.items))).collect();
        assert_eq!(
            world,
            [
                ("courier", "Courier", vec!["30"]),
                ("fx_teleport", "Teleport", vec!["503", "501", "500"]),
                ("fx_blink", "Blink Dagger", vec!["500"]),
                ("fx_deny", "Deny", vec!["500"]),
            ],
            "a kind that no event has an effect for is left out"
        );
        let teleport = catalog.slot(None, "FX_Teleport").unwrap();
        assert!(teleport.is_for_an_event() && !teleport.is_for_effects());
        assert_eq!(teleport.group(), Group::Events);
        assert_eq!(catalog.slot(None, "courier").unwrap().group(), Group::Worn);
        assert!(!catalog.slot(None, "courier").unwrap().is_for_an_event());
        assert_eq!((teleport.default.name.as_str(), teleport.default.picture.as_deref()), ("Teleport", Some(SCROLL)));
        let deny = catalog.slot(None, "fx_deny").unwrap();
        assert_eq!((deny.default.name.as_str(), deny.default.picture.as_deref()), ("Deny", None));

        let tournament = teleport.entry("500").unwrap();
        assert_eq!((tournament.name.as_str(), tournament.picture.as_deref(), tournament.rarity), ("The Tournament 2019", Some(SCROLL), None));
        let levels = |entry: &Entry| -> Vec<(u32, String)> { entry.styles.iter().map(|style| (style.index, style.name.clone())).collect() };
        assert_eq!(levels(tournament), [(0, "Level 1".to_string()), (1, "Level 2".to_string())]);
        let team = teleport.entry("501").unwrap();
        assert_eq!(team.name, "In-Game Effects Team");
        assert!(team.styles.is_empty(), "one effect for everybody");
        let championship = teleport.entry("503").unwrap();
        assert_eq!(championship.picture.as_deref(), Some("panorama/images/econ/tools/championship_tp_png.vtex_c"));
        assert!(catalog.slot(None, "teleport_effect").is_none(), "the game's slot has no default item to take the place of");
        assert!(catalog.slot(None, "fx_blink").unwrap().entry("500").unwrap().styles.is_empty());
        let second = Pick::world("fx_teleport", "500").styled(1);
        assert_eq!(catalog.settled(&second), Some(second.clone()));
        assert_eq!(catalog.settled(&Pick::world("fx_teleport", "500").styled(2)), Some(Pick::world("fx_teleport", "500")));
        assert_eq!(catalog.settled(&Pick::world("fx_blink", "501")), None, "the event has nothing for it");
        assert_eq!(catalog.settled(&Pick::hero(AXE, "fx_teleport", "500")), None);

        let first_only = gem_catalog(&["teleport_end_lvl2"], &GEM_NAMES);
        let tournament = first_only.slot(None, "fx_teleport").unwrap().entry("500").unwrap();
        assert!(tournament.styles.is_empty());
        assert_eq!(first_only.settled(&second), Some(Pick::world("fx_teleport", "500")));
        let second_only = gem_catalog(&["t19/teleport_start.vpcf"], &GEM_NAMES);
        let tournament = second_only.slot(None, "fx_teleport").unwrap().entry("500").unwrap();
        assert_eq!(levels(tournament), [(1, "Level 2".to_string())]);
        assert_eq!(tournament.first_style(), 1);
        assert_eq!(second_only.settled(&Pick::world("fx_teleport", "500")), Some(second));
        let neither = gem_catalog(&["t19/teleport_start", "team_teleport_end", "championship/"], &GEM_NAMES);
        assert!(neither.slot(None, "fx_teleport").is_none());
        assert!(neither.slot(None, "fx_blink").is_some());
    }

    #[test]
    fn an_event_shows_a_picture_of_its_own_where_the_game_has_one_for_it() {
        const SCROLL: &str = "panorama/images/items/tpscroll_png.vtex_c";
        let pack = |id: u32, name: &str, rest: &str, effect: &str| {
            format!(
                r#""{id}" {{ "name" "{name}" "prefab" "tool" "item_class" "dota_item_wearable" {rest}
                    "visuals" {{ "asset_modifier" {{ "type" "particle" "asset" "particles/items2_fx/teleport_start.vpcf" "modifier" "particles/{effect}.vpcf" }} }} }}"#
            )
        };
        let other = |id: u32, name: &str, prefab: &str, event: &str, image: &str, rest: &str| {
            format!(r#""{id}" {{ "name" "{name}" "prefab" "{prefab}" "event_id" "{event}" "image_inventory" "econ/tools/{image}" {rest} }}"#)
        };
        let items = [
            pack(100, "The Big Event Effects", r#""event_id" "EVENT_ID_BIG" "image_inventory" "econ/testitem_slot_empty""#, "big/teleport"),
            other(101, "The Big Event Battle Pass - Level 50", "tool", "EVENT_ID_BIG", "big_pass_50", r#""tool" { "type" "compendium" }"#),
            other(102, "The Big Event Battle Pass", "tool", "EVENT_ID_BIG", "big_pass", r#""tool" { "type" "Compendium" }"#),
            other(103, "Big Points", "tool", "Event_Id_Big", "big_points", r#""tool" { "type" "event_points" }"#),
            other(104, "Big Treasure", "treasure_chest", "EVENT_ID_BIG", "big_treasure", ""),
            pack(110, "In-Game Effects Team", "", "ti9_teleport_teams/teleport_start_ti9_team39"),
            pack(111, "In-Game Effects Other Team", "", "ti9_teleport_teams/teleport_start_ti9_team7777"),
            pack(112, "In-Game Effects Escape Gaming", "", "ti6_teams/teleport_start_ti6_lvl3_escape_gaming"),
            other(119, "Small Treasure I", "treasure_chest", "event_id_small", "small_treasure_1", ""),
            pack(120, "The Small Event Effects", r#""event_id" "EVENT_ID_SMALL""#, "small/teleport"),
            other(121, "Small Bundle", "bundle", "EVENT_ID_SMALL", "small_bundle", ""),
            other(122, "Small Emblem", "emblem", "EVENT_ID_SMALL", "small_emblem", ""),
            other(123, "Small Treasure II", "treasure_chest", "EVENT_ID_SMALL", "small_treasure_2", ""),
            pack(130, "The Late Effects", r#""event_id" "EVENT_ID_BIG" "image_inventory" "econ/tools/late_effects""#, "late/teleport"),
            pack(140, "The Lone Effects", r#""event_id" "EVENT_ID_LONE""#, "lone/teleport_lvl2_radiant"),
        ];
        let items = format!(
            r#""items_game" {{ "prefabs" {{ "tool" {{ "item_class" "tool" "item_slot" "none" }} "bundle" {{ "item_slot" "none" }} "treasure_chest" {{ "item_slot" "none" }} "emblem" {{ "item_slot" "none" }} }}
                "items" {{ {} }} }}"#,
            items.join("\n")
        );
        let game = ItemsGame::parse(items.as_bytes()).unwrap();
        let effects = [
            "big/teleport",
            "ti9_teleport_teams/teleport_start_ti9_team39",
            "ti9_teleport_teams/teleport_start_ti9_team7777",
            "ti6_teams/teleport_start_ti6_lvl3_escape_gaming",
            "small/teleport",
            "late/teleport",
            "lone/teleport_lvl2_radiant",
        ];
        let pictures = ["items/tpscroll", "econ/testitem_slot_empty", "econ/tools/big_pass_50", "econ/tools/big_pass", "econ/tools/big_points", "econ/tools/big_treasure"];
        let more = ["teams/39", "econ/tools/small_treasure_1", "econ/tools/small_treasure_2", "econ/tools/small_bundle", "econ/tools/late_effects", "econ/brands/brand_escape", "econ/brands/brand_radiant"];
        let files = |without: &str| -> Vec<String> {
            let effects = effects.iter().map(|effect| format!("particles/{effect}.vpcf_c"));
            let pictures = pictures.iter().chain(&more).map(|picture| format!("panorama/images/{picture}_png.vtex_c"));
            effects.chain(pictures).filter(|file| !file.contains(without)).collect()
        };
        let shown = |without: &str| -> Vec<(String, Option<String>)> {
            let catalog = Catalog::build(&game, &Names::default(), &crate::trees::tests::archive_with(&files(without)));
            let teleport = catalog.slot(None, "fx_teleport").unwrap();
            assert_eq!(teleport.default.picture.as_deref(), Some(SCROLL), "the slot itself shows the Dota item");
            let mut told: Vec<(String, Option<String>)> = teleport.items.iter().map(|entry| (entry.id.clone(), entry.picture.clone())).collect();
            told.sort();
            told
        };
        let picture = |id: &str, image: &str| (id.to_string(), Some(format!("panorama/images/{image}_png.vtex_c")));
        assert_eq!(
            shown("nothing is left out"),
            [
                picture("100", "econ/tools/big_pass"),
                picture("110", "teams/39"),
                picture("111", "items/tpscroll"),
                picture("112", "econ/brands/brand_escape"),
                picture("120", "econ/tools/small_treasure_1"),
                picture("130", "econ/tools/late_effects"),
                picture("140", "items/tpscroll"),
            ]
        );
        assert_eq!(shown("big_pass_png")[0], picture("100", "econ/tools/big_pass_50"));
        assert_eq!(shown("big_pass")[0], picture("100", "econ/tools/big_treasure"));
        assert_eq!(shown("small_treasure_1")[4], picture("120", "econ/tools/small_treasure_2"));
        assert_eq!(shown("small_treasure")[4], picture("120", "econ/tools/small_bundle"));
        assert_eq!(shown("late_effects")[5], picture("130", "econ/tools/big_pass"), "then it shows what its event had");
        assert_eq!(shown("brand_escape")[3], picture("112", "items/tpscroll"));

        assert_eq!(brand_names("particles/events/ti6_teams/teleport_start_ti6_lvl3_escape_gaming.vpcf"), ["escape_gaming", "escapegaming", "escape", "escape"]);
        assert_eq!(brand_names("teleport_start_ti6_lvl3_MVP_Phoenix.vpcf")[..2], ["mvp_phoenix", "mvpphoenix"]);
        assert!(brand_names("teleport_start_ti6_lvl3_ig.vpcf").is_empty(), "a letter or two is no name");
        for other in ["particles/teleport_start_ti9_team39.vpcf", "particles/teams/team_teleport_start.vpcf", "teleport_lvl2.vpcf", ""] {
            assert!(brand_names(other).is_empty(), "{other}");
        }

        assert_eq!(team_number("particles/econ/events/ti9/ti9_teleport_teams/teleport_start_ti9_team39.vpcf"), Some(39));
        assert_eq!(team_number("particles/TEAM2163/x.vpcf"), Some(2163));
        for other in ["particles/teams/team_teleport_start.vpcf", "particles/steam/steam.vpcf", "team", ""] {
            assert_eq!(team_number(other), None, "{other}");
        }
    }

    #[test]
    fn lists_the_emblems_the_game_has_the_effect_of() {
        let text = crate::effects::tests::emblems();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let files = [
            "particles/events/t17/hero_effect.vpcf_c",
            "particles/events/t18/hero_effect.vpcf_c",
            "particles/events/t19/teleport_start.vpcf_c",
            "particles/courier/glow.vpcf_c",
            "panorama/images/econ/tools/sunken_emblem_png.vtex_c",
        ];
        let build = |without: &[&str]| {
            let files: Vec<&str> = files.into_iter().filter(|file| !without.iter().any(|part| file.contains(part))).collect();
            Catalog::build(&game, &names(), &archive(&files))
        };
        let catalog = build(&[]);
        let world: Vec<(&str, &str, Vec<&str>)> = catalog.world.iter().map(|slot| (slot.id.as_str(), slot.name.as_str(), ids(&slot.items))).collect();
        assert_eq!(
            world,
            [
                ("courier", "Courier", vec!["30"]),
                ("emblem", "Emblem", vec!["663", "638"]),
                ("fx_teleport", "Teleport", vec!["500"]),
            ]
        );
        let slot = catalog.slot(None, "Emblem").unwrap();
        assert!(slot.is_for_an_emblem() && !slot.is_for_an_event() && !slot.is_for_effects() && !slot.is_for_trees());
        assert_eq!(slot.group(), Group::Events);
        assert!(slot.effects.is_empty() && slot.shared_with.is_empty());
        assert_eq!((slot.default.id.as_str(), slot.default.name.as_str(), slot.default.picture.as_deref()), ("", "No emblem", None));
        let sunken = slot.entry("638").unwrap();
        assert_eq!(sunken.name, "Sunken Emblem");
        assert_eq!(sunken.picture.as_deref(), Some("panorama/images/econ/tools/sunken_emblem_png.vtex_c"));
        assert!(sunken.styles.is_empty() && sunken.gaps.is_empty());
        assert_eq!(slot.entry("663").unwrap().picture, None);

        let pick = Pick::world("emblem", "638");
        assert_eq!(catalog.settled(&pick), Some(pick.clone()));
        assert_eq!(catalog.settled(&pick.clone().styled(2).with_effect(Some(15))), Some(pick), "an emblem has no styles and takes no effect");
        assert_eq!(catalog.settled(&Pick::world("emblem", "664")), None);
        assert_eq!(catalog.settled(&Pick::hero(AXE, "emblem", "638")), None);

        assert_eq!(ids(&build(&["t18"]).slot(None, "emblem").unwrap().items), ["638"]);
        assert!(build(&["hero_effect"]).slot(None, "emblem").is_none());
        assert!(self::catalog(&everything()).slot(None, "emblem").is_none(), "a game without emblems");
    }

    #[test]
    fn lists_the_looks_the_game_has_for_the_whole_map() {
        use crate::map_effect::tests::{GRAPHIC, MAP_EFFECTS, OWN_FILE};
        let game = ItemsGame::parse(MAP_EFFECTS.as_bytes()).unwrap();
        let picture = "panorama/images/econ/tools/graphic_shader_png.vtex_c";
        let catalog = Catalog::build(&game, &names(), &archive(&[GRAPHIC, OWN_FILE, picture]));
        let world: Vec<(&str, &str, Vec<&str>)> = catalog.world.iter().map(|slot| (slot.id.as_str(), slot.name.as_str(), ids(&slot.items))).collect();
        assert_eq!(world, [("courier", "Courier", vec!["600"]), ("map_effect", "Map Effect", vec!["18380"])]);
        let slot = catalog.slot(None, "map_effect").unwrap();
        assert!(slot.is_for_the_map() && !slot.is_for_trees() && !slot.is_for_an_event() && !slot.is_for_an_emblem());
        assert_eq!(slot.group(), Group::Experiments);
        assert_eq!((slot.default.id.as_str(), slot.default.name.as_str()), ("", "The map as it is"));
        let graphic = slot.entry("18380").unwrap();
        assert_eq!((graphic.name.as_str(), graphic.picture.as_deref()), ("Graphic Shader Effect", Some(picture)));
        let pick = Pick::world("map_effect", "18380");
        assert_eq!(catalog.settled(&pick), Some(pick));
        assert_eq!(catalog.settled(&Pick::world("map_effect", "18381")), None);

        for only in [GRAPHIC, OWN_FILE] {
            assert!(Catalog::build(&game, &names(), &archive(&[only])).slot(None, "map_effect").is_none(), "{only} alone");
        }
        assert!(self::catalog(&everything()).slot(None, "map_effect").is_none(), "a game without map effects");
    }

    #[test]
    fn lists_the_trees_the_game_has_to_show_on_the_normal_map() {
        use crate::trees::tests::{archive_with, trees_of};
        let (autumn, spring) = (trees::kind("autumn").unwrap(), trees::kind("spring").unwrap());
        let mut files = [trees_of(autumn), trees_of(spring)].concat();
        files.sort();
        files.dedup();
        files.push("panorama/images/econ/terrain/autumn_terrain_png.vtex_c".to_string());
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let catalog = Catalog::build(&game, &names(), &archive_with(&files));

        let slot = catalog.world.last().unwrap();
        assert_eq!((slot.id.as_str(), slot.name.as_str()), ("trees", "Trees"));
        assert!(slot.is_for_trees() && !slot.is_for_an_event() && !slot.is_for_effects());
        assert_eq!(slot.group(), Group::Experiments);
        assert!(slot.effects.is_empty() && slot.shared_with.is_empty());
        let listed: Vec<(&str, &str, Option<&str>)> = slot.items.iter().map(|entry| (entry.id.as_str(), entry.name.as_str(), entry.picture.as_deref())).collect();
        assert_eq!(
            listed,
            [("autumn", "Autumn trees", Some("panorama/images/econ/terrain/autumn_terrain_png.vtex_c")), ("spring", "Spring trees", None)],
            "the game lacks the winter trees"
        );
        assert_eq!(slot.default.name, "The map's own trees");

        let autumn = Pick::world("trees", "autumn");
        assert_eq!(catalog.settled(&autumn), Some(autumn));
        assert_eq!(catalog.settled(&Pick::world("trees", "winter")), None);
        assert_eq!(catalog.settled(&Pick::hero(AXE, "trees", "autumn")), None);
        assert!(self::catalog(&everything()).slot(None, "trees").is_none());
    }

    #[test]
    fn items_and_sets_say_how_rare_they_are() {
        let catalog = catalog(&everything());
        let rarities: Vec<(&str, &str, [u8; 3])> =
            catalog.rarities.iter().map(|rarity| (rarity.id.as_str(), rarity.name.as_str(), rarity.color)).collect();
        assert_eq!(
            rarities,
            [
                ("common", "Common", [0xb0, 0xc3, 0xd9]),
                ("mythical", "Mythical", [0x88, 0x47, 0xff]),
                ("legendary", "Legendary", NO_COLOR),
                ("immortal", "Immortal", [0xe4, 0xae, 0x39]),
            ],
            "the commonest first, by the game's names where it has them"
        );

        let name_of = |place: Option<usize>| place.map(|place| catalog.rarities[place].id.as_str());
        let rarity = |pick: Pick| name_of(catalog.entry(&pick).unwrap().rarity);
        assert_eq!(rarity(Pick::hero(AXE, "weapon", "9040")), Some("mythical"));
        assert_eq!(rarity(Pick::hero(AXE, "head", "8910")), Some("common"), "from the kind of item it is");
        assert_eq!(rarity(Pick::world("courier", "10003")), Some("immortal"));
        assert_eq!(rarity(Pick::world("weather", "556")), None, "the game does not say");

        let axe = catalog.hero(AXE).unwrap();
        let set = |key: &str| name_of(axe.sets.iter().find(|set| set.key == key).unwrap().rarity);
        assert_eq!(set("armor_of_the_vanguard"), Some("legendary"), "as rare as the bundle it is sold as");
        assert_eq!(set("crested_set"), Some("mythical"), "no bundle: as rare as its rarest piece");
        assert_eq!(axe.slots[0].default.rarity, None, "a default is not a thing to own");
    }

    #[test]
    fn lists_a_slot_for_the_effects_of_each_ability_that_has_items() {
        const JUGG: &str = "npc_dota_hero_juggernaut";
        let text = effects_sample();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let present = [
            "models/items/juggernaut/serra.vmdl_c",
            "models/items/juggernaut/two_winds.vmdl_c",
            "models/items/juggernaut/plain.vmdl_c",
            "models/items/juggernaut/ward_blade.vmdl_c",
            "panorama/images/spellicons/juggernaut_blade_fury_png.vtex_c",
            "panorama/images/econ/items/juggernaut/edge_png.vtex_c",
            "panorama/images/econ/items/juggernaut/serra_gold_png.vtex_c",
        ];
        let mut files: Vec<PackFile> = present.iter().map(|&path| PackFile { path, data: b"x" }).collect();
        files.push(PackFile { path: "scripts/npc/heroes/npc_dota_hero_juggernaut.txt", data: JUGGERNAUT_FILE.as_bytes() });
        let archive = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&files).unwrap()).unwrap();
        let tokens = [
            ("DOTA_Tooltip_ability_juggernaut_blade_fury", "Blade Fury"),
            ("DOTA_Tooltip_ability_juggernaut_omni_slash", "#|m|#Omnislash"),
            ("LoadoutSlot_weapon", "Weapon"),
            ("Style_Gold", "Gold"),
        ];
        let lines: String = tokens.iter().map(|(token, name)| format!("\"{token}\" \"{name}\"\n")).collect();
        let names = Names::parse(format!("\"lang\" {{ \"Tokens\" {{ {lines} }} }}").as_bytes());
        let catalog = Catalog::build(&game, &names, &archive);

        let jugg = catalog.hero(JUGG).unwrap();
        let slots: Vec<(&str, &str, Vec<&str>)> = jugg.slots.iter().map(|slot| (slot.id.as_str(), slot.name.as_str(), ids(&slot.items))).collect();
        assert_eq!(
            slots,
            [
                ("weapon", "Weapon", vec!["203", "201", "207"]),
                ("head", "Head", vec!["204", "208", "202"]),
                ("ability_effects_1", "Blade Fury effects", vec!["200", "202"]),
                ("ability_effects_4", "Omnislash effects", vec!["208", "201"]),
                ("ability_effects_2", "Healing Ward effects", vec!["202", "207"]),
                ("weapon_persona_1", "Persona Weapon", vec!["205"]),
                ("ability_effects_1_persona_1", "Persona Blade Fury effects", vec!["205"]),
            ]
        );
        let slot = |id: &str| catalog.slot(Some(JUGG), id).unwrap();
        assert!(slot("ability_effects_1").is_for_effects() && !slot("weapon").is_for_effects());

        let fury = slot("ability_effects_1");
        assert_eq!(fury.default.name, "Blade Fury");
        assert_eq!(fury.default.picture.as_deref(), Some("panorama/images/spellicons/juggernaut_blade_fury_png.vtex_c"));
        assert_eq!(slot("ability_effects_2").default.name, "Healing Ward", "the game has no name for it here");
        assert_eq!(slot("ability_effects_2").default.picture, None);

        let edge = fury.entry("200").unwrap();
        assert_eq!(edge.name, "Edge of the Lost");
        assert_eq!(edge.picture.as_deref(), Some("panorama/images/econ/items/juggernaut/edge_png.vtex_c"));
        assert_eq!(edge.worn_in.as_deref(), Some("Weapon"));
        assert!(edge.styles.is_empty());
        assert_eq!(fury.entry("202").unwrap().worn_in.as_deref(), Some("Head"));
        assert_eq!(slot("ability_effects_1_persona_1").entry("205").unwrap().worn_in.as_deref(), Some("Persona Weapon"));
        assert_eq!(slot("weapon").entry("201").unwrap().worn_in, None, "said of effects only");

        let serra = slot("ability_effects_4").entry("201").unwrap();
        let styles: Vec<(u32, &str, Option<&str>)> = serra.styles.iter().map(|style| (style.index, style.name.as_str(), style.picture.as_deref())).collect();
        assert_eq!(styles, [(0, "Style 1", None), (1, "Gold", Some("panorama/images/econ/items/juggernaut/serra_gold_png.vtex_c"))]);
        assert_eq!(serra.first_style(), 0);
        assert_eq!(slot("weapon").entry("201").unwrap().styles.len(), 3, "worn, it has all three");

        let bloomer = slot("ability_effects_4").entry("208").unwrap();
        let styles: Vec<(u32, &str)> = bloomer.styles.iter().map(|style| (style.index, style.name.as_str())).collect();
        assert_eq!(styles, [(1, "Style 2")]);
        assert_eq!(bloomer.first_style(), 1);
        assert!(bloomer.has_style(1) && !bloomer.has_style(0));

        let picks = [
            Pick::hero(JUGG, "ability_effects_4", "201").styled(1),
            Pick::hero(JUGG, "ability_effects_4", "201").styled(2),
            Pick::hero(JUGG, "ability_effects_4", "200"),
            Pick::hero(JUGG, "ability_effects_3", "204"),
            Pick::hero(JUGG, "ability_effects_4", "208"),
            Pick::hero(JUGG, "ability_effects_4", "208").styled(1),
        ];
        let settled: Vec<Option<Pick>> = picks.iter().map(|pick| catalog.settled(pick)).collect();
        assert_eq!(
            settled,
            [
                Some(picks[0].clone()),
                Some(Pick::hero(JUGG, "ability_effects_4", "201")),
                None,
                None,
                Some(picks[5].clone()),
                Some(picks[5].clone()),
            ],
            "a style the effects do not differ by gives way to the first that they have"
        );

        let bare = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&[PackFile { path: "x", data: b"x" }]).unwrap()).unwrap();
        let without = Catalog::build(&game, &names, &bare);
        assert!(without.hero(JUGG).unwrap().slots.iter().all(|slot| !slot.is_for_effects()));
    }

    const TINY: &str = "npc_dota_hero_tiny";
    const TINY_ITEMS: &str = r##"
"items_game"
{
    "prefabs"
    {
        "default_item" { "item_class" "dota_item_wearable" "item_slot" "weapon" "baseitem" "1" }
        "wearable" { "item_class" "dota_item_wearable" "item_slot" "weapon" }
    }
    "items"
    {
        "1" { "name" "Tiny's Head" "prefab" "default_item" "item_slot" "head" "used_by_heroes" { "npc_dota_hero_tiny" "1" } }
        "2" { "name" "Tiny's Tiny Form" "prefab" "default_item" "item_slot" "shoulder" "used_by_heroes" { "npc_dota_hero_tiny" "1" } }
        "3" { "name" "Tiny's Tree" "prefab" "default_item" "used_by_heroes" { "npc_dota_hero_tiny" "1" } }
        "11" { "name" "Old Head" "prefab" "wearable" "item_slot" "head" "used_by_heroes" { "npc_dota_hero_tiny" "1" } }
        "12" { "name" "Icy Tiny" "prefab" "wearable" "item_slot" "shoulder" "used_by_heroes" { "npc_dota_hero_tiny" "1" } }
        "13" { "name" "Icy Tree" "prefab" "wearable" "used_by_heroes" { "npc_dota_hero_tiny" "1" }
            "visuals" { "styles" { "0" { "name" "#Style_Frost" } "1" { "name" "#Style_Thaw" } } } }
        "14" { "name" "Tree of Moods" "prefab" "wearable" "used_by_heroes" { "npc_dota_hero_tiny" "1" }
            "visuals" { "styles" { "0" { "auto_style_rule" "modifier_tiny_grow == 0" } "1" { "auto_style_rule" "modifier_tiny_grow > 0" "skin" "1" } } } }
    }
    "item_sets"
    {
        "icy" { "name" "#Set_Icy" "items" { "Old Head" "1" "Icy Tiny" "1" "Icy Tree" "1" } }
        "old" { "name" "#Set_Old" "items" { "Old Head" "1" } }
    }
}
"##;
    const TINY_FILE: &str = r##"
"DOTAHeroes"
{
    "npc_dota_hero_tiny"
    {
        "NameAliases" "Stone Giant"
        "ItemSlots"
        {
            "0" { "SlotName" "weapon" "SlotText" "#LoadoutSlot_Weapon" }
            "1" { "SlotName" "shoulder" "SlotText" "#LoadoutSlot_Tiny_Body_0" }
            "2" { "SlotName" "taunt" "SlotText" "#LoadoutSlot_Taunt" }
        }
    }
}
"##;

    fn catalog_of(items: &str, hero_file: Option<&str>, tokens: &[(&str, &str)]) -> Catalog {
        let game = ItemsGame::parse(items.as_bytes()).unwrap();
        let mut files = vec![PackFile { path: "x", data: b"x" }];
        files.extend(hero_file.map(|file| PackFile { path: "scripts/npc/heroes/a_hero.txt", data: file.as_bytes() }));
        let archive = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&files).unwrap()).unwrap();
        let lines: String = tokens.iter().map(|(token, name)| format!("\"{token}\" \"{name}\"\n")).collect();
        Catalog::build(&game, &Names::parse(format!("\"lang\" {{ \"Tokens\" {{ {lines} }} }}").as_bytes()), &archive)
    }

    fn tiny_catalog(with_his_file: bool) -> Catalog {
        let tokens = [("npc_dota_hero_tiny:n", "Tiny"), ("LoadoutSlot_Weapon", "Weapon"), ("LoadoutSlot_Tiny_Body_0", "Tiny"), ("LoadoutSlot_Shoulder", "Shoulder")];
        catalog_of(TINY_ITEMS, with_his_file.then_some(TINY_FILE), &tokens)
    }

    #[test]
    fn slots_that_the_game_gives_one_name_are_told_apart() {
        const ALCHEMIST: &str = "npc_dota_hero_alchemist";
        let default = |id: u32, slot: &str| format!(r#""{id}" {{ "name" "Default {slot}" "prefab" "default_item" "item_slot" "{slot}" "used_by_heroes" {{ "{ALCHEMIST}" "1" }} }}"#);
        let wearable = |id: u32, slot: &str| format!(r#""{id}" {{ "name" "A {slot}" "prefab" "wearable" "item_slot" "{slot}" "used_by_heroes" {{ "{ALCHEMIST}" "1" }} }}"#);
        let slots = ["weapon", "body_head", "head", "weapon_persona_1", "shapeshift_persona_1"];
        let items: String = slots.iter().enumerate().map(|(place, slot)| default(place as u32 + 1, slot) + &wearable(place as u32 + 11, slot)).collect();
        let items = format!(
            r#""items_game" {{ "prefabs" {{ "default_item" {{ "item_slot" "weapon" "baseitem" "1" }} "wearable" {{ "item_slot" "weapon" }} }} "items" {{ {items} }} }}"#
        );
        let texts = ["#LoadoutSlot_Weapon", "#LoadoutSlot_Head", "#LoadoutSlot_Head", "#LoadoutSlot_Weapon", "#LoadoutSlot_Persona_Elder_Dragon"];
        let listed: String = slots.iter().zip(texts).map(|(slot, text)| format!(r#""{slot}" {{ "SlotName" "{slot}" "SlotText" "{text}" }}"#)).collect();
        let file = format!(r#""DOTAHeroes" {{ "{ALCHEMIST}" {{ "ItemSlots" {{ {listed} }} }} }}"#);
        let tokens = [
            ("LoadoutSlot_Weapon", "Weapon"),
            ("LoadoutSlot_Head", "Head"),
            ("LoadoutSlot_body_head", "Body - Head"),
            ("LoadoutSlot_Persona_Elder_Dragon", "Persona Dragon"),
        ];

        let catalog = catalog_of(&items, Some(&file), &tokens);
        let names: Vec<&str> = catalog.hero(ALCHEMIST).unwrap().slots.iter().map(|slot| slot.name.as_str()).collect();
        assert_eq!(names, ["Weapon", "Body - Head", "Head", "Persona Weapon", "Persona Dragon"]);

        let bare = catalog_of(&items, None, &tokens);
        let names: Vec<&str> = bare.hero(ALCHEMIST).unwrap().slots.iter().map(|slot| slot.name.as_str()).collect();
        assert_eq!(names, ["Weapon", "Body - Head", "Head", "Persona Weapon", "Persona Shapeshift"]);
    }

    #[test]
    fn a_hero_has_the_slots_its_own_file_lists_by_the_names_it_gives_them() {
        let catalog = tiny_catalog(true);
        let tiny = catalog.hero(TINY).unwrap();
        let slots: Vec<(&str, &str)> = tiny.slots.iter().map(|slot| (slot.id.as_str(), slot.name.as_str())).collect();
        assert_eq!(slots, [("weapon", "Weapon"), ("shoulder", "Tiny")]);
        assert_eq!(tiny.aliases, ["Stone Giant"]);

        let sets: Vec<(&str, Vec<&str>)> = tiny.sets.iter().map(|set| (set.key.as_str(), set.pieces.iter().map(|piece| piece.slot.as_str()).collect())).collect();
        assert_eq!(sets, [("icy", vec!["shoulder", "weapon"])]);
        assert_eq!(catalog.settled(&Pick::hero(TINY, "head", "11")), None);

        let bare = tiny_catalog(false);
        let tiny = bare.hero(TINY).unwrap();
        let slots: Vec<(&str, &str)> = tiny.slots.iter().map(|slot| (slot.id.as_str(), slot.name.as_str())).collect();
        assert_eq!(slots, [("head", "Head"), ("shoulder", "Shoulder"), ("weapon", "Weapon")]);
        assert_eq!(tiny.sets.len(), 2);
        assert!(tiny.aliases.is_empty());
    }

    #[test]
    fn a_slot_says_which_heroes_share_its_default_item() {
        const SVEN: &str = "npc_dota_hero_sven";
        let items = r#""items_game" { "prefabs" { "default_item" { "item_slot" "weapon" "baseitem" "1" } "wearable" { "item_slot" "weapon" } } "items" {
            "1" { "name" "Everybody's Pet" "prefab" "default_item" "item_slot" "summon" "used_by_heroes" { "npc_dota_hero_axe" "1" "npc_dota_hero_sven" "1" "npc_dota_hero_riki" "1" } }
            "2" { "name" "Axe's Axe" "prefab" "default_item" "used_by_heroes" { "npc_dota_hero_axe" "1" } }
            "11" { "name" "Axe's Dog" "prefab" "wearable" "item_slot" "summon" "used_by_heroes" { "npc_dota_hero_axe" "1" } }
            "12" { "name" "Sven's Cat" "prefab" "wearable" "item_slot" "summon" "used_by_heroes" { "npc_dota_hero_sven" "1" } }
            "13" { "name" "A Red Axe" "prefab" "wearable" "used_by_heroes" { "npc_dota_hero_axe" "1" } }
        } }"#;
        let catalog = catalog_of(items, None, &[("npc_dota_hero_sven:n", "Sven"), ("npc_dota_hero_riki:n", "Riki")]);
        let sharers = |hero: &str, slot: &str| -> Vec<(String, String)> {
            catalog.slot(Some(hero), slot).unwrap().shared_with.iter().map(|other| (other.id.clone(), other.name.clone())).collect()
        };
        let named = |id: &str, name: &str| (id.to_string(), name.to_string());
        assert_eq!(sharers(AXE, "summon"), [named("npc_dota_hero_riki", "Riki"), named(SVEN, "Sven")]);
        assert_eq!(sharers(SVEN, "summon"), [named(AXE, "Axe"), named("npc_dota_hero_riki", "Riki")]);
        assert!(sharers(AXE, "weapon").is_empty(), "a default item of his alone");
    }

    #[test]
    fn styles_the_game_switches_by_itself_are_not_listed() {
        let catalog = tiny_catalog(true);
        let weapon = catalog.slot(Some(TINY), "weapon").unwrap();
        assert_eq!(weapon.entry("13").unwrap().styles.len(), 2, "styles to choose between");
        assert!(weapon.entry("14").unwrap().styles.is_empty(), "states the game goes through by itself");

        let moody = Pick::hero(TINY, "weapon", "14");
        assert_eq!(catalog.settled(&moody.clone().styled(1)), Some(moody.clone()));
        assert_eq!(catalog.settled(&moody), Some(moody));
        let thawed = Pick::hero(TINY, "weapon", "13").styled(1);
        assert_eq!(catalog.settled(&thawed), Some(thawed.clone()));
        assert_eq!(catalog.settled(&thawed.clone().styled(7)), Some(thawed.styled(0)));
        assert_eq!(catalog.settled(&Pick::hero(TINY, "weapon", "99")), None);
        assert_eq!(catalog.settled(&Pick::hero(TINY, "head", "11")), None, "a slot he has no more");
    }

    #[test]
    fn a_hero_is_found_by_its_name_and_by_its_other_names() {
        let hero = |name: &str, aliases: &[&str]| Hero {
            id: String::new(),
            name: name.to_string(),
            aliases: aliases.iter().map(|alias| alias.to_string()).collect(),
            picture: None,
            sets: Vec::new(),
            slots: Vec::new(),
        };
        let anti_mage = hero("Anti-Mage", &["am", "wei", "antimage"]);
        let shaman = hero("Shadow Shaman", &["ss", "Rhasta"]);
        let prophet = hero("Nature's Prophet", &["np", "furion"]);

        assert_eq!(anti_mage.fits(""), Some(0), "nothing typed fits every hero");
        assert_eq!(anti_mage.fits("am"), Some(0));
        assert_eq!(anti_mage.fits("anti"), Some(0));
        assert_eq!(shaman.fits("rhasta"), Some(0), "whatever the case");
        assert_eq!(anti_mage.fits("mage"), Some(1));
        assert_eq!(shaman.fits("sham"), Some(1));
        assert_eq!(prophet.fits("prop"), Some(1));
        assert_eq!(prophet.fits("fur"), Some(1));
        assert_eq!(shaman.fits("am"), Some(2));
        assert_eq!(prophet.fits("urio"), Some(2));
        assert_eq!(shaman.fits("axe"), None);
    }

    #[test]
    fn what_belongs_to_a_persona_is_told_apart_and_listed_last() {
        assert_eq!(persona_base("weapon_persona_1"), Some("weapon"));
        assert_eq!(persona_base("offhand_weapon_persona_1"), Some("offhand_weapon"));
        assert_eq!(persona_base("head_persona_12"), Some("head"));
        for other in ["weapon", "persona_selector", "_persona_1", "weapon_persona_", "weapon_persona_x"] {
            assert_eq!(persona_base(other), None, "{other}");
        }

        let mut slots = [
            "weapon",
            "ability_effects_2_persona_1",
            "weapon_persona_1",
            "ability_effects_2",
            "persona_selector",
            "head",
            "head_persona_1",
            "ability_effects_1",
            "costume",
        ];
        slots.sort_by_key(|slot| list_place(slot));
        assert_eq!(
            slots,
            [
                "weapon",
                "head",
                "costume",
                "ability_effects_2",
                "ability_effects_1",
                "persona_selector",
                "weapon_persona_1",
                "head_persona_1",
                "ability_effects_2_persona_1"
            ]
        );
    }

    #[test]
    fn names_are_tidied() {
        assert_eq!(plain("#|f|#Crystal Maiden"), "Crystal Maiden");
        assert_eq!(plain("Axe"), "Axe");
        assert_eq!(plain("#1 Fan"), "#1 Fan");
        assert_eq!(title("crystal_maiden"), "Crystal Maiden");
        assert_eq!(title("weapon"), "Weapon");
        assert_eq!(title("offhand_weapon_persona_1"), "Offhand Weapon Persona 1");
    }
}
