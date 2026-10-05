use serde::Deserialize;

use crate::download::Download;
use crate::hash::Hash;
use crate::mojang::libraries::{ResolvedLibrary, library_key, maven_path};
use crate::net::Net;
use crate::paths::Paths;
use crate::{Error, Result};

pub const META: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Debug, Deserialize)]
struct LoaderInfo {
    version: String,
    stable: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricProfile {
    pub id: String,
    pub inherits_from: String,
    pub main_class: String,
    #[serde(default)]
    pub arguments: FabricArguments,
    pub libraries: Vec<FabricLibrary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FabricArguments {
    #[serde(default)]
    pub game: Vec<String>,
    #[serde(default)]
    pub jvm: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FabricLibrary {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub sha512: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
}

pub fn token(value: &str) -> Result<&str> {
    let ok = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    if ok && value != "." && value != ".." {
        Ok(value)
    } else {
        Err(Error::Unsupported(format!(
            "not a valid version name: {value}"
        )))
    }
}

pub async fn latest_stable_loader(net: &Net, game: &str) -> Result<String> {
    let url = format!("{META}/versions/loader/{}", token(game)?);
    let entries: Vec<LoaderEntry> = net.json(&url, "the Fabric loader list").await?;
    entries
        .iter()
        .find(|e| e.loader.stable)
        .or_else(|| entries.first())
        .map(|e| e.loader.version.clone())
        .ok_or_else(|| Error::Fabric(format!("Fabric has no loader for {game} yet")))
}

pub async fn profile(net: &Net, game: &str, loader: &str) -> Result<FabricProfile> {
    let url = format!(
        "{META}/versions/loader/{}/{}/profile/json",
        token(game)?,
        token(loader)?
    );
    let profile: FabricProfile = net.json(&url, "the Fabric launch profile").await?;
    if profile.inherits_from != game {
        return Err(Error::Fabric(format!(
            "Fabric sent a profile for {} instead of {game}",
            profile.inherits_from
        )));
    }
    Ok(profile)
}

pub async fn libraries(
    net: &Net,
    paths: &Paths,
    profile: &FabricProfile,
) -> Result<Vec<ResolvedLibrary>> {
    let mut out = Vec::new();
    for library in &profile.libraries {
        let relative = maven_path(&library.name)?;
        let base = library.url.trim_end_matches('/');
        let url = format!("{base}/{relative}");
        let hash = match (&library.sha512, &library.sha256, &library.sha1) {
            (Some(h), _, _) => Hash::sha512(h),
            (None, Some(h), _) => Hash::sha256(h),
            (None, None, Some(h)) => Hash::sha1(h),
            (None, None, None) => sidecar_sha256(net, &url).await?,
        };
        let path = paths.libraries().join(&relative);
        out.push(ResolvedLibrary {
            key: library_key(&library.name)?,
            path: path.clone(),
            download: Download {
                url,
                dest: path,
                hash,
                size: library.size,
                executable: false,
            },
        });
    }
    Ok(out)
}

async fn sidecar_sha256(net: &Net, artifact_url: &str) -> Result<Hash> {
    let text = net.bytes(&format!("{artifact_url}.sha256")).await?;
    parse_sidecar(&String::from_utf8_lossy(&text))
        .map(Hash::sha256)
        .ok_or_else(|| {
            Error::Fabric(format!(
                "Fabric's maven has no usable sha256 for {artifact_url}"
            ))
        })
}

fn parse_sidecar(text: &str) -> Option<&str> {
    let first = text.split_whitespace().next()?;
    (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit())).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_block_path_tricks() {
        assert!(token("1.21.4").is_ok());
        assert!(token("0.19.5").is_ok());
        assert!(token("26.4-snapshot-2").is_ok());
        assert!(token("../x").is_err());
        assert!(token("a/b").is_err());
        assert!(token("..").is_err());
        assert!(token("").is_err());
    }

    #[test]
    fn sidecars_need_a_real_sha256() {
        let good = "a".repeat(64);
        assert_eq!(
            parse_sidecar(&format!("{good}  fabric-loader.jar\n")),
            Some(good.as_str())
        );
        assert_eq!(parse_sidecar("nope"), None);
    }

    #[test]
    fn profile_parses_the_live_shape() {
        let profile: FabricProfile = serde_json::from_str(
            r#"{"id":"fabric-loader-0.19.5-26.3","inheritsFrom":"26.3","releaseTime":"x","time":"x","type":"release","mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient","arguments":{"game":[],"jvm":["-DFabricMcEmu= net.minecraft.client.main.Main "]},"libraries":[{"name":"net.fabricmc:fabric-loader:0.19.5","url":"https://maven.fabricmc.net/"}]}"#,
        )
        .unwrap();
        assert_eq!(
            profile.main_class,
            "net.fabricmc.loader.impl.launch.knot.KnotClient"
        );
        assert_eq!(
            profile.arguments.jvm,
            vec!["-DFabricMcEmu= net.minecraft.client.main.Main "]
        );
        assert!(profile.libraries[0].sha1.is_none());
    }
}
