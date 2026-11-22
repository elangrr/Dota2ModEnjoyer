use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::apply::{self, Change, EffectsFor, Pick};
use crate::dota::{DotaInstall, GameVersion, MOD_FOLDER};
use crate::error::{Error, Result, VERIFY_HINT};
use crate::heroes::HeroFiles;
use crate::items::ItemsGame;
use crate::loadout::{Loadout, same_picks};
use crate::resource::{self, Wholeness};
use crate::signatures::{self, Signatures};
use crate::vpk::{self, Archive, NewFile, PackFile};
use crate::{bodies, effects, fsutil, gameinfo, map_effect, process, sounds, trees};

pub const ITEMS_GAME: &str = "scripts/items/items_game.txt";

const REWRITE_RULES: u32 = 12;

const BRANCH_FILE: &str = "gameinfo_branchspecific.gi";

#[derive(Clone, Debug)]
pub struct Engine {
    pub dota: DotaInstall,
    pub data_dir: PathBuf,
    pub refuse_while_running: bool,
}

#[derive(Debug)]
pub struct Applied {
    pub changes: Vec<Change>,
    pub missing_files: Vec<String>,
    pub package_len: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Restored {
    pub gameinfo_restored: bool,
    pub signatures_restored: bool,
    pub package_removed: bool,
    pub notes: Vec<Note>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Note {
    #[error("gameinfo_branchspecific.gi still differs from the file Valve shipped; this app did not make that change. {VERIFY_HINT}")]
    BranchNotValves,
    #[error("dota.signatures has extra lines that belong to another tool's change; they were left alone.")]
    OtherToolsLines,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModState {
    Off,
    On,
    NeedsRepair(String),
    ChangedByOther(String),
}

pub const UNDECLARED: &str = "dota.signatures does not declare the edited file (a Dota update resets it)";
pub const LEFTOVER: &str = "dota.signatures has a leftover line";

impl ModState {
    pub fn is_half_undone(&self) -> bool {
        matches!(self, ModState::NeedsRepair(why) if why == UNDECLARED || why == LEFTOVER)
    }
}

#[derive(Debug)]
pub struct Status {
    pub version: GameVersion,
    pub running: bool,
    pub state: ModState,
    pub picks: Vec<Pick>,
    pub effects_for: EffectsFor,
    pub package_is_current: bool,
    pub steam_busy: bool,
    pub applied_build: String,
}

impl Status {
    pub fn undone_by_an_update(&self) -> bool {
        let other_build = !self.picks.is_empty() && !self.applied_build.is_empty() && self.applied_build != self.version.build;
        self.state.is_half_undone() || (other_build && matches!(self.state, ModState::Off | ModState::NeedsRepair(_)))
    }
}

#[derive(Debug)]
pub enum Mended {
    Later,
    Back(Applied),
    Normal { why: Error, restored: Restored },
}

#[derive(Debug)]
pub enum Ready {
    Already,
    Applied(Applied),
    Normal(Restored),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    picks: Vec<Pick>,
    #[serde(default, skip_serializing_if = "EffectsFor::is_usual")]
    effects_for: EffectsFor,
    game_build: String,
    applied_unix: u64,
    #[serde(default)]
    rewrite_rules: u32,
}

struct GameFiles {
    valve_signatures: String,
    has_extra_lines: bool,
    extra_line_matches_branch: bool,
    declared_path: Option<String>,
    line_break: &'static str,
    has_our_lines: bool,
    branch_without_ours: String,
    branch_is_valves: bool,
}

impl Engine {
    pub fn new(dota: DotaInstall) -> Engine {
        Engine { dota, data_dir: default_data_dir(), refuse_while_running: true }
    }

    pub fn make_ready(&self, picks: &[Pick], effects_for: &EffectsFor) -> Result<Ready> {
        let status = self.status()?;
        if picks.is_empty() {
            return match status.state {
                ModState::Off => Ok(Ready::Normal(Restored::default())),
                _ => self.restore().map(Ready::Normal),
            };
        }
        if status.state == ModState::On && status.package_is_current && same_picks(&status.picks, picks) && status.effects_for == *effects_for {
            return Ok(Ready::Already);
        }
        self.apply_for(picks, effects_for).map(Ready::Applied)
    }

    pub fn mend(&self, picks: &[Pick], effects_for: &EffectsFor) -> Result<Mended> {
        if picks.is_empty() {
            return Err(Error::Refused("there are no picks to put back".to_string()));
        }
        let status = self.status()?;
        let in_use = self.refuse_while_running && status.running;
        if !status.undone_by_an_update() || in_use || status.steam_busy {
            return Ok(Mended::Later);
        }
        match self.apply_for(picks, effects_for) {
            Ok(applied) => Ok(Mended::Back(applied)),
            Err(Error::DotaRunning) => Ok(Mended::Later),
            Err(why) => self.restore().map(|restored| Mended::Normal { why, restored }),
        }
    }

    pub fn loadout_file(&self) -> PathBuf {
        self.data_dir.join("picks.json")
    }

    pub fn loadout(&self) -> Loadout {
        let file = self.loadout_file();
        if file.exists() { Loadout::load(&file) } else { Loadout::from_picks(self.load().picks) }
    }

    pub fn apply(&self, picks: &[Pick]) -> Result<Applied> {
        self.apply_for(picks, &EffectsFor::Every)
    }

    pub fn apply_for(&self, picks: &[Pick], effects_for: &EffectsFor) -> Result<Applied> {
        self.check_not_running()?;

        let archive = Archive::open(&self.dota.game_archive())?;
        let vanilla = archive
            .read(ITEMS_GAME)?
            .ok_or_else(|| Error::format("the game's archive", format!("{ITEMS_GAME} is missing")))?;
        let game = ItemsGame::parse(&vanilla)?;
        let is_a_file = |pick: &Pick| trees::is_pick(pick) || map_effect::is_pick(pick);
        let for_items: Vec<Pick> = picks.iter().filter(|pick| !is_a_file(pick)).cloned().collect();
        let hero_files = for_items.iter().any(effects::is_emblem_pick).then(|| HeroFiles::read(&archive));
        let live = |hero: &str, slot: &str| hero_files.as_ref().is_none_or(|files| files.has_slot(hero, slot));
        let mut patched = apply::patch_for(&game, &vanilla, &for_items, &live, effects_for)?;

        let mut missing = missing_files(&self.dota, &archive, &patched.changes);
        let moved = moved_files(&archive, &patched.changes)?;
        let is_there = |file: &String| moved.iter().any(|(path, _)| *path == format!("{}_c", resource::normal(file)));
        missing.models.retain(|file| !is_there(file));
        missing.others.retain(|file| !is_there(file));
        if !missing.models.is_empty() {
            return Err(Error::MissingModels(missing.models));
        }
        let compiled = |model: &str| format!("{}_c", resource::normal(model));
        let moved_data = |path: &str| moved.iter().find(|(known, _)| known == path).map(|(_, data)| data.clone());
        let exists = |model: &str| archive.contains(&compiled(model)) || moved_data(&compiled(model)).is_some();
        apply::place_models(&game, &mut patched, &exists)?;
        let after = ItemsGame::parse(&patched.bytes)?;
        let body_swaps = bodies::swaps(&game, &after);
        let bodies_placed = if body_swaps.is_empty() {
            Vec::new()
        } else {
            let read_now;
            let heroes = match &hero_files {
                Some(files) => files,
                None => {
                    read_now = HeroFiles::read(&archive);
                    &read_now
                }
            };
            bodies::placements(&body_swaps, &|hero| heroes.model(hero).map(str::to_string), &exists, &patched.placed).0
        };
        let mut placed_files: Vec<NewFile> = Vec::new();
        for placed in patched.placed.iter().chain(&bodies_placed) {
            let from = compiled(&placed.model);
            let data = match archive.read(&from)? {
                Some(data) => data,
                None => moved_data(&from).ok_or_else(|| Error::format("the game's archive", format!("{from} is missing")))?,
            };
            placed_files.push((compiled(&placed.at), data));
        }
        let swaps = sounds::swaps(&game, &after);
        drop(after);
        let sound_files = if swaps.is_empty() { Vec::new() } else { sounds::Sounds::read(&archive)?.files_for(&swaps)?.files };
        let mut emblem_files: Vec<NewFile> = Vec::new();
        for change in patched.changes.iter().filter(|change| change.hero.is_none() && change.slot == effects::EMBLEM_SLOT) {
            for path in change.files.iter().map(|file| compiled(file)) {
                let held = archive.read(&path)?.and_then(|data| effects::held_on_the_hero(&data).ok().flatten());
                emblem_files.extend(held.map(|data| (path, data)));
            }
        }
        let apply::Patched { bytes, mut changes, .. } = patched;
        let mut own_files: Vec<NewFile> = moved;
        own_files.retain(|(path, _)| !placed_files.iter().chain(&sound_files).chain(&emblem_files).any(|(placed, _)| placed == path));
        own_files.extend(placed_files);
        own_files.extend(sound_files);
        own_files.extend(emblem_files);
        let mut slots_taken: Vec<&str> = Vec::new();
        for (place, pick) in picks.iter().enumerate().filter(|(_, pick)| is_a_file(pick)) {
            if slots_taken.iter().any(|taken| taken.eq_ignore_ascii_case(&pick.slot)) {
                return Err(Error::Refused(format!("{} is picked more than once", pick.place())));
            }
            slots_taken.push(&pick.slot);
            let (slot, id, name, files) = if trees::is_pick(pick) {
                let kind = trees::kind(&pick.item).ok_or_else(|| Error::NotFound(format!("there are no trees called {}", pick.item)))?;
                (trees::SLOT, kind.id, kind.name, kind.files(&archive)?)
            } else {
                let item = game.item(&pick.item).ok_or_else(|| Error::NotFound(format!("there is no item with id {}", pick.item)))?;
                (map_effect::SLOT, item.id, item.name.unwrap_or(item.id), map_effect::files(&game, &archive, item)?)
            };
            own_files.extend(files);
            let change = Change {
                hero: None,
                slot: slot.to_string(),
                default_id: String::new(),
                item_id: id.to_string(),
                item_name: name.to_string(),
                style: 0,
                model: None,
                files: Vec::new(),
            };
            changes.insert(place.min(changes.len()), change);
        }
        let mut files = vec![PackFile { path: ITEMS_GAME, data: &bytes }];
        files.extend(own_files.iter().map(|(path, data)| PackFile { path, data }));
        let package = vpk::build(&files)?;

        let files = self.read_game_files()?;
        if !files.branch_is_valves {
            return Err(Error::NotValves);
        }
        let declared_path = files.declared_path.as_deref().unwrap_or_default();
        let gameinfo = read_text(&self.dota.gameinfo())?;
        let new_branch = gameinfo::add_our_lines(&files.branch_without_ours, &gameinfo, MOD_FOLDER)?;
        let our_line = signatures::make_line(declared_path, new_branch.as_bytes());
        let new_signatures = format!("{}{our_line}{}", files.valve_signatures, files.line_break);

        self.check_not_running()?;
        self.back_up_originals(&files)?;
        self.forget_applied()?;
        fsutil::create_dir_all(&self.dota.mod_dir())?;
        fsutil::write_atomic(&self.dota.mod_package(), &package)?;
        fsutil::write_atomic(&self.dota.signatures(), new_signatures.as_bytes())?;
        fsutil::write_atomic(&self.dota.gameinfo_branch(), new_branch.as_bytes())?;
        self.save(&Saved {
            picks: picks.to_vec(),
            effects_for: effects_for.clone(),
            game_build: self.dota.version().map(|v| v.build).unwrap_or_default(),
            applied_unix: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
            rewrite_rules: REWRITE_RULES,
        })?;

        Ok(Applied { changes, missing_files: missing.others, package_len: package.len() })
    }

    pub fn restore(&self) -> Result<Restored> {
        self.check_not_running()?;
        let files = self.read_game_files()?;
        let mut done = Restored::default();

        if files.has_our_lines {
            fsutil::write_atomic(&self.dota.gameinfo_branch(), files.branch_without_ours.as_bytes())?;
            done.gameinfo_restored = true;
        }
        if !files.branch_is_valves {
            done.notes.push(Note::BranchNotValves);
        }
        if files.has_extra_lines {
            if files.has_our_lines || files.branch_is_valves {
                fsutil::write_atomic(&self.dota.signatures(), files.valve_signatures.as_bytes())?;
                done.signatures_restored = true;
            } else {
                done.notes.push(Note::OtherToolsLines);
            }
        }
        done.package_removed = fsutil::remove_dir_all_if_exists(&self.dota.mod_dir())?;
        self.forget_applied()?;
        Ok(done)
    }

    pub fn status(&self) -> Result<Status> {
        let files = self.read_game_files()?;
        let package = self.dota.mod_package().is_file();

        let state = if !files.branch_is_valves {
            ModState::ChangedByOther(format!("{BRANCH_FILE} is not the file Valve shipped"))
        } else if files.has_our_lines {
            if !files.extra_line_matches_branch {
                ModState::NeedsRepair(UNDECLARED.into())
            } else if !package {
                ModState::NeedsRepair("the mod package is missing".into())
            } else {
                ModState::On
            }
        } else if files.has_extra_lines {
            ModState::NeedsRepair(LEFTOVER.into())
        } else if package {
            ModState::NeedsRepair("the mod package is there but the game is not set up to load it".into())
        } else {
            ModState::Off
        };

        let version = self.dota.version()?;
        let saved = self.load();
        Ok(Status {
            package_is_current: saved.game_build == version.build && saved.rewrite_rules == REWRITE_RULES,
            version,
            running: process::is_dota_running(),
            steam_busy: self.dota.steam_is_busy(),
            state,
            applied_build: saved.game_build,
            picks: saved.picks,
            effects_for: saved.effects_for,
        })
    }

    fn check_not_running(&self) -> Result<()> {
        if self.refuse_while_running && process::is_dota_running() {
            Err(Error::DotaRunning)
        } else {
            Ok(())
        }
    }

    fn read_game_files(&self) -> Result<GameFiles> {
        let signature_bytes = fsutil::read(&self.dota.signatures())?;
        let parsed = Signatures::parse(&signature_bytes)?;
        let branch = read_text(&self.dota.gameinfo_branch())?;
        let branch_without_ours = gameinfo::remove_our_lines(&branch)?;

        let declared = parsed.valve_line_for(BRANCH_FILE);
        Ok(GameFiles {
            valve_signatures: parsed.without_extra(),
            has_extra_lines: !parsed.extra.trim().is_empty(),
            extra_line_matches_branch: parsed
                .extra_lines()
                .any(|line| is_branch_file(line.path) && signatures::matches(&line, branch.as_bytes())),
            declared_path: declared.map(|line| line.path.to_string()),
            line_break: parsed.newline,
            has_our_lines: gameinfo::has_our_lines(&branch),
            branch_is_valves: declared.is_some_and(|line| signatures::matches(&line, branch_without_ours.as_bytes())),
            branch_without_ours,
        })
    }

    fn back_up_originals(&self, files: &GameFiles) -> Result<()> {
        let build = self.dota.version().map(|v| v.build).unwrap_or_else(|_| "unknown".to_string());
        let folder = self.data_dir.join("backup").join(build);
        fsutil::create_dir_all(&folder)?;
        for (name, contents) in [("dota.signatures", &files.valve_signatures), (BRANCH_FILE, &files.branch_without_ours)] {
            let path = folder.join(name);
            if !path.exists() {
                fsutil::write_atomic(&path, contents.as_bytes())?;
            }
        }
        Ok(())
    }

    fn state_file(&self) -> PathBuf {
        self.data_dir.join("state.json")
    }

    fn save(&self, saved: &Saved) -> Result<()> {
        fsutil::create_dir_all(&self.data_dir)?;
        let json = serde_json::to_vec_pretty(saved).expect("the saved state is plain data");
        fsutil::write_atomic(&self.state_file(), &json)
    }

    fn forget_applied(&self) -> Result<()> {
        match std::fs::remove_file(self.state_file()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Error::io("delete", &self.state_file(), e)),
            _ => Ok(()),
        }
    }

    fn load(&self) -> Saved {
        std::fs::read(self.state_file())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }
}

pub fn default_data_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Dota2ModEnjoyer")
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct MissingFiles {
    pub models: Vec<String>,
    pub others: Vec<String>,
}

pub fn missing_files(dota: &DotaInstall, archive: &Archive, changes: &[Change]) -> MissingFiles {
    let mut missing = MissingFiles::default();
    for change in changes {
        let model = change.model.as_deref();
        if let Some(model) = model
            && !game_has_file(dota, archive, model)
        {
            missing.models.push(model.to_string());
        }
        let others = change.files.iter().filter(|file| Some(file.as_str()) != model);
        missing.others.extend(others.filter(|file| !game_has_file(dota, archive, file)).cloned());
    }
    for list in [&mut missing.models, &mut missing.others] {
        list.sort();
        list.dedup();
    }
    missing
}

fn moved_files(archive: &Archive, changes: &[Change]) -> Result<Vec<NewFile>> {
    let wholeness = Wholeness::new(archive);
    for file in changes.iter().flat_map(|change| &change.files) {
        wholeness.is_whole(file);
    }
    let copy = |(named, there): (String, String)| {
        let data = archive.read(&there)?.ok_or_else(|| Error::format("the game's archive", format!("{there} is missing")))?;
        Ok((named, data))
    };
    wholeness.stand_ins().into_iter().map(copy).collect()
}

pub fn game_has_file(dota: &DotaInstall, archive: &Archive, file: &str) -> bool {
    archive.contains(&format!("{file}_c")) || dota.map_package(file).is_some_and(|package| package.is_file())
}

fn read_text(path: &Path) -> Result<String> {
    String::from_utf8(fsutil::read(path)?)
        .map_err(|_| Error::format(path.display().to_string(), "the file is not plain text"))
}

fn is_branch_file(declared_path: &str) -> bool {
    declared_path
        .rsplit(['\\', '/'])
        .next()
        .is_some_and(|name| name.eq_ignore_ascii_case(BRANCH_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gameinfo::tests::{BRANCH_TEXT, GAMEINFO_TEXT};
    use crate::items::ItemsGame;
    use crate::items::tests::SAMPLE;

    const AXE: &str = "npc_dota_hero_axe";
    const FILES: [&str; 4] = [
        "models/items/axe/vanguard_weapon/vanguard_weapon.vmdl_c",
        "models/items/axe/vanguard_head/vanguard_head.vmdl_c",
        "models/items/axe/vanguard_misc/vanguard_misc.vmdl_c",
        "particles/econ/items/axe/vanguard/weapon.vpcf_c",
    ];

    struct Fake {
        _dir: tempfile::TempDir,
        engine: Engine,
        signatures: String,
    }

    fn fake_install(game_files: &[&str]) -> Fake {
        let files: Vec<(&str, &[u8])> = game_files.iter().map(|&path| (path, b"compiled".as_slice())).collect();
        fake_install_of(SAMPLE, &files)
    }

    fn fake_install_of(items_game: &str, game_files: &[(&str, &[u8])]) -> Fake {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("dota 2 beta").join("game");
        let (bin, dota) = (game.join("bin").join("win64"), game.join("dota"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&dota).unwrap();
        std::fs::write(bin.join("dota2.exe"), b"").unwrap();
        std::fs::write(dota.join("gameinfo.gi"), GAMEINFO_TEXT).unwrap();
        std::fs::write(dota.join(BRANCH_FILE), BRANCH_TEXT).unwrap();
        std::fs::write(dota.join("steam.inf"), "ClientVersion=6973\r\nVersionDate=Nov 09 2026\r\n").unwrap();

        let signatures = format!(
            "{}\r\n{}\r\nDIGEST:0123456789ABCDEF\r\n",
            signatures::make_line("...\\..\\..\\dota\\gameinfo.gi", GAMEINFO_TEXT.as_bytes()),
            signatures::make_line("...\\..\\..\\dota\\gameinfo_branchspecific.gi", BRANCH_TEXT.as_bytes()),
        );
        std::fs::write(bin.join("dota.signatures"), &signatures).unwrap();

        let mut files = vec![PackFile { path: ITEMS_GAME, data: items_game.as_bytes() }];
        files.extend(game_files.iter().map(|&(path, data)| PackFile { path, data }));
        std::fs::write(dota.join("pak01_dir.vpk"), vpk::build(&files).unwrap()).unwrap();

        let engine = Engine {
            dota: DotaInstall::at(dir.path().join("dota 2 beta")).unwrap(),
            data_dir: dir.path().join("data"),
            refuse_while_running: false,
        };
        Fake { _dir: dir, engine, signatures }
    }

    fn picks() -> Vec<Pick> {
        let game = ItemsGame::parse(SAMPLE.as_bytes()).unwrap();
        apply::set_picks(&game, AXE, "armor_of_the_vanguard", 0).unwrap().picks
    }

    fn read(path: PathBuf) -> String {
        String::from_utf8(std::fs::read(path).unwrap()).unwrap()
    }

    fn state(fake: &Fake) -> ModState {
        fake.engine.status().unwrap().state
    }

    #[test]
    fn a_fresh_install_is_off() {
        let fake = fake_install(&FILES);
        let status = fake.engine.status().unwrap();
        assert_eq!(status.state, ModState::Off);
        assert_eq!(status.version.build, "6973");
        assert!(status.picks.is_empty());
    }

    #[test]
    fn apply_writes_the_package_and_sets_the_game_up() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let applied = fake.engine.apply(&picks()).unwrap();
        assert_eq!(applied.changes.len(), 3);
        assert!(applied.missing_files.is_empty());

        let package = Archive::open(&dota.mod_package()).unwrap();
        assert_eq!(package.len(), 3);
        assert!(package.verify_checksums().unwrap().all_ok());
        let inside = package.read(ITEMS_GAME).unwrap().unwrap();
        assert_eq!(applied.package_len, std::fs::metadata(dota.mod_package()).unwrap().len() as usize);
        let game = ItemsGame::parse(&inside).unwrap();
        assert_eq!(game.item("2").unwrap().model, Some("models/heroes/axe/axe_weapon.vmdl"));

        let branch = read(dota.gameinfo_branch());
        assert!(branch.contains("Game\tmod_enjoyer"));
        let signatures = read(dota.signatures());
        assert!(signatures.starts_with(&fake.signatures));
        let parsed = Signatures::parse(signatures.as_bytes()).unwrap();
        let ours: Vec<_> = parsed.extra_lines().collect();
        assert_eq!(ours.len(), 1);
        assert_eq!(ours[0].path, "...\\..\\..\\dota\\gameinfo_branchspecific.gi");
        assert!(signatures::matches(&ours[0], branch.as_bytes()));

        let status = fake.engine.status().unwrap();
        assert_eq!(status.state, ModState::On);
        assert_eq!(status.picks, picks());

        for folder in [dota.signatures(), dota.gameinfo_branch(), dota.mod_package()] {
            let leftovers = std::fs::read_dir(folder.parent().unwrap())
                .unwrap()
                .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".dme-tmp"))
                .count();
            assert_eq!(leftovers, 0);
        }
    }

    #[test]
    fn other_trees_go_in_the_package_as_files_of_their_own() {
        let materials = crate::trees::tests::trees_of(trees::kind("autumn").unwrap());
        let mut files = FILES.to_vec();
        files.extend(materials.iter().map(String::as_str));
        let fake = fake_install(&files);
        let dota = &fake.engine.dota;
        let autumn = Pick::world("trees", "autumn");
        let mut both = picks();
        both.insert(1, autumn.clone());

        let applied = fake.engine.apply(&both).unwrap();
        let told: Vec<(&str, &str)> = applied.changes.iter().map(|change| (change.slot.as_str(), change.item_name.as_str())).collect();
        assert_eq!(told, [("weapon", "Axe of the Vanguard"), ("trees", "Autumn trees"), ("head", "Helm of the \\\"Vanguard\\\""), ("misc", "Bracers of the Vanguard")]);

        let package = Archive::open(&dota.mod_package()).unwrap();
        assert_eq!(package.len(), 10, "with the two picked models where the default items' own are");
        assert!(package.verify_checksums().unwrap().all_ok());
        for own in ["tree_oak_leaves_00", "tree_oak_leaves_05", "tree_pine_frond_00", "dire_tree007c"] {
            assert!(package.contains(&format!("materials/models/props_tree/{own}.vmat_c")), "{own}");
        }
        assert!(!package.contains("materials/models/props_tree/tree_oak_autumn_leaves_00.vmat_c"));
        let inside = package.read(ITEMS_GAME).unwrap().unwrap();
        let game = ItemsGame::parse(&inside).unwrap();
        assert_eq!(game.item("2").unwrap().model, Some("models/heroes/axe/axe_weapon.vmdl"));

        assert_eq!(fake.engine.status().unwrap().picks, both);
        assert!(matches!(fake.engine.make_ready(&both, &EffectsFor::Every).unwrap(), Ready::Already));
        assert!(matches!(fake.engine.make_ready(std::slice::from_ref(&autumn), &EffectsFor::Every).unwrap(), Ready::Applied(applied) if applied.changes.len() == 1));
        let package = Archive::open(&dota.mod_package()).unwrap();
        assert_eq!(package.len(), 8);
        assert_eq!(package.read(ITEMS_GAME).unwrap().unwrap(), SAMPLE.as_bytes());
        assert!(fake.engine.restore().unwrap().package_removed);
        assert!(!dota.mod_dir().exists());

        assert!(matches!(fake.engine.apply(&[Pick::world("trees", "winter")]), Err(Error::NotFound(_))));
        assert!(matches!(fake.engine.apply(&[Pick::world("trees", "palms")]), Err(Error::NotFound(_))));
        assert!(matches!(fake.engine.apply(&[autumn.clone(), autumn]), Err(Error::Refused(_))));
        assert_eq!(state(&fake), ModState::Off);
    }

    #[test]
    fn another_look_for_the_map_goes_in_the_package_as_a_file_of_its_own() {
        use crate::map_effect::tests::{GRAPHIC, MAP_EFFECTS, OWN_FILE};
        let fake = fake_install_of(MAP_EFFECTS, &[(GRAPHIC, b"the graphic look"), (OWN_FILE, b"the game's own look")]);
        let dota = &fake.engine.dota;
        let graphic = Pick::world("map_effect", "18380");
        let picks = [graphic.clone(), Pick::world("courier", "600")];

        let applied = fake.engine.apply(&picks).unwrap();
        let told: Vec<(&str, &str)> = applied.changes.iter().map(|change| (change.slot.as_str(), change.item_name.as_str())).collect();
        assert_eq!(told, [("map_effect", "Graphic Shader Effect"), ("courier", "A Courier")], "one change for each pick, in their order");

        let package = Archive::open(&dota.mod_package()).unwrap();
        assert_eq!(package.len(), 2);
        assert_eq!(package.read(OWN_FILE).unwrap().unwrap(), b"the graphic look");
        assert!(!package.contains(GRAPHIC));
        assert_eq!(fake.engine.status().unwrap().picks, picks);
        assert!(matches!(fake.engine.make_ready(&picks, &EffectsFor::Every).unwrap(), Ready::Already));

        fake.engine.apply(std::slice::from_ref(&graphic)).unwrap();
        let package = Archive::open(&dota.mod_package()).unwrap();
        assert_eq!(package.read(ITEMS_GAME).unwrap().unwrap(), MAP_EFFECTS.as_bytes());
        assert!(fake.engine.restore().unwrap().package_removed);

        for wrong in ["18381", "600", "99999"] {
            assert!(matches!(fake.engine.apply(&[Pick::world("map_effect", wrong)]), Err(Error::NotFound(_))), "{wrong}");
        }
        assert!(matches!(fake.engine.apply(&[graphic.clone(), graphic]), Err(Error::Refused(_))));
        assert_eq!(state(&fake), ModState::Off);
    }

    #[test]
    fn an_emblem_rides_on_a_slot_that_the_heros_own_file_lists() {
        let items = crate::effects::tests::emblems();
        let axe = br##""DOTAHeroes" { "npc_dota_hero_axe" { "ItemSlots" {
            "0" { "SlotName" "weapon" "SlotText" "#LoadoutSlot_Weapon" }
            "1" { "SlotName" "head" "SlotText" "#LoadoutSlot_Head" }
        } } }"##;
        let effect = ("particles/events/t17/hero_effect.vpcf_c", b"compiled".as_slice());
        let carriers = |fake: &Fake| -> Vec<String> {
            let inside = Archive::open(&fake.engine.dota.mod_package()).unwrap().read(ITEMS_GAME).unwrap().unwrap();
            let game = ItemsGame::parse(&inside).unwrap();
            let has_it = |item: &&crate::items::Item<'_>| String::from_utf8_lossy(&inside[item.block.start()..item.block.end()]).contains("t17/hero_effect.vpcf");
            game.items().iter().filter(|item| item.is_default()).filter(has_it).map(|item| item.id.to_string()).collect()
        };
        let emblem = [Pick::world("emblem", "638")];

