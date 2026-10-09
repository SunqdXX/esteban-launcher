use std::future::Future;

use reqwest::Url;
use serde::Deserialize;

use crate::fabric::token;
use crate::net::Net;
use crate::{Error, Result};

pub const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Deserialize)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub version_type: String,
    pub date_published: String,
    #[serde(default)]
    pub game_versions: Vec<String>,
    #[serde(default)]
    pub loaders: Vec<String>,
    pub files: Vec<VersionFile>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    pub size: u64,
    pub hashes: FileHashes,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileHashes {
    #[serde(default)]
    pub sha512: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    pub dependency_type: String,
}

pub trait Source {
    fn versions(
        &self,
        project: &str,
        game: &str,
    ) -> impl Future<Output = Result<Vec<Version>>> + Send;
    fn projects(&self, ids: &[String]) -> impl Future<Output = Result<Vec<Project>>> + Send;
    fn version(&self, id: &str) -> impl Future<Output = Result<Version>> + Send;
}

pub struct Modrinth<'a> {
    net: &'a Net,
}

impl<'a> Modrinth<'a> {
    pub fn new(net: &'a Net) -> Self {
        Self { net }
    }
}

fn json_list(items: &[String]) -> Result<String> {
    serde_json::to_string(items).map_err(|source| Error::Json {
        what: "a Modrinth query".into(),
        source,
    })
}

impl Modrinth<'_> {
    pub async fn versions_for_loader(
        &self,
        project: &str,
        loader: &str,
        game: &str,
    ) -> Result<Vec<Version>> {
        let base = format!("{API}/project/{}/version", token(project)?);
        let url = Url::parse_with_params(
            &base,
            &[
                ("loaders", json_list(&[token(loader)?.to_string()])?),
                ("game_versions", json_list(&[game.to_string()])?),
            ],
        )
        .map_err(|_| Error::BadUrl(base.clone()))?;
        self.net
            .json(url.as_str(), &format!("Modrinth versions of {project}"))
            .await
    }
}

impl Source for Modrinth<'_> {
    async fn versions(&self, project: &str, game: &str) -> Result<Vec<Version>> {
        let base = format!("{API}/project/{}/version", token(project)?);
        let url = Url::parse_with_params(
            &base,
            &[
                ("loaders", json_list(&["fabric".to_string()])?),
                ("game_versions", json_list(&[game.to_string()])?),
            ],
        )
        .map_err(|_| Error::BadUrl(base.clone()))?;
        self.net
            .json(url.as_str(), &format!("Modrinth versions of {project}"))
            .await
    }

    async fn projects(&self, ids: &[String]) -> Result<Vec<Project>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        for id in ids {
            token(id)?;
        }
        let base = format!("{API}/projects");
        let url = Url::parse_with_params(&base, &[("ids", json_list(ids)?)])
            .map_err(|_| Error::BadUrl(base.clone()))?;
        self.net.json(url.as_str(), "Modrinth projects").await
    }

    async fn version(&self, id: &str) -> Result<Version> {
        let url = format!("{API}/version/{}", token(id)?);
        self.net.json(&url, &format!("Modrinth version {id}")).await
    }
}
