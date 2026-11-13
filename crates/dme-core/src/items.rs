use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;

use crate::error::{Error, Result};
use crate::kv::{self, Block, Value};

const WHAT: &str = "items_game.txt";
const HERO_PREFIX: &str = "npc_dota_hero_";
const EFFECTS_PREFIX: &str = "ability_effects_";
const EFFECTS_ATTRIBUTE: &str = "can_equip_as_ability_effects";
pub const EFFECTS_MARK: &str = "apply_when_equipped_in_ability_effects_slot";
pub const AUTO_STYLE_RULE: &str = "auto_style_rule";
const ATTACHED_EFFECTS: &str = "attribute_controlled_attached_particles";
const SOCKET_PREFIX: &str = "static socket ";
pub const SOCKETS: RangeInclusive<u32> = 1..=5;

pub struct ItemsGame<'a> {
    items: Vec<Item<'a>>,
    by_id: HashMap<&'a str, usize>,
    by_name: HashMap<&'a str, usize>,
    prefabs: HashMap<String, Block<'a>>,
    sets: Vec<ItemSet<'a>>,
    world_slots: Vec<&'a str>,
    hero_defaults: HashMap<(String, String), usize>,
    world_defaults: HashMap<String, usize>,
    rarities: Vec<RarityDef<'a>>,
    attached: Vec<AttachedEffect<'a>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachedEffect<'a> {
    pub id: u32,
    pub system: &'a str,
}

impl AttachedEffect<'_> {
    pub fn is_for_couriers(&self) -> bool {
        self.system.to_ascii_lowercase().contains("/courier/")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RarityDef<'a> {
    pub id: &'a str,
    pub rank: u32,
    pub name_token: Option<&'a str>,
    pub color: Option<[u8; 3]>,
}

#[derive(Debug)]
pub struct Item<'a> {
    pub id: &'a str,
    pub block: Block<'a>,
    pub name: Option<&'a str>,
    pub prefab: Option<&'a str>,
    pub own_slot: Option<&'a str>,
    pub model: Option<&'a str>,
    pub heroes: Vec<&'a str>,
}

#[derive(Debug)]
pub struct ItemSet<'a> {
    pub key: &'a str,
    pub name_token: Option<&'a str>,
    pub item_names: Vec<&'a str>,
    pub portrait: Option<&'a str>,
    pub bundle: Option<&'a str>,
}

#[derive(Clone, Copy, Debug)]
pub struct Style<'a> {
    pub index: u32,
    pub name_token: Option<&'a str>,
    pub block: Block<'a>,
}

impl<'a> Item<'a> {
    pub fn is_default(&self) -> bool {
        self.has_prefab("default_item")
    }

    pub fn styles(&self) -> Vec<Style<'a>> {
        let Some(styles) = self.block.block("visuals").and_then(|visuals| visuals.block("styles")) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (index, pair) in styles.pairs().enumerate() {
            let Some(block) = pair.value.as_block().filter(|_| pair.key == index.to_string().as_bytes()) else {
                return Vec::new();
            };
            found.push(Style { index: index as u32, name_token: block.str("name").and_then(as_str), block });
        }
        found
    }

    pub fn switches_styles_itself(&self) -> bool {
        let styles = self.block.block("visuals").and_then(|visuals| visuals.block("styles"));
        let has_a_rule = |pair: kv::Pair<'a>| pair.value.as_block().is_some_and(|style| style.get(AUTO_STYLE_RULE).is_some());
        styles.is_some_and(|styles| styles.pairs().any(has_a_rule))
    }

    pub fn styles_to_choose(&self) -> Vec<Style<'a>> {
        if self.switches_styles_itself() { Vec::new() } else { self.styles() }
    }

    pub fn has_prefab(&self, prefab: &str) -> bool {
        self.prefab
            .is_some_and(|p| p.split_ascii_whitespace().any(|one| one.eq_ignore_ascii_case(prefab)))
    }

    pub fn used_by(&self, hero: &str) -> bool {
        self.heroes.iter().any(|h| h.eq_ignore_ascii_case(hero))
    }

    fn static_values(&self, name: &str) -> Vec<&'a [u8]> {
        let attributes = self.block.block("static_attributes");
        let named = attributes.into_iter().flat_map(|attributes| attributes.pairs()).filter(|pair| pair.key.eq_ignore_ascii_case(name.as_bytes()));
        let values = named.filter_map(|pair| match pair.value {
            Value::Str(value) => Some(value),
            Value::Block(block) => block.str("value"),
        });
        values.collect()
    }

    pub fn effects_numbers(&self) -> Vec<u32> {
        self.static_values(EFFECTS_ATTRIBUTE).into_iter().filter_map(number_of).collect()
    }

    pub fn static_sockets(&self) -> Vec<kv::Pair<'a>> {
        let attributes = self.block.block("static_attributes");
        attributes.into_iter().flat_map(|attributes| attributes.pairs()).filter(|pair| socket_number(pair.key).is_some()).collect()
    }

    pub fn effects_for(&self, number: u32) -> Vec<kv::Pair<'a>> {
        let visuals = self.block.block("visuals");
        let marked = |pair: &kv::Pair<'a>| pair.value.as_block().and_then(|block| block.str(EFFECTS_MARK)).and_then(number_of) == Some(number);
        visuals.into_iter().flat_map(|visuals| visuals.pairs()).filter(marked).collect()
    }

    pub fn effects_in_style(&self, number: u32, style: u32) -> Vec<kv::Pair<'a>> {
        let belongs = |pair: &kv::Pair<'a>| {
            let marked_for = pair.value.as_block().and_then(|block| block.str("style")).and_then(number_of);
            marked_for.is_none_or(|marked_for| marked_for == style)
        };
        self.effects_for(number).into_iter().filter(belongs).collect()
    }

    pub fn effects_picked(&self, number: u32, style: u32) -> Vec<kv::Pair<'a>> {
        if self.switches_styles_itself() { self.effects_for(number) } else { self.effects_in_style(number, style) }
    }

    pub fn effects_styles(&self, number: u32) -> Vec<Style<'a>> {
        if self.switches_styles_itself() {
            return Vec::new();
        }
        let of_a_style = |pair: &kv::Pair<'a>| pair.value.as_block().and_then(|block| block.str("style")).and_then(number_of);
        let named: Vec<u32> = self.effects_for(number).iter().filter_map(of_a_style).collect();
        if !named.iter().any(|&style| style > 0) {
            return Vec::new();
        }
        let has_effects = |style: &Style<'a>| !self.effects_in_style(number, style.index).is_empty();
        self.styles().into_iter().filter(|style| (style.index == 0 || named.contains(&style.index)) && has_effects(style)).collect()
    }

    pub fn first_effects_style(&self, number: u32) -> u32 {
        self.effects_styles(number).first().map_or(0, |style| style.index)
    }
}

fn number_of(text: &[u8]) -> Option<u32> {
    as_str(text)?.trim().parse().ok()
}

pub fn socket_number(name: &[u8]) -> Option<u32> {
    let start = name.get(..SOCKET_PREFIX.len())?;
    let digits = &name[SOCKET_PREFIX.len()..];
    let is_one = start.eq_ignore_ascii_case(SOCKET_PREFIX.as_bytes()) && !digits.is_empty() && digits.iter().all(u8::is_ascii_digit);
    number_of(digits).filter(|_| is_one)
}

pub fn socket_name(number: u32) -> String {
    format!("{SOCKET_PREFIX}{number}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectsSlot<'s> {
    pub number: u32,
    pub whose: &'s str,
}

pub fn effects_slot(slot: &str) -> Option<EffectsSlot<'_>> {
    let start = slot.get(..EFFECTS_PREFIX.len())?;
    if !start.eq_ignore_ascii_case(EFFECTS_PREFIX) {
        return None;
    }
    let rest = &slot[EFFECTS_PREFIX.len()..];
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let number = rest[..digits].parse().ok()?;
    let whose = &rest[digits..];
    (whose.is_empty() || persona_of(whose) == whose).then_some(EffectsSlot { number, whose })
}

pub fn effects_slot_name(number: u32, whose: &str) -> String {
    format!("{EFFECTS_PREFIX}{number}{}", whose.to_ascii_lowercase())
}

pub fn persona_of(slot: &str) -> &str {
    const MARK: &str = "_persona_";
    let lower = slot.to_ascii_lowercase();
    match lower.rfind(MARK) {
        Some(at) if lower.len() > at + MARK.len() && lower[at + MARK.len()..].bytes().all(|digit| digit.is_ascii_digit()) => &slot[at..],
        _ => "",
    }
}

