use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use dme_core::draft::{Mode, Rank, Role, Style};
use dme_core::fsutil;
use dme_core::launch::LaunchOptions;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub notice_accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dota_folder: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub low_end: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub launch_map_dota: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub launch_novid: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub launch_high: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub shop_builds: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub auto_update: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub tour_seen: bool,
    #[serde(default, skip_serializing_if = "crate::theme::Theme::is_night")]
    pub theme: crate::theme::Theme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_version: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub no_glass: bool,
    #[serde(default, deserialize_with = "or_default", skip_serializing_if = "Rank::is_default")]
    pub draft_rank: Rank,
    #[serde(default, deserialize_with = "or_default", skip_serializing_if = "Option::is_none")]
    pub draft_role: Option<Role>,
    #[serde(default, deserialize_with = "or_default", skip_serializing_if = "Style::is_default")]
    pub draft_style: Style,
    #[serde(default, deserialize_with = "or_default", skip_serializing_if = "Mode::is_default")]
    pub draft_mode: Mode,
    #[serde(default, deserialize_with = "pool", skip_serializing_if = "Vec::is_empty")]
    pub draft_heroes: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub draft_all_heroes: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn pool<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let mut seen = BTreeSet::new();
    let list: Vec<String> = or_default(deserializer)?;
    Ok(list.into_iter().filter(|hero| seen.insert(hero.clone())).collect())
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> dme_core::Result<()> {
        if let Some(folder) = path.parent() {
            fsutil::create_dir_all(folder)?;
        }
        fsutil::write_atomic(path, &serde_json::to_vec_pretty(self).expect("settings are plain data"))
    }

    pub fn launch_options(&self) -> LaunchOptions {
        LaunchOptions {
            low_end: self.low_end,
            map_dota: self.launch_map_dota,
            novid: self.launch_novid,
            high: self.launch_high,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_survive_saving_and_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());

        let settings = Settings {
            notice_accepted: true,
            dota_folder: Some(PathBuf::from(r"D:\Games\dota 2 beta")),
            low_end: true,
            launch_map_dota: true,
            launch_novid: true,
            launch_high: true,
            shop_builds: true,
            auto_update: true,
            tour_seen: true,
            theme: crate::theme::Theme::Day,
            seen_version: Some("0.2.3".to_string()),
            no_glass: true,
            draft_rank: Rank::Immortal,
            draft_role: Some(Role::Mid),
            draft_style: Style::Lane,
            draft_mode: Mode::CaptainsMode,
            draft_heroes: vec!["npc_dota_hero_lina".to_string(), "npc_dota_hero_axe".to_string()],
            draft_all_heroes: true,
            language: Some("ru".to_string()),
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        assert!(!Settings::default().auto_update, "the app updates by itself only if it is told to");

        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "missing fields take their defaults");
        std::fs::write(&path, b"nonsense").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }

    #[test]
    fn a_pool_of_more_than_three_heroes_loads_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let five = ["npc_dota_hero_axe", "npc_dota_hero_invoker", "npc_dota_hero_lina", "npc_dota_hero_puck", "npc_dota_hero_zuus"];
        std::fs::write(&path, format!(r#"{{"notice_accepted":true,"draft_heroes":{}}}"#, serde_json::to_string(&five).unwrap())).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.draft_heroes, five);
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).draft_heroes, five, "and is written back whole");
    }

    #[test]
    fn a_setting_in_words_this_version_does_not_know_leaves_the_others_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"notice_accepted":true,"tour_seen":true,"draft_rank":"legend_ancient","draft_role":"jungler","draft_style":"jungle"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.notice_accepted && settings.tour_seen, "what was read before is not lost");
        assert_eq!((settings.draft_rank, settings.draft_role, settings.draft_style), (Rank::default(), None, Style::Team));
        std::fs::write(&path, br#"{"notice_accepted":true,"draft_rank":"divine","draft_role":"offlane"}"#).unwrap();
        assert_eq!((Settings::load(&path).draft_rank, Settings::load(&path).draft_role), (Rank::Divine, Some(Role::Offlane)));
    }

    #[test]
    fn a_file_from_a_version_without_the_draft_page_is_read_with_the_draft_settings_unchosen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"notice_accepted":true,"tour_seen":true,"seen_version":"0.2.3","no_glass":true}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.notice_accepted && settings.tour_seen && settings.no_glass);
        assert_eq!(settings.seen_version.as_deref(), Some("0.2.3"));
        assert_eq!((settings.draft_rank, settings.draft_role), (Rank::default(), None));
        assert!(settings.draft_heroes.is_empty() && !settings.draft_all_heroes, "a version without \"My heroes\" has none");
    }

    #[test]
    fn the_hero_pool_is_kept_as_a_list_in_its_order_and_one_that_is_not_a_list_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"notice_accepted":true,"draft_heroes":["npc_dota_hero_lina","npc_dota_hero_axe","npc_dota_hero_lina"]}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.draft_heroes, ["npc_dota_hero_lina", "npc_dota_hero_axe"], "in the order chosen, each once");
        std::fs::write(&path, br#"{"notice_accepted":true,"draft_heroes":"lina"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.notice_accepted && settings.draft_heroes.is_empty());
        assert!(!String::from_utf8(serde_json::to_vec(&Settings::default()).unwrap()).unwrap().contains("draft_heroes"), "none is not written");
    }
}
