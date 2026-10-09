use std::collections::BTreeMap;
use std::path::PathBuf;

use super::rules::{self, Context};
use super::version::{Artifact, Library};
use crate::download::Download;
use crate::hash::Hash;
use crate::paths::Paths;
use crate::system::{Arch, Platform};
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct ResolvedLibrary {
    pub key: String,
    pub path: PathBuf,
    pub download: Download,
}

#[derive(Clone, Debug)]
pub struct NativeJar {
    pub path: PathBuf,
    pub download: Download,
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Resolved {
    pub classpath: Vec<ResolvedLibrary>,
    pub natives: Vec<NativeJar>,
}

pub fn resolve(libraries: &[Library], paths: &Paths, ctx: &Context<'_>) -> Result<Resolved> {
    let mut out = Resolved::default();
    for library in libraries {
        if !rules::allowed(&library.rules, ctx) {
            continue;
        }
        if let Some(natives) = &library.natives
            && let Some(classifier) = native_classifier(natives, ctx.platform)
        {
            let artifact = library
                .downloads
                .as_ref()
                .and_then(|d| d.classifiers.as_ref())
                .and_then(|c| c.get(&classifier));
            match artifact {
                Some(artifact) => {
                    let path = paths.libraries().join(&artifact.path);
                    out.natives.push(NativeJar {
                        path: path.clone(),
                        download: download_of(artifact, path),
                        exclude: library
                            .extract
                            .as_ref()
                            .map(|e| e.exclude.clone())
                            .unwrap_or_default(),
                    });
                }
                None => tracing::warn!(
                    library = %library.name,
                    %classifier,
                    "the version file lists natives without a download for this system"
                ),
            }
        }
        let Some(artifact) = library.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let path = paths.libraries().join(&artifact.path);
        out.classpath.push(ResolvedLibrary {
            key: library_key(&library.name)?,
            path: path.clone(),
            download: download_of(artifact, path),
        });
    }
    Ok(out)
}

fn download_of(artifact: &Artifact, dest: PathBuf) -> Download {
    Download {
        url: artifact.url.clone(),
        dest,
        hash: Hash::sha1(&artifact.sha1),
        size: Some(artifact.size),
        executable: false,
    }
}

fn native_classifier(natives: &BTreeMap<String, String>, platform: &Platform) -> Option<String> {
    let bits = if platform.arch == Arch::X86 {
        "32"
    } else {
        "64"
    };
    natives
        .get(platform.mojang_os())
        .map(|c| c.replace("${arch}", bits))
}

pub struct Coordinate<'a> {
    pub group: &'a str,
    pub artifact: &'a str,
    pub version: &'a str,
    pub classifier: Option<&'a str>,
}

pub fn parse_coordinate(name: &str) -> Result<Coordinate<'_>> {
    let name = name.split('@').next().unwrap_or(name);
    let parts: Vec<&str> = name.split(':').collect();
    match parts.as_slice() {
        [group, artifact, version] => Ok(Coordinate {
            group,
            artifact,
            version,
            classifier: None,
        }),
        [group, artifact, version, classifier] => Ok(Coordinate {
            group,
            artifact,
            version,
            classifier: Some(classifier),
        }),
        _ => Err(Error::Unsupported(format!(
            "not a maven coordinate: {name}"
        ))),
    }
}

pub fn library_key(name: &str) -> Result<String> {
    let c = parse_coordinate(name)?;
    Ok(match c.classifier {
        Some(classifier) => format!("{}:{}:{classifier}", c.group, c.artifact),
        None => format!("{}:{}", c.group, c.artifact),
    })
}

