use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

const HELD_FOR: Duration = if cfg!(test) { Duration::from_millis(1200) } else { Duration::from_secs(5) };
const PAUSE: Duration = Duration::from_millis(200);

pub fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|e| Error::io("read", path, e))
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = temp_sibling(path);
    let written = patiently(|| {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })
    .and_then(|()| patiently(|| fs::rename(&tmp, path)));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map_err(|e| Error::io("write", path, e))
}

fn temp_sibling(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{}.dme-tmp", std::process::id()));
    path.with_file_name(name)
}

pub fn create_dir_all(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| Error::io("create folder", path, e))
}

pub fn remove_dir_all_if_exists(path: &Path) -> Result<bool> {
    match patiently(|| fs::remove_dir_all(path)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Error::io("delete folder", path, e)),
    }
}

pub fn is_held(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

pub fn is_disk_full(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(39 | 112))
}

pub(crate) fn patiently<T>(mut work: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let until = Instant::now() + HELD_FOR;
    loop {
        match work() {
            Err(error) if is_held(&error) && Instant::now() < until => std::thread::sleep(PAUSE),
            done => return done,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_replaces_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        write_atomic(&path, b"one").unwrap();
        write_atomic(&path, b"two").unwrap();
        assert_eq!(read(&path).unwrap(), b"two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn remove_missing_dir_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!remove_dir_all_if_exists(&dir.path().join("nope")).unwrap());
    }

    #[test]
    fn two_programs_never_write_through_the_same_temporary_file() {
        let path = Path::new(r"C:\data\picks.json");
        let tmp = temp_sibling(path);
        assert_eq!(tmp.parent(), path.parent());
        assert_eq!(tmp.file_name().unwrap().to_string_lossy(), format!("picks.json.{}.dme-tmp", std::process::id()));
    }

    #[cfg(windows)]
    fn hold(path: &Path, for_how_long: Duration) -> std::thread::JoinHandle<()> {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 1;
        let file = fs::OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(path).unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(for_how_long);
            drop(file);
        })
    }

    #[cfg(windows)]
    #[test]
    fn a_file_another_program_holds_for_a_moment_is_written_once_it_lets_go() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dota.signatures");
        fs::write(&path, b"Valve's").unwrap();
        let held = hold(&path, Duration::from_millis(500));
        write_atomic(&path, b"ours").unwrap();
        held.join().unwrap();
        assert_eq!(read(&path).unwrap(), b"ours");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
    }

    #[cfg(windows)]
    #[test]
    fn a_file_held_for_longer_is_given_up_on_with_the_reason() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dota.signatures");
        fs::write(&path, b"Valve's").unwrap();
        let held = hold(&path, HELD_FOR + Duration::from_millis(800));
        let Err(Error::Io { source, .. }) = write_atomic(&path, b"ours") else { panic!("the write went through") };
        assert!(is_held(&source), "{source}");
        held.join().unwrap();
        assert_eq!(read(&path).unwrap(), b"Valve's", "nothing was changed");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file is left");
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_with_a_file_held_for_a_moment_is_deleted_once_it_is_let_go() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("mod_enjoyer");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("pak01_dir.vpk");
        fs::write(&file, b"package").unwrap();
        let held = hold(&file, Duration::from_millis(500));
        assert!(remove_dir_all_if_exists(&folder).unwrap());
        held.join().unwrap();
        assert!(!folder.exists());
    }
}
