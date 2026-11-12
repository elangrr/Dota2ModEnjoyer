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
}
