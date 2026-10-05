pub mod version;

use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use std::path::Path;

use serde::Deserialize;

use crate::error::IoContext;
use crate::{Error, Result};

pub use version::{Requirement, Version};

const NESTING: u8 = 2;

#[derive(Clone, Debug)]
pub struct ModInfo {
    pub file: String,
    pub title: String,
    pub id: String,
    pub version: Version,
    provided: Vec<(String, Version)>,
    depends: BTreeMap<String, Requirement>,
    breaks: BTreeMap<String, Requirement>,
}

#[derive(Clone, Debug)]
pub struct Environment {
    pub game: String,
    pub java_major: String,
    pub loader: String,
}

#[derive(Debug)]
pub enum Jar {
    Mod(Box<ModInfo>),
    Plain,
    Unreadable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub file: String,
    pub title: String,
    pub message: String,
}

#[derive(Deserialize)]
struct FabricModJson {
    id: String,
    version: String,
    #[serde(default)]
    provides: Vec<String>,
    #[serde(default)]
    depends: BTreeMap<String, OneOrMany>,
    #[serde(default)]
    breaks: BTreeMap<String, OneOrMany>,
    #[serde(default)]
    jars: Vec<NestedJar>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn requirement(&self) -> Option<Requirement> {
        match self {
            Self::One(text) => Requirement::parse(std::slice::from_ref(text)),
            Self::Many(list) => Requirement::parse(list),
        }
    }
}

#[derive(Deserialize)]
struct NestedJar {
    file: String,
}

pub async fn read(path: &Path, title: &str) -> Result<Jar> {
    let bytes = tokio::fs::read(path).await.at(path)?;
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    read_bytes(&bytes, path, &file, title)
}

fn read_bytes(bytes: &[u8], path: &Path, file: &str, title: &str) -> Result<Jar> {
    let meta = match metadata(bytes, path)? {
        Meta::Mod(meta) => meta,
        Meta::Plain => return Ok(Jar::Plain),
        Meta::Unreadable => return Ok(Jar::Unreadable),
    };
    let mut provided = Vec::new();
    for nested in &meta.jars {
        collect_nested(bytes, path, &nested.file, 1, &mut provided)?;
    }
    let requirements = |map: &BTreeMap<String, OneOrMany>| {
        map.iter()
            .filter_map(|(id, value)| match value.requirement() {
                Some(r) => Some((id.clone(), r)),
                None => {
                    tracing::debug!(file, dependency = %id, "skipping a version rule this launcher can't read");
                    None
                }
            })
            .collect()
    };
    let version = Version::parse(&meta.version);
    provided.extend(meta.provides.iter().map(|p| (p.clone(), version.clone())));
    Ok(Jar::Mod(Box::new(ModInfo {
        file: file.to_string(),
        title: title.to_string(),
        id: meta.id.clone(),
        version,
        provided,
        depends: requirements(&meta.depends),
        breaks: requirements(&meta.breaks),
    })))
}

enum Meta {
    Mod(Box<FabricModJson>),
    Plain,
    Unreadable,
}

fn metadata(bytes: &[u8], path: &Path) -> Result<Meta> {
    let zip_error = |source| Error::Zip {
        path: path.to_path_buf(),
        source,
    };
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    let Ok(mut entry) = archive.by_name("fabric.mod.json") else {
        return Ok(Meta::Plain);
    };
    let mut text = Vec::new();
    entry.read_to_end(&mut text).at(path)?;
    Ok(match serde_json::from_slice(&text) {
        Ok(meta) => Meta::Mod(Box::new(meta)),
        Err(_) => Meta::Unreadable,
    })
}

