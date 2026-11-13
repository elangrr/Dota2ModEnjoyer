use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::error::{Error, Result};
use crate::vpk::{Archive, OpenFiles};

const CORE_ARCHIVE: [&str; 3] = ["..", "core", "pak01_dir.vpk"];
const INTERFACE_PICTURES: &str = "panorama/images/";

const WHAT: &str = "a compiled file of the game's";

pub fn references(file: &[u8]) -> Result<Vec<String>> {
    let cut_short = || Error::format(WHAT, "the file is cut short");
    let u32_at = |at: usize| file.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).ok_or_else(cut_short);

    let blocks_at = 8 + u32_at(8)?;
    let block_count = u32_at(12)?;
    let mut list = None;
    for index in 0..block_count.min(64) {
        let entry = blocks_at + 12 * index;
        if file.get(entry..entry + 4).ok_or_else(cut_short)? == b"RERL" {
            list = Some(entry + 4 + u32_at(entry + 4)?);
        }
    }
    let Some(list) = list else { return Ok(Vec::new()) };
    let entries = list + u32_at(list)?;
    let count = u32_at(list + 4)?;
    let mut found = Vec::new();
    for index in 0..count.min(100_000) {
        let at = entries + 16 * index + 8;
        let path_at = at + u32_at(at)?;
        let rest = file.get(path_at..).ok_or_else(cut_short)?;
        let path = &rest[..rest.iter().position(|&byte| byte == 0).ok_or_else(cut_short)?];
        found.push(String::from_utf8_lossy(path).into_owned());
    }
    Ok(found)
}

const CHECKED: [&str; 4] = [".vpcf", ".vtex", ".vmat", ".vmdl"];
const NAMING: [&str; 3] = [".vpcf", ".vmat", ".vmdl"];

pub struct Wholeness<'a> {
    archive: &'a Archive,
    core: Option<Archive>,
    known: RefCell<HashMap<String, bool>>,
    waiting: RefCell<Vec<String>>,
    waiting_at: RefCell<HashMap<String, usize>>,
    gone: RefCell<BTreeSet<String>>,
    stand_ins: RefCell<BTreeMap<String, String>>,
    open: OpenFiles,
}

