use super::*;
use crate::kv3::tests::{compiled, object, text};

fn event(file: &str) -> Value {
    object(&[("type", text("dota_src1_3d")), ("vsnd_files", Value::Array(vec![text(file)]))])
}

fn sounds() -> Sounds {
    let axe = object(&[
        ("Hero_Axe.Attack", event("sounds/axe/attack.vsnd")),
        ("Hero_Axe.Attack.Jungle", event("sounds/axe/jungle.vsnd")),
        ("Hero_Axe.Call", event("sounds/axe/call.vsnd")),
        ("Hero_Axe.Call.Loud", event("sounds/axe/call_loud.vsnd")),
        ("Hero_Axe.Attack.Layered", object(&[("type", text("dota_src1_3d")), ("soundevent_layer2", text("Hero_Axe.Attack.Layer"))])),
        ("Hero_Axe.Attack.Layer", object(&[("soundevent_layer2", text("hero_axe.attack"))])),
        ("Hero_Axe.Attack.Counted", object(&[("limiter_event_name", text("Hero_Axe.Attack")), ("vsnd_files", Value::Array(vec![text("sounds/axe/counted.vsnd")]))])),
    ]);
    let juggernaut = object(&[("Hero_Juggernaut.Attack", event("sounds/jugg/attack.vsnd")), ("Hero_Juggernaut.Attack.Serra", event("sounds/jugg/serra.vsnd"))]);
    let file = |path: &str, values: &Value| {
        let bytes = compiled(values);
        let Value::Object(values) = Compiled::parse(&bytes).unwrap().values().unwrap() else { unreachable!() };
        SoundFile { path: format!("{FOLDER}{path}"), bytes, values }
    };
    Sounds::from_files(vec![file("game_sounds_axe.vsndevts_c", &axe), file("game_sounds_juggernaut.vsndevts_c", &juggernaut)])
}

fn swap(from: &str, to: &str) -> Swap {
    Swap { from: from.to_string(), to: to.to_string() }
}

fn values_of(file: &NewFile) -> Value {
    Compiled::parse(&file.1).unwrap().values().unwrap()
}

