use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::fabric::{self, token};
use crate::loader::Loader;
use crate::mojang::manifest::{MANIFEST_URL, VersionManifest};
use crate::net::Net;
use crate::paths::Paths;
use crate::versions::{is_pinned, pinned_tag};
use crate::{Result, fsx};

pub const FABRIC_GAMES: &str = "https://meta.fabricmc.net/v2/versions/game";
pub const FORGE_PROMOS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub const FORGE_METADATA: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
pub const FABRIC_FIRST: &str = "1.14";
pub const FORGE_FIRST: &str = "1.5.2";
pub const FORGE_LAST: &str = "1.20.1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Offer {
    pub available: bool,
    pub reason: Option<String>,
}

impl Offer {
    fn yes() -> Self {
        Self {
            available: true,
            reason: None,
        }
    }

    fn no(reason: String) -> Self {
        Self {
            available: false,
            reason: Some(reason),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub id: String,
    pub date: String,
    pub tag: &'static str,
    pub pinned: bool,
    pub fabric: Offer,
    pub forge: Offer,
}

impl Release {
    pub fn offer(&self, loader: Loader) -> Offer {
        match loader {
            Loader::Vanilla => Offer::yes(),
            Loader::Fabric => self.fabric.clone(),
            Loader::Forge => self.forge.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub latest: String,
    pub releases: Vec<Release>,
    pub offline: bool,
}

impl Catalog {
    pub fn find(&self, id: &str) -> Option<&Release> {
        self.releases.iter().find(|r| r.id == id)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct FabricGame {
    pub version: String,
    pub stable: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ForgePromos {
    #[serde(default)]
    pub promos: std::collections::BTreeMap<String, String>,
}

pub struct Sources<'a> {
    pub manifest: &'a VersionManifest,
    pub fabric: Option<&'a [FabricGame]>,
    pub forge: Option<&'a [String]>,
}

pub fn build(sources: &Sources<'_>) -> Vec<Release> {
    let releases: Vec<_> = sources
        .manifest
        .versions
        .iter()
        .filter(|v| v.kind == "release")
        .collect();
    let index = |id: &str| releases.iter().position(|r| r.id == id);
    let fabric_first = index(FABRIC_FIRST).unwrap_or(usize::MAX);
    let forge_first = index(FORGE_FIRST).unwrap_or(usize::MAX);
    let forge_last = index(FORGE_LAST).unwrap_or(0);
    let fabric_games: Option<BTreeSet<&str>> = sources.fabric.map(|games| {
        games
            .iter()
            .filter(|g| g.stable)
            .map(|g| g.version.as_str())
            .collect()
    });
    let forge_games: Option<BTreeSet<&str>> = sources.forge.map(|versions| {
        versions
            .iter()
            .filter_map(|v| v.split('-').next())
            .collect()
    });
    releases
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let fabric = match &fabric_games {
                Some(games) if games.contains(r.id.as_str()) => Offer::yes(),
                Some(_) if i > fabric_first => {
                    Offer::no(format!("Fabric starts at {FABRIC_FIRST}."))
                }
                Some(_) => Offer::no(format!("Fabric isn't out for {} yet.", r.id)),
                None => Offer::no("Couldn't reach Fabric to check.".into()),
            };
            let forge = if i < forge_last {
                Offer::no(format!("Forge stops at {FORGE_LAST} here."))
            } else if i > forge_first {
                Offer::no(format!("Forge starts at {FORGE_FIRST}."))
            } else {
                match &forge_games {
                    Some(games) if games.contains(r.id.as_str()) => Offer::yes(),
                    Some(_) => Offer::no(format!("Forge has no build for {}.", r.id)),
                    None => Offer::no("Couldn't reach Forge to check.".into()),
                }
            };
            Release {
                id: r.id.clone(),
                date: r.release_time.chars().take(10).collect(),
                tag: pinned_tag(&r.id),
                pinned: is_pinned(&r.id),
                fabric,
                forge,
            }
        })
        .collect()
}

pub fn forge_versions_from_metadata(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        let version = after[..end].trim();
        if token(version).is_ok() {
            out.push(version.to_string());
        }
        rest = &after[end..];
    }
    out
}

async fn cached(net: &Net, paths: &Paths, url: &str, name: &str) -> Result<(Vec<u8>, bool)> {
    let path = paths.cache().join(name);
    match net.bytes(url).await {
        Ok(bytes) => {
            fsx::write_atomic(&path, &bytes).await?;
            Ok((bytes.to_vec(), true))
        }
        Err(e) => match tokio::fs::read(&path).await {
            Ok(bytes) => {
                tracing::warn!(%url, error = %e, "offline, using the saved copy");
                Ok((bytes, false))
            }
            Err(_) => Err(e),
        },
    }
}

async fn fabric_games(net: &Net, paths: &Paths) -> Option<(Vec<FabricGame>, bool)> {
    let (bytes, fresh) = cached(net, paths, FABRIC_GAMES, "fabric-games.json")
        .await
        .ok()?;
    let games = crate::error::json(&bytes, "Fabric's game list").ok()?;
    Some((games, fresh))
}

async fn forge_list(net: &Net, paths: &Paths) -> Option<(Vec<String>, bool)> {
    let (bytes, fresh) = cached(net, paths, FORGE_METADATA, "forge-metadata.xml")
        .await
        .ok()?;
    let versions = forge_versions_from_metadata(&String::from_utf8_lossy(&bytes));
    (!versions.is_empty()).then_some((versions, fresh))
}

async fn forge_promos(net: &Net, paths: &Paths) -> Option<(ForgePromos, bool)> {
    let (bytes, fresh) = cached(net, paths, FORGE_PROMOS, "forge-promotions.json")
        .await
        .ok()?;
    let promos = crate::error::json(&bytes, "Forge's recommended builds").ok()?;
    Some((promos, fresh))
}

pub async fn load(net: &Net, paths: &Paths) -> Result<Catalog> {
    let (bytes, manifest_fresh) =
        cached(net, paths, MANIFEST_URL, "version_manifest_v2.json").await?;
    let manifest: VersionManifest = crate::error::json(&bytes, "the version manifest")?;
    let fabric = fabric_games(net, paths).await;
    let forge = forge_list(net, paths).await;
    let releases = build(&Sources {
        manifest: &manifest,
        fabric: fabric.as_ref().map(|(g, _)| g.as_slice()),
        forge: forge.as_ref().map(|(v, _)| v.as_slice()),
    });
    let offline = !manifest_fresh
        || fabric.as_ref().is_none_or(|(_, fresh)| !fresh)
        || forge.as_ref().is_none_or(|(_, fresh)| !fresh);
    Ok(Catalog {
        latest: manifest.latest.release.clone(),
        releases,
        offline,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    pub version: String,
    pub label: String,
    pub stable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersions {
    pub loader: Loader,
    pub versions: Vec<LoaderVersion>,
    pub default: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderInfo,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderInfo {
    version: String,
    stable: bool,
}

pub fn fabric_choices(entries: &[(String, bool)]) -> LoaderVersions {
    let versions: Vec<LoaderVersion> = entries
        .iter()
        .map(|(version, stable)| LoaderVersion {
            version: version.clone(),
            label: version.clone(),
            stable: *stable,
        })
        .collect();
    let default = versions
        .iter()
        .find(|v| v.stable)
        .or_else(|| versions.first())
        .map(|v| v.version.clone());
    LoaderVersions {
        loader: Loader::Fabric,
        versions,
        default,
        note: None,
    }
}

pub fn forge_choices(mc: &str, all: &[String], promos: &ForgePromos) -> LoaderVersions {
    let prefix = format!("{mc}-");
    let suffix = format!("-{mc}");
    let mut versions: Vec<String> = all
        .iter()
        .filter_map(|v| v.strip_prefix(&prefix))
        .map(str::to_string)
        .collect();
    let number = |v: &String| -> Vec<u64> {
        v.strip_suffix(&suffix)
            .unwrap_or(v)
            .split(['.', '-'])
            .map(|part| part.parse().unwrap_or(0))
            .collect()
    };
    versions.sort_by_key(|v| std::cmp::Reverse(number(v)));
    versions.dedup();
    let matches = |promo: &str, v: &str| v == promo || v.starts_with(&format!("{promo}-"));
    let promo = |kind: &str| promos.promos.get(&format!("{mc}-{kind}"));
    let recommended = promo("recommended")
        .and_then(|p| versions.iter().find(|v| matches(p, v)))
        .cloned();
    let latest = promo("latest")
        .and_then(|p| versions.iter().find(|v| matches(p, v)))
        .cloned()
        .or_else(|| versions.first().cloned());
    let note = match (&recommended, &latest) {
        (None, Some(_)) => Some(format!(
            "Forge hasn't marked a recommended build for {mc}, so the newest one is used."
        )),
        _ => None,
    };
    LoaderVersions {
        loader: Loader::Forge,
        versions: versions
            .iter()
            .map(|v| LoaderVersion {
                version: v.clone(),
                label: v.strip_suffix(&suffix).unwrap_or(v).to_string(),
                stable: Some(v) == recommended.as_ref(),
            })
            .collect(),
        default: recommended.or(latest),
        note,
    }
}

pub async fn loader_versions(
    net: &Net,
    paths: &Paths,
    mc: &str,
    loader: Loader,
) -> Result<LoaderVersions> {
    let mc = token(mc)?;
    match loader {
        Loader::Vanilla => Ok(LoaderVersions {
            loader,
            versions: Vec::new(),
            default: None,
            note: None,
        }),
        Loader::Fabric => {
            let url = format!("{}/versions/loader/{mc}", fabric::META);
            let (bytes, _) = cached(net, paths, &url, &format!("fabric-loaders-{mc}.json")).await?;
            let entries: Vec<FabricLoaderEntry> =
                crate::error::json(&bytes, "the Fabric loader list")?;
            let pairs: Vec<(String, bool)> = entries
                .into_iter()
                .map(|e| (e.loader.version, e.loader.stable))
                .collect();
            Ok(fabric_choices(&pairs))
        }
        Loader::Forge => {
            let all = forge_list(net, paths)
                .await
                .map(|(v, _)| v)
                .unwrap_or_default();
            let promos = forge_promos(net, paths)
                .await
                .map(|(p, _)| p)
                .unwrap_or_default();
            Ok(forge_choices(mc, &all, &promos))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(ids: &[&str]) -> VersionManifest {
        let versions: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| {
                serde_json::json!({"id": id, "type": if id.contains('w') { "snapshot" } else { "release" },
                    "url": "https://piston-meta.mojang.com/x.json", "sha1": "00", "releaseTime": "2025-03-25T12:00:00+00:00"})
            })
            .collect();
        serde_json::from_value(serde_json::json!({"latest": {"release": ids[0], "snapshot": "x"}, "versions": versions}))
            .unwrap()
    }

    const IDS: &[&str] = &[
        "26.3", "26w14a", "1.21.4", "1.20.2", "1.20.1", "1.14", "1.12.2", "1.12.1", "1.7.10",
        "1.5.2", "1.5.1", "1.0",
    ];

    #[test]
    fn offers_follow_the_rules_and_say_why_not() {
        let m = manifest(IDS);
        let fabric: Vec<FabricGame> = ["26.3", "1.21.4", "1.20.2", "1.20.1", "1.14"]
            .iter()
            .map(|v| FabricGame {
                version: (*v).to_string(),
                stable: true,
            })
            .collect();
        let forge: Vec<String> = [
            "1.21.4-54.1.14",
            "1.20.1-47.4.10",
            "1.14-28.0.0",
            "1.12.2-14.23.5.2859",
            "1.7.10-10.13.4.1614-1.7.10",
            "1.5.2-7.8.1.738",
            "1.5.1-7.7.2.682",
        ]
        .iter()
        .map(|v| (*v).to_string())
        .collect();
        let releases = build(&Sources {
            manifest: &m,
            fabric: Some(&fabric),
            forge: Some(&forge),
        });
        let get = |id: &str| releases.iter().find(|r| r.id == id).unwrap().clone();
        assert!(releases.iter().all(|r| r.id != "26w14a"));
        assert_eq!(releases.len(), IDS.len() - 1);

        let latest = get("26.3");
        assert!(latest.pinned && latest.tag == "latest");
        assert!(latest.fabric.available);
        assert_eq!(
            latest.forge.reason.as_deref(),
            Some("Forge stops at 1.20.1 here.")
        );
        assert!(!get("1.21.4").forge.available);
        assert!(get("1.20.1").forge.available && get("1.20.1").fabric.available);
        assert!(get("1.14").forge.available && get("1.14").fabric.available);
        assert_eq!(
            get("1.12.2").fabric.reason.as_deref(),
            Some("Fabric starts at 1.14.")
        );
        assert!(get("1.12.2").forge.available);
        assert_eq!(
            get("1.12.1").forge.reason.as_deref(),
            Some("Forge has no build for 1.12.1.")
        );
        assert!(get("1.7.10").forge.available);
        assert!(get("1.5.2").forge.available);
        assert_eq!(
            get("1.5.1").forge.reason.as_deref(),
            Some("Forge starts at 1.5.2.")
        );
        assert!(!get("1.0").forge.available && !get("1.0").fabric.available);
        assert!(get("1.0").offer(Loader::Vanilla).available);
        assert_eq!(get("1.21.4").date, "2025-03-25");

        let unreachable = build(&Sources {
            manifest: &m,
            fabric: None,
            forge: None,
        });
        assert_eq!(
            unreachable[0].fabric.reason.as_deref(),
            Some("Couldn't reach Fabric to check.")
        );
        assert_eq!(
            unreachable[4].forge.reason.as_deref(),
            Some("Couldn't reach Forge to check.")
        );
    }

    #[test]
    fn forge_metadata_versions_are_read_and_junk_is_dropped() {
        let xml = "<metadata><versioning><versions>\n<version>1.20.1-47.4.10</version>\n<version> 1.8.9-11.15.1.2318-1.8.9 </version><version>../evil</version></versions></versioning></metadata>";
        assert_eq!(
            forge_versions_from_metadata(xml),
            vec!["1.20.1-47.4.10", "1.8.9-11.15.1.2318-1.8.9"]
        );
    }

    #[test]
    fn forge_defaults_to_recommended_then_latest_with_a_note() {
        let all: Vec<String> = [
            "1.8.9-11.15.1.2318-1.8.9",
            "1.8.9-11.15.0.1656-1.8.9",
            "1.8.9-11.15.1.2317-1.8.9",
            "1.20.1-47.4.10",
            "1.20.1-47.4.26",
            "1.20.1-47.4.27",
        ]
        .iter()
        .map(|v| (*v).to_string())
        .collect();
        let promos = ForgePromos {
            promos: [
                ("1.20.1-recommended", "47.4.10"),
                ("1.20.1-latest", "47.4.26"),
                ("1.8.9-latest", "11.15.1.2318"),
            ]
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        };
        let modern = forge_choices("1.20.1", &all, &promos);
        assert_eq!(modern.default.as_deref(), Some("47.4.10"));
        assert!(modern.note.is_none());
        assert_eq!(modern.versions[0].version, "47.4.27");
        assert!(
            modern
                .versions
                .iter()
                .any(|v| v.version == "47.4.10" && v.stable)
        );
        let old = forge_choices("1.8.9", &all, &promos);
        assert_eq!(old.default.as_deref(), Some("11.15.1.2318-1.8.9"));
        assert_eq!(old.versions[0].label, "11.15.1.2318");
        assert_eq!(old.versions[2].label, "11.15.0.1656");
        assert!(
            old.note
                .unwrap()
                .contains("hasn't marked a recommended build")
        );
        let none = forge_choices("1.2.5", &all, &promos);
        assert!(none.default.is_none() && none.versions.is_empty());
    }

    #[test]
    fn fabric_defaults_to_the_newest_stable_loader() {
        let pairs = vec![
            ("0.20.0-beta.1".to_string(), false),
            ("0.19.5".to_string(), true),
            ("0.19.4".to_string(), true),
        ];
        let choices = fabric_choices(&pairs);
        assert_eq!(choices.default.as_deref(), Some("0.19.5"));
        assert_eq!(choices.versions.len(), 3);
        assert!(fabric_choices(&[]).default.is_none());
    }

    #[tokio::test]
    async fn the_saved_copy_is_used_when_offline() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::path("/list.json"))
            .respond_with(wiremock::ResponseTemplate::new(200).set_body_string("[1]"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::path("/list.json"))
            .respond_with(wiremock::ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let net = Net::new(crate::net::Policy::local_test()).unwrap();
        let url = format!("{}/list.json", server.uri());
        let (bytes, fresh) = cached(&net, &paths, &url, "list.json").await.unwrap();
        assert_eq!(bytes, b"[1]");
        assert!(fresh);
        let (bytes, fresh) = cached(&net, &paths, &url, "list.json").await.unwrap();
        assert_eq!(bytes, b"[1]");
        assert!(!fresh);
        assert!(cached(&net, &paths, &url, "never.json").await.is_err());
    }
}
