use std::path::{Path, PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Dota 2 was not found. {0}")]
    DotaNotFound(String),

    #[error("{0} is not a Dota 2 folder: game\\bin\\win64\\dota2.exe is missing")]
    NotDota(PathBuf),

    #[error("Dota 2 is running. Close it and try again.")]
    DotaRunning,

    #[error("could not {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("unexpected content in {what}: {detail}")]
    Format { what: String, detail: String },

    #[error("{0}")]
    NotFound(String),

    #[error("{0}")]
    Refused(String),

    #[error("Dota 2 could not be started: {0} Start it from Steam yourself.")]
    Launch(String),
}

impl Error {
    pub fn io(action: &'static str, path: &Path, source: std::io::Error) -> Error {
        Error::Io {
            action,
            path: path.to_path_buf(),
            source,
        }
    }

    pub fn format(what: impl Into<String>, detail: impl Into<String>) -> Error {
        Error::Format {
            what: what.into(),
            detail: detail.into(),
        }
    }

    pub fn plain(&self, dota: &Path) -> String {
        let Error::Io { path, source, .. } = self else { return self.to_string() };
        let whose = if path.starts_with(dota) { "one of Dota's files" } else { "one of the app's own files" };
        if crate::fsutil::is_held(source) {
            format!("Windows did not let the app change {whose}. Another program may be using it, such as an antivirus or Steam. Wait a moment, then try again.")
        } else if crate::fsutil::is_disk_full(source) {
            "The disk is full. Make some room on it, then try again.".to_string()
        } else {
            self.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_another_program_holds_or_a_full_disk_is_said_in_plain_words() {
        let dota = Path::new(r"C:\Steam\steamapps\common\dota 2 beta");
        let io = |path: &Path, code: i32| Error::io("write", path, std::io::Error::from_raw_os_error(code));
        let signatures = dota.join(r"game\bin\win64\dota.signatures");
        let picks = Path::new(r"D:\People\someone\AppData\Local\Dota2ModEnjoyer\picks.json");

        for code in [5, 32, 33] {
            let said = io(&signatures, code).plain(dota);
            assert!(said.contains("one of Dota's files") && said.contains("antivirus"), "{said}");
            assert!(!said.contains("os error") && !said.contains(r"C:\"), "no words of Windows' own: {said}");
        }
        assert!(io(picks, 32).plain(dota).contains("one of the app's own files"));
        assert_eq!(io(&signatures, 112).plain(dota), "The disk is full. Make some room on it, then try again.");
        let missing = io(&signatures, 3);
        assert_eq!(missing.plain(dota), missing.to_string());
        assert_eq!(Error::DotaRunning.plain(dota), "Dota 2 is running. Close it and try again.");
    }
}
