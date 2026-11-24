use super::*;

fn items_game(items: &str) -> String {
    format!("\"items_game\"\n{{\n\"items\"\n{{\n{items}\n}}\n}}\n")
}

fn default_item(id: &str, hero: &str, visuals: &str) -> String {
    format!(
        "\"{id}\"\n{{\n\"name\" \"Default {id}\"\n\"prefab\" \"default_item\"\n\"item_slot\" \"weapon\"\n\"used_by_heroes\"\n{{\n\"{hero}\" \"1\"\n}}\n\"visuals\"\n{{\n{visuals}\n}}\n}}\n"
    )
}

fn block(kind: &str, asset: &str, modifier: &str, more: &str) -> String {
    format!("\"asset_modifier\"\n{{\n\"type\" \"{kind}\"\n\"asset\" \"{asset}\"\n\"modifier\" \"{modifier}\"\n{more}\n}}\n")
}

fn swap(from: &str, model: &str) -> Swap {
    Swap { from: from.to_string(), model: model.to_string() }
}

#[test]
fn the_swaps_are_the_bodies_and_forms_the_rewrite_gave_default_items_of_heroes() {
    let wolf = "models/heroes/lycan/lycan_wolf.vmdl";
    let before = items_game(&[default_item("1", "npc_dota_hero_axe", ""), default_item("2", "npc_dota_hero_lycan", &block("hero_model_change", wolf, wolf, ""))].concat());
    let axe = [
        block("entity_model", "npc_dota_hero_axe", "models/items/axe/bare.vmdl", ""),
        block("entity_model", "npc_dota_hero_axe", "models/items/axe/bare_gold.vmdl", "\"style\" \"1\""),
        block("entity_model", "npc_dota_observer_wards", "models/items/wards/eye.vmdl", ""),
        block("model", "models/heroes/axe/axe_weapon.vmdl", "models/items/axe/fists.vmdl", ""),
        block("sound", "Hero_Axe.Attack", "Hero_Axe.Attack.Jungle", ""),
    ]
    .concat();
    let lycan = [block("hero_model_change", wolf, wolf, ""), block("hero_model_change", wolf, "models/items/lycan/grey_wolf.vmdl", "\"style\" \"0\"")].concat();
    let after = items_game(&[default_item("1", "npc_dota_hero_axe", &axe), default_item("2", "npc_dota_hero_lycan", &lycan)].concat());
    let (before, after) = (ItemsGame::parse(before.as_bytes()).unwrap(), ItemsGame::parse(after.as_bytes()).unwrap());
    assert_eq!(
        swaps(&before, &after),
        vec![swap("npc_dota_hero_axe", "models/items/axe/bare.vmdl"), swap(wolf, "models/items/lycan/grey_wolf.vmdl")],
        "not another style's, not a ward's, not a piece's model, and not what the default item swapped already"
    );
    assert!(swaps(&before, &before).is_empty());
}

fn hero_model(hero: &str) -> Option<String> {
    (hero == "npc_dota_hero_axe").then(|| "models/heroes/axe/axe.vmdl".to_string())
}

#[test]
fn a_body_goes_where_the_heros_own_or_the_forms_is() {
    let swaps = [swap("npc_dota_hero_axe", "models/items/axe/bare.vmdl"), swap("models/heroes/lycan/lycan_wolf.vmdl", "models/items/lycan/grey_wolf.vmdl")];
    let (placed, left) = placements(&swaps, &hero_model, &|_| true, &[]);
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(
        placed,
        vec![
            Placed { at: "models/heroes/axe/axe.vmdl".to_string(), model: "models/items/axe/bare.vmdl".to_string() },
            Placed { at: "models/heroes/lycan/lycan_wolf.vmdl".to_string(), model: "models/items/lycan/grey_wolf.vmdl".to_string() },
        ]
    );
}

#[test]
fn bodies_that_cannot_be_put_in_place_are_left_with_why() {
    let swaps = [
        swap("npc_dota_hero_lion", "models/items/lion/body.vmdl"),
        swap("models/props_gameplay/frog.vmdl", "models/items/hex/fish.vmdl"),
        swap("models/heroes/lycan/lycan_wolf.vmdl", "models/heroes/lycan/lycan_wolf.vmdl"),
        swap("models/heroes/lycan/lycan_wolf.vmdl", "models/items/lycan/gone.vmdl"),
        swap("npc_dota_hero_axe", "models/items/axe/bare.vmdl"),
        swap("npc_dota_hero_axe", "models/items/axe/other.vmdl"),
    ];
    let taken = [Placed { at: "Models/Heroes/Axe/Axe.vmdl".to_string(), model: "models/items/axe/piece.vmdl".to_string() }];
    let (placed, left) = placements(&swaps, &hero_model, &|model| !model.contains("gone"), &[]);
    assert_eq!(placed.len(), 1);
    let whys: Vec<&str> = left.iter().map(|(_, why)| *why).collect();
    assert_eq!(
        whys,
        [
            "the hero's own file names no body",
            "what it swaps is not a hero's own model",
            "it swaps the model for itself",
            "the game does not have the item's model",
            "another pick puts a model there already",
        ]
    );
    let (placed, left) = placements(&swaps[4..5], &hero_model, &|_| true, &taken);
    assert!(placed.is_empty(), "a model put in place for a piece keeps its place");
    assert_eq!(left.len(), 1);
}

