use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;

use crate::apply::{self, Patched, Placed};
use crate::error::{Error, Result};
use crate::items::{Item, ItemsGame};
use crate::resource;

const HEROES_MODELS: &str = "models/heroes/";
const HERO_PARTICLE_MODELS: &str = "particles/models/heroes/";
const GAMEPLAY_MODELS: &str = "models/props_gameplay/";
const DONKEY_RADIANT: &str = "models/props_gameplay/donkey.vmdl";
const DONKEY_DIRE: &str = "models/props_gameplay/donkey_dire.vmdl";
const DONKEY_FLY_RADIANT: &str = "models/props_gameplay/donkey_wings.vmdl";
const DONKEY_FLY_DIRE: &str = "models/props_gameplay/donkey_dire_wings.vmdl";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swap {
    pub from: String,
    pub model: String,
}

pub fn swaps(before: &ItemsGame<'_>, after: &ItemsGame<'_>) -> Vec<Swap> {
    let mut found: Vec<Swap> = Vec::new();
    for item in after.items().iter().filter(|item| item.is_default() && !item.heroes.is_empty()) {
        let own = before.item(item.id).map(item_swaps).unwrap_or_default();
        for swap in item_swaps(item) {
            if !own.contains(&swap) && !found.contains(&swap) {
                found.push(swap);
            }
        }
    }
    found
}

fn item_swaps(item: &Item<'_>) -> Vec<Swap> {
    swaps_in(item, false)
}

pub fn all_swaps(item: &Item<'_>) -> Vec<Swap> {
    swaps_in(item, true)
}

fn swaps_in(item: &Item<'_>, every_style: bool) -> Vec<Swap> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).trim().to_string();
    let of_hero = item.visuals_of_type("entity_model", every_style).into_iter().filter(|block| block.str("asset").is_some_and(|asset| asset.starts_with(b"npc_dota_hero_")));
    let of_form = item.visuals_of_type("hero_model_change", every_style);
    let mut found: Vec<Swap> = Vec::new();
    let swaps = of_hero
        .chain(of_form)
        .filter_map(|block| Some(Swap { from: text(block.str("asset")?), model: text(block.str("modifier")?) }))
        .filter(|swap| !swap.from.is_empty() && !swap.model.is_empty());
    for swap in swaps {
        if !found.contains(&swap) {
            found.push(swap);
        }
    }
    found
}