#[test]
fn the_heros_own_sound_becomes_a_copy_of_the_items() {
    let sounds = sounds();
    let made = sounds.files_for(&[swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle")]).unwrap();
    assert_eq!(made.done, vec![swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle")]);
    assert!(made.left.is_empty());
    assert_eq!(made.files.len(), 1);
    assert_eq!(made.files[0].0, "soundevents/game_sounds_heroes/game_sounds_axe.vsndevts_c");
    let values = values_of(&made.files[0]);
    assert_eq!(values.get("Hero_Axe.Attack"), Some(&event("sounds/axe/jungle.vsnd")));
    assert_eq!(values.get("Hero_Axe.Attack.Jungle"), Some(&event("sounds/axe/jungle.vsnd")), "the item's own stays");
    assert_eq!(values.get("Hero_Axe.Call"), Some(&event("sounds/axe/call.vsnd")));
    let Value::Object(members) = &values else { unreachable!() };
    assert_eq!(members.len(), 7, "nothing is added or taken away");
}

#[test]
fn several_swaps_in_one_file_make_one_file_and_each_file_is_written_once() {
    let swaps = [
        swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle"),
        swap("hero_axe.call", "Hero_Axe.Call.Loud"),
        swap("Hero_Juggernaut.Attack", "Hero_Juggernaut.Attack.Serra"),
    ];
    let made = sounds().files_for(&swaps).unwrap();
    assert_eq!(made.done.len(), 3);
    assert_eq!(made.files.len(), 2);
    let axe = values_of(&made.files[0]);
    assert_eq!(axe.get("Hero_Axe.Attack"), Some(&event("sounds/axe/jungle.vsnd")));
    assert_eq!(axe.get("Hero_Axe.Call"), Some(&event("sounds/axe/call_loud.vsnd")), "names are matched in any case");
    assert_eq!(values_of(&made.files[1]).get("Hero_Juggernaut.Attack"), Some(&event("sounds/jugg/serra.vsnd")));
}

#[test]
fn a_swap_copies_the_items_sound_as_the_game_has_it_not_as_another_swap_made_it() {
    let swaps = [swap("Hero_Axe.Call", "Hero_Axe.Attack"), swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle")];
    let made = sounds().files_for(&swaps).unwrap();
    let axe = values_of(&made.files[0]);
    assert_eq!(axe.get("Hero_Axe.Call"), Some(&event("sounds/axe/attack.vsnd")));
    assert_eq!(axe.get("Hero_Axe.Attack"), Some(&event("sounds/axe/jungle.vsnd")));
}

#[test]
fn the_first_swap_of_a_sound_counts() {
    let swaps = [swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle"), swap("Hero_Axe.Attack", "Hero_Axe.Call.Loud")];
    let made = sounds().files_for(&swaps).unwrap();
    assert_eq!(made.done, vec![swaps[0].clone()]);
    assert_eq!(made.left.len(), 1);
    assert_eq!(values_of(&made.files[0]).get("Hero_Axe.Attack"), Some(&event("sounds/axe/jungle.vsnd")));
}

#[test]
fn sounds_the_heroes_files_lack_are_left_alone() {
    let swaps = [swap("DOTA_Item.BlinkDagger.Activate", "Hero_Axe.Attack.Jungle"), swap("Hero_Axe.Attack", "Hero_Axe.Attack.Gone")];
    let made = sounds().files_for(&swaps).unwrap();
    assert!(made.files.is_empty());
    assert!(made.done.is_empty());
    assert_eq!(made.left.len(), 2);
}

#[test]
fn an_items_sound_that_plays_the_heros_own_along_with_it_is_left_alone() {
    let made = sounds().files_for(&[swap("Hero_Axe.Attack", "Hero_Axe.Attack.Layered")]).unwrap();
    assert!(made.files.is_empty(), "the hero's own would play itself");
    assert_eq!(made.left.len(), 1);
    let made = sounds().files_for(&[swap("Hero_Axe.Attack", "Hero_Axe.Attack.Counted")]).unwrap();
    assert_eq!(made.done.len(), 1, "naming it to count it plays nothing");
}

#[test]
fn without_swaps_there_are_no_files() {
    let made = sounds().files_for(&[]).unwrap();
    assert!(made.files.is_empty() && made.done.is_empty() && made.left.is_empty());
}

fn items_game(items: &str) -> String {
    format!("\"items_game\"\n{{\n\"items\"\n{{\n{items}\n}}\n}}\n")
}

fn default_item(id: &str, visuals: &str) -> String {
    format!(
        "\"{id}\"\n{{\n\"name\" \"Default {id}\"\n\"prefab\" \"default_item\"\n\"item_slot\" \"weapon\"\n\"used_by_heroes\"\n{{\n\"npc_dota_hero_axe\" \"1\"\n}}\n{visuals}\n}}\n"
    )
}

fn sound(asset: &str, modifier: &str, more: &str) -> String {
    format!("\"asset_modifier\"\n{{\n\"type\" \"sound\"\n\"asset\" \"{asset}\"\n\"modifier\" \"{modifier}\"\n{more}\n}}\n")
}

#[test]
fn the_swaps_are_those_the_rewrite_gave_default_items_of_heroes() {
    let before = items_game(&[default_item("1", ""), default_item("2", &format!("\"visuals\"\n{{\n{}\n}}", sound("Hero_Axe.Persona", "Hero_Axe.Persona.Own", "")))].concat());
    let visuals = [
        sound("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle", ""),
        sound("Hero_Axe.PreAttack", "Hero_Axe.PreAttack.Jungle", "\"style\" \"0\""),
        sound("Hero_Axe.Call", "Hero_Axe.Call.Gold", "\"style\" \"1\""),
        sound("Hero_Axe.Same", "hero_axe.same", ""),
        sound("Hero_Axe.Culling", "Hero_Axe.Culling.Jungle", "\"apply_when_equipped_in_ability_effects_slot\" \"4\""),
        "\"asset_modifier\"\n{\n\"type\" \"particle\"\n\"asset\" \"a.vpcf\"\n\"modifier\" \"b.vpcf\"\n}\n".to_string(),
    ]
    .concat();
    let wearable = "\"3\"\n{\n\"name\" \"Axe's own\"\n\"prefab\" \"wearable\"\n\"used_by_heroes\"\n{\n\"npc_dota_hero_axe\" \"1\"\n}\n\"visuals\"\n{\n".to_string() + &sound("Hero_Axe.Death", "Hero_Axe.Death.Own", "") + "}\n}\n";
    let after = items_game(&[default_item("1", &format!("\"visuals\"\n{{\n{visuals}\n}}")), default_item("2", &format!("\"visuals\"\n{{\n{}{}\n}}", sound("Hero_Axe.Persona", "Hero_Axe.Persona.Own", ""), sound("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle", ""))), wearable].concat());
    let (before, after) = (ItemsGame::parse(before.as_bytes()).unwrap(), ItemsGame::parse(after.as_bytes()).unwrap());
    assert_eq!(
        swaps(&before, &after),
        vec![
            swap("Hero_Axe.Attack", "Hero_Axe.Attack.Jungle"),
            swap("Hero_Axe.PreAttack", "Hero_Axe.PreAttack.Jungle"),
            swap("Hero_Axe.Culling", "Hero_Axe.Culling.Jungle"),
        ],
        "not those of another style, of the same sound, a default item's own, or an item that is not a default item; and each once"
    );
    assert!(swaps(&before, &before).is_empty());
}