impl<'a> ItemsGame<'a> {
    pub fn parse(src: &'a [u8]) -> Result<ItemsGame<'a>> {
        kv::validate(src, WHAT)?;
        let root = Block::document(src)
            .block("items_game")
            .ok_or_else(|| Error::format(WHAT, "there is no items_game block"))?;

        let mut items = Vec::new();
        let mut prefabs = HashMap::new();
        let mut sets = Vec::new();
        let mut world_slots = Vec::new();
        let mut attached = Vec::new();
        let (mut rarities, mut colors) = (None, None);
        for section in root.pairs() {
            let Value::Block(block) = section.value else { continue };
            if section.key.eq_ignore_ascii_case(b"rarities") {
                rarities = Some(block);
            } else if section.key.eq_ignore_ascii_case(b"colors") {
                colors = Some(block);
            } else if section.key.eq_ignore_ascii_case(ATTACHED_EFFECTS.as_bytes()) {
                attached.extend(block.pairs().filter_map(read_attached_effect));
            } else if section.key.eq_ignore_ascii_case(b"items") {
                items.extend(block.pairs().filter_map(read_item));
            } else if section.key.eq_ignore_ascii_case(b"prefabs") {
                for prefab in block.pairs() {
                    if let (Some(name), Some(body)) = (as_str(prefab.key), prefab.value.as_block()) {
                        prefabs.entry(name.to_ascii_lowercase()).or_insert(body);
                    }
                }
            } else if section.key.eq_ignore_ascii_case(b"item_sets") {
                sets.extend(block.pairs().filter_map(read_set));
            } else if section.key.eq_ignore_ascii_case(b"player_loadout_slots") {
                world_slots.extend(block.pairs().filter_map(|slot| slot.value.as_str().and_then(as_str)));
            }
        }
        if items.is_empty() {
            return Err(Error::format(WHAT, "there are no items"));
        }

        let mut by_id = HashMap::with_capacity(items.len());
        let mut by_name = HashMap::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            by_id.entry(item.id).or_insert(index);
            if let Some(name) = item.name {
                by_name.entry(name).or_insert(index);
            }
        }
        let mut game = ItemsGame {
            items,
            by_id,
            by_name,
            prefabs,
            sets,
            world_slots,
            hero_defaults: HashMap::new(),
            world_defaults: HashMap::new(),
            rarities: rarities.map(|rarities| read_rarities(rarities, colors)).unwrap_or_default(),
            attached,
        };
        (game.hero_defaults, game.world_defaults) = game.find_defaults();
        Ok(game)
    }

    pub fn attached_effects(&self) -> &[AttachedEffect<'a>] {
        &self.attached
    }

    pub fn attached_effect(&self, id: u32) -> Option<&AttachedEffect<'a>> {
        self.attached.iter().find(|effect| effect.id == id)
    }

    pub fn rarities(&self) -> &[RarityDef<'a>] {
        &self.rarities
    }

    pub fn rarity(&self, item: &Item<'a>) -> Option<&'a str> {
        self.value(item, "item_rarity")
    }

    fn find_defaults(&self) -> (HashMap<(String, String), usize>, HashMap<String, usize>) {
        let mut hero_defaults = HashMap::new();
        let mut world_defaults: HashMap<String, usize> = HashMap::new();
        for (index, item) in self.items.iter().enumerate() {
            if !item.heroes.is_empty() {
                if item.is_default()
                    && let Some(slot) = self.slot(item)
                {
                    for hero in &item.heroes {
                        hero_defaults.entry((hero.to_ascii_lowercase(), slot.clone())).or_insert(index);
                    }
                }
            } else if self.is_base(item)
                && let Some(slot) = self.world_slot(item)
            {
                let older = |other: &usize| numeric_id(self.items[*other].id) <= numeric_id(item.id);
                if !world_defaults.get(&slot).is_some_and(older) {
                    world_defaults.insert(slot, index);
                }
            }
        }
        (hero_defaults, world_defaults)
    }

    pub fn items(&self) -> &[Item<'a>] {
        &self.items
    }

    pub fn sets(&self) -> &[ItemSet<'a>] {
        &self.sets
    }

    pub fn item(&self, id: &str) -> Option<&Item<'a>> {
        self.by_id.get(id).map(|&i| &self.items[i])
    }

    pub fn item_named(&self, name: &str) -> Option<&Item<'a>> {
        self.by_name.get(name).map(|&i| &self.items[i])
    }

    pub fn set(&self, key: &str) -> Option<&ItemSet<'a>> {
        self.sets.iter().find(|s| s.key.eq_ignore_ascii_case(key))
    }

    pub fn value(&self, item: &Item<'a>, key: &str) -> Option<&'a str> {
        item.block
            .str(key)
            .or_else(|| self.prefab_value(item.prefab?, key, 0))
            .and_then(as_str)
    }

    fn prefab_value(&self, prefab_names: &str, key: &str, depth: u8) -> Option<&'a [u8]> {
        if depth > 8 {
            return None;
        }
        prefab_names.split_ascii_whitespace().find_map(|name| {
            let prefab = self.prefabs.get(&name.to_ascii_lowercase())?;
            prefab.str(key).or_else(|| {
                let parent = prefab.str("prefab").and_then(as_str)?;
                self.prefab_value(parent, key, depth + 1)
            })
        })
    }

    pub fn slot(&self, item: &Item<'a>) -> Option<String> {
        item.own_slot
            .or_else(|| self.value(item, "item_slot"))
            .map(str::to_ascii_lowercase)
    }

    pub fn is_base(&self, item: &Item<'a>) -> bool {
        self.value(item, "baseitem") == Some("1")
    }

    pub fn world_slots(&self) -> &[&'a str] {
        &self.world_slots
    }

    pub fn world_slot_name(&self, slot: &str) -> Option<&'a str> {
        self.world_slots.iter().copied().find(|known| known.eq_ignore_ascii_case(slot))
    }

    fn is_world_slot(&self, slot: &str) -> bool {
        self.world_slot_name(slot).is_some()
    }

    pub fn world_slot(&self, item: &Item<'a>) -> Option<String> {
        let own = item.own_slot.filter(|slot| self.is_world_slot(slot));
        own.or_else(|| {
            let from_prefab = self.prefab_value(item.prefab?, "item_slot", 0).and_then(as_str)?;
            self.is_world_slot(from_prefab).then_some(from_prefab)
        })
        .map(str::to_ascii_lowercase)
    }

    pub fn default_item(&self, hero: Option<&str>, slot: &str) -> Option<&Item<'a>> {
        let slot = slot.to_ascii_lowercase();
        let index = match hero {
            Some(hero) => self.hero_defaults.get(&(hero.to_ascii_lowercase(), slot)),
            None => self.world_defaults.get(&slot),
        };
        index.map(|&i| &self.items[i])
    }

    pub fn fits(&self, item: &Item<'a>, hero: Option<&str>, slot: &str) -> bool {
        match hero {
            Some(hero) if !item.used_by(hero) || item.is_default() => false,
            Some(hero) => match effects_slot(slot) {
                Some(effects) => self.gives_effects(item, hero, effects),
                None => self.slot(item).is_some_and(|s| s.eq_ignore_ascii_case(slot)),
            },
            None => {
                item.heroes.is_empty()
                    && !self.is_base(item)
                    && self.world_slot(item).is_some_and(|s| s.eq_ignore_ascii_case(slot))
            }
        }
    }

    fn gives_effects(&self, item: &Item<'a>, hero: &str, effects: EffectsSlot<'_>) -> bool {
        let Some(worn) = self.slot(item) else { return false };
        persona_of(&worn).eq_ignore_ascii_case(effects.whose)
            && item.effects_numbers().contains(&effects.number)
            && !item.effects_for(effects.number).is_empty()
            && self.default_item(Some(hero), &worn).is_some()
    }

    pub fn carrier(&self, hero: &str, item: &Item<'a>) -> Option<&Item<'a>> {
        self.default_item(Some(hero), &self.slot(item)?)
    }

    pub fn effects_slots(&self, hero: &str) -> Vec<String> {
        let mut found = std::collections::BTreeSet::new();
        for item in self.items.iter().filter(|item| item.used_by(hero) && !item.is_default()) {
            let numbers = item.effects_numbers();
            if numbers.is_empty() {
                continue;
            }
            let Some(worn) = self.slot(item) else { continue };
            let whose = persona_of(&worn).to_string();
            for number in numbers {
                if self.fits(item, Some(hero), &effects_slot_name(number, &whose)) {
                    found.insert((whose.clone(), number));
                }
            }
        }
        found.into_iter().map(|(whose, number)| effects_slot_name(number, &whose)).collect()
    }

    pub fn items_for(&self, hero: Option<&str>, slot: &str) -> Vec<&Item<'a>> {
        self.items.iter().filter(|item| self.fits(item, hero, slot)).collect()
    }

    pub fn hero_slots(&self, hero: &str) -> Vec<(&str, &Item<'a>)> {
        let hero = hero.to_ascii_lowercase();
        let mut slots: Vec<(&str, usize)> = self
            .hero_defaults
            .iter()
            .filter(|((owner, _), _)| *owner == hero)
            .map(|((_, slot), &index)| (slot.as_str(), index))
            .collect();
        slots.sort_by_key(|&(_, index)| index);
        slots.into_iter().map(|(slot, index)| (slot, &self.items[index])).collect()
    }

    pub fn hero_default_items(&self) -> Vec<(&str, &Item<'a>)> {
        let mut found: Vec<(usize, &str)> = self.hero_defaults.iter().map(|((hero, _), &index)| (index, hero.as_str())).collect();
        found.sort_unstable();
        found.dedup_by_key(|(index, _)| *index);
        found.into_iter().map(|(index, hero)| (hero, &self.items[index])).collect()
    }

    pub fn items_to_pick(&self) -> HashMap<(String, String), usize> {
        let mut found = HashMap::new();
        for item in self.items.iter().filter(|item| !item.heroes.is_empty() && !item.is_default()) {
            let Some(slot) = self.slot(item) else { continue };
            let heroes: HashSet<String> = item.heroes.iter().map(|hero| hero.to_ascii_lowercase()).collect();
            for hero in heroes {
                *found.entry((hero, slot.clone())).or_default() += 1;
            }
        }
        found
    }

    pub fn default_items(&self, hero: &str) -> Vec<&Item<'a>> {
        self.hero_slots(hero).into_iter().map(|(_, item)| item).collect()
    }

    pub fn set_items(&self, set: &ItemSet<'a>) -> Vec<Option<&Item<'a>>> {
        set.item_names.iter().map(|name| self.item_named(name)).collect()
    }

    pub fn set_is_for(&self, set: &ItemSet<'a>, hero: &str) -> bool {
        !set.item_names.is_empty()
            && self.set_items(set).iter().all(|item| item.is_some_and(|i| i.used_by(hero)))
    }

    pub fn sets_for(&self, hero: &str) -> Vec<&ItemSet<'a>> {
        self.sets.iter().filter(|set| self.set_is_for(set, hero)).collect()
    }

    pub fn heroes(&self) -> Vec<&'a str> {
        let mut heroes: Vec<&str> = self
            .items
            .iter()
            .filter(|i| i.is_default())
            .flat_map(|i| i.heroes.iter().copied())
            .filter(|h| h.starts_with(HERO_PREFIX))
            .collect();
        heroes.sort_unstable();
        heroes.dedup();
        heroes
    }
}