#[test]
fn place_all_rewrites_hero_summon_and_ward_modifiers_back_to_default_paths() {
    let before = format!(
        "\"items_game\"\n{{\n\"player_loadout_slots\" {{ \"0\" \"ward\" }}\n\"prefabs\" {{ \"ward\" {{ \"item_slot\" \"ward\" \"baseitem\" \"1\" }} }}\n\"items\"\n{{\n{}{}{}\n}}\n}}\n",
        default_item("1", "npc_dota_hero_axe", ""),
        default_item("2", "npc_dota_hero_phoenix", ""),
        "\"596\"\n{\n\"name\" \"Default Ward\"\n\"prefab\" \"ward\"\n\"visuals\"\n{\n\"asset_modifier0\" { \"type\" \"entity_model\" \"asset\" \"npc_dota_observer_wards\" \"modifier\" \"models/props_gameplay/default_ward.vmdl\" }\n\"asset_modifier1\" { \"type\" \"entity_model\" \"asset\" \"npc_dota_sentry_wards\" \"modifier\" \"models/props_gameplay/default_ward.vmdl\" }\n}\n}\n",
    );
    let after = format!(
        "\"items_game\"\n{{\n\"player_loadout_slots\" {{ \"0\" \"ward\" }}\n\"prefabs\" {{ \"ward\" {{ \"item_slot\" \"ward\" \"baseitem\" \"1\" }} }}\n\"items\"\n{{\n{}{}{}\n}}\n}}\n",
        default_item("1", "npc_dota_hero_axe", &block("entity_model", "npc_dota_hero_axe", "models/items/axe/bare.vmdl", "")),
        default_item("2", "npc_dota_hero_phoenix", &block("entity_model", "npc_dota_phoenix_sun", "models/items/phoenix/sun.vmdl", "")),
        "\"596\"\n{\n\"name\" \"Default Ward\"\n\"prefab\" \"ward\"\n\"visuals\"\n{\n\"asset_modifier0\" { \"type\" \"entity_model\" \"asset\" \"npc_dota_observer_wards\" \"modifier\" \"models/items/wards/eye.vmdl\" }\n\"asset_modifier1\" { \"type\" \"entity_model\" \"asset\" \"npc_dota_sentry_wards\" \"modifier\" \"models/items/wards/eye.vmdl\" }\n}\n}\n",
    );
    let before_game = ItemsGame::parse(before.as_bytes()).unwrap();
    let mut patched = Patched { bytes: after.into_bytes(), changes: Vec::new(), placed: Vec::new() };
    let unit_model = |unit: &str| match unit {
        "npc_dota_phoenix_sun" => Some("models/heroes/phoenix/phoenix_egg.vmdl".to_string()),
        "npc_dota_observer_wards" | "npc_dota_sentry_wards" => Some("models/props_gameplay/default_ward.vmdl".to_string()),
        _ => None,
    };
    let placed = place_all(&before_game, &mut patched, &hero_model, &unit_model, &|_| true).unwrap();
    assert_eq!(
        placed,
        vec![
            Placed { at: "models/heroes/axe/axe.vmdl".to_string(), model: "models/items/axe/bare.vmdl".to_string() },
            Placed { at: "models/heroes/phoenix/phoenix_egg.vmdl".to_string(), model: "models/items/phoenix/sun.vmdl".to_string() },
            Placed { at: "models/props_gameplay/default_ward.vmdl".to_string(), model: "models/items/wards/eye.vmdl".to_string() },
        ]
    );
    let rewritten = ItemsGame::parse(&patched.bytes).unwrap();
    assert_eq!(rewritten.item("1").unwrap().visuals_of_type("entity_model", true)[0].str("modifier").unwrap(), b"models/heroes/axe/axe.vmdl");
    assert_eq!(rewritten.item("2").unwrap().visuals_of_type("entity_model", true)[0].str("modifier").unwrap(), b"models/heroes/phoenix/phoenix_egg.vmdl");
    let ward_mods = rewritten.item("596").unwrap().visuals_of_type("entity_model", true);
    assert_eq!(ward_mods.len(), 2);
    assert_eq!(ward_mods[0].str("modifier").unwrap(), b"models/props_gameplay/default_ward.vmdl");
    assert_eq!(ward_mods[1].str("modifier").unwrap(), b"models/props_gameplay/default_ward.vmdl");
}