fn collect_nested(
    outer: &[u8],
    path: &Path,
    name: &str,
    depth: u8,
    out: &mut Vec<(String, Version)>,
) -> Result<()> {
    let zip_error = |source| Error::Zip {
        path: path.to_path_buf(),
        source,
    };
    let mut archive = zip::ZipArchive::new(Cursor::new(outer)).map_err(zip_error)?;
    let Ok(mut entry) = archive.by_name(name) else {
        return Ok(());
    };
    let mut inner = Vec::new();
    entry.read_to_end(&mut inner).at(path)?;
    let Meta::Mod(meta) = metadata(&inner, path)? else {
        return Ok(());
    };
    let version = Version::parse(&meta.version);
    out.push((meta.id.clone(), version.clone()));
    out.extend(meta.provides.iter().map(|p| (p.clone(), version.clone())));
    if depth < NESTING {
        for nested in &meta.jars {
            collect_nested(&inner, path, &nested.file, depth + 1, out)?;
        }
    }
    Ok(())
}

fn friendly(id: &str) -> Option<&'static str> {
    match id {
        "minecraft" => Some("Minecraft"),
        "java" => Some("Java"),
        "fabricloader" | "fabric-loader" => Some("Fabric Loader"),
        "fabric" | "fabric-api" => Some("Fabric API"),
        _ if id.starts_with("fabric-") => Some("Fabric API"),
        _ => None,
    }
}

fn only_fabric_api_provides(id: &str) -> bool {
    id == "fabric" || (id.starts_with("fabric-") && id != "fabric-loader")
}

struct Providers<'a> {
    by_id: HashMap<&'a str, Vec<(&'a ModInfo, &'a Version)>>,
    builtin: HashMap<&'static str, Version>,
}

