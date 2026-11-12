use crate::apply::Pick;
use crate::items::{Item, ItemsGame};
use crate::kv::Pair;

pub const SLOT_PREFIX: &str = "fx_";

#[derive(Debug, PartialEq, Eq)]
pub struct Kind {
    pub id: &'static str,
    pub name: &'static str,
    pub item: Option<&'static str>,
    pub assets: &'static [&'static str],
}

pub const KINDS: &[Kind] = &[
    Kind {
        id: "teleport",
        name: "Teleport",
        item: Some("item_tpscroll"),
        assets: &["particles/items2_fx/teleport_start.vpcf", "particles/items2_fx/teleport_end.vpcf", "particles/items2_fx/teleport_origin.vpcf"],
    },
    Kind {
        id: "blink",
        name: "Blink Dagger",
        item: Some("item_blink"),
        assets: &["particles/items_fx/blink_dagger_start.vpcf", "particles/items_fx/blink_dagger_end.vpcf"],
    },
    Kind { id: "attack", name: "Attack", item: None, assets: &["particles/econ/attack/attack_modifier_empty.vpcf"] },
    Kind { id: "force_staff", name: "Force Staff", item: Some("item_force_staff"), assets: &["particles/items_fx/force_staff.vpcf"] },
    Kind {
        id: "euls",
        name: "Eul's Scepter",
        item: Some("item_cyclone"),
        assets: &[
            "particles/items_fx/cyclone.vpcf",
            "particles/items_fx/generic_item_spell_caster_core_euls.vpcf",
            "particles/items_fx/generic_item_spell_caster_core_windwaker.vpcf",
        ],
    },
    Kind { id: "bottle", name: "Bottle", item: Some("item_bottle"), assets: &["particles/items_fx/bottle.vpcf"] },
    Kind {
        id: "mekansm",
        name: "Mekansm",
        item: Some("item_mekansm"),
        assets: &["particles/items2_fx/mekanism.vpcf", "particles/items2_fx/mekanism_recipient.vpcf"],
    },
    Kind {
        id: "radiance",
        name: "Radiance",
        item: Some("item_radiance"),
        assets: &["particles/items2_fx/radiance_owner.vpcf", "particles/items2_fx/radiance.vpcf"],
    },
    Kind { id: "phase_boots", name: "Phase Boots", item: Some("item_phase_boots"), assets: &["particles/items2_fx/phase_boots.vpcf"] },
    Kind {
        id: "mjollnir",
        name: "Mjollnir",
        item: Some("item_mjollnir"),
        assets: &["particles/items_fx/chain_lightning.vpcf", "particles/items2_fx/mjollnir_shield.vpcf"],
    },
    Kind {
        id: "shivas_guard",
        name: "Shiva's Guard",
        item: Some("item_shivas_guard"),
        assets: &["particles/items2_fx/shivas_guard_active.vpcf", "particles/items2_fx/shivas_guard_impact.vpcf"],
    },
    Kind { id: "scepter", name: "Aghanim's Scepter", item: Some("item_ultimate_scepter"), assets: &["particles/items4_fx/scepter_aura.vpcf"] },
    Kind { id: "dagon", name: "Dagon", item: Some("item_dagon"), assets: &["particles/items_fx/dagon.vpcf"] },
    Kind { id: "sheepstick", name: "Scythe of Vyse", item: Some("item_sheepstick"), assets: &["particles/items_fx/item_sheepstick.vpcf"] },
    Kind { id: "lotus_orb", name: "Lotus Orb", item: Some("item_lotus_orb"), assets: &["particles/items3_fx/lotus_orb_shield.vpcf"] },
    Kind { id: "level_up", name: "Level up", item: None, assets: &["particles/generic_hero_status/hero_levelup.vpcf"] },
    Kind { id: "fountain", name: "Fountain", item: None, assets: &["particles/generic_gameplay/radiant_fountain_regen.vpcf"] },
    Kind { id: "last_hit", name: "Last hit", item: None, assets: &["particles/generic_gameplay/lasthit_coins.vpcf"] },
    Kind { id: "deny", name: "Deny", item: None, assets: &["particles/msg_fx/msg_deny.vpcf"] },
];

impl Kind {
    pub fn slot(&self) -> String {
        format!("{SLOT_PREFIX}{}", self.id)
    }

    fn has(&self, asset: &[u8]) -> bool {
        self.assets.iter().any(|known| known.as_bytes().eq_ignore_ascii_case(asset))
    }
}

