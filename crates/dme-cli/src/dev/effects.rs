use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use dme_core::dota::DotaInstall;
use dme_core::items::{self, Item, ItemsGame};
use dme_core::kv::{self, Block, Value};
use dme_core::vpk::Archive;
use dme_core::{catalog, pictures};

use super::{Counter, ITEMS_GAME};

const ATTRIBUTE: &str = "can_equip_as_ability_effects";
const MARK: &str = "apply_when_equipped_in_ability_effects_slot";
const SLOT_PREFIX: &str = "ability_effects_";
const HERO_FILES: &str = "scripts/npc/heroes/";

#[derive(Default)]
struct Replacers<'a> {
    marks: BTreeSet<String>,
    items: BTreeSet<&'a str>,
}

struct HeroSlot {
    hero: String,
    slot: String,
    text: String,
    shown: String,
}

pub fn effects(dota: &DotaInstall) -> Result<()> {
    let archive = Archive::open(&dota.game_archive())?;
    let bytes = archive.read(ITEMS_GAME)?.with_context(|| format!("{ITEMS_GAME} is not in the game's archive"))?;
    let game = ItemsGame::parse(&bytes)?;
    let names = catalog::read_names(&archive);
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

    let (hero_slots, hero_files) = hero_slots(&archive)?;
    let mut slot_names = Counter::default();
    let mut shown = Counter::default();
    let (mut named, mut with_icon) = (0, 0);
    let mut unnamed = Vec::new();
    for slot in &hero_slots {
        slot_names.add(slot.slot.as_bytes());
        shown.add(slot.shown.as_bytes());
        let name = names.get(&format!("DOTA_Tooltip_ability_{}", slot.text));
        named += usize::from(name.is_some());
        with_icon += usize::from(archive.contains(&pictures::archive_path(&format!("spellicons/{}", slot.text))));
        if name.is_none() && unnamed.len() < 8 {
            unnamed.push(format!("{} {} {:?}", items::hero_short_name(&slot.hero), slot.slot, slot.text));
        }
    }
    let heroes_with: BTreeSet<&str> = hero_slots.iter().map(|slot| slot.hero.as_str()).collect();
    println!("{} hero files; {} heroes have slots for ability effects, {} slots in all", hero_files, heroes_with.len(), hero_slots.len());
    slot_names.print("the slots");
    shown.print("their DisplayInLoadout");
    println!("\nslots whose text is the name of an ability with a name to show: {named}; with an icon in spellicons/: {with_icon}");
    for line in unnamed {
        println!("  no name: {line}");
    }

    println!("\ndefault items of those slots:");
    for item in game.items().iter().filter(|item| item.is_default()) {
        let Some(slot) = game.slot(item).filter(|slot| slot.starts_with(SLOT_PREFIX)) else { continue };
        let keys: Vec<String> = item.block.pairs().map(|pair| text(pair.key)).collect();
        let used_by: Vec<String> = item.block.block("used_by_heroes").map(|heroes| heroes.pairs().map(|pair| text(pair.key)).collect()).unwrap_or_default();
        println!("  {:>5}  {slot:<28} used by {used_by:?}, numbers {:?}, keys: {}", item.id, numbers(item), keys.join(" "));
    }

    let mut by_number = Counter::default();
    let mut by_numbers = Counter::default();
    let mut by_rarity = Counter::default();
    let mut by_worn_slot = Counter::default();
    let mut by_heroes = Counter::default();
    let mut by_prefab = Counter::default();
    let (mut wearable, mut pickable_now, mut with_styles, mut no_marks, mut later_styles_only) = (0, 0, 0, Vec::new(), Vec::new());
    let mut offered: BTreeMap<(String, String, bool), Vec<&Item<'_>>> = BTreeMap::new();
    let mut for_personas = Vec::new();
    for item in game.items().iter().filter(|item| !item.is_default()) {
        let numbers = numbers(item);
        if numbers.is_empty() {
            continue;
        }
        wearable += 1;
        by_numbers.add(format!("{} numbers", numbers.len()).as_bytes());
        by_rarity.add(game.rarity(item).unwrap_or("(none)").as_bytes());
        by_prefab.add(item.prefab.unwrap_or("(none)").as_bytes());
        by_heroes.add(format!("{} heroes", item.heroes.len()).as_bytes());
        let worn = game.slot(item).unwrap_or_else(|| "(none)".to_string());
        by_worn_slot.add(worn.as_bytes());
        with_styles += usize::from(item.styles().len() > 1);
        pickable_now += usize::from(item.heroes.iter().any(|hero| game.default_item(Some(hero), &worn).is_some()));
        let persona = worn.contains("persona");
        if persona {
            for_personas.push(format!("{} {:?} of {:?}, worn in {worn}, numbers {numbers:?}", item.id, item.name.unwrap_or("?"), item.heroes));
        }
        for n in &numbers {
            by_number.add(n.as_bytes());
            let for_n: Vec<_> = marked(item).filter(|(_, mark)| text(mark) == *n).collect();
            if for_n.is_empty() {
                no_marks.push(format!("{} {:?} (number {n})", item.id, item.name.unwrap_or("?")));
            } else if for_n.iter().all(|(block, _)| block.str("style").is_some_and(|style| style != b"0" && style.iter().all(u8::is_ascii_digit))) {
                later_styles_only.push(format!("{} {:?} (number {n})", item.id, item.name.unwrap_or("?")));
            }
            for hero in &item.heroes {
                offered.entry((hero.to_ascii_lowercase(), n.clone(), persona)).or_default().push(item);
            }
        }
    }
    println!("\n{wearable} items can be worn for their ability effects alone");
    by_numbers.print("by how many slots they can go in");
    by_number.print("by the number of the slot");
    by_rarity.print("by rarity");
    by_prefab.print("by prefab");
    by_heroes.print("by how many heroes use them");
    by_worn_slot.print("by the slot they are worn in as a whole");
    println!("\nworn as a whole in a slot that has a default item: {pickable_now}; with styles to choose from: {with_styles}");
    println!("worn as a whole by a persona: {}", for_personas.len());
    for_personas.iter().for_each(|line| println!("  {line}"));
    let persona_slots: BTreeSet<String> =
        hero_slots.iter().filter(|slot| slot.slot.contains("persona")).map(|slot| format!("{} {} {:?}", items::hero_short_name(&slot.hero), slot.slot, slot.text)).collect();
    println!("slots for a persona in the heroes' files: {}", persona_slots.len());
    persona_slots.iter().for_each(|line| println!("  {line}"));
    println!("a number with nothing marked for it: {}", no_marks.len());
    for line in no_marks.iter().take(8) {
        println!("  {line}");
    }
    println!("a number with nothing marked for the first style: {}", later_styles_only.len());
    for line in later_styles_only.iter().take(8) {
        println!("  {line}");
    }

    let slot_name = |n: &str, persona: bool| format!("{SLOT_PREFIX}{n}{}", if persona { "_persona_1" } else { "" });
    let slot_of = |hero: &str, n: &str, persona: bool| hero_slots.iter().find(|slot| slot.hero == hero && slot.slot == slot_name(n, persona));
    let mut sizes = Counter::default();
    let (mut no_slot, mut icon_agrees, mut icon_differs) = (Vec::new(), 0, Vec::new());
    for ((hero, n, persona), items) in &offered {
        sizes.add(format!("{} items", items.len()).as_bytes());
        let Some(slot) = slot_of(hero, n, *persona) else {
            no_slot.push(format!("{} {}: {} items, such as {}", items::hero_short_name(hero), slot_name(n, *persona), items.len(), items[0].id));
            continue;
        };
        for item in items {
            let icons: Vec<String> = marked(item)
                .filter(|(modifier, mark)| text(mark) == *n && modifier.str("type").is_some_and(|kind| kind.eq_ignore_ascii_case(b"ability_icon")))
                .filter_map(|(modifier, _)| modifier.str("asset").map(text))
                .collect();
            if icons.is_empty() {
                continue;
            }
            if icons.iter().any(|icon| icon.eq_ignore_ascii_case(&slot.text)) {
                icon_agrees += 1;
            } else if icon_differs.len() < 8 {
                icon_differs.push(format!("{} {}: the slot is {:?}, the item's icons are for {icons:?}", items::hero_short_name(hero), item.id, slot.text));
            }
        }
    }
    let filled: BTreeSet<(String, String)> = offered.keys().map(|(hero, n, persona)| (hero.clone(), slot_name(n, *persona))).collect();
    let empty: Vec<String> = hero_slots
        .iter()
        .filter(|slot| !filled.contains(&(slot.hero.clone(), slot.slot.clone())))
        .map(|slot| format!("{} {}", items::hero_short_name(&slot.hero), slot.slot))
        .collect();
    println!("\n{} slots of heroes have items", offered.len());
    sizes.print("how many items a hero has for one number");
    println!("\nwithout such a slot in the hero's file: {}", no_slot.len());
    for line in no_slot.iter().take(12) {
        println!("  {line}");
    }
    println!("slots in a hero's file with no item for them: {}", empty.len());
    for line in empty.iter().take(12) {
        println!("  {line}");
    }
    println!("items whose marked icon is for the ability the slot names: {icon_agrees}; for another: {}", icon_differs.len());
    for line in &icon_differs {
        println!("  {line}");
    }

    let (mut slots, mut choices, mut heroes) = (0, 0, BTreeSet::new());
    for slot in &hero_slots {
        let n = slot.slot[SLOT_PREFIX.len()..].split('_').next().unwrap_or_default();
        let items = offered.get(&(slot.hero.clone(), n.to_string(), slot.slot.contains("persona")));
        let usable = items.into_iter().flatten().filter(|item| marked(item).any(|(_, mark)| text(mark) == n)).count();
        if usable > 0 {
            slots += 1;
            choices += usable;
            heroes.insert(slot.hero.as_str());
        }
    }
    println!("\nslots of the heroes' files with an item that has something marked for them: {slots}, of {} heroes, with {choices} choices in all", heroes.len());

    let mut mark_values = Counter::default();
    let mut types = Counter::default();
    let mut keys = Counter::default();
    let mut styles = Counter::default();
    let mut families = Counter::default();
    let mut family_examples: BTreeMap<String, String> = BTreeMap::new();
    let mut block_names = Counter::default();
    let (mut other_number, mut no_attribute, mut styled_items) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    let mut replaced: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    let mut by_hero: BTreeMap<(String, String, String), Replacers<'_>> = BTreeMap::new();
    for item in game.items() {
        let own = numbers(item);
        if let Some(visuals) = item.block.block("visuals") {
            for pair in visuals.pairs().filter(|pair| pair.value.as_block().is_some_and(|block| block.get("type").is_some())) {
                block_names.add(text(pair.key).trim_end_matches(|c: char| c.is_ascii_digit()).as_bytes());
            }
        }
        for (modifier, mark) in marked(item) {
            let mark = text(mark);
            mark_values.add(mark.as_bytes());
            let kind = modifier.str("type").map(text).unwrap_or_else(|| "(no type)".to_string()).to_ascii_lowercase();
            types.add(kind.as_bytes());
            modifier.pairs().for_each(|pair| keys.add(pair.key));
            if let Some(style) = modifier.str("style") {
                styles.add(style);
                if style != b"0" {
                    styled_items.insert(item.id);
                }
            }
            if own.is_empty() {
                no_attribute.insert(format!("{} {:?} (marked {mark})", item.id, item.name.unwrap_or("?")));
            } else if !own.contains(&mark) {
                other_number.insert(format!("{} {:?}: numbers {own:?}, marked {mark}", item.id, item.name.unwrap_or("?")));
            }
            let asset = modifier.str("asset").map(text).unwrap_or_default().to_ascii_lowercase().replace('\\', "/");
            let family = family(&kind, &asset);
            family_examples.entry(family.clone()).or_insert_with(|| format!("item {} replaces {asset}", item.id));
            families.add(family.as_bytes());
            for hero in &item.heroes {
                let hero = items::hero_short_name(hero).to_string();
                replaced.entry((mark.clone(), kind.clone(), asset.clone())).or_default().insert(hero.clone());
                let replacers = by_hero.entry((hero, kind.clone(), asset.clone())).or_default();
                replacers.marks.insert(mark.clone());
                replacers.items.insert(item.id);
            }
        }
    }
    mark_values.print("what is marked, by the number it is marked with");
    types.print("by type");
    keys.print("keys of what is marked");
    styles.print("values of \"style\" on what is marked");
    block_names.print("names of the blocks of visuals that have a type, digits at the end left off (all items)");
    println!("\nitems with something marked for a style other than the first: {}", styled_items.len());
    println!("marked with a number the item does not have: {}", other_number.len());
    other_number.iter().take(8).for_each(|line| println!("  {line}"));
    println!("marked on an item that cannot be worn for its effects alone: {}", no_attribute.len());
    no_attribute.iter().take(8).for_each(|line| println!("  {line}"));

    let twice: Vec<_> = by_hero.iter().filter(|((_, _, asset), by)| by.marks.len() > 1 && !asset.is_empty()).collect();
    println!("\nthe same thing of one hero replaced under more than one number: {}", twice.len());
    for ((hero, kind, asset), by) in twice.iter().take(12) {
        println!("  {hero} {kind} {asset}: numbers {:?}, items {:?}", by.marks, by.items);
    }

    let mut meets = BTreeSet::new();
    for item in game.items().iter().filter(|item| !item.is_default()) {
        let Some(visuals) = item.block.block("visuals") else { continue };
        for modifier in visuals.pairs().filter_map(|pair| pair.value.as_block()).filter(|block| block.get(MARK).is_none()) {
            let kind = modifier.str("type").map(text).unwrap_or_default().to_ascii_lowercase();
            let asset = modifier.str("asset").map(text).unwrap_or_default().to_ascii_lowercase().replace('\\', "/");
            if asset.is_empty() {
                continue;
            }
            for hero in &item.heroes {
                let key = (items::hero_short_name(hero).to_string(), kind.clone(), asset.clone());
                if by_hero.get(&key).is_some_and(|by| by.items.iter().any(|other| *other != item.id)) {
                    meets.insert((key, item.id));
                }
            }
        }
    }
    println!("\nan unmarked block of one item replaces what a marked block of another item of the hero replaces: {}", meets.len());
    for ((hero, kind, asset), item) in meets.iter().take(10) {
        println!("  {hero} {kind} {asset}: unmarked on item {item}");
    }

    println!("\nwhat the marked blocks replace, by where it is:");
    let mut rows: Vec<(&String, &usize)> = families.0.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (family, count) in rows {
        println!("  {count:>5}  {family:<44} {}", family_examples[family]);
    }

    let shared: Vec<_> = replaced.iter().filter(|(_, heroes)| heroes.len() > 1).collect();
    println!("\nthe same thing replaced, for the same number, by items of more than one hero: {}", shared.len());
    for ((n, kind, asset), heroes) in shared.iter().take(30) {
        println!("  number {n} {kind} {asset}: {}", heroes.iter().cloned().collect::<Vec<_>>().join(", "));
    }
    Ok(())
}