        let with_his_file = fake_install_of(&items, &[effect, ("scripts/npc/heroes/npc_dota_hero_axe.txt", axe.as_slice())]);
        let applied = with_his_file.engine.apply(&emblem).unwrap();
        assert_eq!(applied.changes.len(), 1);
        assert!(applied.missing_files.is_empty());
        assert_eq!(carriers(&with_his_file), ["2", "6", "8"], "his hair, which he always wears; the other heroes as the item database has them");

        let without = fake_install_of(&items, &[effect]);
        without.engine.apply(&emblem).unwrap();
        assert_eq!(carriers(&without), ["3", "6", "8"]);
    }

    #[test]
    fn an_emblems_effect_goes_in_the_package_held_by_the_hero() {
        use crate::effects::tests::{drivers_of, emblem_file};
        use crate::kv3::tests::text;
        let items = crate::effects::tests::emblems();
        let path = "particles/events/t17/hero_effect.vpcf_c";
        let emblem = [Pick::world("emblem", "638")];
        let package_of = |file: &[u8]| {
            let fake = fake_install_of(&items, &[(path, file)]);
            fake.engine.apply(&emblem).unwrap();
            Archive::open(&fake.engine.dota.mod_package()).unwrap()
        };

        let package = package_of(&emblem_file("self", Some("PATTACH_WORLDORIGIN")));
        let written = package.read(path).unwrap().expect("the emblem's effect is in the package");
        assert_eq!(drivers_of(&written)[0], [("m_entityName".to_string(), text("parent"))]);
        assert_eq!(package.len(), 2);

        assert!(!package_of(&emblem_file("parent", None)).contains(path));
        assert!(!package_of(b"compiled").contains(path));
    }

    #[test]
    fn restore_gives_back_the_original_bytes() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        fake.engine.apply(&picks()).unwrap();

        let restored = fake.engine.restore().unwrap();
        assert_eq!(
            restored,
            Restored { gameinfo_restored: true, signatures_restored: true, package_removed: true, notes: vec![] }
        );
        assert_eq!(read(dota.gameinfo_branch()), BRANCH_TEXT);
        assert_eq!(read(dota.signatures()), fake.signatures);
        assert!(!dota.mod_dir().exists());
        let status = fake.engine.status().unwrap();
        assert_eq!(status.state, ModState::Off);
        assert!(status.picks.is_empty());

        assert_eq!(fake.engine.restore().unwrap(), Restored::default());
    }

    #[test]
    fn originals_are_backed_up_once_and_never_overwritten() {
        let fake = fake_install(&FILES);
        fake.engine.apply(&picks()).unwrap();
        fake.engine.apply(&picks()).unwrap();
        let backup = fake.engine.data_dir.join("backup").join("6973");
        assert_eq!(read(backup.join("dota.signatures")), fake.signatures);
        assert_eq!(read(backup.join(BRANCH_FILE)), BRANCH_TEXT);
    }

    #[test]
    fn applying_again_does_not_stack_lines() {
        let fake = fake_install(&FILES);
        fake.engine.apply(&picks()).unwrap();
        let first = (read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch()));
        fake.engine.apply(&picks()).unwrap();
        let second = (read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch()));
        assert_eq!(first, second);
        assert_eq!(state(&fake), ModState::On);
    }

    #[test]
    fn a_dota_update_is_noticed_and_repaired_by_applying_again() {
        let fake = fake_install(&FILES);
        fake.engine.apply(&picks()).unwrap();

        std::fs::write(fake.engine.dota.signatures(), &fake.signatures).unwrap();
        assert!(state(&fake).is_half_undone());
        fake.engine.apply(&picks()).unwrap();
        assert_eq!(state(&fake), ModState::On);

        let (signatures, ours) = (read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch()));
        std::fs::write(fake.engine.dota.gameinfo_branch(), BRANCH_TEXT).unwrap();
        assert!(state(&fake).is_half_undone());
        fake.engine.apply(&picks()).unwrap();
        assert_eq!(state(&fake), ModState::On);

        std::fs::remove_file(fake.engine.dota.mod_package()).unwrap();
        assert!(matches!(state(&fake), ModState::NeedsRepair(_)) && !state(&fake).is_half_undone());
        assert!(!ModState::On.is_half_undone() && !ModState::Off.is_half_undone());

        fake.engine.apply(&picks()).unwrap();
        std::fs::write(fake.engine.dota.signatures(), &fake.signatures).unwrap();
        fake.engine.restore().unwrap();
        assert_eq!(state(&fake), ModState::Off);
        assert_eq!((read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch())), (fake.signatures.clone(), BRANCH_TEXT.to_string()));
        fake.engine.apply(&picks()).unwrap();
        assert_eq!((read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch())), (signatures, ours));
        std::fs::write(fake.engine.dota.gameinfo_branch(), BRANCH_TEXT).unwrap();
        fake.engine.restore().unwrap();
        assert_eq!(state(&fake), ModState::Off);
        assert_eq!((read(fake.engine.dota.signatures()), read(fake.engine.dota.gameinfo_branch())), (fake.signatures.clone(), BRANCH_TEXT.to_string()));
    }

    fn update_to(fake: &Fake, build: &str, game_files: &[&str]) {
        let dota = &fake.engine.dota;
        std::fs::write(dota.steam_inf(), format!("ClientVersion={build}
VersionDate=Nov 20 2026
")).unwrap();
        std::fs::write(dota.signatures(), &fake.signatures).unwrap();
        let mut files = vec![PackFile { path: ITEMS_GAME, data: SAMPLE.as_bytes() }];
        files.extend(game_files.iter().map(|&path| PackFile { path, data: b"compiled of the new build" }));
        std::fs::write(dota.game_archive(), vpk::build(&files).unwrap()).unwrap();
    }

    #[test]
    fn the_picks_are_put_back_in_a_game_that_a_dota_update_left_half_undone() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        fake.engine.apply(&picks()).unwrap();
        let edited = (read(dota.signatures()), read(dota.gameinfo_branch()));
        assert!(!fake.engine.status().unwrap().undone_by_an_update(), "the picks are in the game as Play put them");
        assert!(matches!(fake.engine.mend(&picks(), &EffectsFor::Every).unwrap(), Mended::Later), "nothing to put back");

        update_to(&fake, "6974", &FILES);
        let status = fake.engine.status().unwrap();
        assert!(status.state.is_half_undone() && status.undone_by_an_update());
        let Mended::Back(applied) = fake.engine.mend(&picks(), &EffectsFor::Every).unwrap() else { panic!("the picks are put back") };
        assert_eq!(applied.changes.len(), 3);
        let status = fake.engine.status().unwrap();
        assert_eq!(status.state, ModState::On);
        assert!(status.package_is_current && status.applied_build == "6974" && !status.undone_by_an_update());
        assert_eq!((read(dota.signatures()), read(dota.gameinfo_branch())), edited, "both files edited again, as Play edits them");
        let package = Archive::open(&dota.mod_package()).unwrap();
        let models = package.entries().iter().filter(|entry| package.path_of(entry).ends_with(".vmdl_c")).map(|entry| package.read_entry(entry).unwrap()).collect::<Vec<_>>();
        assert!(!models.is_empty() && models.iter().all(|data| data == b"compiled of the new build"));

        let backup = fake.engine.data_dir.join("backup").join("6974");
        assert_eq!(read(backup.join("dota.signatures")), fake.signatures);
        assert_eq!(read(backup.join(BRANCH_FILE)), BRANCH_TEXT);
        fake.engine.restore().unwrap();
        assert_eq!((read(dota.signatures()), read(dota.gameinfo_branch())), (fake.signatures.clone(), BRANCH_TEXT.to_string()));
    }

    #[test]
    fn the_picks_are_put_back_where_an_update_changed_both_files_but_not_where_steam_verified_them() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        fake.engine.apply(&picks()).unwrap();

        std::fs::write(dota.signatures(), &fake.signatures).unwrap();
        std::fs::write(dota.gameinfo_branch(), BRANCH_TEXT).unwrap();
        assert!(matches!(state(&fake), ModState::NeedsRepair(_)) && !state(&fake).is_half_undone());
        assert!(!fake.engine.status().unwrap().undone_by_an_update());
        assert!(matches!(fake.engine.mend(&picks(), &EffectsFor::Every).unwrap(), Mended::Later));

        update_to(&fake, "6974", &FILES);
        assert!(fake.engine.status().unwrap().undone_by_an_update());
        assert!(matches!(fake.engine.mend(&picks(), &EffectsFor::Every).unwrap(), Mended::Back(_)));
        assert_eq!(state(&fake), ModState::On);

        fake.engine.restore().unwrap();
        update_to(&fake, "6975", &FILES);
        assert!(!fake.engine.status().unwrap().undone_by_an_update());
    }

    #[test]
    fn a_game_that_the_picks_cannot_be_put_back_in_is_put_back_to_normal() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        fake.engine.apply(&picks()).unwrap();
        update_to(&fake, "6974", &[]);
        assert!(state(&fake).is_half_undone());
        let Mended::Normal { why, restored } = fake.engine.mend(&picks(), &EffectsFor::Every).unwrap() else { panic!("put back to normal") };
        assert!(why.to_string().contains("models"), "{why}");
        assert!(restored.gameinfo_restored && restored.package_removed);
        assert_eq!(state(&fake), ModState::Off, "never left half undone");
        assert_eq!((read(dota.signatures()), read(dota.gameinfo_branch())), (fake.signatures.clone(), BRANCH_TEXT.to_string()));
        assert!(!fake.engine.status().unwrap().undone_by_an_update(), "and not tried again");
        assert!(fake.engine.mend(&[], &EffectsFor::Every).is_err(), "with no picks, going back to normal is what is wanted");
    }

    #[test]
    fn a_file_changed_by_something_else_is_not_touched() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let foreign = BRANCH_TEXT.replace("570", "571");
        std::fs::write(dota.gameinfo_branch(), &foreign).unwrap();

        assert!(matches!(state(&fake), ModState::ChangedByOther(_)));
        assert!(matches!(fake.engine.apply(&picks()), Err(Error::NotValves)));
        assert_eq!(read(dota.gameinfo_branch()), foreign);
        assert_eq!(read(dota.signatures()), fake.signatures);
        assert!(!dota.mod_dir().exists());
        assert!(!fake.engine.data_dir.exists());

        let restored = fake.engine.restore().unwrap();
        assert!(!restored.gameinfo_restored && !restored.signatures_restored);
        assert_eq!(restored.notes.len(), 1);
        assert_eq!(read(dota.gameinfo_branch()), foreign);
    }

    #[test]
    fn another_tools_signature_line_is_left_alone() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let foreign_branch = BRANCH_TEXT.replace("570", "571");
        let foreign_signatures = format!(
            "{}{}\r\n",
            fake.signatures,
            signatures::make_line("...\\..\\..\\dota\\gameinfo_branchspecific.gi", foreign_branch.as_bytes())
        );
        std::fs::write(dota.gameinfo_branch(), &foreign_branch).unwrap();
        std::fs::write(dota.signatures(), &foreign_signatures).unwrap();

        let restored = fake.engine.restore().unwrap();
        assert!(!restored.signatures_restored);
        assert_eq!(restored.notes.len(), 2);
        assert_eq!(read(dota.signatures()), foreign_signatures);
    }

    #[test]
    fn an_item_whose_model_the_game_lacks_is_refused_before_anything_is_written() {
        let fake = fake_install(&FILES[..2]);
        let dota = &fake.engine.dota;
        let error = fake.engine.apply(&picks()).unwrap_err();
        assert!(matches!(&error, Error::MissingModels(models) if models.iter().any(|model| model.contains("vanguard_misc.vmdl"))), "{error}");
        assert_eq!(read(dota.gameinfo_branch()), BRANCH_TEXT);
        assert_eq!(read(dota.signatures()), fake.signatures);
        assert!(!dota.mod_dir().exists());
        assert_eq!(state(&fake), ModState::Off);
    }

    #[test]
    fn the_picked_models_go_in_the_package_where_the_default_items_own_are() {
        let files: Vec<(&str, &[u8])> =
            vec![(FILES[0], b"the vanguard axe"), (FILES[1], b"the vanguard helm"), (FILES[2], b"the vanguard bracers"), (FILES[3], b"the effect")];
        let fake = fake_install_of(SAMPLE, &files);
        let applied = fake.engine.apply(&picks()).unwrap();
        assert_eq!(applied.changes[0].model.as_deref(), Some("models/items/axe/vanguard_weapon/vanguard_weapon.vmdl"));

        let package = Archive::open(&fake.engine.dota.mod_package()).unwrap();
        assert_eq!(package.read("models/heroes/axe/axe_weapon.vmdl_c").unwrap().unwrap(), b"the vanguard axe");
        assert_eq!(package.read("models/heroes/axe/axe_ponytail.vmdl_c").unwrap().unwrap(), b"the vanguard helm");
        assert_eq!(package.len(), 3);
        let inside = package.read(ITEMS_GAME).unwrap().unwrap();
        let game = ItemsGame::parse(&inside).unwrap();
        assert_eq!(game.item("2").unwrap().model, Some("models/heroes/axe/axe_weapon.vmdl"));
        assert_eq!(game.item("4").unwrap().model, Some("models/heroes/axe/axe_ponytail.vmdl"));
        assert_eq!(game.item("8653").unwrap().model, Some("models/items/axe/vanguard_misc/vanguard_misc.vmdl"));
    }

    #[test]
    fn the_picked_items_sounds_go_in_the_package_in_the_place_of_the_heros_own() {
        use crate::kv3::Value;
        use crate::kv3::tests::{compiled, object, text};
        let event = |file: &str| object(&[("type", text("dota_src1_3d")), ("vsnd_files", Value::Array(vec![text(file)]))]);
        let axe_sounds = compiled(&object(&[
            ("Hero_Axe.Attack", event("sounds/axe/attack.vsnd")),
            ("Hero_Axe.Attack.Vanguard", event("sounds/axe/vanguard.vsnd")),
        ]));
        let path = "soundevents/game_sounds_heroes/game_sounds_axe.vsndevts_c";
        let mut files: Vec<(&str, &[u8])> = FILES.iter().map(|&path| (path, b"compiled".as_slice())).collect();
        files.push((path, &axe_sounds));
        let swap = "\t\t\t\t\"asset_modifier1\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"sound\"\r\n\t\t\t\t\t\"asset\"\t\t\"Hero_Axe.Attack\"\r\n\t\t\t\t\t\"modifier\"\t\t\"Hero_Axe.Attack.Vanguard\"\r\n\t\t\t\t}\r\n";
        let marker = "\"modifier\"\t\t\"particles/econ/items/axe/vanguard/weapon.vpcf\"\r\n\t\t\t\t}\r\n";
        assert_eq!(SAMPLE.matches(marker).count(), 1);
        let items_game = SAMPLE.replacen(marker, &format!("{marker}{swap}"), 1);
        let fake = fake_install_of(&items_game, &files);
        fake.engine.apply(&picks()).unwrap();

        let package = Archive::open(&fake.engine.dota.mod_package()).unwrap();
        let written = package.read(path).unwrap().expect("Axe's sounds are in the package");
        let values = crate::kv3::Compiled::parse(&written).unwrap().values().unwrap();
        assert_eq!(values.get("Hero_Axe.Attack"), Some(&event("sounds/axe/vanguard.vsnd")));
        assert_eq!(values.get("Hero_Axe.Attack.Vanguard"), Some(&event("sounds/axe/vanguard.vsnd")));

        let fake = fake_install_of(SAMPLE, &files);
        fake.engine.apply(&picks()).unwrap();
        assert!(!Archive::open(&fake.engine.dota.mod_package()).unwrap().contains(path));
    }

    #[test]
    fn the_body_a_picked_item_gives_the_hero_goes_in_the_package_where_the_heros_own_is() {
        let hero_file = "\"DOTAHeroes\"\n{\n\"npc_dota_hero_axe\"\n{\n\"Model\" \"models/heroes/axe/axe.vmdl\"\n\"ItemSlots\"\n{\n\"0\"\n{\n\"SlotName\" \"weapon\"\n}\n}\n}\n}\n";
        let mut files: Vec<(&str, &[u8])> = FILES.iter().map(|&path| (path, b"compiled".as_slice())).collect();
        files.push(("scripts/npc/heroes/npc_dota_hero_axe.txt", hero_file.as_bytes()));
        files.push(("models/items/axe/bare/axe_bare.vmdl_c", b"the bare body"));
        let swap = "\t\t\t\t\"asset_modifier1\"\r\n\t\t\t\t{\r\n\t\t\t\t\t\"type\"\t\t\"entity_model\"\r\n\t\t\t\t\t\"asset\"\t\t\"npc_dota_hero_axe\"\r\n\t\t\t\t\t\"modifier\"\t\t\"models/items/axe/bare/axe_bare.vmdl\"\r\n\t\t\t\t}\r\n";
        let marker = "\"modifier\"\t\t\"particles/econ/items/axe/vanguard/weapon.vpcf\"\r\n\t\t\t\t}\r\n";
        let items_game = SAMPLE.replacen(marker, &format!("{marker}{swap}"), 1);
        let fake = fake_install_of(&items_game, &files);
        fake.engine.apply(&picks()).unwrap();

        let package = Archive::open(&fake.engine.dota.mod_package()).unwrap();
        assert_eq!(package.read("models/heroes/axe/axe.vmdl_c").unwrap().unwrap(), b"the bare body");

        let fake = fake_install_of(SAMPLE, &files);
        fake.engine.apply(&picks()).unwrap();
        assert!(!Archive::open(&fake.engine.dota.mod_package()).unwrap().contains("models/heroes/axe/axe.vmdl_c"));
    }

    #[test]
    fn a_missing_effect_is_reported_and_does_not_stop_the_apply() {
        let fake = fake_install(&FILES[..3]);
        let applied = fake.engine.apply(&picks()).unwrap();
        assert_eq!(applied.missing_files, ["particles/econ/items/axe/vanguard/weapon.vpcf"]);
        assert_eq!(state(&fake), ModState::On);
    }

    #[test]
    fn an_effect_the_game_has_one_folder_away_goes_in_the_package_under_its_name() {
        let mut files: Vec<(&str, &[u8])> = FILES[..3].iter().map(|&path| (path, b"compiled".as_slice())).collect();
        files.push(("particles/_2econ/items/axe/vanguard/weapon.vpcf_c", b"the effect"));
        let fake = fake_install_of(SAMPLE, &files);
        let applied = fake.engine.apply(&picks()).unwrap();
        assert!(applied.missing_files.is_empty(), "{:?}", applied.missing_files);

        let package = Archive::open(&fake.engine.dota.mod_package()).unwrap();
        assert_eq!(package.len(), 4, "the item database, the effect and two picked models");
        assert_eq!(package.read("particles/econ/items/axe/vanguard/weapon.vpcf_c").unwrap().unwrap(), b"the effect");
    }

    #[test]
    fn world_picks_and_styles_are_applied_and_remembered() {
        let mut files = FILES.to_vec();
        files.extend([
            "models/items/axe/crested/crested_gold.vmdl_c",
            "particles/gold.vpcf_c",
            "models/courier/drodo/drodo.vmdl_c",
            "particles/econ/courier/courier_drodo/ambient.vpcf_c",
        ]);
        let fake = fake_install(&files);
        let picks = vec![
            Pick::hero(AXE, "weapon", "9040"),
            Pick::hero(AXE, "head", "9500").styled(1),
            Pick::world("courier", "10003"),
        ];
        let applied = fake.engine.apply(&picks).unwrap();
        assert_eq!(applied.changes.len(), 3);
        assert!(applied.missing_files.is_empty(), "{:?}", applied.missing_files);

        let package = Archive::open(&fake.engine.dota.mod_package()).unwrap();
        assert!(package.contains("models/heroes/axe/axe_ponytail.vmdl_c"));
        let inside = package.read(ITEMS_GAME).unwrap().unwrap();
        let game = ItemsGame::parse(&inside).unwrap();
        assert_eq!(game.item("4").unwrap().model, Some("models/heroes/axe/axe_ponytail.vmdl"));
        assert_eq!(game.item("595").unwrap().block.str("particle_folder").unwrap(), b"particles/econ/courier/courier_drodo");

        let later = Engine {
            dota: fake.engine.dota.clone(),
            data_dir: fake.engine.data_dir.clone(),
            refuse_while_running: false,
        };
        assert_eq!(later.status().unwrap().picks, picks);
    }

    #[test]
    fn maps_are_looked_for_in_the_maps_folder() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let archive = Archive::open(&dota.game_archive()).unwrap();
        assert!(game_has_file(dota, &archive, "models/items/axe/vanguard_head/vanguard_head.vmdl"));
        assert!(game_has_file(dota, &archive, "Models\\Items\\Axe\\vanguard_head\\vanguard_head.vmdl"));
        assert!(!game_has_file(dota, &archive, "models/items/axe/other.vmdl"));

        let scene = "maps/scenes/mvp/mvp_pudge_arcana.vmap";
        assert!(!game_has_file(dota, &archive, "maps/dota_desert.vmap"));
        assert!(!game_has_file(dota, &archive, scene));
        for map in ["maps/dota_desert.vmap", scene] {
            let package = dota.map_package(map).unwrap();
            std::fs::create_dir_all(package.parent().unwrap()).unwrap();
            std::fs::write(package, b"").unwrap();
        }
        assert!(game_has_file(dota, &archive, "maps/dota_desert.vmap"));
        assert!(game_has_file(dota, &archive, scene));
        assert!(!game_has_file(dota, &archive, "maps/dota_winter.vmap"));
        assert!(!game_has_file(dota, &archive, "maps/mvp_pudge_arcana.vmap"), "the folder matters");
    }

    #[test]
    fn a_wrong_pick_changes_nothing() {
        let fake = fake_install(&FILES);
        let wrong = vec![Pick::hero(AXE, "weapon", "1")];
        assert!(matches!(fake.engine.apply(&wrong), Err(Error::NotFound(_))));
        assert_eq!(state(&fake), ModState::Off);
        assert!(!fake.engine.dota.mod_dir().exists());
    }

    #[test]
    fn getting_ready_writes_only_when_something_has_to_change() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let package_time = || std::fs::metadata(dota.mod_package()).unwrap().modified().unwrap();

        assert!(matches!(fake.engine.make_ready(&[], &EffectsFor::Every).unwrap(), Ready::Normal(done) if done == Restored::default()));
        assert!(!dota.mod_dir().exists());

        assert!(matches!(fake.engine.make_ready(&picks(), &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        let written = package_time();

        let mut reversed = picks();
        reversed.reverse();
        assert!(matches!(fake.engine.make_ready(&reversed, &EffectsFor::Every).unwrap(), Ready::Already));
        assert_eq!(package_time(), written);

        let fewer = &picks()[..2];
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Already));

        std::fs::write(dota.steam_inf(), "ClientVersion=6974\r\n").unwrap();
        assert!(!fake.engine.status().unwrap().package_is_current);
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Already));

        std::fs::write(dota.signatures(), &fake.signatures).unwrap();
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        assert_eq!(state(&fake), ModState::On);

        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Picked).unwrap(), Ready::Applied(_)));
        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Picked).unwrap(), Ready::Already));
        assert_eq!(fake.engine.status().unwrap().effects_for, EffectsFor::Picked);

        assert!(matches!(fake.engine.make_ready(&[], &EffectsFor::Every).unwrap(), Ready::Normal(done) if done.package_removed));
        assert_eq!(state(&fake), ModState::Off);
        assert_eq!(read(dota.signatures()), fake.signatures);
        assert_eq!(read(dota.gameinfo_branch()), BRANCH_TEXT);
    }

    #[test]
    fn picks_whose_writing_failed_half_way_are_not_taken_for_those_in_the_game() {
        let fake = fake_install(&FILES);
        let dota = &fake.engine.dota;
        let package = || std::fs::read(dota.mod_package()).unwrap();
        let fewer = &picks()[..2];
        fake.engine.apply(fewer).unwrap();
        let of_fewer = package();

        let writable = std::fs::metadata(dota.signatures()).unwrap().permissions();
        let mut read_only = writable.clone();
        read_only.set_readonly(true);
        std::fs::set_permissions(dota.signatures(), read_only).unwrap();
        let failed = fake.engine.apply(&picks());
        std::fs::set_permissions(dota.signatures(), writable).unwrap();
        assert!(matches!(failed, Err(Error::Io { .. })), "{failed:?}");
        assert_ne!(package(), of_fewer, "the package of the picks that failed is in the game");

        assert!(matches!(fake.engine.make_ready(fewer, &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        assert_eq!(package(), of_fewer);
    }

    #[test]
    fn picks_never_saved_are_the_ones_in_the_game() {
        let fake = fake_install(&FILES);
        assert!(fake.engine.loadout().is_empty(), "nothing saved, nothing in the game");

        fake.engine.apply(&picks()).unwrap();
        assert!(same_picks(fake.engine.loadout().picks(), &picks()));

        let saved = Loadout::from_picks([Pick::world("courier", "10003")]);
        saved.save(&fake.engine.loadout_file()).unwrap();
        assert_eq!(fake.engine.loadout(), saved);
        Loadout::default().save(&fake.engine.loadout_file()).unwrap();
        assert!(fake.engine.loadout().is_empty());

        saved.save(&fake.engine.loadout_file()).unwrap();
        fake.engine.restore().unwrap();
        assert_eq!(fake.engine.loadout(), saved);
        std::fs::remove_file(fake.engine.loadout_file()).unwrap();
        assert!(fake.engine.loadout().is_empty());
    }

    #[test]
    fn a_package_made_by_older_rules_is_written_again() {
        let fake = fake_install(&FILES);
        fake.engine.apply(&picks()).unwrap();
        assert!(fake.engine.status().unwrap().package_is_current);

        let state_file = fake.engine.data_dir.join("state.json");
        let current = read(state_file.clone());
        let older = current.replace(&format!("\"rewrite_rules\": {REWRITE_RULES}"), "\"rewrite_rules\": 1");
        assert_ne!(older, current);
        std::fs::write(&state_file, older).unwrap();

        assert!(!fake.engine.status().unwrap().package_is_current);
        assert!(matches!(fake.engine.make_ready(&picks(), &EffectsFor::Every).unwrap(), Ready::Applied(_)));
        assert!(matches!(fake.engine.make_ready(&picks(), &EffectsFor::Every).unwrap(), Ready::Already));
    }

    #[test]
    fn applying_no_picks_still_gives_a_loadable_vanilla_package() {
        let fake = fake_install(&FILES);
        let applied = fake.engine.apply(&[]).unwrap();
        assert!(applied.changes.is_empty());
        let inside = Archive::open(&fake.engine.dota.mod_package()).unwrap().read(ITEMS_GAME).unwrap().unwrap();
        assert_eq!(inside, SAMPLE.as_bytes());
        assert_eq!(state(&fake), ModState::On);
    }
}
