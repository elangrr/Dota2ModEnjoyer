use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::items::{Item, ItemsGame};
use crate::kv3::{Compiled, Value};
use crate::vpk::{Archive, NewFile};

pub const FOLDER: &str = "soundevents/game_sounds_heroes/";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swap {
    pub from: String,
    pub to: String,
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
    swaps_in(item, true)
}

pub fn all_swaps(item: &Item<'_>) -> Vec<Swap> {
    swaps_in(item, false)
}

fn swaps_in(item: &Item<'_>, first_style: bool) -> Vec<Swap> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let mut found: Vec<Swap> = Vec::new();
    let swaps = item
        .visuals_of_type("sound", !first_style)
        .into_iter()
        .filter_map(|block| Some(Swap { from: text(block.str("asset")?), to: text(block.str("modifier")?) }))
        .filter(|swap| !swap.from.eq_ignore_ascii_case(&swap.to));
    for swap in swaps {
        if !found.contains(&swap) {
            found.push(swap);
        }
    }
    found
}

pub struct Sounds {
    files: Vec<SoundFile>,
    events: HashMap<String, (usize, usize)>,
}

struct SoundFile {
    path: String,
    bytes: Vec<u8>,
    values: Vec<(String, Value)>,
}

#[derive(Debug, Default)]
pub struct Made {
    pub files: Vec<NewFile>,
    pub done: Vec<Swap>,
    pub left: Vec<(Swap, &'static str)>,
}

impl Sounds {
    pub fn read(archive: &Archive) -> Result<Sounds> {
        let mut paths: Vec<String> = archive
            .entries()
            .iter()
            .map(|entry| archive.path_of(entry))
            .filter(|path| path.starts_with(FOLDER) && path.ends_with(".vsndevts_c"))
            .collect();
        paths.sort();
        let mut files = Vec::with_capacity(paths.len());
        for path in paths {
            let bytes = archive.read(&path)?.ok_or_else(|| Error::format("the game's archive", format!("{path} is missing")))?;
            let values = match Compiled::parse(&bytes)?.values()? {
                Value::Object(members) => members,
                _ => return Err(Error::format(&path, "it does not hold a list of sound events")),
            };
            files.push(SoundFile { path, bytes, values });
        }
        Ok(Sounds::from_files(files))
    }

    fn from_files(files: Vec<SoundFile>) -> Sounds {
        let mut events = HashMap::new();
        for (file, sound_file) in files.iter().enumerate() {
            for (place, (name, _)) in sound_file.values.iter().enumerate() {
                events.entry(name.to_ascii_lowercase()).or_insert((file, place));
            }
        }
        Sounds { files, events }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn event(&self, name: &str) -> Option<&Value> {
        let &(file, place) = self.events.get(&name.to_ascii_lowercase())?;
        Some(&self.files[file].values[place].1)
    }

    fn plays(&self, event: &str, name: &str) -> bool {
        let mut seen: Vec<String> = Vec::new();
        let mut todo = vec![event.to_string()];
        while let Some(next) = todo.pop() {
            if seen.iter().any(|known| known.eq_ignore_ascii_case(&next)) {
                continue;
            }
            let Some(value) = self.event(&next) else { continue };
            seen.push(next);
            let mut found = false;
            value.for_each_string(&mut |key, text| {
                if key.is_some_and(|key| key.starts_with("limiter_")) {
                    return;
                }
                if text.eq_ignore_ascii_case(name) {
                    found = true;
                } else if self.events.contains_key(&text.to_ascii_lowercase()) {
                    todo.push(text.to_string());
                }
            });
            if found {
                return true;
            }
        }
        false
    }

    pub fn files_for(&self, swaps: &[Swap]) -> Result<Made> {
        let mut made = Made::default();
        let mut edits: Vec<(usize, Vec<(usize, Value)>)> = Vec::new();
        for swap in swaps {
            let Some(&(file, place)) = self.events.get(&swap.from.to_ascii_lowercase()) else {
                made.left.push((swap.clone(), "the hero has no such sound of its own"));
                continue;
            };
            let Some(to) = self.event(&swap.to) else {
                made.left.push((swap.clone(), "the game has no such sound in the heroes' files"));
                continue;
            };
            if made.done.iter().any(|done| done.from.eq_ignore_ascii_case(&swap.from)) {
                made.left.push((swap.clone(), "another pick changes this sound already"));
                continue;
            }
            if self.plays(&swap.to, &swap.from) {
                made.left.push((swap.clone(), "the item's sound plays the hero's own along with it"));
                continue;
            }
            match edits.iter_mut().find(|(known, _)| *known == file) {
                Some((_, places)) => places.push((place, to.clone())),
                None => edits.push((file, vec![(place, to.clone())])),
            }
            made.done.push(swap.clone());
        }
        for (file, places) in edits {
            let sound_file = &self.files[file];
            let mut values = sound_file.values.clone();
            for (place, value) in places {
                values[place].1 = value;
            }
            let bytes = Compiled::parse(&sound_file.bytes)?.with_values(&Value::Object(values))?;
            made.files.push((sound_file.path.clone(), bytes));
        }
        Ok(made)
    }
}

#[cfg(test)]
mod tests;
