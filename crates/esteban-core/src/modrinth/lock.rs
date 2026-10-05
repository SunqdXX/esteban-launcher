use std::path::Path;

use serde::{Deserialize, Serialize};

use super::resolve::ResolvedMod;
use crate::Result;
use crate::error::IoContext;
use crate::fsx;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModLock {
    pub game_version: String,
    pub mods: Vec<ResolvedMod>,
    #[serde(default)]
    pub extras: Vec<String>,
    #[serde(default)]
    pub skipped: Vec<Skipped>,
    #[serde(default)]
    pub disabled: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub title: String,
    pub message: String,
}

impl ModLock {
    pub async fn read(path: &Path) -> Result<Option<Self>> {
        match tokio::fs::read(path).await {
            Ok(bytes) => Ok(Some(crate::error::json(&bytes, "the mod lock file")?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).at(path),
        }
    }

    pub async fn write(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| crate::Error::Json {
            what: "the mod lock file".into(),
            source,
        })?;
        fsx::write_atomic(path, &bytes).await
    }

    pub fn filenames(&self) -> impl Iterator<Item = &str> {
        self.mods
            .iter()
            .map(|m| m.filename.as_str())
            .chain(self.extras.iter().map(String::as_str))
    }
}