impl<'a> Wholeness<'a> {
    pub fn new(archive: &'a Archive) -> Wholeness<'a> {
        let core = archive.dir_path().parent().map(|folder| CORE_ARCHIVE.iter().fold(folder.to_path_buf(), |path, part| path.join(part)));
        let core = core.filter(|path| path.is_file()).and_then(|path| Archive::open(&path).ok());
        Wholeness {
            archive,
            core,
            known: RefCell::new(HashMap::new()),
            waiting: RefCell::new(Vec::new()),
            waiting_at: RefCell::new(HashMap::new()),
            gone: RefCell::new(BTreeSet::new()),
            stand_ins: RefCell::new(BTreeMap::new()),
            open: OpenFiles::default(),
        }
    }

    fn stand_in(&self, compiled: &str) -> Option<String> {
        let (folders, file) = compiled.rsplit_once('/')?;
        let (name, ext) = file.rsplit_once('.')?;
        let wanted: Vec<&str> = folders.split('/').collect();
        let one_folder_apart = |path: &String| {
            let there: Vec<&str> = path.rsplit_once('/').map_or(Vec::new(), |(folders, _)| folders.split('/').collect());
            there.len() == wanted.len() && there.iter().zip(&wanted).filter(|(there, wanted)| !there.eq_ignore_ascii_case(wanted)).count() == 1
        };
        let mut near = self.archive.named(name, ext).into_iter().filter(one_folder_apart);
        let found = near.next()?;
        near.next().is_none().then_some(found)
    }

    pub fn stand_ins(&self) -> Vec<(String, String)> {
        self.stand_ins.borrow().iter().map(|(named, there)| (named.clone(), there.clone())).collect()
    }

    fn find(&self, compiled: &str) -> Option<(&Archive, String)> {
        let archives = || std::iter::once(self.archive).chain(self.core.as_ref());
        let paths = [compiled.to_string(), format!("{INTERFACE_PICTURES}{compiled}")];
        paths.into_iter().find_map(|path| Some((archives().find(|archive| archive.contains(&path))?, path)))
    }

    pub fn gone(&self) -> Vec<String> {
        self.gone.borrow().iter().cloned().collect()
    }

    pub fn is_whole(&self, path: &str) -> bool {
        self.look(&normal(path)).0
    }

    fn look(&self, path: &str) -> (bool, Option<usize>) {
        if !CHECKED.iter().any(|ending| path.ends_with(ending)) {
            return (true, None);
        }
        if let Some(&known) = self.known.borrow().get(path) {
            return (known, None);
        }
        if let Some(&place) = self.waiting_at.borrow().get(path) {
            return (true, Some(place));
        }
        let place = self.waiting.borrow().len();
        self.waiting.borrow_mut().push(path.to_string());
        self.waiting_at.borrow_mut().insert(path.to_string(), place);
        let compiled = format!("{path}_c");
        let found = self.find(&compiled).or_else(|| {
            let there = self.stand_in(&compiled)?;
            self.stand_ins.borrow_mut().insert(compiled.clone(), there.clone());
            Some((self.archive, there))
        });
        let (whole, leans_on) = match found {
            None => {
                self.gone.borrow_mut().insert(path.to_string());
                (false, None)
            }
            Some(_) if !NAMING.iter().any(|ending| path.ends_with(ending)) => (true, None),
            Some((archive, at)) => match archive.read_keeping_open(&at, &self.open) {
                Ok(Some(file)) => {
                    let parts = references(&file).unwrap_or_default();
                    let answers = parts.iter().filter(|named| !named.eq_ignore_ascii_case(path)).map(|named| self.look(&normal(named)));
                    answers.fold((true, None), |(whole, first), (part, on)| (whole && part, first.into_iter().chain(on).min()))
                }
                _ => (false, None),
            },
        };
        if leans_on.is_some_and(|on| on < place) {
            return (whole, leans_on);
        }
        let settled = self.waiting.borrow_mut().split_off(place);
        let (mut waiting_at, mut known) = (self.waiting_at.borrow_mut(), self.known.borrow_mut());
        for file in settled {
            waiting_at.remove(&file);
            known.insert(file, whole);
        }
        (whole, None)
    }
}

pub fn normal(path: &str) -> String {
    let mut path = path.trim().replace('\\', "/").to_ascii_lowercase();
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    path
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::vpk::{self, PackFile};

    pub(crate) fn compiled_file(named: &[&str]) -> Vec<u8> {
        let mut file = Vec::new();
        let push = |file: &mut Vec<u8>, number: u32| file.extend_from_slice(&number.to_le_bytes());
        push(&mut file, 0);
        file.extend_from_slice(&[12, 0, 1, 0]);
        push(&mut file, 8);
        push(&mut file, 2);
        file.extend_from_slice(b"RERL");
        push(&mut file, 40 - 20);
        push(&mut file, 8 + 16 * named.len() as u32);
        file.extend_from_slice(b"DATA");
        push(&mut file, 0);
        push(&mut file, 0);
        push(&mut file, 8);
        push(&mut file, named.len() as u32);
        let paths_at = 48 + 16 * named.len();
        let mut paths = Vec::new();
        for (index, path) in named.iter().enumerate() {
            let entry = 48 + 16 * index;
            file.extend_from_slice(&(index as u64 + 1).to_le_bytes());
            push(&mut file, (paths_at + paths.len() - (entry + 8)) as u32);
            push(&mut file, 0);
            paths.extend_from_slice(path.as_bytes());
            paths.push(0);
        }
        file.extend_from_slice(&paths);
        file
    }

    #[test]
    fn a_compiled_file_says_which_files_it_names() {
        let file = compiled_file(&["particles/a/child.vpcf", "materials/a/smoke.vtex"]);
        assert_eq!(references(&file).unwrap(), ["particles/a/child.vpcf", "materials/a/smoke.vtex"]);
        assert!(references(&compiled_file(&[])).unwrap().is_empty());
        assert!(references(&file[..30]).is_err(), "cut short");
    }

    #[test]
    fn a_file_is_whole_when_the_game_has_it_and_all_it_is_made_of() {
        let files: Vec<(String, Vec<u8>)> = vec![
            ("particles/a/whole.vpcf_c".into(), compiled_file(&["particles/a/child.vpcf", "materials/a/smoke.vtex", "particles/a/whole.vpcf"])),
            ("particles/a/child.vpcf_c".into(), compiled_file(&["materials/a/smoke.vtex", "shaders/a.vfx"])),
            ("materials/a/smoke.vtex_c".into(), b"texture".to_vec()),
            ("particles/a/broken.vpcf_c".into(), compiled_file(&["particles/a/child.vpcf", "particles/a/lost_child.vpcf"])),
            ("particles/a/lost_child.vpcf_c".into(), compiled_file(&["materials/a/gone.vtex"])),
            ("particles/a/screen.vpcf_c".into(), compiled_file(&["loadingscreens/a/loadingscreen.vtex"])),
            ("panorama/images/loadingscreens/a/loadingscreen.vtex_c".into(), b"texture".to_vec()),
        ];
        let packed: Vec<PackFile<'_>> = files.iter().map(|(path, data)| PackFile { path, data }).collect();
        let archive = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&packed).unwrap()).unwrap();
        let whole = Wholeness::new(&archive);
        assert!(whole.is_whole("particles/a/whole.vpcf"), "it, its child and its picture; a shader is not looked for");
        assert!(whole.is_whole("Particles\\A\\Whole.vpcf"), "however it is spelled");
        assert!(!whole.is_whole("particles/a/broken.vpcf"), "a picture two steps down is gone");
        assert!(!whole.is_whole("particles/a/lost_child.vpcf"));
        assert!(!whole.is_whole("particles/a/nowhere.vpcf"));
        assert!(whole.is_whole("sounds/a.vsnd"), "not a kind that is checked");
        assert!(whole.is_whole("particles/a/screen.vpcf"), "a picture of the interface, named from where those are");
        assert_eq!(whole.gone(), ["materials/a/gone.vtex", "particles/a/nowhere.vpcf"]);
        assert!(whole.stand_ins().is_empty());
    }

    #[test]
    fn a_file_one_folder_away_from_where_it_is_named_stands_in() {
        let files: Vec<(String, Vec<u8>)> = vec![
            ("particles/_2econ/items/nyx/hit.vpcf_c".into(), compiled_file(&["particles/_2econ/items/nyx/hit_blood.vpcf"])),
            ("particles/_2econ/items/nyx/hit_blood.vpcf_c".into(), compiled_file(&[])),
            ("particles/econ/items/nyx/carapace.vpcf_c".into(), compiled_file(&["particles/econ/items/nyx/hit.vpcf"])),
            ("particles/_2econ/others/nyx/far.vpcf_c".into(), compiled_file(&[])),
            ("particles/econ/items/axe/twice.vpcf_c".into(), compiled_file(&[])),
            ("particles/econ/items/lina/twice.vpcf_c".into(), compiled_file(&[])),
            ("models/items/qop/lvl2/refit.vmdl_c".into(), compiled_file(&["materials/qop/gone.vmat"])),
        ];
        let packed: Vec<PackFile<'_>> = files.iter().map(|(path, data)| PackFile { path, data }).collect();
        let archive = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&packed).unwrap()).unwrap();
        let whole = Wholeness::new(&archive);

        assert!(whole.is_whole("particles/econ/items/nyx/hit.vpcf"), "the one file of that name, one folder away");
        assert!(whole.is_whole("Particles/Econ/Items/Nyx/Carapace.vpcf"), "a file that names it");
        assert!(!whole.is_whole("particles/econ/items/nyx/far.vpcf"), "two folders away");
        assert!(!whole.is_whole("particles/econ/items/nyx/twice.vpcf"), "which of the two is not to be known");
        assert!(!whole.is_whole("models/items/qop/lvl1/refit.vmdl"), "what stands in has lost a part");
        assert_eq!(
            whole.stand_ins(),
            [
                ("models/items/qop/lvl1/refit.vmdl_c".to_string(), "models/items/qop/lvl2/refit.vmdl_c".to_string()),
                ("particles/econ/items/nyx/hit.vpcf_c".to_string(), "particles/_2econ/items/nyx/hit.vpcf_c".to_string()),
            ]
        );
        assert_eq!(whole.gone(), ["materials/qop/gone.vmat", "particles/econ/items/nyx/far.vpcf", "particles/econ/items/nyx/twice.vpcf"]);
    }

    #[test]
    fn files_that_name_each_other_are_whole_or_not_together_whichever_is_asked_first() {
        let files: Vec<(String, Vec<u8>)> = vec![
            ("particles/a/debut.vpcf_c".into(), compiled_file(&["models/a/arcana.vmdl", "materials/a/gone.vtex"])),
            ("models/a/arcana.vmdl_c".into(), compiled_file(&["particles/a/debut.vpcf"])),
        ];
        let packed: Vec<PackFile<'_>> = files.iter().map(|(path, data)| PackFile { path, data }).collect();
        let archive = Archive::from_bytes(PathBuf::from("pak01_dir.vpk"), vpk::build(&packed).unwrap()).unwrap();
        for first in ["particles/a/debut.vpcf", "models/a/arcana.vmdl"] {
            let whole = Wholeness::new(&archive);
            assert!(!whole.is_whole(first), "{first} first");
            assert!(!whole.is_whole("particles/a/debut.vpcf"), "{first} first");
            assert!(!whole.is_whole("models/a/arcana.vmdl"), "{first} first: it is made of the effect, which has lost a picture");
            assert_eq!(whole.gone(), ["materials/a/gone.vtex"]);
        }
    }
}
