use std::path::PathBuf;

use super::rules::{self, Context};
use super::version::Library;
use crate::download::Download;
use crate::hash::Hash;
use crate::paths::Paths;
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct ResolvedLibrary {
    pub key: String,
    pub path: PathBuf,
    pub download: Download,
}

pub fn resolve(
    libraries: &[Library],
    paths: &Paths,
    ctx: &Context<'_>,
) -> Result<Vec<ResolvedLibrary>> {
    let mut out = Vec::new();
    for library in libraries {
        if !rules::allowed(&library.rules, ctx) {
            continue;
        }
        let legacy_natives = library.natives.is_some()
            || library.extract.is_some()
            || library
                .downloads
                .as_ref()
                .is_some_and(|d| d.classifiers.is_some());
        if legacy_natives {
            return Err(Error::Unsupported(format!(
                "{} uses the legacy natives layout, which only very old versions need",
                library.name
            )));
        }
        let Some(artifact) = library.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let path = paths.libraries().join(&artifact.path);
        out.push(ResolvedLibrary {
            key: library_key(&library.name)?,
            path: path.clone(),
            download: Download {
                url: artifact.url.clone(),
                dest: path,
                hash: Hash::sha1(&artifact.sha1),
                size: Some(artifact.size),
                executable: false,
            },
        });
    }
    Ok(out)
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
    let file = match c.classifier {
        Some(classifier) => format!("{}-{}-{classifier}.jar", c.artifact, c.version),
        None => format!("{}-{}.jar", c.artifact, c.version),
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
    }

    fn lib(json: &str) -> Library {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn natives_follow_os_rules_and_legacy_layout_is_refused() {
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
        let keys: Vec<&str> = resolved.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["org.lwjgl:lwjgl", "org.lwjgl:lwjgl:natives-linux"]
        );

        let legacy = vec![lib(
            r#"{"name":"a:b:1","natives":{"linux":"natives-linux"}}"#,
        )];
        assert!(matches!(
            resolve(&legacy, &paths, &ctx),
            Err(Error::Unsupported(_))
        ));
    }
}
