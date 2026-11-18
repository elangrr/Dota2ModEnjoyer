use crate::apply::Placed;
use crate::items::{Item, ItemsGame};
use crate::resource;

const HEROES_MODELS: &str = "models/heroes/";

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

#[cfg(test)]
mod tests;