pub fn hero_id(name: &str) -> String {
    let name = name.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    if name.starts_with(HERO_PREFIX) { name } else { format!("{HERO_PREFIX}{name}") }
}

pub fn hero_short_name(hero: &str) -> &str {
    hero.strip_prefix(HERO_PREFIX).unwrap_or(hero)
}

fn read_item<'a>(pair: kv::Pair<'a>) -> Option<Item<'a>> {
    let block = pair.value.as_block()?;
    let mut item = Item {
        id: as_str(pair.key)?,
        block,
        name: None,
        prefab: None,
        own_slot: None,
        model: None,
        heroes: Vec::new(),
    };
    for field in block.pairs() {
        let key = field.key;
        match field.value {
            Value::Str(value) => {
                if key.eq_ignore_ascii_case(b"name") {
                    item.name = as_str(value);
                } else if key.eq_ignore_ascii_case(b"prefab") {
                    item.prefab = as_str(value);
                } else if key.eq_ignore_ascii_case(b"item_slot") {
                    item.own_slot = as_str(value);
                } else if key.eq_ignore_ascii_case(b"model_player") {
                    item.model = as_str(value);
                } else if key.eq_ignore_ascii_case(b"used_by_heroes") {
                    item.heroes.extend(as_str(value).filter(|h| is_hero_id(h)));
                }
            }
            Value::Block(heroes) if key.eq_ignore_ascii_case(b"used_by_heroes") => {
                item.heroes = heroes
                    .pairs()
                    .filter(|h| h.value.as_str().is_some_and(|v| v != b"0"))
                    .filter_map(|h| as_str(h.key))
                    .filter(|h| is_hero_id(h))
                    .collect();
            }
            Value::Block(_) => {}
        }
    }
    Some(item)
}

fn read_set<'a>(pair: kv::Pair<'a>) -> Option<ItemSet<'a>> {
    let block = pair.value.as_block()?;
    Some(ItemSet {
        key: as_str(pair.key)?,
        name_token: block.str("name").and_then(as_str),
        item_names: block
            .block("items")
            .map(|items| items.pairs().filter_map(|i| as_str(i.key)).collect())
            .unwrap_or_default(),
        portrait: block.str("portrait_image").and_then(as_str),
        bundle: block.str("store_bundle").and_then(as_str),
    })
}

fn read_attached_effect<'a>(pair: kv::Pair<'a>) -> Option<AttachedEffect<'a>> {
    let block = pair.value.as_block()?;
    Some(AttachedEffect { id: number_of(pair.key)?, system: block.str("system").and_then(as_str)? })
}

fn read_rarities<'a>(rarities: Block<'a>, colors: Option<Block<'a>>) -> Vec<RarityDef<'a>> {
    let color_of = |rarity: Block<'a>| {
        let entry = colors?.block(rarity.str("color").and_then(as_str)?)?;
        hex_color(entry.str("hex_color").and_then(as_str)?)
    };
    let mut found: Vec<RarityDef<'a>> = rarities
        .pairs()
        .filter_map(|pair| {
            let block = pair.value.as_block()?;
            Some(RarityDef {
                id: as_str(pair.key)?,
                rank: block.str("value").and_then(as_str)?.trim().parse().ok()?,
                name_token: block.str("loc_key").and_then(as_str),
                color: color_of(block),
            })
        })
        .collect();
    found.sort_by_key(|rarity| rarity.rank);
    found
}

