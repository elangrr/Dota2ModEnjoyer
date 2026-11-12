use crate::apply::Pick;
use crate::error::{Error, Result};
use crate::items::{Item, ItemsGame};
use crate::vpk::{Archive, NewFile};

pub const SLOT: &str = "map_effect";

pub const OWN: &str = "materials/dev/deferred_post_process.vmat";

const TYPE: &[u8] = b"map_effect";
const FOR_THE_MAP: &[u8] = b"map_shader";

pub fn is_pick(pick: &Pick) -> bool {
    pick.hero.is_none() && pick.slot.eq_ignore_ascii_case(SLOT)
}

pub fn material<'a>(game: &ItemsGame<'a>, item: &Item<'a>) -> Option<&'a str> {
    if !item.heroes.is_empty() || game.world_slot(item).as_deref() != Some(SLOT) {
        return None;
    }
    let is = |value: Option<&[u8]>, wanted: &[u8]| value.is_some_and(|value| value.eq_ignore_ascii_case(wanted));
    let blocks = item.block.block("visuals")?.pairs().filter_map(|pair| pair.value.as_block());
    let for_the_map = blocks.into_iter().find(|block| is(block.str("type"), TYPE) && is(block.str("asset"), FOR_THE_MAP))?;
    for_the_map.str("modifier").and_then(|material| std::str::from_utf8(material).ok())
}

const CHECKERED: &[&str] = &["materials/dev/deferred_post_process_aghanim_bp.vmat"];

pub fn shows_checkered(game: &ItemsGame<'_>, item: &Item<'_>) -> bool {
    material(game, item).is_some_and(|material| CHECKERED.iter().any(|known| known.eq_ignore_ascii_case(material)))
}

pub fn is_there(game: &ItemsGame<'_>, archive: &Archive, item: &Item<'_>) -> bool {
    material(game, item).is_some_and(|material| archive.contains(&compiled(material))) && archive.contains(&compiled(OWN))
}

pub fn files(game: &ItemsGame<'_>, archive: &Archive, item: &Item<'_>) -> Result<Vec<NewFile>> {
    let name = item.name.unwrap_or(item.id);
    let material = material(game, item).ok_or_else(|| Error::NotFound(format!("{name} is no map effect")))?;
    let gone = |file: &str| Error::NotFound(format!("this copy of Dota does not have {file}, which {name} needs"));
    if !archive.contains(&compiled(OWN)) {
        return Err(gone(OWN));
    }
    let bytes = archive.read(&compiled(material))?.ok_or_else(|| gone(material))?;
    Ok(vec![(compiled(OWN), bytes)])
}

fn compiled(material: &str) -> String {
    format!("{material}_c")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::trees::tests::archive_with;

    pub(crate) const MAP_EFFECTS: &str = r##"
"items_game"
{
	"player_loadout_slots"
	{
		"0"		"courier"
		"1"		"map_effect"
	}
	"prefabs"
	{
		"courier"
		{
			"item_slot"		"courier"
		}
		"map_effect"
		{
			"item_slot"		"map_effect"
		}
	}
	"items"
	{
		"595"
		{
			"name"		"Default Courier"
			"prefab"		"courier"
			"baseitem"		"1"
		}
		"600"
		{
			"name"		"A Courier"
			"prefab"		"courier"
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"map_effect"
					"asset"		"map_shader"
					"modifier"		"materials/dev/deferred_post_process_graphic.vmat"
				}
			}
		}
		"18380"
		{
			"name"		"Graphic Shader Effect"
			"prefab"		"map_effect"
			"image_inventory"		"econ/tools/graphic_shader"
			"item_name"		"#DOTA_Item_Graphic_Shader_Effect"
			"item_rarity"		"seasonal"
			"visuals"
			{
				"asset_modifier0"
				{
					"type"		"map_effect"
					"asset"		"MAP_SHADER"
					"modifier"		"materials/dev/deferred_post_process_graphic.vmat"
				}
				"asset_modifier1"
				{
					"type"		"map_effect"
					"modifier"		"materials/dev/deferred_post_process_graphic_ui.vmat"
				}
			}
		}
		"18381"
		{
			"name"		"Screen Effect"
			"prefab"		"map_effect"
			"visuals"
			{
				"asset_modifier"
				{
					"type"		"map_effect"
					"asset"		"post_effect_shader"
					"modifier"		"materials/dev/postprocess_special_effect_crt.vmat"
				}
			}
		}
	}
}
"##;

    pub(crate) const GRAPHIC: &str = "materials/dev/deferred_post_process_graphic.vmat_c";
    pub(crate) const OWN_FILE: &str = "materials/dev/deferred_post_process.vmat_c";

    #[test]
    fn a_map_effect_names_the_material_to_draw_the_map_through() {
        let game = ItemsGame::parse(MAP_EFFECTS.as_bytes()).unwrap();
        let of = |id: &str| material(&game, game.item(id).unwrap());
        assert_eq!(of("18380"), Some("materials/dev/deferred_post_process_graphic.vmat"));
        assert_eq!(of("18381"), None, "it changes something else than the map's material");
        assert_eq!(of("600"), None, "a courier is no map effect, whatever it names");
        assert_eq!(of("595"), None);

        assert!(is_pick(&Pick::world("map_effect", "18380")));
        assert!(is_pick(&Pick::world("Map_Effect", "1")));
        assert!(!is_pick(&Pick::world("courier", "18380")));
        assert!(!is_pick(&Pick::hero("npc_dota_hero_axe", "map_effect", "18380")));
    }

    #[test]
    fn the_map_effect_with_checkered_fog_is_known() {
        let text = MAP_EFFECTS.replace("deferred_post_process_graphic.vmat\"", "Deferred_Post_Process_Aghanim_BP.vmat\"");
        let game = ItemsGame::parse(text.as_bytes()).unwrap();
        assert!(shows_checkered(&game, game.item("18380").unwrap()));
        assert!(!shows_checkered(&game, game.item("600").unwrap()), "a courier is no map effect");
        let game = ItemsGame::parse(MAP_EFFECTS.as_bytes()).unwrap();
        assert!(!shows_checkered(&game, game.item("18380").unwrap()));
    }

    #[test]
    fn its_material_goes_in_the_package_under_the_name_of_the_games_own() {
        let game = ItemsGame::parse(MAP_EFFECTS.as_bytes()).unwrap();
        let effect = game.item("18380").unwrap();
        let both = archive_with(&[GRAPHIC.to_string(), OWN_FILE.to_string()]);
        assert!(is_there(&game, &both, effect));
        let made = files(&game, &both, effect).unwrap();
        assert_eq!(made, [(OWN_FILE.to_string(), GRAPHIC.as_bytes().to_vec())]);

        for there in [GRAPHIC, OWN_FILE] {
            let archive = archive_with(&[there.to_string()]);
            assert!(!is_there(&game, &archive, effect), "{there} alone");
            assert!(matches!(files(&game, &archive, effect), Err(Error::NotFound(_))), "{there} alone");
        }
        let other = game.item("18381").unwrap();
        assert!(!is_there(&game, &both, other));
        assert!(matches!(files(&game, &both, other), Err(Error::NotFound(why)) if why == "Screen Effect is no map effect"));
    }
}
