use std::path::Path;

use dme_core::apply::Pick;
use dme_core::fsutil;
use serde::{Deserialize, Serialize};

pub const LONGEST_NAME: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub picks: Vec<Pick>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presets {
    #[serde(default)]
    presets: Vec<Preset>,
}

impl Presets {
    pub fn load(path: &Path) -> Presets {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> dme_core::Result<()> {
        if let Some(folder) = path.parent() {
            fsutil::create_dir_all(folder)?;
        }
        fsutil::write_atomic(path, &serde_json::to_vec_pretty(self).expect("presets are plain data"))
    }

    pub fn all(&self) -> &[Preset] {
        &self.presets
    }

    pub fn get(&self, place: usize) -> Option<&Preset> {
        self.presets.get(place)
    }

    pub fn keep(&mut self, name: &str, picks: &[Pick]) -> Option<String> {
        let name: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(LONGEST_NAME).collect();
        let name = name.trim().to_string();
        if name.is_empty() || picks.is_empty() {
            return None;
        }
        let preset = Preset { name: name.clone(), picks: picks.to_vec() };
        match self.presets.iter_mut().find(|known| known.name.eq_ignore_ascii_case(&name)) {
            Some(known) => *known = preset,
            None => self.presets.push(preset),
        }
        Some(name)
    }

    pub fn forget(&mut self, place: usize) -> Option<Preset> {
        (place < self.presets.len()).then(|| self.presets.remove(place))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_kept_under_their_names_and_between_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deeper").join("presets.json");
        assert_eq!(Presets::load(&path), Presets::default());

        let ranked = [Pick::hero("npc_dota_hero_axe", "weapon", "5675"), Pick::world("courier", "10096")];
        let mut presets = Presets::default();
        assert_eq!(presets.keep("  Ranked   look ", &ranked).as_deref(), Some("Ranked look"), "the name is tidied");
        assert_eq!(presets.keep("Fun", &ranked[..1]).as_deref(), Some("Fun"));
        assert_eq!(presets.keep("   ", &ranked), None, "no name");
        assert_eq!(presets.keep("Empty", &[]), None, "nothing to keep");
        assert_eq!(presets.keep(&"x".repeat(90), &ranked).map(|name| name.len()), Some(LONGEST_NAME));
        presets.forget(2);
        assert_eq!(presets.all().iter().map(|preset| preset.name.as_str()).collect::<Vec<_>>(), ["Ranked look", "Fun"]);

        presets.keep("ranked LOOK", &ranked[1..]);
        assert_eq!(presets.all().len(), 2);
        assert_eq!(presets.get(0).unwrap().picks, ranked[1..]);

        presets.save(&path).unwrap();
        assert_eq!(Presets::load(&path), presets);
        assert_eq!(presets.forget(0).map(|preset| preset.name), Some("ranked LOOK".to_string()));
        assert_eq!(presets.forget(5), None);
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(Presets::load(&path), Presets::default());
    }
}
