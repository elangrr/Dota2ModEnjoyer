use super::*;
use crate::effects::tests::{PACKS, emblems};
use crate::items::tests::{SAMPLE, effects_sample, gems_sample};

const AXE: &str = "npc_dota_hero_axe";

fn game() -> ItemsGame<'static> {
    ItemsGame::parse(SAMPLE.as_bytes()).unwrap()
}

fn vanguard() -> Vec<Pick> {
    set_picks(&game(), AXE, "armor_of_the_vanguard", 0).unwrap().picks
}

fn patched(picks: &[Pick]) -> Patched {
    patch_items_game(SAMPLE.as_bytes(), picks).unwrap()
}

fn text_of<'a>(bytes: &'a [u8], game: &ItemsGame<'a>, id: &str) -> &'a str {
    let block = game.item(id).unwrap().block;
    std::str::from_utf8(&bytes[block.start()..block.end()]).unwrap()
}

#[test]
fn no_key_is_on_both_lists() {
    for key in LOOKS {
        assert!(!NOT_COPIED.contains(key), "{key} is both a look and not copied");
    }
}

#[test]
fn a_set_becomes_one_pick_per_piece_with_a_slot() {
    let set = set_picks(&game(), AXE, "armor_of_the_vanguard", 0).unwrap();
    assert_eq!(
        set.picks,
        [Pick::hero(AXE, "weapon", "9040"), Pick::hero(AXE, "head", "8910"), Pick::hero(AXE, "misc", "8907")]
    );
    assert_eq!(set.skipped.len(), 1);
    assert_eq!(set.skipped[0].item_name, "Cape of the Vanguard");
    assert!(set.skipped[0].reason.contains("back"));
}

#[test]
fn a_set_style_goes_only_to_the_pieces_that_have_it() {
    let set = set_picks(&game(), AXE, "crested_set", 1).unwrap();
    assert_eq!(set.picks, [Pick::hero(AXE, "head", "9500").styled(1), Pick::hero(AXE, "weapon", "9040")]);
}

#[test]
fn wrong_sets_are_refused() {
    let game = game();
    let try_set = |hero: &str, set: &str| set_picks(&game, hero, set, 0);
    assert!(matches!(try_set("npc_dota_hero_nobody", "armor_of_the_vanguard"), Err(Error::NotFound(_))));
    assert!(matches!(try_set(AXE, "no_such_set"), Err(Error::NotFound(_))));
    assert!(matches!(try_set("npc_dota_hero_bane", "armor_of_the_vanguard"), Err(Error::NotFound(_))));
    assert!(matches!(try_set(AXE, "broken_set"), Err(Error::NotFound(_))));
}

#[test]
fn reports_what_changed() {
    let result = patched(&vanguard());
    let summary: Vec<(&str, &str, &str)> = result
        .changes
        .iter()
        .map(|c| (c.slot.as_str(), c.default_id.as_str(), c.item_id.as_str()))
        .collect();
    assert_eq!(summary, [("weapon", "2", "9040"), ("head", "4", "8910"), ("misc", "8653", "8907")]);
    assert_eq!(result.changes[0].hero.as_deref(), Some(AXE));
    assert_eq!(result.changes[0].model.as_deref(), Some("models/items/axe/vanguard_weapon/vanguard_weapon.vmdl"));
    assert_eq!(
        result.changes[0].files,
        ["models/items/axe/vanguard_weapon/vanguard_weapon.vmdl", "particles/econ/items/axe/vanguard/weapon.vpcf"]
    );
}

#[test]
fn default_item_keeps_its_identity_and_takes_the_looks() {
    let result = patched(&vanguard());
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let weapon = game.item("2").unwrap();
    assert_eq!(weapon.name, Some("Axe's Axe"));
    assert!(weapon.is_default());
    assert_eq!(weapon.heroes, [AXE]);
    assert_eq!(weapon.model, Some("models/items/axe/vanguard_weapon/vanguard_weapon.vmdl"));
    assert_eq!(weapon.block.str("image_inventory").unwrap(), b"econ/items/axe/vanguard_weapon");
    assert_eq!(weapon.block.str("item_name").unwrap(), b"#DOTA_Item_Axe_of_the_Vanguard");
    let fov = weapon.block.block("portraits").unwrap().block("icon").unwrap().str("PortraitFOV").unwrap();
    assert_eq!(fov, b"50", "portrait settings come from the picked item");
    let effect = weapon.block.block("visuals").unwrap().block("asset_modifier0").unwrap();
    assert_eq!(effect.str("type").unwrap(), b"particle_create");
    for not_copied in ["creation_date", "item_rarity"] {
        assert!(weapon.block.get(not_copied).is_none(), "{not_copied} must not be copied");
    }
    let mut keys: Vec<Vec<u8>> = weapon.block.pairs().map(|p| p.key.to_ascii_lowercase()).collect();
    let count = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), count);
}

#[test]
fn looks_the_picked_item_lacks_are_removed_from_the_default() {
    let result = patched(&vanguard());
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let head = game.item("4").unwrap();
    assert_eq!(head.own_slot, Some("head"), "the default's own slot text is kept");
    assert_eq!(head.model, Some("models/items/axe/vanguard_head/vanguard_head.vmdl"));
    assert!(head.block.get("visuals").is_none(), "the default's old visuals are gone");
    assert!(head.block.get("price_info").is_none());

    let misc = game.item("8653").unwrap();
    assert_eq!(misc.model, Some("models/items/axe/vanguard_misc/vanguard_misc.vmdl"));
}

#[test]
fn everything_else_is_copied_byte_for_byte() {
    let result = patched(&vanguard());
    let before = game();
    let after = ItemsGame::parse(&result.bytes).unwrap();
    for id in ["30", "9040", "8910", "8907", "8999", "21096", "595", "10003"] {
        assert_eq!(text_of(&result.bytes, &after, id), text_of(SAMPLE.as_bytes(), &before, id), "item {id}");
    }

    let without = |bytes: &[u8], game: &ItemsGame| {
        let mut ranges: Vec<_> = ["2", "4", "8653"].iter().map(|id| game.item(id).unwrap().block).collect();
        ranges.sort_by_key(|b| b.start());
        let mut out = Vec::new();
        let mut at = 0;
        for block in ranges {
            out.extend_from_slice(&bytes[at..block.start()]);
            at = block.end();
        }
        out.extend_from_slice(&bytes[at..]);
        out
    };
    assert_eq!(without(&result.bytes, &after), without(SAMPLE.as_bytes(), &before));
}

#[test]
fn rewritten_block_is_laid_out_like_the_rest_of_the_file() {
    let result = patched(&vanguard());
    let game = ItemsGame::parse(&result.bytes).unwrap();
    let misc = text_of(&result.bytes, &game, "8653");
    assert_eq!(
        misc,
        "\r\n\t\t\t\"name\"\t\t\"Axe's Miscellaneous\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"item_slot\"\t\t\"misc\"\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/vanguard_misc/vanguard_misc.vmdl\"\r\n\t\t"
    );
    assert!(!String::from_utf8_lossy(&result.bytes).replace("\r\n", "").contains('\n'), "line breaks stay CRLF");
}

#[test]
fn pieces_of_different_sets_can_be_mixed_slot_by_slot() {
    let picks = [Pick::hero(AXE, "weapon", "9040"), Pick::hero(AXE, "head", "9500")];
    let result = patched(&picks);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(game.item("2").unwrap().model, Some("models/items/axe/vanguard_weapon/vanguard_weapon.vmdl"));
    assert_eq!(game.item("4").unwrap().model, Some("models/items/axe/crested/crested.vmdl"));
    assert_eq!(game.item("8653").unwrap().model, None, "slots that were not picked stay as they were");
}

#[test]
fn the_default_style_is_copied_as_written() {
    let result = patched(&[Pick::hero(AXE, "head", "9500")]);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    let before = self::game();
    let visuals = |game: &ItemsGame, bytes: &[u8], id: &str| {
        let pair = game.item(id).unwrap().block.pairs().find(|p| is(p.key, "visuals")).unwrap();
        bytes[pair.start..pair.end].to_vec()
    };
    assert_eq!(visuals(&game, &result.bytes, "4"), visuals(&before, SAMPLE.as_bytes(), "9500"));
}