fn hex_color(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().strip_prefix('#')?;
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

fn as_str(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

fn numeric_id(id: &str) -> u64 {
    id.parse().unwrap_or(u64::MAX)
}

fn is_hero_id(text: &str) -> bool {
    text.len() > HERO_PREFIX.len()
        && text.get(..HERO_PREFIX.len()).is_some_and(|start| start.eq_ignore_ascii_case(HERO_PREFIX))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const SAMPLE: &str = concat!(
        "\"items_game\"\r\n{\r\n",
        "\t\"game_info\"\r\n\t{\r\n\t\t\"first_valid_class\"\t\t\"1\"\r\n\t}\r\n",
        "\t\"rarities\"\r\n\t{\r\n",
        "\t\t\"immortal\"\r\n\t\t{\r\n\t\t\t\"value\"\t\t\"7\"\r\n\t\t\t\"loc_key\"\t\t\"Rarity_Immortal\"\r\n\t\t\t\"color\"\t\t\"desc_immortal\"\r\n\t\t}\r\n",
        "\t\t\"common\"\r\n\t\t{\r\n\t\t\t\"value\"\t\t\"1\"\r\n\t\t\t\"loc_key\"\t\t\"Rarity_Common\"\r\n\t\t\t\"color\"\t\t\"desc_common\"\r\n\t\t\t\"next_rarity\"\t\t\"mythical\"\r\n\t\t}\r\n",
        "\t\t\"mythical\"\r\n\t\t{\r\n\t\t\t\"value\"\t\t\"4\"\r\n\t\t\t\"loc_key\"\t\t\"Rarity_Mythical\"\r\n\t\t\t\"color\"\t\t\"desc_mythical\"\r\n\t\t}\r\n",
        "\t\t\"legendary\"\r\n\t\t{\r\n\t\t\t\"value\"\t\t\"5\"\r\n\t\t\t\"loc_key\"\t\t\"Rarity_Legendary\"\r\n\t\t\t\"color\"\t\t\"desc_nothing\"\r\n\t\t}\r\n",
        "\t}\r\n",
        "\t\"colors\"\r\n\t{\r\n",
        "\t\t\"desc_common\"\r\n\t\t{\r\n\t\t\t\"color_name\"\t\t\"ItemRarityCommon\"\r\n\t\t\t\"hex_color\"\t\t\"#b0c3d9\"\r\n\t\t}\r\n",
        "\t\t\"desc_mythical\"\r\n\t\t{\r\n\t\t\t\"color_name\"\t\t\"ItemRarityMythical\"\r\n\t\t\t\"hex_color\"\t\t\"#8847ff\"\r\n\t\t}\r\n",
        "\t\t\"desc_immortal\"\r\n\t\t{\r\n\t\t\t\"color_name\"\t\t\"ItemRarityImmortal\"\r\n\t\t\t\"hex_color\"\t\t\"#E4AE39\"\r\n\t\t}\r\n",
        "\t}\r\n",
        "\t\"player_loadout_slots\"\r\n\t{\r\n\t\t\"0\"\t\t\"courier\"\r\n\t\t\"1\"\t\t\"cursor_pack\"\r\n\t\t\"2\"\t\t\"weather\"\r\n\t\t\"3\"\t\t\"terrain\"\r\n\t}\r\n",
        "\t\"prefabs\"\r\n\t{\r\n",
        "\t\t\"courier\"\r\n\t\t{\r\n\t\t\t\"item_slot\"\t\t\"courier\"\r\n\t\t}\r\n",
        "\t\t\"cursor_pack\"\r\n\t\t{\r\n\t\t\t\"item_slot\"\t\t\"cursor_pack\"\r\n\t\t}\r\n",
        "\t\t\"weather\"\r\n\t\t{\r\n\t\t\t\"item_slot\"\t\t\"weather\"\r\n\t\t}\r\n",
        "\t\t\"misc\"\r\n\t\t{\r\n\t\t\t\"item_slot\"\t\t\"none\"\r\n\t\t}\r\n",
        "\t\t\"default_item\"\r\n\t\t{\r\n\t\t\t\"item_class\"\t\t\"dota_item_wearable\"\r\n\t\t\t\"item_slot\"\t\t\"weapon\"\r\n\t\t\t\"baseitem\"\t\t\"1\"\r\n\t\t}\r\n",
        "\t\t\"wearable\"\r\n\t\t{\r\n\t\t\t\"item_class\"\t\t\"dota_item_wearable\"\r\n\t\t\t\"item_slot\"\t\t\"weapon\"\r\n\t\t\t\"item_rarity\"\t\t\"common\"\r\n\t\t}\r\n",
        "\t\t\"bundle\"\r\n\t\t{\r\n\t\t\t\"item_class\"\t\t\"bundle\"\r\n\t\t\t\"item_slot\"\t\t\"none\"\r\n\t\t}\r\n",
        "\t}\r\n",
        "\t\"items\"\r\n\t{\r\n",
        "\t\t\"2\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Axe's Axe\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"image_inventory\"\t\t\"econ/heroes/axe/axe_weapon\"\r\n\t\t\t\"item_name\"\t\t\"#DOTA_Item_Axes_Axe\"\r\n\t\t\t\"model_player\"\t\t\"models/heroes/axe/axe_weapon.vmdl\"\r\n\t\t\t\"portraits\"\r\n\t\t\t{\r\n\t\t\t\t\"icon\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"PortraitFOV\"\t\t\"47\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"4\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Axe's Hair\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"item_slot\"\t\t\"head\"\r\n\t\t\t\"model_player\"\t\t\"models/heroes/axe/axe_ponytail.vmdl\"\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"skin\"\t\t\"0\"\r\n\t\t\t}\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"8653\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Axe's Miscellaneous\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"item_slot\"\t\t\"misc\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"30\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Bane's Head\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"item_slot\"\t\t\"head\"\r\n\t\t\t\"model_player\"\t\t\"models/heroes/bane/bane_head.vmdl\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_bane\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"9040\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Axe of the Vanguard\"\r\n\t\t\t\"prefab\"\t\t\"wearable\"\r\n\t\t\t\"creation_date\"\t\t\"2017-01-05\"\r\n\t\t\t\"image_inventory\"\t\t\"econ/items/axe/vanguard_weapon\"\r\n\t\t\t\"item_name\"\t\t\"#DOTA_Item_Axe_of_the_Vanguard\"\r\n\t\t\t\"item_rarity\"\t\t\"mythical\"\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/vanguard_weapon/vanguard_weapon.vmdl\"\r\n\t\t\t\"portraits\"\r\n\t\t\t{\r\n\t\t\t\t\"icon\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"PortraitFOV\"\t\t\"50\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle_create\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/econ/items/axe/vanguard/weapon.vpcf\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"8910\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Helm of the \\\"Vanguard\\\"\"\r\n\t\t\t\"prefab\"\t\t\"wearable\"\r\n\t\t\t\"item_slot\"\t\t\"Head\"\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/vanguard_head/vanguard_head.vmdl\"\r\n\t\t\t\"price_info\"\r\n\t\t\t{\r\n\t\t\t\t\"bucket\"\t\t\"Wearables\"\r\n\t\t\t}\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"8907\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Bracers of the Vanguard\"\r\n\t\t\t\"prefab\"\t\t\"wearable\"\r\n\t\t\t\"item_slot\"\t\t\"misc\"\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/vanguard_misc/vanguard_misc.vmdl\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"8999\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Cape of the Vanguard\"\r\n\t\t\t\"prefab\"\t\t\"wearable\"\r\n\t\t\t\"item_slot\"\t\t\"back\"\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/vanguard_back/vanguard_back.vmdl\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"21096\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Armor of the Vanguard\"\r\n\t\t\t\"prefab\"\t\t\"bundle\"\r\n\t\t\t\"item_rarity\"\t\t\"legendary\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"9500\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Crested Helm\"\r\n\t\t\t\"prefab\"\t\t\"wearable\"\r\n\t\t\t\"image_inventory\"\t\t\"econ/items/axe/crested\"\r\n\t\t\t\"item_slot\"\t\t\"head\"\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/crested/crested.vmdl\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n",
        "\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"skin\"\t\t\"0\"\r\n",
        "\t\t\t\t\"styles\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"0\"\r\n\t\t\t\t\t{\r\n\t\t\t\t\t\t\"name\"\t\t\"#Style_Plain\"\r\n\t\t\t\t\t}\r\n\t\t\t\t\t\"1\"\r\n\t\t\t\t\t{\r\n\t\t\t\t\t\t\"name\"\t\t\"#Style_Gold\"\r\n\t\t\t\t\t\t\"skin\"\t\t\"1\"\r\n\t\t\t\t\t\t\"model_player\"\t\t\"models/items/axe/crested/crested_gold.vmdl\"\r\n\t\t\t\t\t\t\"alternate_icon\"\t\t\"1\"\r\n\t\t\t\t\t}\r\n\t\t\t\t}\r\n",
        "\t\t\t\t\"alternate_icons\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"1\"\r\n\t\t\t\t\t{\r\n\t\t\t\t\t\t\"icon_path\"\t\t\"econ/items/axe/crested_gold\"\r\n\t\t\t\t\t}\r\n\t\t\t\t}\r\n",
        "\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle_create\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/plain.vpcf\"\r\n\t\t\t\t\t\"style\"\t\t\"0\"\r\n\t\t\t\t}\r\n",
        "\t\t\t\t\"asset_modifier1\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle_create\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/gold.vpcf\"\r\n\t\t\t\t\t\"style\"\t\t\"1\"\r\n\t\t\t\t}\r\n",
        "\t\t\t\t\"asset_modifier2\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"activity\"\r\n\t\t\t\t\t\"modifier\"\t\t\"crested\"\r\n\t\t\t\t}\r\n",
        "\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"595\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Default Courier\"\r\n\t\t\t\"prefab\"\t\t\"courier\"\r\n\t\t\t\"baseitem\"\t\t\"1\"\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"courier\"\r\n\t\t\t\t\t\"modifier\"\t\t\"models/props_gameplay/donkey.vmdl\"\r\n\t\t\t\t\t\"asset\"\t\t\"radiant\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"700\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Newer Base Courier\"\r\n\t\t\t\"prefab\"\t\t\"courier\"\r\n\t\t\t\"baseitem\"\t\t\"1\"\r\n\t\t}\r\n",
        "\t\t\"10003\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Drodo the Druffin\"\r\n\t\t\t\"prefab\"\t\t\"courier\"\r\n\t\t\t\"item_rarity\"\t\t\"immortal\"\r\n\t\t\t\"particle_folder\"\t\t\"particles/econ/courier/courier_drodo\"\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"courier\"\r\n\t\t\t\t\t\"modifier\"\t\t\"models/courier/drodo/drodo.vmdl\"\r\n\t\t\t\t\t\"asset\"\t\t\"radiant\"\r\n\t\t\t\t}\r\n\t\t\t\t\"asset_modifier1\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle_create\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/econ/courier/courier_drodo/ambient.vpcf\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"202\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Default Cursor Pack\"\r\n\t\t\t\"prefab\"\t\t\"cursor_pack\"\r\n\t\t\t\"baseitem\"\t\t\"1\"\r\n\t\t\t\"item_slot\"\t\t\"weapon\"\r\n\t\t}\r\n",
        "\t\t\"203\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Blue Cursor Pack\"\r\n\t\t\t\"prefab\"\t\t\"cursor_pack\"\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"cursor_pack\"\r\n\t\t\t\t\t\"modifier\"\t\t\"blue\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"555\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Default Weather\"\r\n\t\t\t\"prefab\"\t\t\"weather\"\r\n\t\t\t\"baseitem\"\t\t\"1\"\r\n\t\t}\r\n",
        "\t\t\"556\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Weather Snow\"\r\n\t\t\t\"prefab\"\t\t\"misc\"\r\n\t\t\t\"item_slot\"\t\t\"weather\"\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/rain_fx/econ_snow.vpcf\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"12000\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"Desert Terrain\"\r\n\t\t\t\"prefab\"\t\t\"misc\"\r\n\t\t\t\"item_slot\"\t\t\"terrain\"\r\n\t\t}\r\n",
        "\t}\r\n",
        "\t\"item_sets\"\r\n\t{\r\n",
        "\t\t\"armor_of_the_vanguard\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"#DOTA_Set_Armor_of_the_Vanguard\"\r\n\t\t\t\"items\"\r\n\t\t\t{\r\n\t\t\t\t\"Axe of the Vanguard\"\t\t\"1\"\r\n\t\t\t\t\"Helm of the \\\"Vanguard\\\"\"\t\t\"1\"\r\n\t\t\t\t\"Bracers of the Vanguard\"\t\t\"1\"\r\n\t\t\t\t\"Cape of the Vanguard\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t\t\"store_bundle\"\t\t\"Armor of the Vanguard\"\r\n\t\t\t\"portrait_image\"\t\t\"econ/sets/v2/armor_of_the_vanguard\"\r\n\t\t}\r\n",
        "\t\t\"broken_set\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"#DOTA_Set_Broken\"\r\n\t\t\t\"items\"\r\n\t\t\t{\r\n\t\t\t\t\"No Such Item\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t\t\"crested_set\"\r\n\t\t{\r\n\t\t\t\"name\"\t\t\"#DOTA_Set_Crested\"\r\n\t\t\t\"items\"\r\n\t\t\t{\r\n\t\t\t\t\"Crested Helm\"\t\t\"1\"\r\n\t\t\t\t\"Axe of the Vanguard\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t}\r\n",
        "\t}\r\n",
        "}\r\n",
    );

    const EFFECTS_TEXT: &str = r##"
"items_game"
{
    "prefabs"
    {
        "default_item"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "weapon"
            "baseitem"  "1"
        }
        "wearable"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "weapon"
            "item_rarity"  "common"
        }
    }
    "items"
    {
        "100"
        {
            "name"  "Juggernaut's Sword"
            "prefab"  "default_item"
            "model_player"  "models/heroes/juggernaut/sword.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
        }
        "101"
        {
            "name"  "Juggernaut's Mask"
            "prefab"  "default_item"
            "item_slot"  "head"
            "model_player"  "models/heroes/juggernaut/mask.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "skin"  "0"
            }
        }
        "102"
        {
            "name"  "Juggernaut's Persona Sword"
            "prefab"  "default_item"
            "item_slot"  "weapon_persona_1"
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
        }
        "790"
        {
            "name"  "All Heroes' Default Ability Effects 1"
            "prefab"  "default_item"
            "item_slot"  "ability_effects_1"
            "used_by_heroes"
            {
                "all"  "1"
            }
            "static_attributes"
            {
                "can_equip_as_ability_effects"
                {
                    "attribute_class"  "can_equip_as_ability_effects"
                    "value"  "1"
                }
            }
        }
        "200"
        {
            "name"  "Edge of the Lost"
            "prefab"  "wearable"
            "image_inventory"  "econ/items/juggernaut/edge"
            "item_rarity"  "immortal"
            "model_player"  "models/items/juggernaut/edge.vmdl"
            "static_attributes"
            {
                "can_equip_as_ability_effects"
                {
                    "attribute_class"  "can_equip_as_ability_effects"
                    "value"  "1"
                }
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/edge/ambient.vpcf"
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/fury.vpcf"
                    "modifier"  "particles/edge/fury.vpcf"
                    "style"  "0"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
                "asset_modifier"
                {
                    "type"  "ability_icon"
                    "asset"  "juggernaut_blade_fury"
                    "modifier"  "juggernaut/edge/blade_fury"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
            }
        }
        "201"
        {
            "name"  "Serra"
            "prefab"  "wearable"
            "item_rarity"  "immortal"
            "model_player"  "models/items/juggernaut/serra.vmdl"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "4"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "styles"
                {
                    "0"
                    {
                        "name"  "#Style_Steel"
                    }
                    "1"
                    {
                        "name"  "#Style_Gold"
                        "alternate_icon"  "1"
                    }
                    "2"
                    {
                        "name"  "#Style_Rust"
                    }
                }
                "alternate_icons"
                {
                    "1"
                    {
                        "icon_path"  "econ/items/juggernaut/serra_gold"
                    }
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/slash.vpcf"
                    "modifier"  "particles/serra/slash.vpcf"
                    "style"  "0"
                    "apply_when_equipped_in_ability_effects_slot"  "4"
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/slash.vpcf"
                    "modifier"  "particles/serra/slash_gold.vpcf"
                    "style"  "1"
                    "apply_when_equipped_in_ability_effects_slot"  "4"
                }
                "asset_modifier"
                {
                    "type"  "sound"
                    "asset"  "Hero_Juggernaut.Slash"
                    "modifier"  "Hero_Juggernaut.Slash.Serra"
                    "apply_when_equipped_in_ability_effects_slot"  "4"
                }
            }
        }
        "202"
        {
            "name"  "Mask of Two Winds"
            "prefab"  "wearable"
            "item_slot"  "head"
            "item_rarity"  "mythical"
            "model_player"  "models/items/juggernaut/two_winds.vmdl"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "1"
                "is commodity"  "1"
                "can_equip_as_ability_effects"  "2"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/fury.vpcf"
                    "modifier"  "particles/two_winds/fury.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/ward.vpcf"
                    "modifier"  "particles/two_winds/ward.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "2"
                }
            }
        }
        "203"
        {
            "name"  "Plain Sword"
            "prefab"  "wearable"
            "model_player"  "models/items/juggernaut/plain.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "Particle"
                    "asset"  "particles/Juggernaut/fury.vpcf"
                    "modifier"  "particles/plain/fury.vpcf"
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/fury_hit.vpcf"
                    "modifier"  "particles/plain/fury_hit.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/plain/ambient.vpcf"
                }
            }
        }
        "204"
        {
            "name"  "Empty Promise"
            "prefab"  "wearable"
            "item_slot"  "head"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "3"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
        }
        "205"
        {
            "name"  "Persona Edge"
            "prefab"  "wearable"
            "item_slot"  "weapon_persona_1"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "1"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut_persona/fury.vpcf"
                    "modifier"  "particles/persona_edge/fury.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
            }
        }
        "206"
        {
            "name"  "Cape of the Fourth"
            "prefab"  "wearable"
            "item_slot"  "back"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "4"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/slash.vpcf"
                    "modifier"  "particles/cape/slash.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "4"
                }
            }
        }
        "207"
        {
            "name"  "Ward Blade"
            "prefab"  "wearable"
            "model_player"  "models/items/juggernaut/ward_blade.vmdl"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "2"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/ward.vpcf"
                    "modifier"  "particles/ward_blade/ward.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "2"
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/slash.vpcf"
                    "modifier"  "particles/ward_blade/slash.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "2"
                }
            }
        }
        "208"
        {
            "name"  "Late Bloomer"
            "prefab"  "wearable"
            "item_slot"  "head"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "4"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_juggernaut"  "1"
            }
            "visuals"
            {
                "styles"
                {
                    "0"
                    {
                        "name"  "#Style_Bud"
                    }
                    "1"
                    {
                        "name"  "#Style_Bloom"
                    }
                }
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/juggernaut/slash.vpcf"
                    "modifier"  "particles/bloomer/slash.vpcf"
                    "style"  "1"
                    "apply_when_equipped_in_ability_effects_slot"  "4"
                }
            }
        }
        "300"
        {
            "name"  "Lina's Hair"
            "prefab"  "default_item"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_lina"  "1"
            }
        }
        "301"
        {
            "name"  "Fiery Hair"
            "prefab"  "wearable"
            "item_slot"  "head"
            "static_attributes"
            {
                "can_equip_as_ability_effects"  "1"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_lina"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/lina/slave.vpcf"
                    "modifier"  "particles/fiery/slave.vpcf"
                    "apply_when_equipped_in_ability_effects_slot"  "1"
                }
            }
        }
    }
}
"##;

    pub(crate) fn laid_out(text: &str, line_break: &str) -> String {
        let lines = text.trim().lines().map(|line| {
            let written = line.trim_start_matches(' ');
            let depth = (line.len() - written.len()) / 4;
            format!("{}{}{line_break}", "\t".repeat(depth), written.replace("\"  \"", "\"\t\t\""))
        });
        lines.collect()
    }

    pub(crate) fn effects_sample() -> String {
        laid_out(EFFECTS_TEXT, "\r\n")
    }

    const GEMS_TEXT: &str = r##"