pub fn kind_of(slot: &str) -> Option<&'static Kind> {
    let slot = slot.to_ascii_lowercase();
    let id = slot.strip_prefix(SLOT_PREFIX)?;
    KINDS.iter().find(|kind| kind.id == id)
}

pub fn is_pack(game: &ItemsGame<'_>, item: &Item<'_>) -> bool {
    let worn = game.value(item, "item_class").is_some_and(|class| class.eq_ignore_ascii_case("dota_item_wearable"));
    let for_a_slot = || game.world_slot(item).is_some_and(|slot| SLOTS.iter().any(|known| slot.eq_ignore_ascii_case(known)));
    worn && item.heroes.is_empty() && item.block.block("visuals").is_some() && (item.has_prefab("tool") || for_a_slot())
}

const SLOTS: [&str; 2] = ["teleport_effect", "blink_effect"];

#[derive(Debug)]
pub struct Variant<'a> {
    pub blocks: Vec<Pair<'a>>,
}

pub fn variants<'a>(item: &Item<'a>, kind: &Kind) -> Vec<Variant<'a>> {
    let Some(visuals) = item.block.block("visuals") else { return Vec::new() };
    let mut levels: Vec<(Option<Condition>, Vec<Pair<'a>>)> = Vec::new();
    for pair in visuals.pairs() {
        let Some(block) = pair.value.as_block() else { continue };
        let changes_an_effect = block.str("type").is_some_and(|named| REPLACING.iter().any(|known| named.eq_ignore_ascii_case(known.as_bytes())));
        if !changes_an_effect || !block.str("asset").is_some_and(|asset| kind.has(asset)) {
            continue;
        }
        let condition = block.str("criteria").map(Condition::read);
        match levels.iter_mut().find(|(known, _)| *known == condition) {
            Some((_, blocks)) => blocks.push(pair),
            None => levels.push((condition, vec![pair])),
        }
    }
    levels.sort_by_key(|(condition, _)| condition.as_ref().map_or((0, 0), |condition| condition.range));
    levels.into_iter().map(|(_, blocks)| Variant { blocks }).collect()
}

const REPLACING: [&str; 2] = ["particle", "particle_control_point"];

#[derive(Debug, PartialEq, Eq)]
struct Condition {
    text: String,
    range: (u64, u64),
}

impl Condition {
    fn read(criteria: &[u8]) -> Condition {
        let text: String = String::from_utf8_lossy(criteria).chars().filter(|letter| !letter.is_whitespace()).collect::<String>().to_lowercase();
        let mut numbers = text.split(',').skip(1).map(|number| number.trim_end_matches(')').parse::<u64>().unwrap_or(0));
        let range = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0));
        Condition { text, range }
    }
}

pub const EMBLEM_SLOT: &str = "emblem";

pub fn is_emblem_pick(pick: &Pick) -> bool {
    pick.hero.is_none() && pick.slot.eq_ignore_ascii_case(EMBLEM_SLOT)
}

pub fn emblem_blocks<'a>(game: &ItemsGame<'a>, item: &Item<'a>) -> Vec<Pair<'a>> {
    let is_an_emblem = item.heroes.is_empty() && !game.is_base(item) && game.world_slot(item).as_deref() == Some(EMBLEM_SLOT);
    let Some(visuals) = item.block.block("visuals").filter(|_| is_an_emblem) else { return Vec::new() };
    let shows_on_a_hero = |pair: &Pair<'a>| {
        let Some(block) = pair.value.as_block() else { return false };
        let says = |key: &str, wanted: &[u8]| block.str(key).is_some_and(|value| value.trim_ascii().eq_ignore_ascii_case(wanted));
        let names_something = |key: &str| block.str(key).is_some_and(|value| !value.trim_ascii().is_empty());
        says("type", CREATING) && names_something("modifier") && !names_something("asset") && !says(LOADOUT_ONLY, b"1")
    };
    visuals.pairs().filter(shows_on_a_hero).collect()
}

