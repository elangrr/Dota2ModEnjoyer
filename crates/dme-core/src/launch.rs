use std::path::PathBuf;
use std::process::Command;

use crate::dota;
use crate::error::{Error, Result};

pub const DOTA_APP_ID: &str = "570";

pub type Setting = (&'static str, &'static str);

pub const LOW_END: &[Setting] = &[
    ("cl_globallight_shadow_mode", "0"),
    ("dota_cheap_water", "1"),
    ("r_deferred_simple_light", "1"),
    ("r_deferred_additive_pass", "0"),
    ("r_deferred_specular", "0"),
    ("r_deferred_specular_bloom", "0"),
    ("r_deferred_height_fog", "0"),
    ("r_ssao", "0"),
    ("r_dota_fxaa", "0"),
    ("r_dota_normal_maps", "0"),
    ("r_dota_allow_parallax_mapping", "0"),
    ("r_grass_quality", "0"),
    ("r_dota_allow_wind_on_trees", "0"),
    ("dota_ambient_creatures", "0"),
    ("dota_ambient_cloth", "0"),
    ("dota_portrait_animate", "0"),
    ("r_dashboard_render_quality", "0"),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub low_end: bool,
    pub map_dota: bool,
    pub novid: bool,
    pub high: bool,
}

pub fn settings(low_end: bool) -> &'static [Setting] {
    if low_end { LOW_END } else { &[] }
}

pub fn start_dota(settings: &[Setting]) -> Result<()> {
    spawn_steam(launch_arguments(settings))
}

pub fn start_dota_with(options: &LaunchOptions) -> Result<()> {
    spawn_steam(launch_arguments_for(options))
}

fn spawn_steam(arguments: Vec<String>) -> Result<()> {
    let steam = find_steam(&dota::steam_folders()).ok_or(Error::NoSteam)?;
    Command::new(&steam)
        .args(arguments)
        .spawn()
        .map(drop)
        .map_err(|error| Error::Launch(format!("{} did not start ({error}).", steam.display())))
}

fn find_steam(folders: &[PathBuf]) -> Option<PathBuf> {
    folders.iter().map(|folder| folder.join("steam.exe")).find(|program| program.is_file())
}

fn launch_arguments(settings: &[Setting]) -> Vec<String> {
    let mut arguments = vec!["-applaunch".to_string(), DOTA_APP_ID.to_string()];
    for (name, value) in settings {
        arguments.extend([format!("+{name}"), value.to_string()]);
    }
    arguments
}

pub fn launch_arguments_for(options: &LaunchOptions) -> Vec<String> {
    let mut arguments = vec!["-applaunch".to_string(), DOTA_APP_ID.to_string()];
    if options.map_dota {
        arguments.extend(["-map".to_string(), "dota".to_string()]);
    }
    if options.novid {
        arguments.push("-novid".to_string());
    }
    if options.high {
        arguments.push("-high".to_string());
    }
    for (name, value) in settings(options.low_end) {
        arguments.extend([format!("+{name}"), value.to_string()]);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_is_found_in_the_first_folder_that_has_it() {
        let dir = tempfile::tempdir().unwrap();
        let (empty, real, other) = (dir.path().join("empty"), dir.path().join("real"), dir.path().join("other"));
        for folder in [&empty, &real, &other] {
            std::fs::create_dir_all(folder).unwrap();
        }
        assert_eq!(find_steam(&[empty.clone(), real.clone()]), None);

        std::fs::write(real.join("steam.exe"), b"").unwrap();
        std::fs::write(other.join("steam.exe"), b"").unwrap();
        assert_eq!(find_steam(&[empty, real.clone(), other]), Some(real.join("steam.exe")));
        assert_eq!(find_steam(&[]), None);
    }

    #[test]
    fn steam_is_asked_to_start_dota() {
        assert_eq!(launch_arguments(&[]), ["-applaunch", "570"]);
        assert_eq!(launch_arguments(settings(false)), ["-applaunch", "570"]);
    }

    #[test]
    fn settings_for_the_game_follow_its_number() {
        let arguments = launch_arguments(&[("dota_cheap_water", "1"), ("r_ssao", "0")]);
        assert_eq!(arguments, ["-applaunch", "570", "+dota_cheap_water", "1", "+r_ssao", "0"]);
        assert_eq!(launch_arguments(settings(true)).len(), 2 + 2 * LOW_END.len());
    }

    #[test]
    fn launch_options_pass_boosters_before_low_end_settings() {
        assert_eq!(launch_arguments_for(&LaunchOptions::default()), ["-applaunch", "570"]);
        let all = LaunchOptions { low_end: true, map_dota: true, novid: true, high: true };
        let args = launch_arguments_for(&all);
        assert_eq!(&args[..6], ["-applaunch", "570", "-map", "dota", "-novid", "-high"]);
        assert_eq!(args.len(), 6 + 2 * LOW_END.len());

        let only_map = LaunchOptions { map_dota: true, ..LaunchOptions::default() };
        assert_eq!(launch_arguments_for(&only_map), ["-applaunch", "570", "-map", "dota"]);
    }

    #[test]
    fn the_settings_for_a_low_end_computer_are_plain_and_given_once() {
        for (place, (name, value)) in LOW_END.iter().enumerate() {
            assert!(LOW_END[..place].iter().all(|(earlier, _)| earlier != name), "{name} is given twice");
            assert!(!name.is_empty() && name.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_'), "{name}");
            assert!(matches!(*value, "0" | "1"), "{name} {value}");
        }
    }
}