"items_game"
{
    "prefabs"
    {
        "default_item"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "weapon"
            "baseitem"  "1"
        }
        "wearable"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "weapon"
        }
        "socket_gem"
        {
            "item_class"  "tool"
            "item_slot"  "none"
        }
        "courier"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "courier"
        }
    }
    "player_loadout_slots"
    {
        "0"  "courier"
    }
    "items"
    {
        "1"
        {
            "name"  "Necrophos' Scythe"
            "prefab"  "default_item"
            "model_player"  "models/heroes/necrolyte/scythe.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "2"
        {
            "name"  "Necrophos' Hood"
            "prefab"  "default_item"
            "item_slot"  "head"
            "static_attributes"
            {
                "is commodity"  "1"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "3"
        {
            "name"  "Necrophos' Cape"
            "prefab"  "default_item"
            "item_slot"  "back"
            "static_attributes"
            {
                "Static Socket 1"
                {
                    "attribute_class"  "socket"
                    "value"  "socket { gem_def_index: 3005 } asset_modifier: 7"
                }
                "cannot trade"
                {
                    "attribute_class"  "cannot_trade"
                    "value"  "1"
                }
            }
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "4"
        {
            "name"  "Default Courier"
            "prefab"  "courier"
            "baseitem"  "1"
        }
        "5"
        {
            "name"  "Necrophos' Belt"
            "prefab"  "default_item"
            "item_slot"  "belt"
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "20"
        {
            "name"  "Scythe of Ka"
            "prefab"  "wearable"
            "model_player"  "models/items/necrolyte/ka_scythe.vmdl"
            "static_attributes"
            {
                "static socket 1"
                {
                    "attribute_class"  "socket"
                    "value"  "socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 91"
                }
                "international tag"
                {
                    "attribute_class"  "international_tag"
                    "value"  "2014"
                }
            }
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/necrolyte/ka_scythe.vpcf"
                }
            }
        }
        "21"
        {
            "name"  "Twin Scythe"
            "prefab"  "wearable"
            "static_attributes"
            {
                "static socket 2"
                {
                    "attribute_class"  "socket"
                    "value"  "socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 12"
                }
                "static socket 1"
                {
                    "attribute_class"  "socket"
                    "value"  "socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 11"
                }
            }
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "22"
        {
            "name"  "Hood of Embers"
            "prefab"  "wearable"
            "item_slot"  "head"
            "item_rarity"  "arcana"
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/necrolyte/embers.vpcf"
                }
            }
        }
        "23"
        {
            "name"  "Cape of Five Gems"
            "prefab"  "wearable"
            "item_slot"  "back"
            "static_attributes"
            {
                "static socket 1"  "socket { gem_def_index: 3005 } asset_modifier: 1"
                "static socket 2"  "socket { gem_def_index: 3005 } asset_modifier: 2"
                "static socket 3"  "socket { gem_def_index: 3005 } asset_modifier: 3"
                "static socket 4"  "socket { gem_def_index: 3005 } asset_modifier: 4"
                "static socket 5"  "socket { gem_def_index: 3005 } asset_modifier: 5"
            }
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "24"
        {
            "name"  "Plain Cape"
            "prefab"  "wearable"
            "item_slot"  "back"
            "used_by_heroes"
            {
                "npc_dota_hero_necrolyte"  "1"
            }
        }
        "30"
        {
            "name"  "A Courier With A Trail"
            "prefab"  "courier"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/courier/trail.vpcf"
                }
            }
        }
        "3000"
        {
            "name"  "Ethereal Gem"
            "prefab"  "socket_gem"
            "static_attributes"
            {
                "gem type"
                {
                    "attribute_class"  "gem type"
                    "value"  "1"
                }
                "required socket type"
                {
                    "attribute_class"  "required socket type"
                    "value"  "1"
                }
            }
        }
        "3001"
        {
            "name"  "Prismatic Gem"
            "prefab"  "socket_gem"
            "static_attributes"
            {
                "gem type"
                {
                    "attribute_class"  "gem type"
                    "value"  "2"
                }
                "required socket type"
                {
                    "attribute_class"  "required socket type"
                    "value"  "2"
                }
            }
        }
        "3005"
        {
            "name"  "Kinetic Gem"
            "prefab"  "socket_gem"
            "static_attributes"
            {
                "gem type"  "5"
            }
        }
    }
}
"##;

    pub(crate) fn gems_sample() -> String {
        laid_out(GEMS_TEXT, "\n")
    }

    use crate::effects::tests::PACKS;

    const AXE: &str = "npc_dota_hero_axe";
    const JUGG: &str = "npc_dota_hero_juggernaut";

    fn ids<'a>(items: &[&Item<'a>]) -> Vec<&'a str> {
        items.iter().map(|item| item.id).collect()
    }

    #[test]
    fn reads_items_prefabs_and_sets() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(game.items().len(), 18);
        assert_eq!(game.sets().len(), 3);

        let axe = game.item("2").unwrap();
        assert_eq!(axe.name, Some("Axe's Axe"));
        assert!(axe.is_default());
        assert_eq!(axe.model, Some("models/heroes/axe/axe_weapon.vmdl"));
        assert_eq!(axe.heroes, ["npc_dota_hero_axe"]);
        assert!(axe.used_by("NPC_DOTA_HERO_AXE"));
        assert!(!axe.used_by("npc_dota_hero_bane"));

        assert_eq!(game.item_named("Bracers of the Vanguard").unwrap().id, "8907");
        assert!(game.item("1").is_none());
    }

    #[test]
    fn slot_comes_from_the_item_or_its_prefab() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(game.slot(game.item("2").unwrap()).as_deref(), Some("weapon"), "from the prefab");
        assert_eq!(game.slot(game.item("4").unwrap()).as_deref(), Some("head"));
        assert_eq!(game.slot(game.item("8910").unwrap()).as_deref(), Some("head"), "lower-cased");
        assert_eq!(game.slot(game.item("21096").unwrap()).as_deref(), Some("none"));
        assert_eq!(game.value(game.item("2").unwrap(), "baseitem"), Some("1"));
        assert_eq!(game.value(game.item("9040").unwrap(), "baseitem"), None);
    }

    #[test]
    fn default_items_and_sets_are_found_per_hero() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let ids: Vec<&str> = game.default_items("npc_dota_hero_axe").iter().map(|i| i.id).collect();
        assert_eq!(ids, ["2", "4", "8653"]);
        assert_eq!(game.heroes(), ["npc_dota_hero_axe", "npc_dota_hero_bane"]);

        let sets: Vec<&str> = game.sets_for("npc_dota_hero_axe").iter().map(|s| s.key).collect();
        assert_eq!(sets, ["armor_of_the_vanguard", "crested_set"]);
        assert!(game.sets_for("npc_dota_hero_bane").is_empty());

        let set = game.set("ARMOR_OF_THE_VANGUARD").unwrap();
        assert_eq!(set.name_token, Some("#DOTA_Set_Armor_of_the_Vanguard"));
        assert_eq!(set.portrait, Some("econ/sets/v2/armor_of_the_vanguard"));
        let ids: Vec<&str> = game.set_items(set).iter().map(|i| i.unwrap().id).collect();
        assert_eq!(ids, ["9040", "8910", "8907", "8999"]);

        let broken = game.set("broken_set").unwrap();
        assert!(game.set_items(broken)[0].is_none());
        assert!(!game.set_is_for(broken, "npc_dota_hero_axe"));
    }

    #[test]
    fn each_hero_slot_has_a_default_item() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let slots: Vec<(&str, &str)> = game.hero_slots(AXE).iter().map(|(slot, item)| (*slot, item.id)).collect();
        assert_eq!(slots, [("weapon", "2"), ("head", "4"), ("misc", "8653")]);
        assert_eq!(game.default_item(Some("NPC_DOTA_HERO_AXE"), "Head").unwrap().id, "4");
        assert!(game.default_item(Some(AXE), "back").is_none());
        assert!(game.default_item(Some("npc_dota_hero_nobody"), "weapon").is_none());
        assert!(game.hero_slots("npc_dota_hero_nobody").is_empty());
    }

    #[test]
    fn world_slots_and_their_defaults_are_found() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(game.world_slots(), ["courier", "cursor_pack", "weather", "terrain"]);
        assert_eq!(game.world_slot_name("Cursor_Pack"), Some("cursor_pack"));
        assert_eq!(game.world_slot_name("weapon"), None);

        let world_slot = |id: &str| game.world_slot(game.item(id).unwrap());
        assert_eq!(world_slot("595").as_deref(), Some("courier"), "from the prefab");
        assert_eq!(world_slot("202").as_deref(), Some("cursor_pack"), "the item's own slot is not a world slot");
        assert_eq!(world_slot("556").as_deref(), Some("weather"), "the item's own slot");
        assert_eq!(world_slot("9040"), None);
        assert_eq!(world_slot("21096"), None);

        let default = |slot: &str| game.default_item(None, slot).map(|item| item.id);
        assert_eq!(default("courier"), Some("595"), "the oldest of the base items");
        assert_eq!(default("Cursor_Pack"), Some("202"));
        assert_eq!(default("weather"), Some("555"));
        assert_eq!(default("terrain"), None, "no base item goes there");
        assert_eq!(default("weapon"), None, "a hero's slot is not a world slot");
        assert!(game.is_base(game.item("595").unwrap()));
        assert!(!game.is_base(game.item("10003").unwrap()));
    }

    #[test]
    fn items_fit_only_the_slot_they_are_made_for() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let fits = |id: &str, hero: Option<&str>, slot: &str| game.fits(game.item(id).unwrap(), hero, slot);
        assert!(fits("9040", Some(AXE), "weapon"));
        assert!(fits("8910", Some(AXE), "HEAD"));
        assert!(!fits("9040", Some(AXE), "head"));
        assert!(!fits("9040", Some("npc_dota_hero_bane"), "weapon"));
        assert!(!fits("2", Some(AXE), "weapon"), "a default item is not something to pick");
        assert!(!fits("9040", None, "courier"));
        assert!(fits("10003", None, "courier"));
        assert!(!fits("10003", Some(AXE), "courier"));
        assert!(!fits("700", None, "courier"), "a base item is not something to pick");
        assert!(fits("203", None, "cursor_pack"));
        assert!(!fits("203", None, "weapon"));

        assert_eq!(ids(&game.items_for(Some(AXE), "head")), ["8910", "9500"]);
        assert_eq!(ids(&game.items_for(Some(AXE), "weapon")), ["9040"]);
        assert_eq!(ids(&game.items_for(None, "courier")), ["10003"]);
        assert_eq!(ids(&game.items_for(None, "weather")), ["556"]);
    }

    #[test]
    fn styles_are_read_in_order() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let styles = game.item("9500").unwrap().styles();
        let summary: Vec<(u32, Option<&str>)> = styles.iter().map(|style| (style.index, style.name_token)).collect();
        assert_eq!(summary, [(0, Some("#Style_Plain")), (1, Some("#Style_Gold"))]);
        assert_eq!(styles[1].block.str("skin").unwrap(), b"1");
        assert!(game.item("9040").unwrap().styles().is_empty());
    }

    #[test]
    fn styles_that_are_not_numbered_in_order_are_not_offered() {
        let with_styles = |styles: &str| {
            format!("\"items_game\" {{ \"items\" {{ \"1\" {{ \"visuals\" {{ \"styles\" {{ {styles} }} }} }} }} }}")
        };
        let count = |styles: &str| {
            let text = with_styles(styles);
            ItemsGame::parse(text.as_bytes()).unwrap().item("1").unwrap().styles().len()
        };
        assert_eq!(count("\"0\" { } \"1\" { }"), 2);
        for odd in ["\"1\" { } \"2\" { }", "\"0\" { } \"q\" { } \"2\" { }", "\"01\" { } \"1\" { }", "\"0\" \"text\""] {
            assert_eq!(count(odd), 0, "{odd}");
        }
    }

    #[test]
    fn slots_for_ability_effects_are_told_by_their_name() {
        assert_eq!(effects_slot("ability_effects_1"), Some(EffectsSlot { number: 1, whose: "" }));
        assert_eq!(effects_slot("Ability_Effects_12"), Some(EffectsSlot { number: 12, whose: "" }));
        assert_eq!(effects_slot("ability_effects_4_persona_1"), Some(EffectsSlot { number: 4, whose: "_persona_1" }));
        for other in ["weapon", "ability2", "ability_effects", "ability_effects_", "ability_effects_x", "ability_effects_1x", "ability_effects_1_persona_", "ab"] {
            assert_eq!(effects_slot(other), None, "{other}");
        }
        assert_eq!(effects_slot_name(4, "_Persona_1"), "ability_effects_4_persona_1");
        assert_eq!(effects_slot_name(1, ""), "ability_effects_1");

        assert_eq!(persona_of("weapon_persona_1"), "_persona_1");
        assert_eq!(persona_of("offhand_weapon_Persona_12"), "_Persona_12");
        for own in ["weapon", "persona_selector", "weapon_persona_", "weapon_persona_x", ""] {
            assert_eq!(persona_of(own), "", "{own}");
        }
    }

    #[test]
    fn an_item_says_which_ability_effects_it_has() {
        let text = effects_sample();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let item = |id: &str| game.item(id).unwrap();
        let modifiers = |pairs: Vec<kv::Pair<'_>>| -> Vec<String> {
            let named = |pair: &kv::Pair<'_>| pair.value.as_block().and_then(|block| block.str("modifier")).map(|text| String::from_utf8_lossy(text).into_owned());
            pairs.iter().filter_map(named).collect()
        };

        assert_eq!(item("200").effects_numbers(), [1], "written as a block with a value");
        assert_eq!(item("201").effects_numbers(), [4], "written as a plain value");
        assert_eq!(item("202").effects_numbers(), [1, 2], "one attribute for each slot");
        assert!(item("203").effects_numbers().is_empty());
        assert!(item("100").effects_numbers().is_empty());

        assert_eq!(modifiers(item("200").effects_for(1)), ["particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);
        assert!(item("200").effects_for(4).is_empty());
        assert_eq!(modifiers(item("202").effects_for(2)), ["particles/two_winds/ward.vpcf"]);
        assert!(item("204").effects_for(3).is_empty(), "it says it has some and marks none");
        assert!(item("100").effects_for(1).is_empty(), "no visuals at all");

        let serra = item("201");
        assert_eq!(modifiers(serra.effects_in_style(4, 0)), ["particles/serra/slash.vpcf", "Hero_Juggernaut.Slash.Serra"]);
        assert_eq!(modifiers(serra.effects_in_style(4, 1)), ["particles/serra/slash_gold.vpcf", "Hero_Juggernaut.Slash.Serra"]);
        assert_eq!(modifiers(serra.effects_in_style(4, 2)), ["Hero_Juggernaut.Slash.Serra"]);
        let styles = |id: &str, number: u32| -> Vec<u32> { item(id).effects_styles(number).iter().map(|style| style.index).collect() };
        assert_eq!(styles("201", 4), [0, 1], "the third style changes nothing about the effects");
        assert_eq!(item("201").first_effects_style(4), 0);
        assert!(styles("200", 1).is_empty(), "marked for the first style only: nothing to choose");
        assert!(styles("202", 1).is_empty());
        assert_eq!(item("200").first_effects_style(1), 0);

        assert!(item("208").effects_in_style(4, 0).is_empty());
        assert_eq!(styles("208", 4), [1]);
        assert_eq!(item("208").first_effects_style(4), 1);
    }

    #[test]
    fn ability_effects_fit_the_slot_of_their_ability() {
        let text = effects_sample();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let fits = |id: &str, slot: &str| game.fits(game.item(id).unwrap(), Some(JUGG), slot);
        assert!(fits("200", "ability_effects_1"));
        assert!(fits("200", "Ability_Effects_1"));
        assert!(fits("200", "weapon"), "and the item is still one to wear");
        assert!(!fits("200", "ability_effects_4"), "its effects are for another ability");
        assert!(fits("202", "ability_effects_1") && fits("202", "ability_effects_2"));
        assert!(!fits("203", "ability_effects_1"), "it marks effects but cannot go in the slot");
        assert!(!fits("204", "ability_effects_3"), "it can go in the slot but marks no effects");
        assert!(!fits("205", "ability_effects_1"), "the persona's item is not for the hero's own slot");
        assert!(fits("205", "ability_effects_1_persona_1"));
        assert!(!fits("200", "ability_effects_1_persona_1"));
        assert!(!fits("206", "ability_effects_4"), "where it is worn the hero has no default item to carry its effects");
        assert!(!fits("301", "ability_effects_1"), "another hero's");
        assert!(!fits("790", "ability_effects_1"), "the game's own default for the slot is not something to pick");
        assert!(!game.fits(game.item("200").unwrap(), None, "ability_effects_1"), "not a world slot");

        assert_eq!(ids(&game.items_for(Some(JUGG), "ability_effects_1")), ["200", "202"]);
        assert_eq!(ids(&game.items_for(Some(JUGG), "ability_effects_2")), ["202", "207"]);
        assert_eq!(ids(&game.items_for(Some(JUGG), "ability_effects_4")), ["201", "208"]);
        assert_eq!(
            game.effects_slots(JUGG),
            ["ability_effects_1", "ability_effects_2", "ability_effects_4", "ability_effects_1_persona_1"],
            "the hero's own first, by number"
        );
        assert_eq!(game.effects_slots("npc_dota_hero_lina"), ["ability_effects_1"]);
        assert!(game.effects_slots("npc_dota_hero_nobody").is_empty());

        assert_eq!(game.carrier(JUGG, game.item("200").unwrap()).unwrap().id, "100");
        assert_eq!(game.carrier(JUGG, game.item("202").unwrap()).unwrap().id, "101");
        assert_eq!(game.carrier(JUGG, game.item("205").unwrap()).unwrap().id, "102");
        assert!(game.carrier(JUGG, game.item("206").unwrap()).is_none());
        assert!(game.default_item(Some(JUGG), "ability_effects_1").is_none());
        let slots: Vec<&str> = game.hero_slots(JUGG).iter().map(|(slot, _)| *slot).collect();
        assert_eq!(slots, ["weapon", "head", "weapon_persona_1"]);
    }

    #[test]
    fn rarities_are_read_with_their_order_names_and_colours() {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        let def = |id, rank, name_token, color| RarityDef { id, rank, name_token: Some(name_token), color };
        assert_eq!(
            game.rarities(),
            [
                def("common", 1, "Rarity_Common", Some([0xb0, 0xc3, 0xd9])),
                def("mythical", 4, "Rarity_Mythical", Some([0x88, 0x47, 0xff])),
                def("legendary", 5, "Rarity_Legendary", None),
                def("immortal", 7, "Rarity_Immortal", Some([0xe4, 0xae, 0x39])),
            ],
            "the commonest first; a colour that is not in the file is no colour"
        );

        let rarity = |id: &str| game.rarity(game.item(id).unwrap());
        assert_eq!(rarity("9040"), Some("mythical"), "the item's own");
        assert_eq!(rarity("8910"), Some("common"), "from its prefab");
        assert_eq!(rarity("10003"), Some("immortal"));
        assert_eq!(rarity("2"), None, "neither the item nor its prefab says");

        assert_eq!(game.set("armor_of_the_vanguard").unwrap().bundle, Some("Armor of the Vanguard"));
        assert_eq!(game.set("crested_set").unwrap().bundle, None);

        let bare = ItemsGame::parse(b"\"items_game\" { \"items\" { \"1\" { } } }").unwrap();
        assert!(bare.rarities().is_empty());
    }

    #[test]
    fn effects_to_attach_are_read() {
        let game = ItemsGame::parse(PACKS.as_bytes()).unwrap();
        let effects: Vec<(u32, &str)> = game.attached_effects().iter().map(|effect| (effect.id, effect.system)).collect();
        assert_eq!(
            effects,
            [
                (15, "particles/econ/courier/courier_trail_01/courier_trail_01.vpcf"),
                (21, "particles/econ/Courier/courier_trail_03/courier_trail_03.vpcf"),
                (826, "particles/econ/events/fall_2021/frostbloom.vpcf"),
            ],
            "in the file's order; one without a number or without a particle system is left out"
        );
        assert!(game.attached_effect(15).unwrap().is_for_couriers());
        assert!(game.attached_effect(21).unwrap().is_for_couriers(), "however it is spelled");
        assert!(!game.attached_effect(826).unwrap().is_for_couriers());
        assert!(game.attached_effect(900).is_none());

        let without = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        assert!(without.attached_effects().is_empty());
    }

    #[test]
    fn an_item_says_what_sits_in_the_sockets_it_comes_with() {
        let text = gems_sample();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let sockets = |id: &str| -> Vec<(String, String)> {
            let told = |pair: &kv::Pair<'_>| {
                let held = match pair.value {
                    Value::Str(held) => held,
                    Value::Block(block) => block.str("value").unwrap(),
                };
                (String::from_utf8_lossy(pair.key).into_owned(), String::from_utf8_lossy(held).into_owned())
            };
            game.item(id).unwrap().static_sockets().iter().map(told).collect()
        };
        let named = |key: &str, held: &str| (key.to_string(), held.to_string());
        assert_eq!(sockets("20"), [named("static socket 1", "socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 91")]);
        let twin: Vec<String> = sockets("21").into_iter().map(|(key, _)| key).collect();
        assert_eq!(twin, ["static socket 2", "static socket 1"]);
        assert_eq!(sockets("3").len(), 1, "however its name is spelled");
        assert_eq!(sockets("23").len(), 5, "written as plain values");
        assert!(sockets("2").is_empty(), "static attributes, and no socket among them");
        assert!(sockets("1").is_empty() && sockets("3001").is_empty());

        assert_eq!(socket_number(b"static socket 1"), Some(1));
        assert_eq!(socket_number(b"Static Socket 12"), Some(12));
        for other in ["static socket", "static socket ", "static socket x", "static socket 1 ", "static socket -1", "socket 1", "gem type", ""] {
            assert_eq!(socket_number(other.as_bytes()), None, "{other:?}");
        }
        assert_eq!(socket_name(3), "static socket 3");
        assert_eq!(socket_number(socket_name(5).as_bytes()), Some(5));
        assert_eq!(SOCKETS.into_iter().collect::<Vec<_>>(), [1, 2, 3, 4, 5]);
    }

    #[test]
    fn the_items_to_pick_are_counted_for_each_slot_that_has_any() {
        let text = gems_sample();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let mut found: Vec<((String, String), usize)> = game.items_to_pick().into_iter().collect();
        found.sort();
        let necro = |slot: &str, items: usize| (("npc_dota_hero_necrolyte".to_string(), slot.to_string()), items);
        assert_eq!(found, [necro("back", 2), necro("head", 1), necro("weapon", 2)], "nothing goes in his belt, and a courier is no hero's");
        for ((hero, slot), items) in found {
            assert_eq!(game.items_for(Some(&hero), &slot).len(), items, "{slot}");
        }
    }

    #[test]
    fn the_default_items_of_all_heroes_are_listed_once_each() {
        let game = ItemsGame::parse(PACKS.as_bytes()).unwrap();
        let found: Vec<(&str, &str)> = game.hero_default_items().iter().map(|(hero, item)| (*hero, item.id)).collect();
        assert_eq!(found, [("npc_dota_hero_axe", "1"), ("npc_dota_hero_axe", "2"), ("npc_dota_hero_lina", "3")], "in the file's order, and no world item");

        let shared = r#""items_game" { "prefabs" { "default_item" { "item_slot" "weapon" "baseitem" "1" } } "items" {
            "1" { "name" "A Shared Sword" "prefab" "default_item" "used_by_heroes" { "npc_dota_hero_sven" "1" "npc_dota_hero_kunkka" "1" } } } }"#;
        let game = ItemsGame::parse(shared.as_bytes()).unwrap();
        let found: Vec<(&str, &str)> = game.hero_default_items().iter().map(|(hero, item)| (*hero, item.id)).collect();
        assert_eq!(found, [("npc_dota_hero_kunkka", "1")]);
    }

    #[test]
    fn colours_are_six_hex_digits() {
        assert_eq!(hex_color("#8847ff"), Some([0x88, 0x47, 0xff]));
        assert_eq!(hex_color(" #FFF34F "), Some([0xff, 0xf3, 0x4f]));
        for odd in ["8847ff", "#8847f", "#8847fff", "#88g7ff", "", "#\u{e9}\u{e9}\u{e9}"] {
            assert_eq!(hex_color(odd), None, "{odd}");
        }
    }

    #[test]
    fn hero_names_are_normalised() {
        assert_eq!(hero_id("axe"), "npc_dota_hero_axe");
        assert_eq!(hero_id(" Crystal Maiden "), "npc_dota_hero_crystal_maiden");
        assert_eq!(hero_id("npc_dota_hero_axe"), "npc_dota_hero_axe");
        assert_eq!(hero_short_name("npc_dota_hero_axe"), "axe");
    }

    #[test]
    fn files_that_are_not_an_item_database_are_refused() {
        assert!(ItemsGame::parse(b"").is_err());
        assert!(ItemsGame::parse(b"\"something_else\" { }").is_err());
        assert!(ItemsGame::parse(b"\"items_game\" { \"items\" { } }").is_err());
        assert!(ItemsGame::parse(b"\"items_game\" { \"items\" { \"1\" { ").is_err());
    }
}
