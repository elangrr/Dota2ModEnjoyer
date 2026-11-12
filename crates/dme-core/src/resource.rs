use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};

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
    gone: RefCell<BTreeSet<String>>,
    open: OpenFiles,
}

impl<'a> Wholeness<'a> {
    pub fn new(archive: &'a Archive) -> Wholeness<'a> {
        let core = archive.dir_path().parent().map(|folder| CORE_ARCHIVE.iter().fold(folder.to_path_buf(), |path, part| path.join(part)));
        let core = core.filter(|path| path.is_file()).and_then(|path| Archive::open(&path).ok());
        Wholeness { archive, core, known: RefCell::new(HashMap::new()), gone: RefCell::new(BTreeSet::new()), open: OpenFiles::default() }
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
        let mut path = path.trim().replace('\\', "/").to_ascii_lowercase();
        while path.contains("//") {
            path = path.replace("//", "/");
        }
        if !CHECKED.iter().any(|ending| path.ends_with(ending)) {
            return true;
        }
        if let Some(&known) = self.known.borrow().get(&path) {
            return known;
        }
        self.known.borrow_mut().insert(path.clone(), true);
        let compiled = format!("{path}_c");
        let found = self.find(&compiled);
        let whole = match found {
            None => {
                self.gone.borrow_mut().insert(path.clone());
                false
            }
            Some(_) if !NAMING.iter().any(|ending| path.ends_with(ending)) => true,
            Some((archive, at)) => match archive.read_keeping_open(&at, &self.open) {
                Ok(Some(file)) => references(&file).unwrap_or_default().iter().filter(|named| !named.eq_ignore_ascii_case(&path)).filter(|named| !self.is_whole(named)).count() == 0,
                _ => false,
            },
        };
        self.known.borrow_mut().insert(path, whole);
        whole
    }
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
    }
}