pub fn maven_path(name: &str) -> Result<String> {
    let c = parse_coordinate(name)?;
    let extension = name
        .split_once('@')
        .map(|(_, ext)| ext)
        .filter(|ext| !ext.is_empty() && ext.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .unwrap_or("jar");
    let file = match c.classifier {
        Some(classifier) => format!("{}-{}-{classifier}.{extension}", c.artifact, c.version),
        None => format!("{}-{}.{extension}", c.artifact, c.version),
    };
    Ok(format!(
        "{}/{}/{}/{file}",
        c.group.replace('.', "/"),
        c.artifact,
        c.version
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::system::{Arch, OsName, Platform};

    #[test]
    fn maven_paths_and_keys() {
        assert_eq!(
            maven_path("net.fabricmc:fabric-loader:0.19.5").unwrap(),
            "net/fabricmc/fabric-loader/0.19.5/fabric-loader-0.19.5.jar"
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap(),
            "org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar"
        );
        assert_eq!(
            library_key("org.ow2.asm:asm:9.10.1").unwrap(),
            "org.ow2.asm:asm"
        );
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap(),
            "org.lwjgl:lwjgl:natives-linux"
        );
        assert!(maven_path("broken").is_err());
        assert_eq!(
            maven_path("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap(),
            "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        );
        assert_eq!(
            maven_path("net.minecraft:client:1.20.1-20230612.114412:mappings@txt").unwrap(),
            "net/minecraft/client/1.20.1-20230612.114412/client-1.20.1-20230612.114412-mappings.txt"
        );
    }

    fn lib(json: &str) -> Library {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn natives_follow_os_rules() {
        let linux = Platform {
            os: OsName::Linux,
            arch: Arch::X86_64,
            os_version: None,
        };
        let features = BTreeSet::new();
        let ctx = Context {
            platform: &linux,
            features: &features,
        };
        let paths = Paths::new("/base");
        let libs = vec![
            lib(
                r#"{"name":"org.lwjgl:lwjgl:3.3.3","downloads":{"artifact":{"path":"org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3.jar","sha1":"aa","size":1,"url":"https://libraries.minecraft.net/a.jar"}}}"#,
            ),
            lib(
                r#"{"name":"org.lwjgl:lwjgl:3.3.3:natives-linux","downloads":{"artifact":{"path":"l.jar","sha1":"bb","size":1,"url":"https://libraries.minecraft.net/l.jar"}},"rules":[{"action":"allow","os":{"name":"linux"}}]}"#,
            ),
            lib(
                r#"{"name":"org.lwjgl:lwjgl:3.3.3:natives-windows","downloads":{"artifact":{"path":"w.jar","sha1":"cc","size":1,"url":"https://libraries.minecraft.net/w.jar"}},"rules":[{"action":"allow","os":{"name":"windows"}}]}"#,
            ),
        ];
        let resolved = resolve(&libs, &paths, &ctx).unwrap();
        let keys: Vec<&str> = resolved.classpath.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["org.lwjgl:lwjgl", "org.lwjgl:lwjgl:natives-linux"]
        );
        assert!(resolved.natives.is_empty());
    }

    #[test]
    fn classifier_natives_are_picked_per_os_and_arch() {
        let features = BTreeSet::new();
        let paths = Paths::new("/base");
        let lwjgl = lib(
            r#"{"name":"org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209",
                "natives":{"linux":"natives-linux","windows":"natives-windows-${arch}","osx":"natives-osx"},
                "extract":{"exclude":["META-INF/"]},
                "downloads":{"classifiers":{
                    "natives-linux":{"path":"p/l.jar","sha1":"11","size":1,"url":"https://libraries.minecraft.net/l.jar"},
                    "natives-windows-64":{"path":"p/w64.jar","sha1":"22","size":1,"url":"https://libraries.minecraft.net/w64.jar"},
                    "natives-windows-32":{"path":"p/w32.jar","sha1":"33","size":1,"url":"https://libraries.minecraft.net/w32.jar"}}}}"#,
        );
        let twitch = lib(
            r#"{"name":"tv.twitch:twitch-platform:6.5","natives":{"windows":"natives-windows-${arch}"},
                "downloads":{"classifiers":{"natives-windows-64":{"path":"t.jar","sha1":"44","size":1,"url":"https://libraries.minecraft.net/t.jar"}}}}"#,
        );
        let both = lib(
            r#"{"name":"org.lwjgl:lwjgl:3.2.2","natives":{"linux":"natives-linux"},
                "downloads":{"artifact":{"path":"a.jar","sha1":"55","size":1,"url":"https://libraries.minecraft.net/a.jar"},
                "classifiers":{"natives-linux":{"path":"n.jar","sha1":"66","size":1,"url":"https://libraries.minecraft.net/n.jar"}}}}"#,
        );
        let libs = vec![lwjgl, twitch, both];
        let on = |os, arch| {
            let platform = Platform {
                os,
                arch,
                os_version: None,
            };
            let ctx = Context {
                platform: &platform,
                features: &features,
            };
            resolve(&libs, &paths, &ctx).unwrap()
        };
        let linux = on(OsName::Linux, Arch::X86_64);
        let natives: Vec<_> = linux
            .natives
            .iter()
            .map(|n| n.download.url.as_str())
            .collect();
        assert_eq!(
            natives,
            vec![
                "https://libraries.minecraft.net/l.jar",
                "https://libraries.minecraft.net/n.jar"
            ]
        );
        assert_eq!(linux.natives[0].exclude, vec!["META-INF/".to_string()]);
        assert_eq!(linux.classpath.len(), 1);
        assert_eq!(linux.classpath[0].key, "org.lwjgl:lwjgl");
        let windows = on(OsName::Windows, Arch::X86_64);
        assert_eq!(windows.natives.len(), 2);
        assert!(windows.natives[0].path.ends_with("p/w64.jar"));
        assert!(windows.natives[1].path.ends_with("t.jar"));
        let windows32 = on(OsName::Windows, Arch::X86);
        assert!(windows32.natives[0].path.ends_with("p/w32.jar"));
        assert_eq!(windows32.natives.len(), 1);
    }
}
