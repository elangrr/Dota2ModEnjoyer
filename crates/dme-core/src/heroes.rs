use std::collections::HashMap;

use crate::items;
use crate::kv::{self, Block};
use crate::vpk::Archive;

pub const HERO_FILES: &str = "scripts/npc/heroes/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeroSlot {
    pub slot: String,
    pub text: String,
    pub shown: bool,
}

impl HeroSlot {
    pub fn name_token(&self) -> Option<&str> {
        self.text.strip_prefix('#').filter(|token| !token.is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HeroFile {
    pub slots: Vec<HeroSlot>,
    pub aliases: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilitySlot<'f> {
    pub slot: &'f str,
    pub ability: &'f str,
}

#[derive(Debug, Default)]
pub struct HeroFiles {
    by_hero: HashMap<String, HeroFile>,
}

impl HeroFiles {
    pub fn read(archive: &Archive) -> HeroFiles {
        let mut files = HeroFiles::default();
        let mut paths: Vec<String> =
            archive.entries().iter().map(|entry| archive.path_of(entry)).filter(|path| path.to_ascii_lowercase().starts_with(HERO_FILES)).collect();
        paths.sort();
        for path in paths {
            if let Ok(Some(file)) = archive.read(&path) {
                files.add(&file);
            }
        }
        files
    }

    pub fn add(&mut self, file: &[u8]) {
        if kv::validate(file, "a hero's file").is_err() {
            return;
        }
        let Some(heroes) = Block::document(file).block("DOTAHeroes") else { return };
        for hero in heroes.pairs() {
            let Some(body) = hero.value.as_block() else { continue };
            let Some(listed) = body.block("ItemSlots") else { continue };
            let read = |slot: Block<'_>| {
                let text = |key: &str| slot.str(key).and_then(|value| std::str::from_utf8(value).ok());
                Some(HeroSlot {
                    slot: text("SlotName")?.trim().to_ascii_lowercase(),
                    text: text("SlotText").unwrap_or_default().trim().to_string(),
                    shown: text("DisplayInLoadout").is_none_or(|shown| shown.trim() != "0"),
                })
            };
            let slots: Vec<HeroSlot> = listed.pairs().filter_map(|pair| pair.value.as_block()).filter_map(read).collect();
            if slots.is_empty() {
                continue;
            }
            let aliases = body.str("NameAliases").and_then(|names| std::str::from_utf8(names).ok()).unwrap_or_default();
            let aliases = aliases.split([';', ',']).map(str::trim).filter(|alias| !alias.is_empty()).map(str::to_string).collect();
            self.by_hero.insert(String::from_utf8_lossy(hero.key).to_ascii_lowercase(), HeroFile { slots, aliases });
        }
    }

    pub fn of(&self, hero: &str) -> Option<&HeroFile> {
        self.by_hero.get(&hero.to_ascii_lowercase())
    }

    pub fn slot(&self, hero: &str, slot: &str) -> Option<&HeroSlot> {
        self.of(hero)?.slots.iter().find(|known| known.slot.eq_ignore_ascii_case(slot))
    }

    pub fn has_slot(&self, hero: &str, slot: &str) -> bool {
        self.of(hero).is_none_or(|file| file.slots.iter().any(|known| known.slot.eq_ignore_ascii_case(slot)))
    }

    pub fn place(&self, hero: &str, slot: &str) -> Option<usize> {
        self.of(hero)?.slots.iter().position(|known| known.slot.eq_ignore_ascii_case(slot))
    }

    pub fn ability_slots<'f>(&'f self, hero: &str) -> Vec<AbilitySlot<'f>> {
        let slots = self.of(hero).map_or(&[][..], |file| file.slots.as_slice());
        let for_an_ability = |slot: &'f HeroSlot| {
            items::effects_slot(&slot.slot)?;
            (!slot.text.is_empty() && !slot.text.starts_with('#')).then_some(AbilitySlot { slot: &slot.slot, ability: &slot.text })
        };
        slots.iter().filter_map(for_an_ability).collect()
    }

    pub fn ability(&self, hero: &str, slot: &str) -> Option<&str> {
        self.ability_slots(hero).into_iter().find(|known| known.slot.eq_ignore_ascii_case(slot)).map(|known| known.ability)
    }

    pub fn aliases(&self, hero: &str) -> &[String] {
        self.of(hero).map_or(&[], |file| file.aliases.as_slice())
    }

    pub fn len(&self) -> usize {
        self.by_hero.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hero.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const JUGGERNAUT_FILE: &str = r##"
"DOTAHeroes"
{
	"npc_dota_hero_juggernaut"
	{
		"Model"		"models/heroes/juggernaut/juggernaut.vmdl"
		"NameAliases"		"Yurnero; jugg ,"
		"ItemSlots"
		{
			"0"
			{
				"SlotIndex"		"0"
				"SlotName"		"weapon"
				"SlotText"		"#LoadoutSlot_Weapon"
			}
			"1"
			{
				"SlotIndex"		"1"
				"SlotName"		"Head"
				"SlotText"		"#LoadoutSlot_Juggernaut_Mask"
			}
			"2"
			{
				"SlotIndex"		"2"
				"SlotName"		"weapon_persona_1"
				"SlotText"		"#LoadoutSlot_Weapon_Juggernaut_Persona"
			}
			"9"
			{
				"SlotIndex" "9"
				"SlotName" "ability_effects_1"
				"SlotText" "juggernaut_blade_fury"
				"DisplayInLoadout" "0"
			}
			"10"
			{
				"SlotIndex" "10"
				"SlotName" "Ability_Effects_4"
				"SlotText" "juggernaut_omni_slash"
				"DisplayInLoadout" "0"
			}
			"11"
			{
				"SlotIndex" "11"
				"SlotName" "ability_effects_2"
				"SlotText" "juggernaut_healing_ward"
			}
			"12"
			{
				"SlotIndex" "12"
				"SlotName" "ability_effects_1_persona_1"
				"SlotText" "juggernaut_blade_fury"
			}
			"13"
			{
				"SlotIndex" "13"
				"SlotName" "ability_effects_3"
			}
		}
	}
}
"##;

    const JUGG: &str = "npc_dota_hero_juggernaut";

    #[test]
    fn a_heros_file_lists_its_slots() {
        let mut files = HeroFiles::default();
        files.add(JUGGERNAUT_FILE.as_bytes());
        let file = files.of("NPC_DOTA_HERO_JUGGERNAUT").unwrap();
        let slots: Vec<&str> = file.slots.iter().map(|slot| slot.slot.as_str()).collect();
        assert_eq!(
            slots,
            [
                "weapon",
                "head",
                "weapon_persona_1",
                "ability_effects_1",
                "ability_effects_4",
                "ability_effects_2",
                "ability_effects_1_persona_1",
                "ability_effects_3"
            ],
            "in the file's order, lower-cased"
        );

        let head = files.slot(JUGG, "HEAD").unwrap();
        assert_eq!(head.name_token(), Some("LoadoutSlot_Juggernaut_Mask"));
        assert!(head.shown);
        let fury = files.slot(JUGG, "ability_effects_1").unwrap();
        assert_eq!(fury.name_token(), None, "it names an ability, not itself");
        assert!(!fury.shown);
        assert_eq!(files.slot(JUGG, "ability_effects_3").unwrap().text, "");
        assert!(files.slot(JUGG, "back").is_none());

        assert_eq!(files.place(JUGG, "Weapon"), Some(0));
        assert_eq!(files.place(JUGG, "weapon_persona_1"), Some(2));
        assert_eq!(files.place(JUGG, "back"), None);
        assert!(files.has_slot(JUGG, "head") && !files.has_slot(JUGG, "back"));
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn a_heros_file_names_its_slots_for_ability_effects() {
        let mut files = HeroFiles::default();
        files.add(JUGGERNAUT_FILE.as_bytes());
        let found: Vec<(&str, &str)> = files.ability_slots(JUGG).iter().map(|slot| (slot.slot, slot.ability)).collect();
        assert_eq!(
            found,
            [
                ("ability_effects_1", "juggernaut_blade_fury"),
                ("ability_effects_4", "juggernaut_omni_slash"),
                ("ability_effects_2", "juggernaut_healing_ward"),
                ("ability_effects_1_persona_1", "juggernaut_blade_fury"),
            ],
            "in the file's order; other slots, and one that names no ability, are left out"
        );
        assert_eq!(files.ability(JUGG, "Ability_Effects_4"), Some("juggernaut_omni_slash"));
        assert_eq!(files.ability(JUGG, "weapon"), None);
        assert!(files.ability_slots("npc_dota_hero_axe").is_empty());
    }

    #[test]
    fn a_heros_file_has_the_names_the_game_finds_the_hero_by() {
        let mut files = HeroFiles::default();
        files.add(JUGGERNAUT_FILE.as_bytes());
        assert_eq!(files.aliases(JUGG), ["Yurnero", "jugg"], "separated by ; or by , and tidied");
        assert!(files.aliases("npc_dota_hero_axe").is_empty());
    }

    #[test]
    fn a_hero_without_a_file_has_whatever_slot_it_is_asked_about() {
        let files = HeroFiles::default();
        assert!(files.of("npc_dota_hero_axe").is_none());
        assert!(files.has_slot("npc_dota_hero_axe", "weapon"), "nothing says otherwise");
        assert_eq!(files.place("npc_dota_hero_axe", "weapon"), None);
    }

    #[test]
    fn files_that_are_not_a_heros_add_nothing() {
        let mut files = HeroFiles::default();
        for odd in ["", "\"DOTAHeroes\" { \"npc_dota_hero_axe\" { \"ItemSlots\" { ", "\"Something\" { }", "\"DOTAHeroes\" { \"npc_dota_hero_axe\" { } }"] {
            files.add(odd.as_bytes());
        }
        assert!(files.is_empty());
    }
}