fn numbers(item: &Item<'_>) -> Vec<String> {
    let attributes = item.block.block("static_attributes");
    let values = attributes.into_iter().flat_map(|attributes| attributes.pairs()).filter(|pair| pair.key.eq_ignore_ascii_case(ATTRIBUTE.as_bytes()));
    values
        .filter_map(|pair| match pair.value {
            Value::Str(value) => Some(value),
            Value::Block(block) => block.str("value"),
        })
        .map(|value| String::from_utf8_lossy(value).trim().to_string())
        .collect()
}

fn marked<'a>(item: &Item<'a>) -> impl Iterator<Item = (Block<'a>, &'a [u8])> {
    let visuals = item.block.block("visuals");
    visuals.into_iter().flat_map(|visuals| visuals.pairs()).filter_map(|pair| {
        let block = pair.value.as_block()?;
        Some((block, block.str(MARK)?))
    })
}

fn family(kind: &str, asset: &str) -> String {
    if !asset.contains('/') {
        return format!("({kind})");
    }
    let folders: Vec<&str> = asset.split('/').collect();
    folders[..folders.len().saturating_sub(1).min(3)].join("/")
}

fn hero_slots(archive: &Archive) -> Result<(Vec<HeroSlot>, usize)> {
    let mut found = Vec::new();
    let mut files = 0;
    let mut paths: Vec<String> = archive.entries().iter().map(|entry| archive.path_of(entry)).filter(|path| path.starts_with(HERO_FILES)).collect();
    paths.sort();
    for path in paths {
        let Some(bytes) = archive.read(&path)? else { continue };
        if let Err(error) = kv::validate(&bytes, &path) {
            println!("skipped: {error}");
            continue;
        }
        files += 1;
        let Some(heroes) = Block::document(&bytes).block("DOTAHeroes") else { continue };
        for hero in heroes.pairs() {
            let Some(slots) = hero.value.as_block().and_then(|hero| hero.block("ItemSlots")) else { continue };
            for slot in slots.pairs().filter_map(|pair| pair.value.as_block()) {
                let read = |key: &str| slot.str(key).map(|value| String::from_utf8_lossy(value).into_owned());
                let Some(name) = read("SlotName").map(|name| name.to_ascii_lowercase()).filter(|name| name.starts_with(SLOT_PREFIX)) else { continue };
                found.push(HeroSlot {
                    hero: String::from_utf8_lossy(hero.key).to_ascii_lowercase(),
                    slot: name,
                    text: read("SlotText").unwrap_or_default(),
                    shown: read("DisplayInLoadout").unwrap_or_else(|| "(not said)".to_string()),
                });
            }
        }
    }
    Ok((found, files))
}