impl<'a> Providers<'a> {
    fn new(mods: &[&'a ModInfo], env: &Environment) -> Self {
        let mut by_id: HashMap<&str, Vec<(&ModInfo, &Version)>> = HashMap::new();
        for m in mods {
            by_id.entry(&m.id).or_default().push((m, &m.version));
            for (id, version) in &m.provided {
                by_id.entry(id).or_default().push((m, version));
            }
        }
        let builtin = HashMap::from([
            ("minecraft", Version::parse(&env.game)),
            ("java", Version::parse(&env.java_major)),
            ("fabricloader", Version::parse(&env.loader)),
            ("fabric-loader", Version::parse(&env.loader)),
        ]);
        Self { by_id, builtin }
    }

    fn versions(&self, id: &str) -> Vec<(Option<&'a ModInfo>, Version)> {
        if let Some(version) = self.builtin.get(id) {
            return vec![(None, version.clone())];
        }
        self.by_id
            .get(id)
            .map(|list| list.iter().map(|(m, v)| (Some(*m), (*v).clone())).collect())
            .unwrap_or_default()
    }

    fn name(&self, id: &str) -> String {
        if let Some(name) = friendly(id) {
            return name.to_string();
        }
        self.by_id
            .get(id)
            .and_then(|list| list.first())
            .map_or_else(|| id.to_string(), |(m, _)| m.title.clone())
    }
}

pub fn check(mods: &[ModInfo], env: &Environment, unreadable: bool) -> Vec<Problem> {
    let mut active: Vec<&ModInfo> = mods.iter().collect();
    let mut gone: HashMap<String, String> = HashMap::new();
    let mut problems = Vec::new();
    while let Some((index, message)) = first_problem(&active, &gone, env, unreadable) {
        let removed = active.remove(index);
        gone.insert(removed.id.clone(), removed.title.clone());
        for (id, _) in &removed.provided {
            gone.insert(id.clone(), removed.title.clone());
        }
        problems.push(Problem {
            file: removed.file.clone(),
            title: removed.title.clone(),
            message,
        });
    }
    problems
}

fn first_problem(
    active: &[&ModInfo],
    gone: &HashMap<String, String>,
    env: &Environment,
    unreadable: bool,
) -> Option<(usize, String)> {
    let providers = Providers::new(active, env);
    let game = &env.game;
    for (index, m) in active.iter().enumerate() {
        let me = format!("{} {}", m.title, m.version.short());
        for (id, requirement) in &m.depends {
            let found = providers.versions(id);
            if found.is_empty() {
                if let Some(title) = gone.get(id) {
                    return Some((
                        index,
                        format!(
                            "{me} needs {title}, which was skipped, so {} was skipped too.",
                            m.title
                        ),
                    ));
                }
                if !unreadable && only_fabric_api_provides(id) {
                    return Some((
                        index,
                        format!(
                            "{me} needs {}, which isn't installed, so {} was skipped.",
                            providers.name(id),
                            m.title
                        ),
                    ));
                }
                continue;
            }
            if found.iter().any(|(_, v)| requirement.matches(v)) {
                continue;
            }
            let name = providers.name(id);
            let have = found
                .first()
                .map(|(_, v)| v.short().to_string())
                .unwrap_or_default();
            let message = match id.as_str() {
                "minecraft" => format!(
                    "{me} is made for Minecraft {}, not {game}, so it was skipped.",
                    requirement.text()
                ),
                _ => format!(
                    "{me} needs {name} {}, but the {name} for {game} is {have}, so {} was skipped.",
                    requirement.text(),
                    m.title
                ),
            };
            return Some((index, message));
        }
        for (id, requirement) in &m.breaks {
            for (owner, version) in providers.versions(id) {
                let Some(owner) = owner else { continue };
                if !requirement.matches(&version) || std::ptr::eq(owner, *m) {
                    continue;
                }
                let target_is_api = owner.id == "fabric-api";
                let (drop, other) = if target_is_api {
                    (*m, owner)
                } else {
                    (owner, *m)
                };
                let Some(drop_index) = active.iter().position(|a| std::ptr::eq(*a, drop)) else {
                    continue;
                };
                return Some((
                    drop_index,
                    format!(
                        "{} {} doesn't work with {} {}, so {} was skipped.",
                        drop.title,
                        drop.version.short(),
                        other.title,
                        other.version.short(),
                        drop.title
                    ),
                ));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            for (name, data) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    fn unwrap_mod(jar: Jar) -> ModInfo {
        match jar {
            Jar::Mod(m) => *m,
            other => panic!("expected a mod, got {other:?}"),
        }
    }

    fn info(file: &str, title: &str, json: &str) -> ModInfo {
        let bytes = jar(&[("fabric.mod.json", json.as_bytes())]);
        unwrap_mod(read_bytes(&bytes, Path::new(file), file, title).unwrap())
    }

    fn env(game: &str) -> Environment {
        Environment {
            game: game.into(),
            java_major: "25".into(),
            loader: "0.19.5".into(),
        }
    }

    fn api() -> ModInfo {
        let module = jar(&[(
            "fabric.mod.json",
            br#"{"id":"fabric-renderer-api-v1","version":"8.0.1"}"#,
        )]);
        let bytes = jar(&[
            (
                "fabric.mod.json",
                br#"{"id":"fabric-api","version":"0.161.0+26.3","jars":[{"file":"META-INF/jars/r.jar"}]}"#,
            ),
            ("META-INF/jars/r.jar", &module),
        ]);
        unwrap_mod(read_bytes(&bytes, Path::new("api.jar"), "api.jar", "Fabric API").unwrap())
    }

    #[test]
    fn a_dependency_on_the_wrong_minor_is_caught() {
        let sodium = info(
            "sodium.jar",
            "Sodium",
            r#"{"id":"sodium","version":"0.8.9+mc26.1.1","depends":{"minecraft":"26.1"}}"#,
        );
        let iris = info(
            "iris.jar",
            "Iris",
            r#"{"id":"iris","version":"1.11.4+26.1","depends":{"fabricloader":">=0.12.3","sodium":["0.9.x"]}}"#,
        );
        let problems = check(&[sodium, iris], &env("26.1"), false);
        assert_eq!(
            problems,
            vec![Problem {
                file: "iris.jar".into(),
                title: "Iris".into(),
                message: "Iris 1.11.4 needs Sodium 0.9.x, but the Sodium for 26.1 is 0.8.9, so Iris was skipped.".into(),
            }]
        );
    }

    #[test]
    fn matching_sets_have_no_problems() {
        let sodium = info(
            "sodium.jar",
            "Sodium",
            r#"{"id":"sodium","version":"0.9.2+mc26.3","depends":{"minecraft":["26.3"],"fabric-renderer-api-v1":"*","java":">=25"}}"#,
        );
        let iris = info(
            "iris.jar",
            "Iris",
            r#"{"id":"iris","version":"1.11.7+26.3","depends":{"sodium":["0.9.x"]}}"#,
        );
        assert!(check(&[api(), sodium, iris], &env("26.3"), false).is_empty());
    }

    #[test]
    fn missing_fabric_api_modules_drop_the_mod_and_its_dependents() {
        let sodium = info(
            "sodium.jar",
            "Sodium",
            r#"{"id":"sodium","version":"0.9.2","depends":{"fabric-renderer-api-v1":"*"}}"#,
        );
        let iris = info(
            "iris.jar",
            "Iris",
            r#"{"id":"iris","version":"1.11.7","depends":{"sodium":"0.9.x"}}"#,
        );
        let problems = check(&[sodium, iris], &env("26.3"), false);
        let files: Vec<&str> = problems.iter().map(|p| p.file.as_str()).collect();
        assert_eq!(files, vec!["sodium.jar", "iris.jar"]);
        assert_eq!(
            problems[0].message,
            "Sodium 0.9.2 needs Fabric API, which isn't installed, so Sodium was skipped."
        );
        assert_eq!(
            problems[1].message,
            "Iris 1.11.7 needs Sodium, which was skipped, so Iris was skipped too."
        );
    }

    #[test]
    fn unknown_ids_outside_fabric_api_are_left_to_the_game() {
        let m = info(
            "m.jar",
            "M",
            r#"{"id":"m","version":"1.0.0","depends":{"mixinextras":">=0.3"}}"#,
        );
        assert!(check(&[m], &env("26.3"), false).is_empty());
    }

    #[test]
    fn an_unreadable_jar_turns_off_missing_checks() {
        let sodium = info(
            "sodium.jar",
            "Sodium",
            r#"{"id":"sodium","version":"0.9.2","depends":{"fabric-renderer-api-v1":"*"}}"#,
        );
        assert!(check(&[sodium], &env("26.3"), true).is_empty());
    }

    #[test]
    fn breaks_drop_the_old_target_not_the_newer_mod() {
        let sodium = info(
            "sodium.jar",
            "Sodium",
            r#"{"id":"sodium","version":"0.6.13+mc1.21.4","breaks":{"iris":"<1.8.7"}}"#,
        );
        let iris = info(
            "iris.jar",
            "Iris",
            r#"{"id":"iris","version":"1.8.6+mc1.21.4"}"#,
        );
        let problems = check(&[sodium, iris], &env("1.21.4"), false);
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].file, "iris.jar");
        assert_eq!(
            problems[0].message,
            "Iris 1.8.6 doesn't work with Sodium 0.6.13, so Iris was skipped."
        );
    }

    #[test]
    fn wrong_game_version_is_explained() {
        let m = info(
            "if.jar",
            "ImmediatelyFast",
            r#"{"id":"immediatelyfast","version":"1.15.3+26.1","depends":{"minecraft":"26.1"}}"#,
        );
        let problems = check(&[m], &env("26.1.2"), false);
        assert_eq!(
            problems[0].message,
            "ImmediatelyFast 1.15.3 is made for Minecraft 26.1, not 26.1.2, so it was skipped."
        );
    }

    #[test]
    fn jars_without_fabric_metadata_are_not_mods() {
        let plain = jar(&[("readme.txt", b"hi")]);
        assert!(matches!(
            read_bytes(&plain, Path::new("x.jar"), "x.jar", "X").unwrap(),
            Jar::Plain
        ));
        let broken = jar(&[("fabric.mod.json", b"{not json")]);
        assert!(matches!(
            read_bytes(&broken, Path::new("y.jar"), "y.jar", "Y").unwrap(),
            Jar::Unreadable
        ));
    }
}
