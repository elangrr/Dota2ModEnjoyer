use std::path::{Path, PathBuf};

use dme_core::draft::{Rank, Role};
use dme_core::fsutil;
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
}

fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
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
            auto_update: true,
            tour_seen: true,
            theme: crate::theme::Theme::Day,
            seen_version: Some("0.2.3".to_string()),
            no_glass: true,
            draft_rank: Rank::Immortal,
            draft_role: Some(Role::Mid),
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
    fn a_setting_in_words_this_version_does_not_know_leaves_the_others_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"notice_accepted":true,"tour_seen":true,"draft_rank":"legend_ancient","draft_role":"jungler"}"#).unwrap();
        let settings = Settings::load(&path);
        assert!(settings.notice_accepted && settings.tour_seen, "what was read before is not lost");
        assert_eq!((settings.draft_rank, settings.draft_role), (Rank::default(), None));
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
    }
}