const CREATING: &[u8] = b"particle_create";
const LOADOUT_ONLY: &str = "spawn_in_loadout_only";

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const PACKS: &str = r##"
"items_game"
{
	"colors"
	{
		"desc_common"
		{
			"color_name"		"ItemRarityCommon"
			"hex_color"		"#b0c3d9"
		}
		"unusual_red"
		{
			"color_name"		"UnusualRed"
			"hex_color"		"#D03D33"
		}
		"Unusual_Gold"
		{
			"hex_color"		"#CFAB31"
		}
		"unusual_odd"
		{
			"color_name"		"UnusualOdd"
			"hex_color"		"red"
		}
	}
	"attribute_controlled_attached_particles"
	{
		"15"
		{
			"system"		"particles/econ/courier/courier_trail_01/courier_trail_01.vpcf"
			"attach_type"		"absorigin_follow"
			"attach_entity"		"parent"
		}
		"21"
		{
			"system"		"particles/econ/Courier/courier_trail_03/courier_trail_03.vpcf"
			"default_color"
			{
				"r"		"255"
				"g"		"66"
				"b"		"0"
			}
		}
		"826"
		{
			"system"		"particles/econ/events/fall_2021/frostbloom.vpcf"
		}
		"900"
		{
			"attach_type"		"absorigin_follow"
		}
		"unnumbered"
		{
			"system"		"particles/econ/unnumbered.vpcf"
		}
	}
	"prefabs"
	{
		"default_item"
		{
			"item_class"		"dota_item_wearable"
			"item_slot"		"weapon"
			"baseitem"		"1"
		}
		"wearable"
		{
			"item_class"		"dota_item_wearable"
			"item_slot"		"weapon"
		}
		"tool"
		{
			"item_class"		"tool"
			"item_slot"		"none"
		}
		"courier"
		{
			"item_class"		"dota_item_wearable"
			"item_slot"		"courier"
			"player_loadout"		"1"
		}
		"teleport_effect"
		{
			"item_class"		"dota_item_wearable"
			"item_slot"		"teleport_effect"
			"player_loadout"		"1"
		}
	}
	"player_loadout_slots"
	{
		"0"		"courier"
		"1"		"teleport_effect"
	}
	"items"
	{
		"1"
		{
			"name"		"Axe's Axe"
			"prefab"		"default_item"
			"model_player"		"models/heroes/axe/axe_weapon.vmdl"
			"used_by_heroes"
			{
				"npc_dota_hero_axe"		"1"
			}
		}
		"2"
		{
			"name"		"Axe's Belt"
			"prefab"		"default_item"
			"item_slot"		"belt"
			"used_by_heroes"
			{
				"npc_dota_hero_axe"		"1"
			}
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items_fx/Blink_Dagger_Start.vpcf"
					"modifier"		"particles/axe/belt_blink.vpcf"
				}
				"asset_modifier"
				{
					"type"		"particle_create"
					"modifier"		"particles/axe/belt_glow.vpcf"
				}
			}
		}
		"3"
		{
			"name"		"Lina's Hair"
			"prefab"		"default_item"
			"item_slot"		"head"
			"used_by_heroes"
			{
				"npc_dota_hero_lina"		"1"
			}
		}
		"4"
		{
			"name"		"Default Courier"
			"prefab"		"courier"
			"baseitem"		"1"
		}
		"20"
		{
			"name"		"Red Axe"
			"prefab"		"wearable"
			"model_player"		"models/items/axe/red_axe.vmdl"
			"used_by_heroes"
			{
				"npc_dota_hero_axe"		"1"
			}
		}
		"21"
		{
			"name"		"Glowing Axe"
			"prefab"		"wearable"
			"item_rarity"		"arcana"
			"model_player"		"models/items/axe/glowing_axe.vmdl"
			"used_by_heroes"
			{
				"npc_dota_hero_axe"		"1"
			}
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"particle_create"
					"modifier"		"particles/axe/glow.vpcf"
				}
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/axe/cull.vpcf"
					"modifier"		"particles/axe/glowing_cull.vpcf"
				}
				"asset_modifier"
				{
					"type"		"particle_control_point"
					"asset"		"particles/axe/Glow.vpcf"
					"control_point_number"		"15"
					"cp_position"		"1 2 3"
				}
				"asset_modifier"
				{
					"type"		"particle_create"
					"modifier"		"particles/axe/GLOW.vpcf"
				}
				"asset_modifier"
				{
					"type"		"activity"
					"modifier"		"glowing"
				}
			}
		}
		"30"
		{
			"name"		"Drodo the Druffin"
			"prefab"		"courier"
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"courier"
					"modifier"		"models/courier/drodo/drodo.vmdl"
					"asset"		"radiant"
				}
				"asset_modifier"
				{
					"type"		"particle_create"
					"modifier"		"particles/econ/courier/courier_trail_01/courier_trail_01.vpcf"
				}
			}
		}
		"500"
		{
			"name"		"The Tournament 2019 Effects"
			"prefab"		"tool"
			"item_class"		"dota_item_wearable"
			"image_inventory"		"econ/testitem_slot_empty"
			"item_name"		"#DOTA_Item_Tournament_2019_Effects"
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_start.vpcf"
					"modifier"		"particles/events/t19/teleport_start_lvl2.vpcf"
					"criteria"		"isbetween( effects_mask, 2, 3, 3, 5 )"
				}
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_start.vpcf"
					"modifier"		"particles/events/t19/teleport_start.vpcf"
					"criteria"		"isbetween( effects_mask, 1, 1, 3, 5 )"
				}
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/Teleport_End.vpcf"
					"modifier"		"particles/events/t19/teleport_end.vpcf"
					"criteria"		"isbetween ( effects_mask , 1 , 1 , 3 , 5 )"
				}
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_end.vpcf"
					"modifier"		"particles/events/t19/teleport_end_lvl2.vpcf"
					"criteria"		"isbetween( effects_mask, 2, 3, 3, 5 )"
				}
				"asset_modifier"
				{
					"type"		"particle"
					"asset"		"particles/items_fx/blink_dagger_start.vpcf"
					"modifier"		"particles/events/t19/blink_start.vpcf"
					"criteria"		"isbetween( effects_mask, 1, 1, 6, 7 )"
				}
				"asset_modifier"
				{
					"type"		"particle_control_point"
					"asset"		"particles/msg_fx/msg_deny.vpcf"
					"control_point_number"		"4"
					"cp_position"		"30 0 0"
					"criteria"		"isbetween( effects_mask, 1, 1, 21, 21 )"
				}
				"asset_modifier"
				{
					"type"		"model"
					"asset"		"models/props/bottle.vmdl"
					"modifier"		"models/events/t19/bottle.vmdl"
				}
			}
		}
		"501"
		{
			"name"		"In-Game Effects Team"
			"prefab"		"tool"
			"item_class"		"dota_item_wearable"
			"visuals"
			{
				"asset_modifier0"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_start.vpcf"
					"modifier"		"particles/teams/team_teleport_start.vpcf"
				}
				"asset_modifier1"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_end.vpcf"
					"modifier"		"particles/teams/team_teleport_end.vpcf"
				}
			}
		}
		"502"
		{
			"name"		"A Tool That Is No Effect"
			"prefab"		"tool"
		}
		"503"
		{
			"name"		"Championship 2015 Teleport"
			"prefab"		"teleport_effect"
			"image_inventory"		"econ/tools/championship_tp"
			"visuals"
			{
				"asset_modifier2"
				{
					"type"		"particle"
					"asset"		"particles/items2_fx/teleport_start.vpcf"
					"modifier"		"particles/championship/teleport_start.vpcf"
				}
			}
		}
	}
}
"##;

    const EMBLEMS_TEXT: &str = r##"
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
        "tool"
        {
            "item_class"  "tool"
            "item_slot"  "none"
        }
        "courier"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "courier"
        }
        "emblem"
        {
            "item_class"  "dota_item_wearable"
            "item_slot"  "emblem"
            "item_rarity"  "legendary"
        }
    }
    "player_loadout_slots"
    {
        "0"  "courier"
        "1"  "emblem"
    }
    "items"
    {
        "1"
        {
            "name"  "Axe's Axe"
            "prefab"  "default_item"
            "model_player"  "models/heroes/axe/axe_weapon.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
        }
        "2"
        {
            "name"  "Axe's Hair"
            "prefab"  "default_item"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
        }
        "3"
        {
            "name"  "Axe's Ambient Effects"
            "prefab"  "default_item"
            "item_slot"  "ambient_effects"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/axe/ambient.vpcf"
                }
            }
        }
        "4"
        {
            "name"  "Axe's Pet"
            "prefab"  "default_item"
            "item_slot"  "summon"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
        }
        "5"
        {
            "name"  "Lina's Hair"
            "prefab"  "default_item"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_lina"  "1"
            }
        }
        "6"
        {
            "name"  "Lina's Belt"
            "prefab"  "default_item"
            "item_slot"  "belt"
            "used_by_heroes"
            {
                "npc_dota_hero_lina"  "1"
            }
        }
        "7"
        {
            "name"  "Sven's Sword"
            "prefab"  "default_item"
            "used_by_heroes"
            {
                "npc_dota_hero_sven"  "1"
            }
        }
        "8"
        {
            "name"  "Sven's Helm"
            "prefab"  "default_item"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_sven"  "1"
            }
        }
        "9"
        {
            "name"  "Riki's Voice"
            "prefab"  "default_item"
            "item_slot"  "voice"
            "used_by_heroes"
            {
                "npc_dota_hero_riki"  "1"
            }
        }
        "10"
        {
            "name"  "Riki's Persona Dagger"
            "prefab"  "default_item"
            "item_slot"  "weapon_persona_1"
            "used_by_heroes"
            {
                "npc_dota_hero_riki"  "1"
            }
        }
        "11"
        {
            "name"  "Default Courier"
            "prefab"  "courier"
            "baseitem"  "1"
        }
        "20"
        {
            "name"  "Red Axe"
            "prefab"  "wearable"
            "model_player"  "models/items/axe/red_axe.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
        }
        "21"
        {
            "name"  "Lina's Wig"
            "prefab"  "wearable"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_lina"  "1"
            }
        }
        "22"
        {
            "name"  "Sven's Blade"
            "prefab"  "wearable"
            "used_by_heroes"
            {
                "npc_dota_hero_sven"  "1"
            }
        }
        "23"
        {
            "name"  "Sven's Hat"
            "prefab"  "wearable"
            "item_slot"  "head"
            "used_by_heroes"
            {
                "npc_dota_hero_sven"  "1"
            }
        }
        "24"
        {
            "name"  "Sven's Other Blade"
            "prefab"  "wearable"
            "model_player"  "models/items/sven/other_blade.vmdl"
            "used_by_heroes"
            {
                "npc_dota_hero_sven"  "1"
            }
        }
        "30"
        {
            "name"  "A Courier With A Glow"
            "prefab"  "courier"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/courier/glow.vpcf"
                }
            }
        }
        "500"
        {
            "name"  "The Tournament 2019 Effects"
            "prefab"  "tool"
            "item_class"  "dota_item_wearable"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle"
                    "asset"  "particles/items2_fx/teleport_start.vpcf"
                    "modifier"  "particles/events/t19/teleport_start.vpcf"
                }
            }
        }
        "638"
        {
            "name"  "Sunken Emblem"
            "prefab"  "emblem"
            "image_inventory"  "econ/tools/sunken_emblem"
            "item_name"  "#DOTA_Item_Sunken_Emblem"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/events/t17/hero_effect.vpcf"
                }
            }
        }
        "663"
        {
            "name"  "Emblem of the Crystal Echelon"
            "prefab"  "emblem"
            "image_inventory"  "econ/tools/crystal_emblem"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "Particle_Create"
                    "modifier"  "particles/events/t18/hero_effect.vpcf"
                    "spawn_in_loadout_only"  "2"
                }
                "asset_modifier"
                {
                    "type"  "diretide_effect"
                    "asset"  "emblem_attack_effect"
                    "modifier"  "particles/events/t18/attack.vpcf"
                    "spawn_in_loadout_only"  "2"
                }
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "asset"  "npc_dota_loadout_generic"
                    "modifier"  "particles/events/t18/loadout_ambient.vpcf"
                }
                "asset_modifier"
                {
                    "type"  "entity_scale"
                    "asset"  "npc_dota_loadout_generic"
                    "scale_size"  "1"
                }
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/events/t18/loadout_effect.vpcf"
                    "spawn_in_loadout_only"  "1"
                }
            }
        }
        "664"
        {
            "name"  "Emblem of the Loadout"
            "prefab"  "emblem"
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "asset"  "npc_dota_loadout_generic"
                    "modifier"  "particles/events/t18/loadout_ambient.vpcf"
                }
            }
        }
        "665"
        {
            "name"  "A Hero's Emblem"
            "prefab"  "emblem"
            "used_by_heroes"
            {
                "npc_dota_hero_axe"  "1"
            }
            "visuals"
            {
                "asset_modifier"
                {
                    "type"  "particle_create"
                    "modifier"  "particles/events/t17/hero_effect.vpcf"
                }
            }
        }
    }
}
"##;

    pub(crate) fn emblems() -> String {
        crate::items::tests::laid_out(EMBLEMS_TEXT, "\n")
    }

    fn modifiers(variant: &Variant<'_>) -> Vec<String> {
        let named = |pair: &Pair<'_>| {
            let block = pair.value.as_block().unwrap();
            String::from_utf8_lossy(block.str("modifier").or(block.str("cp_position")).unwrap()).into_owned()
        };
        variant.blocks.iter().map(named).collect()
    }

    #[test]
    fn what_of_an_emblem_shows_on_a_hero_is_told_from_the_rest() {
        let text = emblems();
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        let of = |id: &str| -> Vec<String> {
            let named = |pair: &Pair<'_>| String::from_utf8_lossy(pair.value.as_block().unwrap().str("modifier").unwrap()).into_owned();
            emblem_blocks(&game, game.item(id).unwrap()).iter().map(named).collect()
        };
        assert_eq!(of("638"), ["particles/events/t17/hero_effect.vpcf"]);
        assert_eq!(of("663"), ["particles/events/t18/hero_effect.vpcf"], "however its type is spelled");
        assert!(of("664").is_empty(), "nothing of it shows in a game");
        assert!(of("665").is_empty(), "an item of a hero's is no emblem");
        assert!(of("30").is_empty(), "a courier creates an effect too");
        assert!(of("500").is_empty() && of("3").is_empty());

        assert!(is_emblem_pick(&Pick::world("emblem", "638")));
        assert!(is_emblem_pick(&Pick::world("Emblem", "1")));
        assert!(!is_emblem_pick(&Pick::world("courier", "638")));
        assert!(!is_emblem_pick(&Pick::hero("npc_dota_hero_axe", "emblem", "638")), "no hero's slot");
    }

    #[test]
    fn a_slot_for_an_events_effects_is_told_by_its_name() {
        assert_eq!(kind_of("fx_teleport").unwrap().id, "teleport");
        assert_eq!(kind_of("FX_Blink").unwrap().name, "Blink Dagger");
        assert_eq!(kind_of("fx_blink").unwrap().slot(), "fx_blink");
        for other in ["teleport", "fx_", "fx_nothing", "courier", ""] {
            assert!(kind_of(other).is_none(), "{other}");
        }
        for (place, kind) in KINDS.iter().enumerate() {
            assert!(!kind.assets.is_empty(), "{}", kind.id);
            assert!(KINDS[..place].iter().all(|other| other.id != kind.id), "{}", kind.id);
            assert_eq!(kind_of(&kind.slot()), Some(kind));
        }
    }

    #[test]
    fn an_events_item_is_told_from_other_tools() {
        let game = ItemsGame::parse(PACKS.as_bytes()).unwrap();
        let is = |id: &str| is_pack(&game, game.item(id).unwrap());
        assert!(is("500") && is("501"));
        assert!(is("503"), "made for the game's own slot for such effects");
        assert!(!is("502"), "a tool, and nothing that is worn");
        assert!(!is("20"), "a hero's item");
        assert!(!is("4"), "a courier");
    }

    #[test]
    fn an_events_effects_come_in_the_levels_it_has() {
        let game = ItemsGame::parse(PACKS.as_bytes()).unwrap();
        let of = |id: &str, kind: &str| -> Vec<Vec<String>> {
            variants(game.item(id).unwrap(), kind_of(kind).unwrap()).iter().map(modifiers).collect()
        };
        assert_eq!(
            of("500", "fx_teleport"),
            [
                vec!["particles/events/t19/teleport_start.vpcf", "particles/events/t19/teleport_end.vpcf"],
                vec!["particles/events/t19/teleport_start_lvl2.vpcf", "particles/events/t19/teleport_end_lvl2.vpcf"],
            ]
        );
        assert_eq!(of("500", "fx_blink"), [vec!["particles/events/t19/blink_start.vpcf"]]);
        assert_eq!(of("500", "fx_deny"), [vec!["30 0 0"]], "setting a control point changes an effect too");
        assert!(of("500", "fx_bottle").is_empty(), "the model it changes is no effect");
        assert_eq!(of("501", "fx_teleport"), [vec!["particles/teams/team_teleport_start.vpcf", "particles/teams/team_teleport_end.vpcf"]]);
        assert!(of("501", "fx_blink").is_empty());
        assert_eq!(of("503", "fx_teleport"), [vec!["particles/championship/teleport_start.vpcf"]]);
        assert!(of("20", "fx_teleport").is_empty());
    }
}
