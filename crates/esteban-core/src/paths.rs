use std::path::{Path, PathBuf};

use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct Paths {
    base: PathBuf,
}

impl Paths {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub fn default_base() -> Result<PathBuf> {
        directories::BaseDirs::new()
            .map(|dirs| dirs.data_dir().join("EstebanLauncher"))
            .ok_or_else(|| Error::Unsupported("could not find this user's data folder".into()))
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn assets(&self) -> PathBuf {
        self.base.join("assets")
    }

    pub fn asset_indexes(&self) -> PathBuf {
        self.assets().join("indexes")
    }

    pub fn asset_objects(&self) -> PathBuf {
        self.assets().join("objects")
    }

    pub fn log_configs(&self) -> PathBuf {
        self.assets().join("log_configs")
    }

    pub fn libraries(&self) -> PathBuf {
        self.base.join("libraries")
    }

    pub fn versions(&self) -> PathBuf {
        self.base.join("versions")
    }

    pub fn version_dir(&self, id: &str) -> PathBuf {
        self.versions().join(id)
    }

    pub fn runtime(&self) -> PathBuf {
        self.base.join("runtime")
    }

    pub fn instances(&self) -> PathBuf {
        self.base.join("instances")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.base.join("settings.json")
    }
}