pub fn placements(swaps: &[Swap], hero_model: &dyn Fn(&str) -> Option<String>, exists: &dyn Fn(&str) -> bool, taken: &[Placed]) -> (Vec<Placed>, Vec<(Swap, &'static str)>) {
    let mut placed: Vec<Placed> = Vec::new();
    let mut left = Vec::new();
    for swap in swaps {
        let at = if swap.from.starts_with("npc_dota_hero_") { hero_model(&swap.from) } else { Some(swap.from.clone()) };
        let Some(at) = at else {
            left.push((swap.clone(), "the hero's own file names no body"));
            continue;
        };
        let (at_path, model_path) = (resource::normal(&at), resource::normal(&swap.model));
        let why = if !at_path.starts_with(HEROES_MODELS) {
            Some("what it swaps is not a hero's own model")
        } else if at_path == model_path {
            Some("it swaps the model for itself")
        } else if !exists(&swap.model) {
            Some("the game does not have the item's model")
        } else if placed.iter().chain(taken).any(|known| resource::normal(&known.at) == at_path) {
            Some("another pick puts a model there already")
        } else {
            None
        };
        match why {
            Some(why) => left.push((swap.clone(), why)),
            None => placed.push(Placed { at, model: swap.model.clone() }),
        }
    }
    (placed, left)
}

pub fn place_all(
    before: &ItemsGame<'_>,
    patched: &mut Patched,
    hero_model: &dyn Fn(&str) -> Option<String>,
    unit_model: &dyn Fn(&str) -> Option<String>,
    exists: &dyn Fn(&str) -> bool,
) -> Result<Vec<Placed>> {
    let mut placed: Vec<Placed> = Vec::new();
    let mut claimed: HashMap<String, String> = patched
        .placed
        .iter()
        .map(|entry| (resource::normal(&entry.at), resource::normal(&entry.model)))
        .collect();
    let mut edits: Vec<(Range<usize>, Vec<u8>)> = Vec::new();
    {
        let after = ItemsGame::parse(&patched.bytes)?;
        for own in before.items().iter().filter(|item| item.is_default() || before.is_base(item)) {
            let Some(rewritten) = after.item(own.id) else { continue };
            let own_vis = own.block.block("visuals").map(|b| before.src_slice(b));
            let new_vis = rewritten.block.block("visuals").map(|b| &patched.bytes[b.start()..b.end()]);
            if own_vis == new_vis {
                continue;
            }
            let Some(vis_pair) = rewritten.block.pairs().find(|pair| pair.key.eq_ignore_ascii_case(b"visuals")) else {
                continue;
            };
            let Some(vis_block) = vis_pair.value.as_block() else { continue };
            let own_blocks: Vec<(String, String, String)> = own
                .block
                .block("visuals")
                .into_iter()
                .flat_map(|v| v.pairs())
                .filter_map(|p| p.value.as_block())
                .filter_map(|b| {
                    let k = apply::as_text(b.str("type")?)?.to_ascii_lowercase();
                    let a = apply::as_text(b.str("asset")?).unwrap_or("").to_ascii_lowercase();
                    let m = resource::normal(apply::as_text(b.str("modifier")?)?);
                    Some((k, a, m))
                })
                .collect();

            let is_world_courier = own.heroes.is_empty() && before.world_slot(own).as_deref() == Some("courier");
            let is_world_ward = own.heroes.is_empty() && before.world_slot(own).as_deref() == Some("ward");
            let is_hero = !own.heroes.is_empty();
            if !is_hero && !is_world_courier && !is_world_ward {
                continue;
            }

            let mut new_parts: Vec<Cow<'_, [u8]>> = Vec::new();
            let mut vis_changed = false;
            let mut claim = |at: &str, model: &str| -> bool {
                let (at_norm, model_norm) = (resource::normal(at), resource::normal(model));
                if at_norm == model_norm || !exists(model) {
                    return false;
                }
                match claimed.get(&at_norm) {
                    Some(prev) if *prev == model_norm => true,
                    Some(_) => false,
                    None => {
                        claimed.insert(at_norm, model_norm);
                        placed.push(Placed { at: at.to_string(), model: model.to_string() });
                        true
                    }
                }
            };

            for pair in vis_block.pairs() {
                let Some(block) = pair.value.as_block() else {
                    new_parts.push(Cow::Borrowed(&patched.bytes[pair.start..pair.end]));
                    continue;
                };
                let style_zero = block
                    .str("style")
                    .and_then(apply::as_text)
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .is_none_or(|s| s == 0);
                let kind = block.str("type").and_then(apply::as_text).map(str::to_ascii_lowercase);
                let asset = block.str("asset").and_then(apply::as_text).map(str::trim).unwrap_or("");
                let modifier = block.str("modifier").and_then(apply::as_text).map(str::trim).unwrap_or("");
                if !style_zero || modifier.is_empty() {
                    new_parts.push(Cow::Borrowed(&patched.bytes[pair.start..pair.end]));
                    continue;
                }
                let kind_str = kind.as_deref().unwrap_or("");
                let already_vanilla = own_blocks.iter().any(|(k, a, m)| {
                    k == kind_str && a == &asset.to_ascii_lowercase() && *m == resource::normal(modifier)
                });
                if already_vanilla {
                    new_parts.push(Cow::Borrowed(&patched.bytes[pair.start..pair.end]));
                    continue;
                }

                let rewritten_to: Option<String> = match kind_str {
                    "entity_model" if !asset.is_empty() && !asset.eq_ignore_ascii_case("npc_dota_loadout_generic") => {
                        let at = if asset.starts_with("npc_dota_hero_") {
                            hero_model(asset)
                        } else if resource::normal(asset).ends_with(".vmdl") {
                            Some(asset.to_string())
                        } else {
                            unit_model(asset)
                        };
                        at.filter(|at| {
                            let norm = resource::normal(at);
                            (is_hero && (norm.starts_with(HEROES_MODELS) || norm.starts_with(HERO_PARTICLE_MODELS)))
                                || (is_world_ward && norm.starts_with(GAMEPLAY_MODELS))
                        })
                        .filter(|at| claim(at, modifier))
                    }
                    "hero_model_change" if is_hero && !asset.is_empty() => {
                        let norm = resource::normal(asset);
                        (norm.starts_with(HEROES_MODELS) && claim(asset, modifier)).then(|| asset.to_string())
                    }
                    "courier" | "courier_flying" if is_world_courier => {
                        let flying = kind_str == "courier_flying";
                        let (rad, dire) = if flying { (DONKEY_FLY_RADIANT, DONKEY_FLY_DIRE) } else { (DONKEY_RADIANT, DONKEY_DIRE) };
                        if asset.eq_ignore_ascii_case("radiant") {
                            claim(rad, modifier).then(|| rad.to_string())
                        } else if asset.eq_ignore_ascii_case("dire") {
                            claim(dire, modifier).then(|| dire.to_string())
                        } else {
                            let ok_r = claim(rad, modifier);
                            let _ = claim(dire, modifier);
                            ok_r.then(|| rad.to_string())
                        }
                    }
                    _ => None,
                };

                match rewritten_to {
                    Some(at) => {
                        new_parts.push(Cow::Owned(apply::replace_key(&patched.bytes, &pair, block, "modifier", at.as_bytes())));
                        vis_changed = true;
                    }
                    None => new_parts.push(Cow::Borrowed(&patched.bytes[pair.start..pair.end])),
                }
            }
            if vis_changed {
                edits.push((vis_pair.start..vis_pair.end, apply::rewrite_pair(&patched.bytes, &vis_pair, vis_block, &new_parts)));
            }
        }
    }
    if !edits.is_empty() {
        let bytes = apply::splice(&patched.bytes, edits)?;
        let after = ItemsGame::parse(&bytes)?;
        if after.items().len() != before.items().len() || after.sets().len() != before.sets().len() {
            return Err(Error::format("the edited item database", "the number of items or sets changed as bodies were put in place"));
        }
        patched.bytes = bytes;
    }
    Ok(placed)
}

#[cfg(test)]
mod tests;
