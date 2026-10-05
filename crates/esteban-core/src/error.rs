use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("request to {url} failed: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("{url} answered with HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{0} is not on the list of hosts this launcher may contact")]
    HostNotAllowed(String),
    #[error("refusing a non-https url: {0}")]
    Insecure(String),
    #[error("not a valid url: {0}")]
    BadUrl(String),
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not understand {what}: {source}")]
    Json {
        what: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("{what} failed its {algorithm} check (expected {expected}, got {actual})")]
    HashMismatch {
        what: String,
        algorithm: &'static str,
        expected: String,
        actual: String,
    },
    #[error("{what} is {actual} bytes, expected {expected}")]
    SizeMismatch {
        what: String,
        expected: u64,
        actual: u64,
    },
    #[error("version {0} is not in Mojang's version list")]
    UnknownVersion(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Fabric(String),
    #[error("{0}")]
    Guard(String),
    #[error("{0}")]
    Launch(String),
    #[error("{0}")]
    Account(String),
    #[error("{0}")]
    Mods(String),
    #[error("could not read {path} as a jar: {source}")]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait IoContext<T> {
    fn at(self, path: &Path) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at(self, path: &Path) -> Result<T> {
        self.map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

pub fn json<T: serde::de::DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|source| Error::Json {
        what: what.to_string(),
        source,
    })
}
