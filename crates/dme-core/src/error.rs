use std::path::{Path, PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Dota 2 was not found. {}", looked_in(.0))]
    DotaNotFound(Vec<String>),

    #[error("{0} is not a Dota 2 folder: game\\bin\\win64\\dota2.exe is missing")]
    NotDota(PathBuf),

    #[error("Dota 2 is running. Close it and try again.")]
    DotaRunning,

    #[error("Steam is updating Dota 2. Wait until it is done, then try again.")]
    SteamBusy,

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

    #[error("Dota 2 could not be started: Steam was not found. Start it from Steam yourself.")]
    NoSteam,

    #[error("gameinfo_branchspecific.gi is not the file Valve shipped, so something else has changed it. {VERIFY_HINT}")]
    NotValves,

    #[error("this copy of Dota does not have the models these picks need: {}", .0.join(", "))]
    MissingModels(Vec<String>),
}

pub const VERIFY_HINT: &str = "In Steam, right-click Dota 2, choose Properties, Installed Files, \"Verify integrity of game files\", then try again.";

fn looked_in(folders: &[String]) -> String {
    match folders.is_empty() {
        true => "Steam itself was not found on this computer.".to_string(),
        false => format!("Looked in: {}", folders.join("; ")),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Plain<'a> {
    Held { dota: bool },
    DiskFull,
    DotaRunning,
    SteamBusy,
    DotaNotFound(&'a [String]),
    NotDota(&'a Path),
    NoSteam,
    NotValves,
    MissingModels(&'a [String]),
    Other(String),
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
        match self.plain_kind(dota) {
            Plain::Held { dota } => {
                let whose = if dota { "one of Dota's files" } else { "one of the app's own files" };
                format!("Windows did not let the app change {whose}. Another program may be using it, such as an antivirus or Steam. Wait a moment, then try again.")
            }
            Plain::DiskFull => "The disk is full. Make some room on it, then try again.".to_string(),
            _ => self.to_string(),
        }
    }

    pub fn plain_kind(&self, dota: &Path) -> Plain<'_> {
        match self {
            Error::Io { path, source, .. } if crate::fsutil::is_held(source) => Plain::Held { dota: path.starts_with(dota) },
            Error::Io { source, .. } if crate::fsutil::is_disk_full(source) => Plain::DiskFull,
            Error::DotaRunning => Plain::DotaRunning,
            Error::SteamBusy => Plain::SteamBusy,
            Error::DotaNotFound(folders) => Plain::DotaNotFound(folders),
            Error::NotDota(folder) => Plain::NotDota(folder),
            Error::NoSteam => Plain::NoSteam,
            Error::NotValves => Plain::NotValves,
            Error::MissingModels(models) => Plain::MissingModels(models),
            other => Plain::Other(other.to_string()),
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
        assert_eq!(Error::SteamBusy.plain(dota), "Steam is updating Dota 2. Wait until it is done, then try again.");
        assert_eq!(Error::SteamBusy.plain_kind(dota), Plain::SteamBusy);
        assert_eq!(io(&signatures, 32).plain_kind(dota), Plain::Held { dota: true });
        assert_eq!(io(picks, 112).plain_kind(dota), Plain::DiskFull);
        assert_eq!(missing.plain_kind(dota), Plain::Other(missing.to_string()));
        assert_eq!(Error::DotaNotFound(Vec::new()).to_string(), "Dota 2 was not found. Steam itself was not found on this computer.");
        assert_eq!(Error::DotaNotFound(vec![r"D:\a".into(), r"E:\b".into()]).to_string(), r"Dota 2 was not found. Looked in: D:\a; E:\b");
    }
}
