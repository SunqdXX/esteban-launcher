use serde::Deserialize;

use crate::net::Net;
use crate::{Error, Result};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

impl VersionManifest {
    pub async fn fetch(net: &Net) -> Result<Self> {
        net.json(MANIFEST_URL, "the version manifest").await
    }

    pub fn find(&self, id: &str) -> Result<&VersionEntry> {
        self.versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| Error::UnknownVersion(id.to_string()))
    }
}