#[test]
fn another_style_is_worked_into_the_item() {
    let result = patched(&[Pick::hero(AXE, "head", "9500").styled(1)]);
    assert_eq!(result.changes[0].style, 1);
    assert_eq!(result.changes[0].model.as_deref(), Some("models/items/axe/crested/crested_gold.vmdl"));

    let game = ItemsGame::parse(&result.bytes).unwrap();
    let head = game.item("4").unwrap();
    assert_eq!(head.model, Some("models/items/axe/crested/crested_gold.vmdl"), "the style's model");
    assert_eq!(head.block.str("image_inventory").unwrap(), b"econ/items/axe/crested_gold", "the style's icon");
    assert!(head.styles().is_empty(), "a default item offers no styles");

    let visuals = head.block.block("visuals").unwrap();
    assert!(visuals.get("styles").is_none());
    assert_eq!(visuals.str("skin").unwrap(), b"1", "the style's skin replaces the item's");
    assert!(visuals.get("asset_modifier0").is_none(), "effects of other styles are gone");
    let gold = visuals.block("asset_modifier1").unwrap();
    assert_eq!(gold.str("modifier").unwrap(), b"particles/gold.vpcf");
    assert!(gold.get("style").is_none(), "the chosen style's effects now always apply");
    assert!(visuals.block("asset_modifier2").unwrap().get("style").is_none(), "effects for every style stay");
    assert!(visuals.get("alternate_icons").is_some());

    assert_eq!(
        text_of(&result.bytes, &game, "4"),
        concat!(
            "\r\n\t\t\t\"name\"\t\t\"Axe's Hair\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"\r\n\t\t\t\"item_slot\"\t\t\"head\"",
            "\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\r\n\t\t\t}",
            "\r\n\t\t\t\"image_inventory\"\t\t\"econ/items/axe/crested_gold\"",
            "\r\n\t\t\t\"model_player\"\t\t\"models/items/axe/crested/crested_gold.vmdl\"",
            "\r\n\t\t\t\"visuals\"\r\n\t\t\t{",
            "\r\n\t\t\t\t\"skin\"\t\t\"1\"",
            "\r\n\t\t\t\t\"alternate_icons\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"1\"\r\n\t\t\t\t\t{\r\n\t\t\t\t\t\t\"icon_path\"\t\t\"econ/items/axe/crested_gold\"\r\n\t\t\t\t\t}\r\n\t\t\t\t}",
            "\r\n\t\t\t\t\"asset_modifier1\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle_create\"\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/gold.vpcf\"\r\n\t\t\t\t}",
            "\r\n\t\t\t\t\"asset_modifier2\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"activity\"\r\n\t\t\t\t\t\"modifier\"\t\t\"crested\"\r\n\t\t\t\t}",
            "\r\n\t\t\t}\r\n\t\t",
        )
    );
}

#[test]
fn a_style_the_item_does_not_have_is_refused() {
    let run = |pick: Pick| patch_items_game(SAMPLE.as_bytes(), &[pick]);
    assert!(matches!(run(Pick::hero(AXE, "head", "9500").styled(2)), Err(Error::NotFound(_))));
    assert!(matches!(run(Pick::hero(AXE, "weapon", "9040").styled(1)), Err(Error::NotFound(_))));
}

#[test]
fn world_slots_work_like_hero_slots() {
    let picks = [Pick::world("courier", "10003"), Pick::world("cursor_pack", "203"), Pick::world("weather", "556")];
    let result = patched(&picks);
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let courier = game.item("595").unwrap();
    assert_eq!(courier.name, Some("Default Courier"));
    assert_eq!(courier.prefab, Some("courier"));
    assert!(game.is_base(courier), "it is still the game's default courier");
    assert_eq!(game.default_item(None, "courier").unwrap().id, "595");
    let model = courier.block.block("visuals").unwrap().block("asset_modifier0").unwrap().str("modifier").unwrap();
    assert_eq!(model, b"models/courier/drodo/drodo.vmdl");
    assert_eq!(courier.block.str("particle_folder").unwrap(), b"particles/econ/courier/courier_drodo");
    assert!(courier.block.get("item_rarity").is_none());
    assert_eq!(
        result.changes[0].files,
        ["models/courier/drodo/drodo.vmdl", "particles/econ/courier/courier_drodo/ambient.vpcf"]
    );
    assert_eq!(result.changes[0].hero, None);
    assert_eq!(result.changes[0].model, None);

    let weather = game.item("555").unwrap();
    assert_eq!(weather.prefab, Some("weather"));
    assert!(weather.block.get("visuals").is_some());
}

#[test]
fn a_default_that_names_another_slot_is_made_to_name_its_own() {
    let result = patched(&[Pick::world("Cursor_Pack", "203")]);
    assert_eq!(result.changes[0].slot, "cursor_pack");
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(game.item("202").unwrap().own_slot, Some("cursor_pack"));
    assert_eq!(game.default_item(None, "cursor_pack").unwrap().id, "202");
    assert_eq!(
        text_of(&result.bytes, &game, "202"),
        concat!(
            "\r\n\t\t\t\"name\"\t\t\"Default Cursor Pack\"\r\n\t\t\t\"prefab\"\t\t\"cursor_pack\"\r\n\t\t\t\"baseitem\"\t\t\"1\"",
            "\r\n\t\t\t\"item_slot\"\t\t\"cursor_pack\"",
            "\r\n\t\t\t\"visuals\"\r\n\t\t\t{\r\n\t\t\t\t\"asset_modifier0\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"cursor_pack\"\r\n\t\t\t\t\t\"modifier\"\t\t\"blue\"\r\n\t\t\t\t}\r\n\t\t\t}\r\n\t\t",
        )
    );
}

#[test]
fn a_default_that_names_the_right_slot_or_none_is_left_as_written() {
    let result = patched(&[Pick::hero(AXE, "HEAD", "8910"), Pick::world("Courier", "10003")]);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(game.item("4").unwrap().own_slot, Some("head"));
    assert_eq!(game.item("595").unwrap().own_slot, None, "the prefab supplies the slot; none is added");
    let slots: Vec<&str> = result.changes.iter().map(|change| change.slot.as_str()).collect();
    assert_eq!(slots, ["head", "courier"]);
}

#[test]
fn unix_line_breaks_stay_unix() {
    let unix = SAMPLE.replace("\r\n", "\n");
    let picks = [Pick::hero(AXE, "head", "9500").styled(1), Pick::world("courier", "10003")];
    let result = patch_items_game(unix.as_bytes(), &picks).unwrap();
    assert!(!result.bytes.contains(&b'\r'));
    assert_eq!(result.changes.len(), 2);
}

#[test]
fn no_picks_means_no_changes() {
    let result = patched(&[]);
    assert_eq!(result.bytes, SAMPLE.as_bytes());
    assert!(result.changes.is_empty());
}

#[test]
fn the_same_picks_always_give_the_same_bytes() {
    assert_eq!(patched(&vanguard()).bytes, patched(&vanguard()).bytes);
}

#[test]
fn wrong_picks_are_refused() {
    let run = |picks: &[Pick]| patch_items_game(SAMPLE.as_bytes(), picks);
    let not_found = |pick: Pick| matches!(run(&[pick]), Err(Error::NotFound(_)));
    assert!(not_found(Pick::hero("npc_dota_hero_nobody", "weapon", "9040")), "no such hero");
    assert!(not_found(Pick::hero(AXE, "back", "8999")), "the hero has no default in that slot");
    assert!(not_found(Pick::hero(AXE, "weapon", "1")), "no such item");
    assert!(not_found(Pick::hero(AXE, "weapon", "8910")), "the item goes in another slot");
    assert!(not_found(Pick::hero("npc_dota_hero_bane", "head", "8910")), "the item is another hero's");
    assert!(not_found(Pick::hero(AXE, "head", "4")), "a default item is not something to pick");
    assert!(not_found(Pick::world("courier", "9040")), "a hero's item in a world slot");
    assert!(not_found(Pick::world("courier", "700")), "another base item is not something to pick");
    assert!(not_found(Pick::world("terrain", "12000")), "a world slot without a default item");
    assert!(not_found(Pick::world("no_such_slot", "10003")));
    assert!(matches!(
        run(&[Pick::hero(AXE, "weapon", "9040"), Pick::hero("NPC_DOTA_HERO_AXE", "Weapon", "9040")]),
        Err(Error::Refused(_))
    ));
}

