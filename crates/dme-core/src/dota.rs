use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::{fsutil, kv};

pub const MOD_FOLDER: &str = "mod_enjoyer";

const DOTA_FOLDER: &str = "dota 2 beta";

#[derive(Clone, Debug)]
pub struct DotaInstall {
    root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameVersion {
    pub build: String,
    pub date: String,
}

impl DotaInstall {
    pub fn at(root: impl Into<PathBuf>) -> Result<DotaInstall> {
        let install = DotaInstall { root: root.into() };
        if install.exe().is_file() {
            Ok(install)
        } else {
            Err(Error::NotDota(install.root))
        }
    }

    pub fn find() -> Result<DotaInstall> {
        let mut looked_in = Vec::new();
        for steam in steam_folders() {
            for library in library_folders(&steam) {
                let candidate = library.join("steamapps").join("common").join(DOTA_FOLDER);
                match DotaInstall::at(&candidate) {
                    Ok(install) => return Ok(install),
                    Err(_) => looked_in.push(candidate.display().to_string()),
                }
            }
        }
        Err(Error::DotaNotFound(if looked_in.is_empty() {
            "Steam itself was not found on this computer.".to_string()
        } else {
            format!("Looked in: {}", looked_in.join("; "))
        }))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn file(&self, parts: &[&str]) -> PathBuf {
        let mut path = self.root.clone();
        path.extend(parts);
        path
    }

    pub fn exe(&self) -> PathBuf {
        self.file(&["game", "bin", "win64", "dota2.exe"])
    }

    pub fn signatures(&self) -> PathBuf {
        self.file(&["game", "bin", "win64", "dota.signatures"])
    }

    pub fn gameinfo(&self) -> PathBuf {
        self.file(&["game", "dota", "gameinfo.gi"])
    }

    pub fn gameinfo_branch(&self) -> PathBuf {
        self.file(&["game", "dota", "gameinfo_branchspecific.gi"])
    }

    pub fn steam_inf(&self) -> PathBuf {
        self.file(&["game", "dota", "steam.inf"])
    }

    pub fn game_archive(&self) -> PathBuf {
        self.file(&["game", "dota", "pak01_dir.vpk"])
    }

    pub fn map_package(&self, vmap: &str) -> Option<PathBuf> {
        let stem_len = vmap.len().checked_sub(".vmap".len())?;
        if !vmap.is_char_boundary(stem_len) || !vmap[stem_len..].eq_ignore_ascii_case(".vmap") {
            return None;
        }
        let mut path = self.file(&["game", "dota"]);
        let mut parts = vmap[..stem_len].split(['/', '\\']).peekable();
        while let Some(part) = parts.next() {
            if part.is_empty() || part == "." || part == ".." || part.contains(':') {
                return None;
            }
            if parts.peek().is_some() {
                path.push(part);
            } else {
                path.push(format!("{part}.vpk"));
            }
        }
        Some(path)
    }

    pub fn mod_dir(&self) -> PathBuf {
        self.file(&["game", MOD_FOLDER])
    }

    pub fn mod_package(&self) -> PathBuf {
        self.file(&["game", MOD_FOLDER, "pak01_dir.vpk"])
    }

    pub fn version(&self) -> Result<GameVersion> {
        let path = self.steam_inf();
        let text = String::from_utf8_lossy(&fsutil::read(&path)?).into_owned();
        parse_steam_inf(&text)
            .ok_or_else(|| Error::format(path.display().to_string(), "there is no ClientVersion line"))
    }

    pub fn app_manifest(&self) -> Option<PathBuf> {
        Some(self.root.parent()?.parent()?.join("appmanifest_570.acf"))
    }

    pub fn steam_is_busy(&self) -> bool {
        let Some(bytes) = self.app_manifest().and_then(|path| std::fs::read(path).ok()) else { return false };
        parse_state_flags(&bytes).is_some_and(|flags| flags != FULLY_INSTALLED)
    }
}

const FULLY_INSTALLED: u64 = 4;

fn parse_state_flags(bytes: &[u8]) -> Option<u64> {
    let state = kv::Block::document(bytes).pairs().find_map(|pair| pair.value.as_block())?;
    std::str::from_utf8(state.str("StateFlags")?).ok()?.trim().parse().ok()
}

fn parse_steam_inf(text: &str) -> Option<GameVersion> {
    let value = |name: &str| {
        text.lines()
            .filter_map(|line| line.split_once('='))
            .find(|(key, _)| key.trim().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.trim().to_string())
    };
    Some(GameVersion {
        build: value("ClientVersion")?,
        date: value("VersionDate").unwrap_or_default(),
    })
}

fn library_folders(steam: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam.to_path_buf()];
    let list = steam.join("steamapps").join("libraryfolders.vdf");
    if let Ok(bytes) = std::fs::read(&list) {
        for path in parse_library_folders(&bytes) {
            push_unique(&mut libraries, path);
        }
    }
    libraries
}

fn parse_library_folders(bytes: &[u8]) -> Vec<PathBuf> {
    let Some(root) = kv::Block::document(bytes).pairs().find_map(|p| p.value.as_block()) else {
        return Vec::new();
    };
    root.pairs()
        .filter_map(|pair| match pair.value {
            kv::Value::Block(library) => library.str("path"),
            kv::Value::Str(path) if pair.key.iter().all(u8::is_ascii_digit) => Some(path),
            kv::Value::Str(_) => None,
        })
        .map(|raw| PathBuf::from(kv::unescape(raw)))
        .collect()
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let key = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase();
    if !paths.iter().any(|known| key(known) == key(&path)) {
        paths.push(path);
    }
}

#[cfg(windows)]
pub(crate) fn steam_folders() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let from_registry = [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
        (HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
    ]
    .into_iter()
    .filter_map(|(hive, key, value)| {
        RegKey::predef(hive).open_subkey(key).ok()?.get_value::<String, _>(value).ok()
    })
    .map(|path| PathBuf::from(path.replace('/', "\\")));

    let mut folders = Vec::new();
    for folder in from_registry.chain([PathBuf::from(r"C:\Program Files (x86)\Steam")]) {
        if folder.is_dir() {
            push_unique(&mut folders, folder);
        }
    }
    folders
}

#[cfg(not(windows))]
pub(crate) fn steam_folders() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steam_is_busy_with_dota_unless_its_file_says_installed_whole_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("steamapps").join("common").join(DOTA_FOLDER);
        std::fs::create_dir_all(root.join("game").join("bin").join("win64")).unwrap();
        std::fs::write(root.join("game").join("bin").join("win64").join("dota2.exe"), b"").unwrap();
        let dota = DotaInstall::at(&root).unwrap();
        assert_eq!(dota.app_manifest(), Some(dir.path().join("steamapps").join("appmanifest_570.acf")));
        let manifest = |flags: &str| {
            let text = format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"570\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"buildid\"\t\t\"26632873\"\n}}\n");
            std::fs::write(dota.app_manifest().unwrap(), text).unwrap();
        };
        assert!(!dota.steam_is_busy(), "no file of Steam's: a copy of the game, with nobody to wait for");
        manifest("4");
        assert!(!dota.steam_is_busy());
        for busy in ["6", "1026", "1030", "1048580"] {
            manifest(busy);
            assert!(dota.steam_is_busy(), "{busy}");
        }
        std::fs::write(dota.app_manifest().unwrap(), b"not what Steam writes").unwrap();
        assert!(!dota.steam_is_busy(), "what cannot be read says nothing");
    }

    #[test]
    fn steam_inf_gives_build_and_date() {
        let text = "ClientVersion=6973\r\nServerVersion=6973\r\nProductName=dota2_workshop\r\nVersionDate=Nov 09 2026\r\nVersionTime=14:11:37\r\n";
        assert_eq!(
            parse_steam_inf(text),
            Some(GameVersion { build: "6973".into(), date: "Nov 09 2026".into() })
        );
        assert_eq!(parse_steam_inf("ServerVersion=1\n"), None);
    }

    #[test]
    fn library_list_new_and_old_layout() {
        let new = b"\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"570\"\t\t\"1\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(
            parse_library_folders(new),
            [PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\SteamLibrary")]
        );

        let old = b"\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"123\"\n\t\"1\"\t\t\"E:\\\\Games\"\n}\n";
        assert_eq!(parse_library_folders(old), [PathBuf::from(r"E:\Games")]);

        assert!(parse_library_folders(b"").is_empty());
    }

    #[test]
    fn same_folder_is_listed_once() {
        let mut paths = vec![PathBuf::from(r"C:\Program Files (x86)\Steam")];
        push_unique(&mut paths, PathBuf::from("c:/program files (x86)/steam/"));
        push_unique(&mut paths, PathBuf::from(r"D:\Lib"));
        assert_eq!(paths.len(), 2);
    }

    #[test]
    fn a_folder_without_the_game_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(DotaInstall::at(dir.path()), Err(Error::NotDota(_))));

        let exe = dir.path().join("game").join("bin").join("win64");
        std::fs::create_dir_all(&exe).unwrap();
        std::fs::write(exe.join("dota2.exe"), b"").unwrap();
        let install = DotaInstall::at(dir.path()).unwrap();
        assert!(install.mod_package().ends_with("game/mod_enjoyer/pak01_dir.vpk"));
    }

    #[test]
    fn a_map_is_a_package_laid_out_like_its_path() {
        let install = DotaInstall { root: PathBuf::from("root") };
        let package = |vmap: &str| install.map_package(vmap);
        assert_eq!(package("maps/dota_desert.vmap"), Some(PathBuf::from("root/game/dota/maps/dota_desert.vpk")));
        assert_eq!(
            package("maps\\scenes\\mvp\\mvp_pudge_arcana.VMAP"),
            Some(PathBuf::from("root/game/dota/maps/scenes/mvp/mvp_pudge_arcana.vpk"))
        );
        assert_eq!(package("maps/dota_7.32.vmap"), Some(PathBuf::from("root/game/dota/maps/dota_7.32.vpk")));
        for not_a_map in ["maps/dota.vmdl", "vmap", ".vmap", "maps/../../x.vmap", "maps//x.vmap", "c:/x.vmap", "/maps/x.vmap"] {
            assert_eq!(package(not_a_map), None, "{not_a_map}");
        }
    }
}
