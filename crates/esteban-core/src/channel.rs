use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::esteban::{HACKS_MOD_ID, HUD_MOD_ID};
use crate::loader::Loader;
use crate::{Error, Result};

pub const CHANNEL_SCHEMA: u32 = 2;

const BUNDLED: &str = include_str!("../../../config/versions.json");

const JAR_HOST: &str = "https://github.com/SunqdXX/esteban/releases/download/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Channel {
    pub schema: u32,
    pub sequence: u64,
    pub issued: String,
    pub expires: String,
    pub builds: Vec<Build>,
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default)]
    pub retired_hacks_sha256: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    pub mc: String,
    pub loader: BuildLoader,
    pub hacked: bool,
    pub jars: Vec<ChannelJar>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildLoader {
    pub kind: Loader,
    pub min: String,
    pub tested: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelJar {
    pub id: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl Channel {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let channel: Self = crate::error::json(bytes, "the Esteban version list")?;
        channel.check()?;
        Ok(channel)
    }

    fn check(&self) -> Result<()> {
        let bad = |why: String| Err(Error::Guard(format!("The Esteban version list {why}.")));
        if self.schema != CHANNEL_SCHEMA {
            return bad(format!(
                "has schema {}, this launcher reads {CHANNEL_SCHEMA}",
                self.schema
            ));
        }
        let mut seen = BTreeSet::new();
        for build in &self.builds {
            let label = format!(
                "{} {}{}",
                build.loader.kind.title(),
                build.mc,
                if build.hacked { " Hacked" } else { "" }
            );
            if !seen.insert((build.mc.as_str(), build.loader.kind, build.hacked)) {
                return bad(format!("lists {label} twice"));
            }
            if build.hacked && build.loader.kind != Loader::Fabric {
                return bad(format!("has a Hacked build outside Fabric ({label})"));
            }
            if build.loader.kind != Loader::Fabric && !build.jars.is_empty() {
                return bad(format!("lists Fabric jars for {label}"));
            }
            for jar in &build.jars {
                if jar.id == HACKS_MOD_ID && !build.hacked {
                    return bad(format!("puts the hacks mod into {label}"));
                }
                if jar.id != HACKS_MOD_ID && jar.id != HUD_MOD_ID {
                    return bad(format!("names an unknown jar {} in {label}", jar.id));
                }
                if !is_sha256(&jar.sha256) {
                    return bad(format!("has a broken sha256 for {}", jar.file));
                }
                let safe_name = jar.file.ends_with(".jar")
                    && !jar.file.starts_with('.')
                    && !jar.file.contains(['/', '\\']);
                if !safe_name {
                    return bad(format!("has an unsafe file name {}", jar.file));
                }
                if !jar.url.starts_with(JAR_HOST) || !jar.url.ends_with(&format!("/{}", jar.file)) {
                    return bad(format!(
                        "points {} somewhere other than the Esteban releases",
                        jar.file
                    ));
                }
            }
        }
        if let Some(hash) = self.retired_hacks_sha256.iter().find(|h| !is_sha256(h)) {
            return bad(format!("has a broken retired hash {hash}"));
        }
        Ok(())
    }

    pub fn build(&self, mc: &str, loader: Loader, hacked: bool) -> Option<&Build> {
        self.builds
            .iter()
            .find(|b| b.mc == mc && b.loader.kind == loader && b.hacked == hacked)
    }

    pub fn jars(&self) -> impl Iterator<Item = (&Build, &ChannelJar)> {
        self.builds
            .iter()
            .flat_map(|b| b.jars.iter().map(move |j| (b, j)))
    }

    pub fn is_known_hacks_jar(&self, sha256: &str) -> bool {
        self.jars()
            .filter(|(_, j)| j.id == HACKS_MOD_ID)
            .map(|(_, j)| j.sha256.as_str())
            .chain(self.retired_hacks_sha256.iter().map(String::as_str))
            .any(|known| known.eq_ignore_ascii_case(sha256))
    }
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

static BUNDLED_CHANNEL: LazyLock<Option<Channel>> =
    LazyLock::new(|| match Channel::parse(BUNDLED.as_bytes()) {
        Ok(channel) => Some(channel),
        Err(e) => {
            tracing::error!(error = %e, "the bundled Esteban version list is broken");
            None
        }
    });

pub fn bundled() -> Option<&'static Channel> {
    BUNDLED_CHANNEL.as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Channel {
        Channel::parse(BUNDLED.as_bytes()).unwrap()
    }

    fn with(edit: impl FnOnce(&mut serde_json::Value)) -> Result<Channel> {
        let mut value: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        edit(&mut value);
        Channel::parse(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn the_bundled_list_parses_and_covers_every_pinned_version_both_ways() {
        let channel = sample();
        assert!(bundled().is_some());
        for v in crate::versions::PINNED_VERSIONS {
            let normal = channel.build(v.id, Loader::Fabric, false).unwrap();
            let ids: Vec<&str> = normal.jars.iter().map(|j| j.id.as_str()).collect();
            assert_eq!(ids, vec![HUD_MOD_ID]);
            let hacked = channel.build(v.id, Loader::Fabric, true).unwrap();
            let ids: Vec<&str> = hacked.jars.iter().map(|j| j.id.as_str()).collect();
            assert_eq!(ids, vec![HUD_MOD_ID, HACKS_MOD_ID]);
            assert!(channel.build(v.id, Loader::Forge, false).is_none());
        }
        assert_eq!(channel.builds.len(), 10);
        assert_eq!(channel.retired_hacks_sha256.len(), 5);
    }

    #[test]
    fn hacks_hashes_include_current_and_retired_but_never_the_hud() {
        let channel = sample();
        let (_, hacks) = channel.jars().find(|(_, j)| j.id == HACKS_MOD_ID).unwrap();
        let (_, hud) = channel.jars().find(|(_, j)| j.id == HUD_MOD_ID).unwrap();
        assert!(channel.is_known_hacks_jar(&hacks.sha256.to_uppercase()));
        assert!(!channel.is_known_hacks_jar(&hud.sha256));
        assert!(channel.is_known_hacks_jar(&channel.retired_hacks_sha256[0]));
    }

    #[test]
    fn a_normal_build_carrying_the_hacks_mod_is_refused() {
        let result = with(|v| {
            let hacks = v["builds"][1]["jars"][1].clone();
            v["builds"][0]["jars"].as_array_mut().unwrap().push(hacks);
        });
        assert!(matches!(result, Err(Error::Guard(m)) if m.contains("puts the hacks mod")));
    }

    #[test]
    fn broken_lists_are_refused() {
        type Edit = fn(&mut serde_json::Value);
        let cases: [Edit; 10] = [
            |v| v["schema"] = 3.into(),
            |v| v["builds"][0]["loader"]["kind"] = "forge".into(),
            |v| v["builds"][1]["loader"]["kind"] = "forge".into(),
            |v| v["builds"][0]["jars"][0]["sha256"] = "abc".into(),
            |v| v["builds"][0]["jars"][0]["id"] = "other".into(),
            |v| v["builds"][0]["jars"][0]["file"] = "../x.jar".into(),
            |v| {
                v["builds"][0]["jars"][0]["url"] =
                    "https://evil.example/esteban-hud-1.4.0+26.3.jar".into();
            },
            |v| v["retiredHacksSha256"][0] = "nope".into(),
            |v| v["surprise"] = true.into(),
            |v| {
                let first = v["builds"][0].clone();
                v["builds"].as_array_mut().unwrap().push(first);
            },
        ];
        for (index, edit) in cases.into_iter().enumerate() {
            assert!(with(edit).is_err(), "case {index} was accepted");
        }
    }
}