#[test]
fn a_default_item_that_heroes_share_takes_one_pick() {
    const SVEN: &str = "npc_dota_hero_sven";
    let text = r##"
"items_game"
{
"prefabs"
{
    "default_item" { "item_class" "dota_item_wearable" "item_slot" "weapon" "baseitem" "1" }
    "wearable" { "item_class" "dota_item_wearable" "item_slot" "weapon" }
}
"items"
{
    "1" { "name" "Everybody's Pet" "prefab" "default_item" "item_slot" "summon" "used_by_heroes" { "npc_dota_hero_axe" "1" "npc_dota_hero_sven" "1" } }
    "11" { "name" "Axe's Dog" "prefab" "wearable" "item_slot" "summon" "model_player" "models/dog.vmdl" "used_by_heroes" { "npc_dota_hero_axe" "1" } }
    "12" { "name" "Sven's Cat" "prefab" "wearable" "item_slot" "summon" "model_player" "models/cat.vmdl" "used_by_heroes" { "npc_dota_hero_sven" "1" } }
}
}
"##;
    let (dog, cat) = (Pick::hero(AXE, "summon", "11"), Pick::hero(SVEN, "summon", "12"));
    let one = patch_items_game(text.as_bytes(), std::slice::from_ref(&dog)).unwrap();
    let game = ItemsGame::parse(&one.bytes).unwrap();
    assert_eq!(game.item("1").unwrap().model, Some("models/dog.vmdl"));
    assert_eq!(game.default_item(Some(SVEN), "summon").unwrap().id, "1");
    let Err(Error::Refused(why)) = patch_items_game(text.as_bytes(), &[dog, cat]) else { panic!("two picks for one default item") };
    assert_eq!(why, "sven's summon slot and axe's have one default item between them, so only one of the two can have a pick");
}

#[test]
fn an_item_that_switches_styles_by_itself_is_copied_with_its_rules() {
    const RUBICK: &str = "npc_dota_hero_rubick";
    let text = r##"
"items_game"
{
"prefabs"
{
    "default_item" { "item_class" "dota_item_wearable" "item_slot" "weapon" "baseitem" "1" }
    "wearable" { "item_class" "dota_item_wearable" "item_slot" "weapon" }
}
"items"
{
    "10"
    {
        "name" "Rubick's Cape"
        "prefab" "default_item"
        "item_slot" "back"
        "model_player" "models/heroes/rubick/cape.vmdl"
        "used_by_heroes" { "npc_dota_hero_rubick" "1" }
    }
    "20"
    {
        "name" "The Cypher"
        "prefab" "wearable"
        "item_slot" "back"
        "model_player" "models/items/rubick/cypher.vmdl"
        "used_by_heroes" { "npc_dota_hero_rubick" "1" }
        "visuals"
        {
            "styles"
            {
                "0" { "auto_style_rule" "stolen < 11" }
                "1" { "auto_style_rule" "stolen == 11" "skin" "1" }
            }
            "asset_modifier" { "type" "particle_create" "modifier" "particles/cypher.vpcf" "style" "1" }
        }
    }
}
"item_sets"
{
    "cypher" { "items" { "The Cypher" "1" } }
}
}
"##;
    let game = ItemsGame::parse(text.as_bytes()).unwrap();
    let cypher = game.item("20").unwrap();
    assert!(cypher.switches_styles_itself());
    assert_eq!(cypher.styles().len(), 2);
    assert!(cypher.styles_to_choose().is_empty(), "none of them is for a player to choose");
    assert!(!game.item("10").unwrap().switches_styles_itself());

    let plain = patch_items_game(text.as_bytes(), &[Pick::hero(RUBICK, "back", "20")]).unwrap();
    let styled = patch_items_game(text.as_bytes(), &[Pick::hero(RUBICK, "back", "20").styled(1)]).unwrap();
    assert_eq!(styled.bytes, plain.bytes);
    assert_eq!(styled.changes[0].style, 0);
    let after = ItemsGame::parse(&plain.bytes).unwrap();
    let cape = after.item("10").unwrap();
    assert_eq!(cape.model, Some("models/items/rubick/cypher.vmdl"));
    assert!(cape.switches_styles_itself(), "the game goes on switching them");
    assert_eq!(cape.styles().len(), 2);
    let marked = cape.block.block("visuals").unwrap().block("asset_modifier").unwrap();
    assert_eq!(marked.str("style"), Some(b"1".as_slice()));

    let set = set_picks(&game, RUBICK, "cypher", 1).unwrap();
    assert_eq!(set.picks, [Pick::hero(RUBICK, "back", "20")]);
}

const JUGG: &str = "npc_dota_hero_juggernaut";

fn modifiers(game: &ItemsGame<'_>, id: &str) -> Vec<String> {
    let visuals = game.item(id).unwrap().block.block("visuals");
    let named = |pair: Pair<'_>| pair.value.as_block()?.str("modifier").map(|text| String::from_utf8_lossy(text).into_owned());
    visuals.into_iter().flat_map(|visuals| visuals.pairs()).filter_map(named).collect()
}

