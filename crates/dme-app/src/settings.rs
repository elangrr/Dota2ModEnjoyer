use std::path::{Path, PathBuf};

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

        let settings = Settings { notice_accepted: true, dota_folder: Some(PathBuf::from(r"D:\Games\dota 2 beta")), low_end: true, auto_update: true, tour_seen: true, theme: crate::theme::Theme::Day };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        assert!(!Settings::default().auto_update, "the app updates by itself only if it is told to");

        std::fs::write(&path, b"{}").unwrap();
        assert_eq!(Settings::load(&path), Settings::default(), "missing fields take their defaults");
        std::fs::write(&path, b"nonsense").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