#[test]
fn ability_effects_ride_on_the_default_item_of_the_slot_their_item_is_worn_in() {
    let text = effects_sample();
    let result = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_1", "200")]).unwrap();
    let before = ItemsGame::parse(text.as_bytes()).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let sword = game.item("100").unwrap();
    assert_eq!(sword.name, Some("Juggernaut's Sword"));
    assert_eq!(sword.model, Some("models/heroes/juggernaut/sword.vmdl"));
    assert_eq!(game.default_item(Some(JUGG), "weapon").unwrap().id, "100");
    for not_taken in ["image_inventory", "item_rarity", "static_attributes"] {
        assert!(sword.block.get(not_taken).is_none(), "{not_taken} is not a part of the effects");
    }
    assert_eq!(modifiers(&game, "100"), ["particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);
    assert_eq!(
        text_of(&result.bytes, &game, "100"),
        concat!(
            "\r\n\t\t\t\"name\"\t\t\"Juggernaut's Sword\"\r\n\t\t\t\"prefab\"\t\t\"default_item\"",
            "\r\n\t\t\t\"model_player\"\t\t\"models/heroes/juggernaut/sword.vmdl\"",
            "\r\n\t\t\t\"used_by_heroes\"\r\n\t\t\t{\r\n\t\t\t\t\"npc_dota_hero_juggernaut\"\t\t\"1\"\r\n\t\t\t}",
            "\r\n\t\t\t\"visuals\"\r\n\t\t\t{",
            "\r\n\t\t\t\t\"asset_modifier\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"particle\"\r\n\t\t\t\t\t\"asset\"\t\t\"particles/juggernaut/fury.vpcf\"",
            "\r\n\t\t\t\t\t\"modifier\"\t\t\"particles/edge/fury.vpcf\"\r\n\t\t\t\t\t\"style\"\t\t\"0\"",
            "\r\n\t\t\t\t\t\"apply_when_equipped_in_ability_effects_slot\"\t\t\"1\"\r\n\t\t\t\t}",
            "\r\n\t\t\t\t\"asset_modifier\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"ability_icon\"\r\n\t\t\t\t\t\"asset\"\t\t\"juggernaut_blade_fury\"",
            "\r\n\t\t\t\t\t\"modifier\"\t\t\"juggernaut/edge/blade_fury\"",
            "\r\n\t\t\t\t\t\"apply_when_equipped_in_ability_effects_slot\"\t\t\"1\"\r\n\t\t\t\t}",
            "\r\n\t\t\t}\r\n\t\t",
        )
    );

    assert_eq!(
        result.changes,
        [Change {
            hero: Some(JUGG.to_string()),
            slot: "ability_effects_1".to_string(),
            default_id: "100".to_string(),
            item_id: "200".to_string(),
            item_name: "Edge of the Lost".to_string(),
            style: 0,
            model: None,
            files: vec!["particles/edge/fury.vpcf".to_string(), "particles/juggernaut/fury.vpcf".to_string()],
        }]
    );

    for id in ["101", "102", "790", "200", "201", "300"] {
        assert_eq!(text_of(&result.bytes, &game, id), text_of(text.as_bytes(), &before, id), "item {id}");
    }
    assert_eq!(result.bytes.len(), text.len() + text_of(&result.bytes, &game, "100").len() - text_of(text.as_bytes(), &before, "100").len());
}

#[test]
fn ability_effects_take_the_place_of_what_the_item_worn_has_for_the_ability() {
    let text = effects_sample();
    let picks = [Pick::hero(JUGG, "weapon", "203"), Pick::hero(JUGG, "ability_effects_1", "200")];
    let result = patch_items_game(text.as_bytes(), &picks).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let sword = game.item("100").unwrap();
    assert_eq!(sword.model, Some("models/items/juggernaut/plain.vmdl"), "the look of the one");
    assert_eq!(modifiers(&game, "100"), ["particles/plain/ambient.vpcf", "particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);

    let summary: Vec<(&str, &str, &str, Option<&str>)> =
        result.changes.iter().map(|c| (c.slot.as_str(), c.default_id.as_str(), c.item_id.as_str(), c.model.as_deref())).collect();
    assert_eq!(
        summary,
        [("weapon", "100", "203", Some("models/items/juggernaut/plain.vmdl")), ("ability_effects_1", "100", "200", None)],
        "one change for each pick, in the order of the picks"
    );
    assert!(result.changes[0].files.contains(&"particles/edge/fury.vpcf".to_string()), "every file the item names now");

    let turned = patch_items_game(text.as_bytes(), &[picks[1].clone(), picks[0].clone()]).unwrap();
    assert_eq!(turned.bytes, result.bytes);
    assert_eq!(turned.changes[0].slot, "ability_effects_1");

    let worn = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "weapon", "200")]).unwrap();
    let worn = ItemsGame::parse(&worn.bytes).unwrap();
    assert_eq!(modifiers(&worn, "100"), ["particles/edge/ambient.vpcf", "particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);
}

#[test]
fn effects_for_several_abilities_can_ride_on_one_item() {
    let text = effects_sample();
    let run = |picks: &[Pick]| {
        let result = patch_items_game(text.as_bytes(), picks).unwrap();
        let files = result.changes.last().unwrap().files.clone();
        (modifiers(&ItemsGame::parse(&result.bytes).unwrap(), "100"), modifiers(&ItemsGame::parse(&result.bytes).unwrap(), "101"), files)
    };
    let fury = Pick::hero(JUGG, "ability_effects_1", "200");
    let slash = Pick::hero(JUGG, "ability_effects_4", "201");

    let (sword, mask, _) = run(&[slash.clone(), fury.clone()]);
    assert_eq!(sword, ["particles/edge/fury.vpcf", "juggernaut/edge/blade_fury", "particles/serra/slash.vpcf", "Hero_Juggernaut.Slash.Serra"]);
    assert!(mask.is_empty());

    let (sword, mask, files) = run(&[fury, Pick::hero(JUGG, "ability_effects_2", "202")]);
    assert_eq!(sword, ["particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);
    assert_eq!(mask, ["particles/two_winds/ward.vpcf"]);
    assert_eq!(files, ["particles/juggernaut/ward.vpcf", "particles/two_winds/ward.vpcf"]);
    let both = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_2", "202"), Pick::hero(JUGG, "ability_effects_1", "202")]).unwrap();
    let game = ItemsGame::parse(&both.bytes).unwrap();
    assert_eq!(modifiers(&game, "101"), ["particles/two_winds/fury.vpcf", "particles/two_winds/ward.vpcf"]);
    assert_eq!(game.item("101").unwrap().block.block("visuals").unwrap().str("skin").unwrap(), b"0");

    let (sword, ..) = run(&[slash, Pick::hero(JUGG, "ability_effects_2", "207")]);
    assert_eq!(sword, ["particles/ward_blade/ward.vpcf", "particles/serra/slash.vpcf", "Hero_Juggernaut.Slash.Serra"]);
}

#[test]
fn ability_effects_come_in_the_style_that_is_picked() {
    let text = effects_sample();
    let result = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_4", "201").styled(1)]).unwrap();
    assert_eq!(result.changes[0].style, 1);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(modifiers(&game, "100"), ["particles/serra/slash_gold.vpcf", "Hero_Juggernaut.Slash.Serra"]);
    let gold = game.item("100").unwrap().effects_for(4)[0].value.as_block().unwrap();
    assert!(gold.get("style").is_none());
    assert!(game.item("100").unwrap().block.block("visuals").unwrap().get("styles").is_none(), "nothing but the effects is taken");

    let rust = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_4", "201").styled(2)]).unwrap();
    assert_eq!(modifiers(&ItemsGame::parse(&rust.bytes).unwrap(), "100"), ["Hero_Juggernaut.Slash.Serra"]);
    let none = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_4", "201").styled(3)]);
    assert!(matches!(none, Err(Error::NotFound(_))), "the item has no such style");

    let bud = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_4", "208")]);
    assert!(matches!(bud, Err(Error::NotFound(_))));
    let bloom = patch_items_game(text.as_bytes(), &[Pick::hero(JUGG, "ability_effects_4", "208").styled(1)]).unwrap();
    assert_eq!(modifiers(&ItemsGame::parse(&bloom.bytes).unwrap(), "101"), ["particles/bloomer/slash.vpcf"]);
}

#[test]
fn a_personas_ability_effects_ride_on_what_the_persona_wears() {
    let text = effects_sample();
    let picks = [Pick::hero(JUGG, "ability_effects_1_persona_1", "205"), Pick::hero(JUGG, "ability_effects_1", "200")];
    let result = patch_items_game(text.as_bytes(), &picks).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(modifiers(&game, "102"), ["particles/persona_edge/fury.vpcf"]);
    assert_eq!(modifiers(&game, "100"), ["particles/edge/fury.vpcf", "juggernaut/edge/blade_fury"]);
    assert_eq!(game.default_item(Some(JUGG), "weapon_persona_1").unwrap().id, "102");
}

#[test]
fn wrong_picks_of_ability_effects_are_refused() {
    let text = effects_sample();
    let run = |picks: &[Pick]| patch_items_game(text.as_bytes(), picks);
    let not_found = |slot: &str, item: &str| matches!(run(&[Pick::hero(JUGG, slot, item)]), Err(Error::NotFound(_)));
    assert!(not_found("ability_effects_4", "200"), "its effects are for another ability");
    assert!(not_found("ability_effects_1", "203"), "it cannot give its effects away");
    assert!(not_found("ability_effects_3", "204"), "it has none to give");
    assert!(not_found("ability_effects_1", "205"), "the persona's, for the hero's own slot");
    assert!(not_found("ability_effects_4", "206"), "nothing to carry them");
    assert!(not_found("ability_effects_1", "301"), "another hero's");
    assert!(not_found("ability_effects_1", "790"), "the game's default for the slot");
    assert!(not_found("ability_effects_1", "999"), "no such item");
    assert!(matches!(run(&[Pick::world("ability_effects_1", "200")]), Err(Error::NotFound(_))), "the world has no abilities");
    assert!(matches!(
        run(&[Pick::hero(JUGG, "ability_effects_1", "200"), Pick::hero(JUGG, "Ability_Effects_1", "202")]),
        Err(Error::Refused(_))
    ));

    let result = run(&[Pick::hero("npc_dota_hero_lina", "ability_effects_1", "301"), Pick::hero(JUGG, "ability_effects_1", "202")]).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(modifiers(&game, "300"), ["particles/fiery/slave.vpcf"]);
    assert_eq!(modifiers(&game, "101"), ["particles/two_winds/fury.vpcf"]);

    let unix = text.replace("\r\n", "\n");
    let result = patch_items_game(unix.as_bytes(), &[Pick::hero(JUGG, "ability_effects_1", "200")]).unwrap();
    assert!(!result.bytes.contains(&b'\r'), "line breaks stay as they are");
}

fn blocks(game: &ItemsGame<'_>, id: &str) -> Vec<String> {
    let visuals = game.item(id).unwrap().block.block("visuals");
    let told = |pair: Pair<'_>| {
        let block = pair.value.as_block()?;
        let text = |key: &str| block.str(key).map(|text| String::from_utf8_lossy(text).into_owned());
        let kind = text("type")?;
        Some(match (kind.as_str(), text("asset")) {
            ("particle_control_point", Some(asset)) => format!("point {} of {asset} = {}", text("control_point_number")?, text("cp_position")?),
            ("particle_create", _) => format!("create {}", text("modifier")?),
            (_, Some(asset)) => format!("{kind} {asset} -> {}", text("modifier")?),
            (_, None) => format!("{kind} {}", text("modifier")?),
        })
    };
    visuals.into_iter().flat_map(|visuals| visuals.pairs()).filter_map(told).collect()
}

fn gems(picks: &[Pick]) -> Patched {
    patch_items_game(PACKS.as_bytes(), picks).unwrap()
}

const FROSTBLOOM: &str = "particles/econ/events/fall_2021/frostbloom.vpcf";
const TRAIL_1: &str = "particles/econ/courier/courier_trail_01/courier_trail_01.vpcf";
const TRAIL_3: &str = "particles/econ/Courier/courier_trail_03/courier_trail_03.vpcf";
const DRODO: &str = "courier radiant -> models/courier/drodo/drodo.vmdl";

#[test]
fn an_effect_is_put_on_an_item_the_way_an_item_names_one_it_comes_with() {
    let result = gems(&[Pick::hero(AXE, "weapon", "20").with_effect(Some(826))]);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(game.item("1").unwrap().model, Some("models/items/axe/red_axe.vmdl"));
    assert_eq!(blocks(&game, "1"), [format!("create {FROSTBLOOM}")]);
    assert_eq!(
        text_of(&result.bytes, &game, "1"),
        concat!(
            "\n\t\t\t\"name\"\t\t\"Axe's Axe\"\n\t\t\t\"prefab\"\t\t\"default_item\"",
            "\n\t\t\t\"used_by_heroes\"\n\t\t\t{\n\t\t\t\t\"npc_dota_hero_axe\"\t\t\"1\"\n\t\t\t}",
            "\n\t\t\t\"model_player\"\t\t\"models/items/axe/red_axe.vmdl\"",
            "\n\t\t\t\"visuals\"\n\t\t\t{",
            "\n\t\t\t\t\"asset_modifier\"\n\t\t\t\t{\n\t\t\t\t\t\"type\"\t\t\"particle_create\"",
            "\n\t\t\t\t\t\"modifier\"\t\t\"particles/econ/events/fall_2021/frostbloom.vpcf\"\n\t\t\t\t}",
            "\n\t\t\t}\n\t\t",
        )
    );
    assert_eq!(result.changes.len(), 1);
    assert_eq!(result.changes[0].files, ["models/items/axe/red_axe.vmdl", FROSTBLOOM], "the effect is one of the files the item names now");

    let game_of = |pick: Pick| gems(&[pick]).bytes;
    let glowing = game_of(Pick::hero(AXE, "weapon", "21").with_effect(Some(826)));
    let glowing = ItemsGame::parse(&glowing).unwrap();
    assert_eq!(blocks(&glowing, "1").last().unwrap(), &format!("create {FROSTBLOOM}"));
    assert_eq!(blocks(&glowing, "1").len(), 6);

    let drodo = game_of(Pick::world("courier", "30").with_effect(Some(15)));
    assert_eq!(blocks(&ItemsGame::parse(&drodo).unwrap(), "4"), [DRODO.to_string(), format!("create {TRAIL_1}")]);
    assert!(drodo == game_of(Pick::world("courier", "30")), "nothing is added");

    assert!(game_of(Pick::hero(AXE, "weapon", "20").with_effect(None)) == game_of(Pick::hero(AXE, "weapon", "20")));
}

#[test]
fn an_effect_put_on_an_item_with_effects_of_its_own_is_laid_out_like_them() {
    let result = gems(&[Pick::world("courier", "30").with_effect(Some(21))]);
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(blocks(&game, "4"), [DRODO.to_string(), format!("create {TRAIL_1}"), format!("create {TRAIL_3}")]);
    let visuals = game.item("4").unwrap().block.pairs().find(|pair| is(pair.key, "visuals")).unwrap();
    assert!(
        String::from_utf8_lossy(&result.bytes[visuals.start..visuals.end]).ends_with(concat!(
            "
				\"asset_modifier\"
				{
					\"type\"		\"particle_create\"",
            "
					\"modifier\"		\"particles/econ/Courier/courier_trail_03/courier_trail_03.vpcf\"
				}
			}",
        )),
        "laid out like the blocks the file has"
    );
}

#[test]
fn effects_the_game_does_not_have_are_refused() {
    let run = |pick: Pick| patch_items_game(PACKS.as_bytes(), &[pick]);
    let axe = || Pick::hero(AXE, "weapon", "20");
    assert!(matches!(run(axe().with_effect(Some(999))), Err(Error::NotFound(_))), "no such effect");
    assert!(matches!(run(axe().with_effect(Some(900))), Err(Error::NotFound(_))), "listed without a particle system");
    assert!(run(axe().with_effect(Some(15))).is_ok());
}

const LINA: &str = "npc_dota_hero_lina";
const TELEPORT_2: [&str; 2] = [
    "particle particles/items2_fx/teleport_start.vpcf -> particles/events/t19/teleport_start_lvl2.vpcf",
    "particle particles/items2_fx/teleport_end.vpcf -> particles/events/t19/teleport_end_lvl2.vpcf",
];
const BELT: [&str; 2] = ["particle particles/items_fx/Blink_Dagger_Start.vpcf -> particles/axe/belt_blink.vpcf", "create particles/axe/belt_glow.vpcf"];

#[test]
fn an_events_effect_goes_on_every_default_item_of_every_hero() {
    let result = gems(&[Pick::world("fx_teleport", "500").styled(1)]);
    let before = ItemsGame::parse(PACKS.as_bytes()).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();

    assert_eq!(blocks(&game, "1"), TELEPORT_2);
    assert_eq!(blocks(&game, "3"), TELEPORT_2);
    assert_eq!(blocks(&game, "2"), [BELT.as_slice(), TELEPORT_2.as_slice()].concat());
    assert_eq!(
        text_of(&result.bytes, &game, "3"),
        concat!(
            "\n\t\t\t\"name\"\t\t\"Lina's Hair\"\n\t\t\t\"prefab\"\t\t\"default_item\"\n\t\t\t\"item_slot\"\t\t\"head\"",
            "\n\t\t\t\"used_by_heroes\"\n\t\t\t{\n\t\t\t\t\"npc_dota_hero_lina\"\t\t\"1\"\n\t\t\t}",
            "\n\t\t\t\"visuals\"\n\t\t\t{",
            "\n\t\t\t\t\"asset_modifier\"\n\t\t\t\t{\n\t\t\t\t\t\"type\"\t\t\"particle\"\n\t\t\t\t\t\"asset\"\t\t\"particles/items2_fx/teleport_start.vpcf\"",
            "\n\t\t\t\t\t\"modifier\"\t\t\"particles/events/t19/teleport_start_lvl2.vpcf\"\n\t\t\t\t}",
            "\n\t\t\t\t\"asset_modifier\"\n\t\t\t\t{\n\t\t\t\t\t\"type\"\t\t\"particle\"\n\t\t\t\t\t\"asset\"\t\t\"particles/items2_fx/teleport_end.vpcf\"",
            "\n\t\t\t\t\t\"modifier\"\t\t\"particles/events/t19/teleport_end_lvl2.vpcf\"\n\t\t\t\t}",
            "\n\t\t\t}\n\t\t",
        )
    );
    assert_eq!(game.default_item(Some(AXE), "belt").unwrap().id, "2");
    assert_eq!(game.default_item(Some(LINA), "head").unwrap().id, "3");
    assert_eq!(game.item("1").unwrap().model, Some("models/heroes/axe/axe_weapon.vmdl"));

    for id in ["4", "20", "21", "30", "500", "501"] {
        assert_eq!(text_of(&result.bytes, &game, id), text_of(PACKS.as_bytes(), &before, id), "item {id}");
    }

    assert_eq!(
        result.changes,
        [Change {
            hero: None,
            slot: "fx_teleport".to_string(),
            default_id: String::new(),
            item_id: "500".to_string(),
            item_name: "The Tournament 2019 Effects".to_string(),
            style: 1,
            model: None,
            files: [
                "particles/events/t19/teleport_end_lvl2.vpcf",
                "particles/events/t19/teleport_start_lvl2.vpcf",
                "particles/items2_fx/teleport_end.vpcf",
                "particles/items2_fx/teleport_start.vpcf",
            ]
            .map(str::to_string)
            .to_vec(),
        }],
        "one change for the pick, whatever the number of items that carry it"
    );
}

#[test]
fn events_effects_go_with_each_other_and_with_what_is_picked_for_a_hero() {
    let teleport = Pick::world("FX_Teleport", "501");
    let blink = Pick::world("fx_blink", "500");
    let axe = Pick::hero(AXE, "weapon", "21").with_effect(Some(826));
    let result = gems(&[teleport.clone(), axe.clone(), blink.clone()]);
    let game = ItemsGame::parse(&result.bytes).unwrap();

    let team = ["particles/teams/team_teleport_start.vpcf", "particles/teams/team_teleport_end.vpcf"];
    let events = [
        "particle particles/items_fx/blink_dagger_start.vpcf -> particles/events/t19/blink_start.vpcf".to_string(),
        format!("particle particles/items2_fx/teleport_start.vpcf -> {}", team[0]),
        format!("particle particles/items2_fx/teleport_end.vpcf -> {}", team[1]),
    ];
    assert_eq!(blocks(&game, "3"), events);
    let weapon = blocks(&game, "1");
    assert_eq!(weapon.len(), 5 + 1 + 3);
    assert_eq!(weapon[5], format!("create {FROSTBLOOM}"));
    assert_eq!(weapon[6..], events);
    assert_eq!(game.item("1").unwrap().model, Some("models/items/axe/glowing_axe.vmdl"));
    assert_eq!(blocks(&game, "2"), [&[BELT[1].to_string()][..], &events[..]].concat());

    let summary: Vec<(&str, &str, u32)> = result.changes.iter().map(|change| (change.slot.as_str(), change.item_id.as_str(), change.style)).collect();
    assert_eq!(summary, [("fx_teleport", "501", 0), ("weapon", "21", 0), ("fx_blink", "500", 0)], "in the order of the picks");
    assert!(result.changes[1].files.contains(&team[0].to_string()), "every file the hero's item names now");

    assert!(gems(&[blink, axe, teleport]).bytes == result.bytes);

    let first = gems(&[Pick::world("fx_teleport", "500")]);
    let first = ItemsGame::parse(&first.bytes).unwrap();
    assert_eq!(
        blocks(&first, "3"),
        [
            "particle particles/items2_fx/teleport_start.vpcf -> particles/events/t19/teleport_start.vpcf",
            "particle particles/items2_fx/Teleport_End.vpcf -> particles/events/t19/teleport_end.vpcf",
        ]
    );
    assert!(first.item("3").unwrap().block.block("visuals").unwrap().pairs().all(|pair| pair.value.as_block().is_some_and(|block| block.get("criteria").is_none())));
    let deny = gems(&[Pick::world("fx_deny", "500")]);
    assert_eq!(blocks(&ItemsGame::parse(&deny.bytes).unwrap(), "3"), ["point 4 of particles/msg_fx/msg_deny.vpcf = 30 0 0"]);
}

#[test]
fn wrong_picks_of_an_events_effects_are_refused() {
    let run = |picks: &[Pick]| patch_items_game(PACKS.as_bytes(), picks);
    let not_found = |pick: Pick| matches!(run(&[pick]), Err(Error::NotFound(_)));
    assert!(not_found(Pick::world("fx_teleport", "999")), "no such item");
    assert!(not_found(Pick::world("fx_teleport", "20")), "a hero's item is no event's");
    assert!(not_found(Pick::world("fx_teleport", "502")), "a tool that holds no effects");
    assert!(not_found(Pick::world("fx_blink", "501")), "the event changes nothing about this");
    assert!(not_found(Pick::world("fx_bottle", "500")), "the model it changes is no effect");
    assert!(not_found(Pick::world("fx_teleport", "500").styled(2)), "the event has no such level");
    assert!(not_found(Pick::world("fx_teleport", "501").styled(1)));
    assert!(not_found(Pick::world("fx_nothing", "500")), "no such kind of effect");
    assert!(not_found(Pick::hero(AXE, "fx_teleport", "500")), "these are no hero's slots");
    assert!(matches!(run(&[Pick::world("fx_teleport", "500"), Pick::world("FX_TELEPORT", "501")]), Err(Error::Refused(_))));

    let unix_to_windows = PACKS.replace('\n', "\r\n");
    let result = patch_items_game(unix_to_windows.as_bytes(), &[Pick::world("fx_teleport", "500"), Pick::world("courier", "30").with_effect(Some(21))]).unwrap();
    assert!(!String::from_utf8_lossy(&result.bytes).replace("\r\n", "").contains('\n'), "line breaks stay as they are");
}

const NECRO: &str = "npc_dota_hero_necrolyte";

fn attributes_of(bytes: &[u8], id: &str) -> Option<String> {
    let game = ItemsGame::parse(bytes).unwrap();
    let pair = game.item(id).unwrap().block.pairs().find(|pair| is(pair.key, "static_attributes"))?;
    Some(String::from_utf8_lossy(&bytes[pair.start..pair.end]).into_owned())
}

fn sockets_of(bytes: &[u8], id: &str) -> Vec<(String, Option<String>)> {
    let game = ItemsGame::parse(bytes).unwrap();
    let attributes = game.item(id).unwrap().block.block("static_attributes");
    let told = |pair: Pair<'_>| {
        let held = match pair.value {
            Value::Str(held) => Some(held),
            Value::Block(block) => block.str("value"),
        };
        let held = held.filter(|_| items::socket_number(pair.key).is_some());
        (String::from_utf8_lossy(pair.key).into_owned(), held.map(|held| String::from_utf8_lossy(held).into_owned()))
    };
    attributes.into_iter().flat_map(|attributes| attributes.pairs()).map(told).collect()
}

#[test]
fn what_sits_in_the_sockets_of_an_item_comes_with_its_looks() {
    let text = gems_sample();
    let run = |picks: &[Pick]| patch_items_game(text.as_bytes(), picks).unwrap().bytes;
    let in_socket = |number: u32, held: &str| (format!("static socket {number}"), Some(held.to_string()));
    let other = |name: &str| (name.to_string(), None);

    let ka = run(&[Pick::hero(NECRO, "weapon", "20")]);
    assert_eq!(
        attributes_of(&ka, "1").unwrap(),
        concat!(
            "\"static_attributes\"\n\t\t\t{",
            "\n\t\t\t\t\"static socket 1\"\n\t\t\t\t{\n\t\t\t\t\t\"attribute_class\"\t\t\"socket\"",
            "\n\t\t\t\t\t\"value\"\t\t\"socket { required_hero: 'npc_dota_hero_necrolyte' gem_def_index: 3005 } asset_modifier: 91\"\n\t\t\t\t}",
            "\n\t\t\t}",
        )
    );
    let game = ItemsGame::parse(&ka).unwrap();
    assert_eq!(game.item("1").unwrap().model, Some("models/items/necrolyte/ka_scythe.vmdl"));
    assert!(text_of(&ka, &game, "1").ends_with("asset_modifier: 91\"\n\t\t\t\t}\n\t\t\t}\n\t\t"), "after everything else the item has");
    let twin = run(&[Pick::hero(NECRO, "weapon", "21")]);
    let held: Vec<String> = sockets_of(&twin, "1").into_iter().map(|(name, _)| name).collect();
    assert_eq!(held, ["static socket 2", "static socket 1"]);

    let hood = run(&[Pick::hero(NECRO, "head", "22")]);
    assert_eq!(attributes_of(&hood, "2"), attributes_of(text.as_bytes(), "2"));
    assert_eq!(sockets_of(&hood, "2"), [other("is commodity")]);
    let plain = run(&[Pick::hero(NECRO, "back", "24")]);
    assert_eq!(
        attributes_of(&plain, "3").unwrap(),
        "\"static_attributes\"\n\t\t\t{\n\t\t\t\t\"cannot trade\"\n\t\t\t\t{\n\t\t\t\t\t\"attribute_class\"\t\t\"cannot_trade\"\n\t\t\t\t\t\"value\"\t\t\"1\"\n\t\t\t\t}\n\t\t\t}"
    );
    let five = run(&[Pick::hero(NECRO, "back", "23")]);
    let held: Vec<String> = sockets_of(&five, "3").into_iter().map(|(name, _)| name).collect();
    assert_eq!(held, ["cannot trade", "static socket 1", "static socket 2", "static socket 3", "static socket 4", "static socket 5"]);
    assert_eq!(sockets_of(&five, "3")[1], in_socket(1, "socket { gem_def_index: 3005 } asset_modifier: 1"), "the item's, not its own");

    let before = ItemsGame::parse(text.as_bytes()).unwrap();
    for id in ["2", "3", "5", "20", "3001"] {
        assert_eq!(text_of(&ka, &game, id), text_of(text.as_bytes(), &before, id), "item {id}");
    }
}

const SVEN: &str = "npc_dota_hero_sven";
const SUNKEN: &str = "particles/events/t17/hero_effect.vpcf";

#[test]
fn an_emblem_goes_on_one_default_item_of_each_hero() {
    let text = emblems();
    let result = patch_items_game(text.as_bytes(), &[Pick::world("emblem", "638")]).unwrap();
    let before = ItemsGame::parse(text.as_bytes()).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    let glow = format!("create {SUNKEN}");

    assert_eq!(blocks(&game, "3"), ["create particles/axe/ambient.vpcf".to_string(), glow.clone()]);
    assert_eq!(blocks(&game, "6"), std::slice::from_ref(&glow));
    assert_eq!(blocks(&game, "8"), std::slice::from_ref(&glow));
    assert_eq!(
        text_of(&result.bytes, &game, "6"),
        concat!(
            "\n\t\t\t\"name\"\t\t\"Lina's Belt\"\n\t\t\t\"prefab\"\t\t\"default_item\"\n\t\t\t\"item_slot\"\t\t\"belt\"",
            "\n\t\t\t\"used_by_heroes\"\n\t\t\t{\n\t\t\t\t\"npc_dota_hero_lina\"\t\t\"1\"\n\t\t\t}",
            "\n\t\t\t\"visuals\"\n\t\t\t{",
            "\n\t\t\t\t\"asset_modifier\"\n\t\t\t\t{\n\t\t\t\t\t\"type\"\t\t\"particle_create\"",
            "\n\t\t\t\t\t\"modifier\"\t\t\"particles/events/t17/hero_effect.vpcf\"\n\t\t\t\t}",
            "\n\t\t\t}\n\t\t",
        )
    );
    for id in ["1", "2", "4", "5", "7", "9", "10", "11", "20", "24", "30", "500", "638", "663"] {
        assert_eq!(text_of(&result.bytes, &game, id), text_of(text.as_bytes(), &before, id), "item {id}");
    }
    assert_eq!(
        result.changes,
        [Change {
            hero: None,
            slot: "emblem".to_string(),
            default_id: String::new(),
            item_id: "638".to_string(),
            item_name: "Sunken Emblem".to_string(),
            style: 0,
            model: None,
            files: vec![SUNKEN.to_string()],
        }],
        "one change for the pick, whatever the number of items that carry it"
    );

    let crystal = patch_items_game(text.as_bytes(), &[Pick::world("Emblem", "663")]).unwrap();
    let game = ItemsGame::parse(&crystal.bytes).unwrap();
    let visuals = game.item("6").unwrap().block.block("visuals").unwrap();
    assert_eq!(visuals.pairs().count(), 1);
    let block = visuals.pairs().next().unwrap().value.as_block().unwrap();
    assert_eq!(block.str("modifier"), Some(b"particles/events/t18/hero_effect.vpcf".as_slice()));
    assert_eq!(block.str("spawn_in_loadout_only"), Some(b"2".as_slice()));
    assert_eq!(crystal.changes[0].files, ["particles/events/t18/hero_effect.vpcf"]);
}

#[test]
fn an_emblem_rides_on_the_item_a_hero_is_surest_to_wear() {
    let text = emblems();
    let game = ItemsGame::parse(text.as_bytes()).unwrap();
    let emblem = Pick::world("emblem", "638");
    let glow = format!("create {SUNKEN}");
    let told = |picks: &[Pick], live: &dyn Fn(&str, &str) -> bool| -> Vec<(String, String, String, usize)> {
        let carriers = emblem_carriers(&game, picks, live);
        carriers.iter().map(|carrier| (items::hero_short_name(carrier.hero).to_string(), carrier.slot.to_string(), carrier.default.id.to_string(), carrier.others)).collect()
    };
    let carrier = |hero: &str, slot: &str, id: &str, others: usize| (hero.to_string(), slot.to_string(), id.to_string(), others);
    assert_eq!(
        told(&[], &|_, _| true),
        [carrier("axe", "ambient_effects", "3", 0), carrier("lina", "belt", "6", 0), carrier("sven", "head", "8", 1)],
        "Riki has nothing to carry one"
    );
    let carriers = emblem_carriers(&game, &[], &|_, _| true);
    assert!(carriers[0].always_worn() && carriers[1].always_worn() && !carriers[2].always_worn());

    let blade = Pick::hero(SVEN, "Weapon", "24");
    assert_eq!(told(std::slice::from_ref(&blade), &|_, _| true)[2], carrier("sven", "weapon", "7", 2));
    let result = patch_items_game(text.as_bytes(), &[emblem.clone(), blade.clone()]).unwrap();
    let after = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(after.item("7").unwrap().model, Some("models/items/sven/other_blade.vmdl"));
    assert_eq!(blocks(&after, "7"), std::slice::from_ref(&glow));
    assert!(blocks(&after, "8").is_empty());
    let summary: Vec<(&str, &str)> = result.changes.iter().map(|change| (change.slot.as_str(), change.item_id.as_str())).collect();
    assert_eq!(summary, [("emblem", "638"), ("weapon", "24")], "in the order of the picks");
    assert!(result.changes[1].files.contains(&SUNKEN.to_string()), "every file the hero's item names now");
    assert!(patch_items_game(text.as_bytes(), &[blade.clone(), emblem.clone()]).unwrap().bytes == result.bytes);
    assert_eq!(told(&[Pick::hero(AXE, "weapon", "20")], &|_, _| true), told(&[], &|_, _| true));

    let no_ambient = |hero: &str, slot: &str| !(hero == AXE && slot == "ambient_effects");
    assert_eq!(told(&[], &no_ambient)[0], carrier("axe", "head", "2", 0));
    let result = patch_with(&game, text.as_bytes(), std::slice::from_ref(&emblem), &no_ambient).unwrap();
    let after = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(blocks(&after, "2"), std::slice::from_ref(&glow));
    assert_eq!(blocks(&after, "3"), ["create particles/axe/ambient.vpcf"]);
    let no_axe = |hero: &str, _: &str| hero != AXE;
    assert_eq!(told(&[], &no_axe), [carrier("lina", "belt", "6", 0), carrier("sven", "head", "8", 1)]);
}

#[test]
fn an_emblem_goes_with_the_effects_of_events() {
    let text = emblems();
    let picks = [Pick::world("fx_teleport", "500"), Pick::world("emblem", "638")];
    let result = patch_items_game(text.as_bytes(), &picks).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    let teleport = "particle particles/items2_fx/teleport_start.vpcf -> particles/events/t19/teleport_start.vpcf".to_string();
    let glow = format!("create {SUNKEN}");
    assert_eq!(blocks(&game, "3"), ["create particles/axe/ambient.vpcf".to_string(), teleport.clone(), glow.clone()]);
    assert_eq!(blocks(&game, "6"), [teleport.clone(), glow.clone()]);
    assert_eq!(blocks(&game, "8"), [teleport.clone(), glow.clone()]);
    for id in ["1", "2", "4", "5", "7", "9", "10"] {
        assert_eq!(blocks(&game, id), std::slice::from_ref(&teleport), "item {id}");
    }
    let summary: Vec<(&str, &str)> = result.changes.iter().map(|change| (change.slot.as_str(), change.item_id.as_str())).collect();
    assert_eq!(summary, [("fx_teleport", "500"), ("emblem", "638")]);
    assert!(patch_items_game(text.as_bytes(), &[picks[1].clone(), picks[0].clone()]).unwrap().bytes == result.bytes);
}

#[test]
fn wrong_picks_of_an_emblem_are_refused() {
    let text = emblems();
    let run = |picks: &[Pick]| patch_items_game(text.as_bytes(), picks);
    let not_found = |pick: Pick| matches!(run(&[pick]), Err(Error::NotFound(_)));
    assert!(not_found(Pick::world("emblem", "664")), "nothing of it shows in a game");
    assert!(not_found(Pick::world("emblem", "665")), "a hero's item is no emblem");
    assert!(not_found(Pick::world("emblem", "30")), "a courier is no emblem");
    assert!(not_found(Pick::world("emblem", "999")), "no such item");
    assert!(not_found(Pick::hero(AXE, "emblem", "638")), "it is no hero's slot");
    assert!(not_found(Pick::world("courier", "638")), "and an emblem is no courier");
    assert!(matches!(run(&[Pick::world("emblem", "638"), Pick::world("EMBLEM", "663")]), Err(Error::Refused(_))));

    let windows = text.replace('\n', "\r\n");
    let result = patch_items_game(windows.as_bytes(), &[Pick::world("emblem", "638")]).unwrap();
    assert!(!String::from_utf8_lossy(&result.bytes).replace("\r\n", "").contains('\n'), "line breaks stay as they are");
}

#[test]
fn what_is_for_every_hero_goes_to_the_heroes_chosen_for_it() {
    let text = emblems();
    let game = ItemsGame::parse(text.as_bytes()).unwrap();
    let carriers = |picks: &[Pick], effects_for: &EffectsFor| -> Vec<String> {
        let result = patch_for(&game, text.as_bytes(), picks, &|_, _| true, effects_for).unwrap();
        let after = ItemsGame::parse(&result.bytes).unwrap();
        let has_it = |id: &str| modifiers(&after, id).iter().any(|file| file.contains("t17/hero_effect"));
        game.hero_default_items().into_iter().filter(|(_, item)| has_it(item.id)).map(|(hero, _)| hero.to_string()).collect()
    };
    let emblem = Pick::world("emblem", "638");
    let every = carriers(std::slice::from_ref(&emblem), &EffectsFor::Every);
    assert!(every.len() > 1, "{every:?}");
    let first = every[0].clone();

    assert_eq!(carriers(std::slice::from_ref(&emblem), &EffectsFor::These(vec![first.to_uppercase()])), std::slice::from_ref(&first));
    assert!(carriers(std::slice::from_ref(&emblem), &EffectsFor::Picked).is_empty(), "nothing is picked for a hero");
    let with_a_pick: Vec<Pick> = vec![emblem.clone(), Pick::hero(&first, "weapon", "x")];
    let reach = EffectsFor::Picked;
    assert!(reach.reaches(&first, &with_a_pick) && !reach.reaches(&every[1], &with_a_pick));
    assert!(EffectsFor::Every.reaches("anyone", &[]) && !EffectsFor::These(Vec::new()).reaches(&first, &[]));
}

const PERSONA_TEXT: &str = r##"
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
}
"items"
{
    "1"
    {
        "name"  "Pudge's Hook"
        "prefab"  "default_item"
        "model_player"  "models/pudge/hook.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "2"
    {
        "name"  "Pudge's Back"
        "prefab"  "default_item"
        "item_slot"  "back"
        "model_player"  "models/pudge/back.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "3"
    {
        "name"  "Pudge's Hair"
        "prefab"  "default_item"
        "item_slot"  "head"
        "model_player"  "models/pudge/hair.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "9"
    {
        "name"  "Pudge's Belt"
        "prefab"  "default_item"
        "item_slot"  "belt"
        "model_player"  "models/pudge/belt.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
        "visuals"
        {
            "asset_modifier"
            {
                "type"  "particle_create"
                "modifier"  "particles/pudge/belt.vpcf"
            }
        }
    }
    "4"
    {
        "name"  "Pudge's Base"
        "prefab"  "default_item"
        "item_slot"  "hero_base"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "5"
    {
        "name"  "Pudge's Persona"
        "prefab"  "default_item"
        "item_slot"  "persona_selector"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "6"
    {
        "name"  "Pudge Persona Weapon"
        "prefab"  "default_item"
        "item_slot"  "weapon_persona_1"
        "model_player"  "models/cute/hook.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "7"
    {
        "name"  "Pudge Persona Head"
        "prefab"  "default_item"
        "item_slot"  "head_persona_1"
        "model_player"  "models/cute/hair.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "8"
    {
        "name"  "Pudge Persona Costume"
        "prefab"  "default_item"
        "item_slot"  "armor_persona_1"
        "model_player"  "models/cute/costume.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "20"
    {
        "name"  "The Toy Butcher"
        "prefab"  "wearable"
        "item_slot"  "persona_selector"
        "image_inventory"  "econ/persona/toy_butcher"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
        "visuals"
        {
            "asset_modifier"
            {
                "type"  "persona"
                "persona"  "1"
            }
            "asset_modifier"
            {
                "type"  "entity_model"
                "asset"  "npc_dota_hero_pudge"
                "modifier"  "models/cute/pudge_cute.vmdl"
            }
            "asset_modifier"
            {
                "type"  "sound"
                "asset"  "Hero_Pudge.Attack"
                "modifier"  "Hero_Pudge.Cute.Attack"
            }
        }
    }
    "21"
    {
        "name"  "Fancy Toy Hair"
        "prefab"  "wearable"
        "item_slot"  "head_persona_1"
        "model_player"  "models/cute/fancy_hair.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
    "22"
    {
        "name"  "Toy Hook of Effects"
        "prefab"  "wearable"
        "item_slot"  "weapon_persona_1"
        "model_player"  "models/cute/toy_hook.vmdl"
        "static_attributes"
        {
            "can_equip_as_ability_effects"  "1"
        }
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
        "visuals"
        {
            "asset_modifier"
            {
                "type"  "particle"
                "asset"  "particles/pudge/hook.vpcf"
                "modifier"  "particles/cute/hook.vpcf"
                "apply_when_equipped_in_ability_effects_slot"  "1"
            }
        }
    }
    "23"
    {
        "name"  "A Rusty Hook"
        "prefab"  "wearable"
        "model_player"  "models/pudge/rusty.vmdl"
        "used_by_heroes"
        {
            "npc_dota_hero_pudge"  "1"
        }
    }
}
}
"##;

const PUDGE: &str = "npc_dota_hero_pudge";

#[test]
fn a_persona_is_put_on_the_heroes_own_default_items() {
    let text = crate::items::tests::laid_out(PERSONA_TEXT, "\n");
    let run = |picks: &[Pick]| patch_items_game(text.as_bytes(), picks);
    let persona = Pick::hero(PUDGE, "persona_selector", "20");
    let result = run(std::slice::from_ref(&persona)).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    let model = |id: &str| game.item(id).unwrap().model;

    assert_eq!(modifiers(&game, "4"), ["models/cute/pudge_cute.vmdl", "Hero_Pudge.Cute.Attack"]);
    assert!(!text_of(&result.bytes, &game, "4").contains("\"persona\""));
    assert_eq!(model("1"), Some("models/cute/hook.vmdl"));
    assert_eq!(model("3"), Some("models/cute/hair.vmdl"));
    assert_eq!(model("2"), Some("models/cute/costume.vmdl"));
    assert_eq!(model("9"), None);
    assert!(game.item("9").unwrap().block.get("visuals").is_none(), "nor its effect");
    let before = ItemsGame::parse(text.as_bytes()).unwrap();
    for id in ["5", "6", "7", "8"] {
        assert_eq!(text_of(&result.bytes, &game, id), text_of(text.as_bytes(), &before, id), "item {id}");
    }
    assert_eq!(result.changes.len(), 1);
    assert_eq!((result.changes[0].slot.as_str(), result.changes[0].default_id.as_str()), ("hero_base", "4"));

    let hair = run(&[persona.clone(), Pick::hero(PUDGE, "head_persona_1", "21")]).unwrap();
    assert_eq!(ItemsGame::parse(&hair.bytes).unwrap().item("3").unwrap().model, Some("models/cute/fancy_hair.vmdl"));
    assert_eq!(hair.changes.len(), 2);
    let effects = run(&[persona.clone(), Pick::hero(PUDGE, "ability_effects_1_persona_1", "22")]).unwrap();
    let effects = ItemsGame::parse(&effects.bytes).unwrap();
    assert!(modifiers(&effects, "1").contains(&"particles/cute/hook.vpcf".to_string()), "on the hero's own hook");

    let baseless = text.replace("\"item_slot\"\t\t\"hero_base\"", "\"item_slot\"\t\t\"costume\"");
    let result = patch_items_game(baseless.as_bytes(), std::slice::from_ref(&persona)).unwrap();
    let game = ItemsGame::parse(&result.bytes).unwrap();
    assert_eq!(modifiers(&game, "5"), ["models/cute/pudge_cute.vmdl", "Hero_Pudge.Cute.Attack"]);
    assert_eq!(game.default_item(Some(PUDGE), "persona_selector").unwrap().id, "5", "still the default of its slot");
    assert_eq!(result.changes[0].slot, "persona_selector");

    assert!(matches!(run(&[persona.clone(), Pick::hero(PUDGE, "weapon", "23")]), Err(Error::Refused(_))));
    assert!(persona_takes_the_place_of("weapon") && persona_takes_the_place_of("HERO_BASE") && persona_takes_the_place_of("ambient_effects"));
    assert!(!persona_takes_the_place_of("ability_effects_1") && !persona_takes_the_place_of("head_persona_1") && !persona_takes_the_place_of("persona_selector"));
}

#[test]
fn picks_are_saved_compactly() {
    let hero = serde_json::to_string(&Pick::hero(AXE, "head", "9500").styled(1)).unwrap();
    assert_eq!(hero, r#"{"hero":"npc_dota_hero_axe","slot":"head","item":"9500","style":1}"#);
    let world = serde_json::to_string(&Pick::world("courier", "10003")).unwrap();
    assert_eq!(world, r#"{"slot":"courier","item":"10003"}"#);
    assert_eq!(serde_json::from_str::<Pick>(&world).unwrap(), Pick::world("courier", "10003"));

    let gem = Pick::world("courier", "10003").with_effect(Some(15));
    let saved = serde_json::to_string(&gem).unwrap();
    assert_eq!(saved, r#"{"slot":"courier","item":"10003","effect":15}"#);
    assert_eq!(serde_json::from_str::<Pick>(&saved).unwrap(), gem);
    let older = r#"{"slot":"courier","item":"10003","effect":15,"color":"unusual_red"}"#;
    assert_eq!(serde_json::from_str::<Pick>(older).unwrap(), gem);
}

#[test]
fn places_read_naturally() {
    assert_eq!(Pick::hero(AXE, "weapon", "9040").place(), "axe's weapon slot");
    assert_eq!(Pick::world("courier", "10003").place(), "the courier slot");
}
