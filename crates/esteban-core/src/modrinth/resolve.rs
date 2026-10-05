use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::api::{Source, Version, VersionFile};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefaultMod {
    pub slug: &'static str,
    pub title: &'static str,
    pub required: bool,
}

pub const DEFAULT_MODS: &[DefaultMod] = &[
    DefaultMod {
        slug: "fabric-api",
        title: "Fabric API",
        required: true,
    },
    DefaultMod {
        slug: "sodium",
        title: "Sodium",
        required: false,
    },
    DefaultMod {
        slug: "lithium",
        title: "Lithium",
        required: false,
    },
    DefaultMod {
        slug: "ferrite-core",
        title: "FerriteCore",
        required: false,
    },
    DefaultMod {
        slug: "immediatelyfast",
        title: "ImmediatelyFast",
        required: false,
    },
    DefaultMod {
        slug: "iris",
        title: "Iris Shaders",
        required: false,
    },
];

pub fn default_mod(name: &str) -> Option<&'static DefaultMod> {
    let name = name.trim().to_ascii_lowercase();
    DEFAULT_MODS
        .iter()
        .find(|m| m.slug == name || m.title.to_ascii_lowercase() == name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedMod {
    #[serde(default)]
    pub slug: String,
    pub project_id: String,
    pub title: String,
    pub version_id: String,
    pub version_number: String,
    pub filename: String,
    pub url: String,
    pub sha512: String,
    pub size: u64,
    #[serde(default)]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Resolution {
    pub mods: Vec<ResolvedMod>,
    pub unavailable: Vec<Unavailable>,
}

pub fn primary_file(version: &Version) -> Option<&VersionFile> {
    version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
}

fn fits(version: &Version, game: &str) -> bool {
    version.loaders.iter().any(|l| l == "fabric") && version.game_versions.iter().any(|g| g == game)
}

pub fn pick<'a>(versions: &'a [Version], game: &str) -> Option<&'a Version> {
    versions
        .iter()
        .filter(|v| v.version_type == "release")
        .filter(|v| fits(v, game))
        .filter(|v| {
            primary_file(v).is_some_and(|f| {
                f.hashes
                    .sha512
                    .as_deref()
                    .is_some_and(|h| crate::hash::is_hex(h, 128))
            })
        })
        .max_by(|a, b| a.date_published.cmp(&b.date_published))
}

pub fn consequence(project: &str) -> &'static str {
    match project {
        "iris" | "YL57xq9U" => " Shaders are off.",
        "sodium" | "AANobbMI" => " Rendering speedups are off.",
        "lithium" | "gvQqBUqZ" => " Game logic speedups are off.",
        "ferrite-core" | "uXXizFIs" => " Memory savings are off.",
        "immediatelyfast" | "5ZwdcRci" => " HUD and text speedups are off.",
        _ => "",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Why {
    TurnedOff,
    NoBuild,
    Skipped,
}

struct Picked {
    version: Version,
    asked_as: String,
    requires: Vec<String>,
}

struct Names {
    titles: HashMap<String, String>,
    slugs: HashMap<String, String>,
}

impl Names {
    fn learn(&mut self, project: super::api::Project) {
        self.titles
            .insert(project.slug.clone(), project.title.clone());
        self.titles.insert(project.id.clone(), project.title);
        self.slugs.insert(project.id, project.slug);
    }

    fn title(&self, id: &str) -> String {
        self.titles
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string())
    }
}

async fn required_projects<S: Source>(
    source: &S,
    version: &Version,
    owners: &mut HashMap<String, String>,
) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for dependency in version
        .dependencies
        .iter()
        .filter(|d| d.dependency_type == "required")
    {
        let id = match (&dependency.project_id, &dependency.version_id) {
            (Some(project), _) => project.clone(),
            (None, Some(version_id)) => match owners.get(version_id) {
                Some(project) => project.clone(),
                None => {
                    let owner = source.version(version_id).await?.project_id;
                    owners.insert(version_id.clone(), owner.clone());
                    owner
                }
            },
            (None, None) => continue,
        };
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

pub async fn resolve<S: Source>(
    source: &S,
    wanted: &[&str],
    off: &[&str],
    game: &str,
) -> Result<Resolution> {
    let lookup: Vec<String> = wanted.iter().chain(off).map(|s| (*s).to_string()).collect();
    let mut names = Names {
        titles: HashMap::new(),
        slugs: HashMap::new(),
    };
    let mut why: HashMap<String, Why> = off
        .iter()
        .map(|s| ((*s).to_string(), Why::TurnedOff))
        .collect();
    for project in source.projects(&lookup).await? {
        if off.contains(&project.slug.as_str()) || off.contains(&project.id.as_str()) {
            why.insert(project.id.clone(), Why::TurnedOff);
            why.insert(project.slug.clone(), Why::TurnedOff);
        }
        names.learn(project);
    }

    let mut queue: VecDeque<String> = wanted
        .iter()
        .filter(|w| !off.contains(w))
        .map(|s| (*s).to_string())
        .collect();
    let mut tried: HashSet<String> = HashSet::new();
    let mut picked: BTreeMap<String, Picked> = BTreeMap::new();
    let mut owners: HashMap<String, String> = HashMap::new();
    let mut unavailable = Vec::new();

    while let Some(project) = queue.pop_front() {
        if !tried.insert(project.clone()) || why.get(&project) == Some(&Why::TurnedOff) {
            continue;
        }
        let versions = match source.versions(&project, game).await {
            Ok(versions) => versions,
            Err(Error::Status { status: 404, .. }) => {
                let title = names.title(&project);
                why.insert(project.clone(), Why::Skipped);
                unavailable.push(Unavailable {
                    message: format!(
                        "{title} is no longer on Modrinth, so it was skipped.{}",
                        consequence(&project)
                    ),
                    title,
                });
                continue;
            }
            Err(e) => return Err(e),
        };
        let Some(version) = pick(&versions, game) else {
            if !names.titles.contains_key(&project) {
                for found in source.projects(std::slice::from_ref(&project)).await? {
                    names.learn(found);
                }
            }
            let title = names.title(&project);
            let reason = if versions.iter().any(|v| fits(v, game)) {
                format!("{title} only has test builds for {game} so far, so it was skipped.")
            } else {
                format!("{title} isn't available for {game} yet, so it was skipped.")
            };
            why.insert(project.clone(), Why::NoBuild);
            unavailable.push(Unavailable {
                message: format!("{reason}{}", consequence(&project)),
                title,
            });
            continue;
        };
        tried.insert(version.project_id.clone());
        if picked.contains_key(&version.project_id) {
            continue;
        }
        let requires = required_projects(source, version, &mut owners).await?;
        for id in &requires {
            if !tried.contains(id) && why.get(id) != Some(&Why::TurnedOff) {
                queue.push_back(id.clone());
            }
        }
        picked.insert(
            version.project_id.clone(),
            Picked {
                version: version.clone(),
                asked_as: project,
                requires,
            },
        );
    }

    let mut missing: Vec<String> = picked
        .values()
        .flat_map(|p| {
            p.version
                .dependencies
                .iter()
                .filter_map(|d| d.project_id.clone())
                .chain(p.requires.iter().cloned())
        })
        .chain(picked.keys().cloned())
        .filter(|id| !names.titles.contains_key(id) || !names.slugs.contains_key(id))
        .collect();
    missing.sort();
    missing.dedup();
    for project in source.projects(&missing).await? {
        names.learn(project);
    }

    loop {
        let clash = picked.iter().find_map(|(id, p)| {
            p.version
                .dependencies
                .iter()
                .filter(|d| d.dependency_type == "incompatible")
                .filter_map(|d| d.project_id.as_ref())
                .find(|other| *other != id && picked.contains_key(*other))
                .map(|other| (id.clone(), other.clone()))
        });
        let Some((id, other)) = clash else {
            break;
        };
        picked.remove(&id);
        why.insert(id.clone(), Why::Skipped);
        let title = names.title(&id);
        unavailable.push(Unavailable {
            message: format!(
                "{title} can't run alongside {}, so {title} was skipped.{}",
                names.title(&other),
                consequence(&id)
            ),
            title,
        });
    }

    let mut mods: BTreeMap<String, ResolvedMod> = BTreeMap::new();
    for (id, p) in &picked {
        let Some(file) = primary_file(&p.version) else {
            continue;
        };
        let Some(sha512) = file.hashes.sha512.clone() else {
            continue;
        };
        let title = names
            .titles
            .get(id)
            .or_else(|| names.titles.get(&p.asked_as))
            .cloned()
            .unwrap_or_else(|| p.asked_as.clone());
        let slug = names
            .slugs
            .get(id)
            .cloned()
            .unwrap_or_else(|| p.asked_as.clone());
        mods.insert(
            id.clone(),
            ResolvedMod {
                slug,
                project_id: id.clone(),
                title,
                version_id: p.version.id.clone(),
                version_number: p.version.version_number.clone(),
                filename: file.filename.clone(),
                url: file.url.clone(),
                sha512,
                size: file.size,
                requires: p.requires.clone(),
            },
        );
    }

    loop {
        let broken = mods.values().find_map(|m| {
            m.requires
                .iter()
                .find(|r| !mods.contains_key(*r))
                .map(|r| (m.project_id.clone(), m.title.clone(), r.clone()))
        });
        let Some((id, title, needed)) = broken else {
            break;
        };
        mods.remove(&id);
        let needed_title = names.title(&needed);
        let needed_slug = names.slugs.get(&needed).cloned().unwrap_or_default();
        let reason = why
            .get(&needed)
            .or_else(|| why.get(&needed_slug))
            .copied()
            .unwrap_or(Why::NoBuild);
        let message = match reason {
            Why::TurnedOff => {
                format!(
                    "{title} needs {needed_title}, which you turned off, so {title} is off too."
                )
            }
            Why::NoBuild => format!(
                "{title} needs {needed_title}, which isn't available for {game}, so {title} was skipped too.{}",
                consequence(&id)
            ),
            Why::Skipped => format!(
                "{title} needs {needed_title}, which was skipped, so {title} was skipped too.{}",
                consequence(&id)
            ),
        };
        why.insert(id.clone(), Why::Skipped);
        unavailable.push(Unavailable { title, message });
    }

    let mut mods: Vec<ResolvedMod> = mods.into_values().collect();
    mods.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(Resolution { mods, unavailable })
}

#[cfg(test)]
mod tests {
    use super::super::api::{Dependency, FileHashes, Project};
    use super::*;

    fn version(
        project: &str,
        id: &str,
        kind: &str,
        date: &str,
        games: &[&str],
        deps: &[&str],
    ) -> Version {
        Version {
            id: id.into(),
            project_id: project.into(),
            version_number: id.into(),
            version_type: kind.into(),
            date_published: date.into(),
            game_versions: games.iter().map(|g| (*g).to_string()).collect(),
            loaders: vec!["fabric".into()],
            files: vec![VersionFile {
                url: format!("https://cdn.modrinth.com/data/{project}/{id}.jar"),
                filename: format!("{id}.jar"),
                primary: true,
                size: 10,
                hashes: FileHashes {
                    sha512: Some("ab".repeat(64)),
                    sha1: None,
                },
            }],
            dependencies: deps
                .iter()
                .map(|d| Dependency {
                    project_id: Some((*d).to_string()),
                    version_id: None,
                    dependency_type: "required".into(),
                })
                .collect(),
        }
    }

    struct Fake {
        versions: HashMap<String, Vec<Version>>,
        projects: Vec<Project>,
    }

    impl Source for Fake {
        async fn versions(&self, project: &str, game: &str) -> Result<Vec<Version>> {
            Ok(self
                .versions
                .get(project)
                .map(|v| {
                    v.iter()
                        .filter(|x| x.game_versions.iter().any(|g| g == game))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default())
        }

        async fn projects(&self, ids: &[String]) -> Result<Vec<Project>> {
            Ok(self
                .projects
                .iter()
                .filter(|p| ids.contains(&p.id) || ids.contains(&p.slug))
                .cloned()
                .collect())
        }

        async fn version(&self, id: &str) -> Result<Version> {
            self.versions
                .values()
                .flatten()
                .find(|v| v.id == id)
                .cloned()
                .ok_or_else(|| Error::Status {
                    url: id.into(),
                    status: 404,
                })
        }
    }

    fn iris_and_sodium(game: &str) -> Fake {
        Fake {
            versions: HashMap::from([
                (
                    "iris".to_string(),
                    vec![version(
                        "YL57xq9U",
                        "i1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &[game],
                        &["AANobbMI"],
                    )],
                ),
                (
                    "sodium".to_string(),
                    vec![version(
                        "AANobbMI",
                        "s1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &[game],
                        &[],
                    )],
                ),
                (
                    "AANobbMI".to_string(),
                    vec![version(
                        "AANobbMI",
                        "s1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &[game],
                        &[],
                    )],
                ),
            ]),
            projects: vec![
                project("YL57xq9U", "iris", "Iris Shaders"),
                project("AANobbMI", "sodium", "Sodium"),
            ],
        }
    }

    fn project(id: &str, slug: &str, title: &str) -> Project {
        Project {
            id: id.into(),
            slug: slug.into(),
            title: title.into(),
        }
    }

    #[test]
    fn picks_the_newest_matching_release() {
        let versions = vec![
            version(
                "p",
                "old",
                "release",
                "2026-01-01T00:00:00Z",
                &["1.21.4"],
                &[],
            ),
            version(
                "p",
                "new",
                "release",
                "2026-05-01T00:00:00Z",
                &["1.21.4"],
                &[],
            ),
            version(
                "p",
                "beta",
                "beta",
                "2026-09-01T00:00:00Z",
                &["1.21.4"],
                &[],
            ),
            version(
                "p",
                "other",
                "release",
                "2026-10-01T00:00:00Z",
                &["26.3"],
                &[],
            ),
        ];
        assert_eq!(
            pick(&versions, "1.21.4").map(|v| v.id.as_str()),
            Some("new")
        );
        assert!(pick(&versions, "1.20.1").is_none());
    }

    #[test]
    fn never_picks_a_file_without_a_sha512() {
        let mut v = version(
            "p",
            "x",
            "release",
            "2026-01-01T00:00:00Z",
            &["1.21.4"],
            &[],
        );
        v.files[0].hashes.sha512 = None;
        assert!(pick(&[v], "1.21.4").is_none());
    }

    #[tokio::test]
    async fn pulls_in_required_dependencies() {
        let fake = Fake {
            versions: HashMap::from([
                (
                    "iris".to_string(),
                    vec![version(
                        "YL57xq9U",
                        "i1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &["1.21.4"],
                        &["AANobbMI"],
                    )],
                ),
                (
                    "AANobbMI".to_string(),
                    vec![version(
                        "AANobbMI",
                        "s1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &["1.21.4"],
                        &[],
                    )],
                ),
            ]),
            projects: vec![
                project("YL57xq9U", "iris", "Iris"),
                project("AANobbMI", "sodium", "Sodium"),
            ],
        };
        let r = resolve(&fake, &["iris"], &[], "1.21.4").await.unwrap();
        let titles: Vec<&str> = r.mods.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, vec!["Iris", "Sodium"]);
        assert!(r.unavailable.is_empty());
    }

    #[tokio::test]
    async fn unavailable_mods_are_skipped_with_a_plain_message() {
        let fake = Fake {
            versions: HashMap::from([
                (
                    "sodium".to_string(),
                    vec![version(
                        "AANobbMI",
                        "s1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &["26.3"],
                        &[],
                    )],
                ),
                ("iris".to_string(), vec![]),
            ]),
            projects: vec![
                project("YL57xq9U", "iris", "Iris"),
                project("AANobbMI", "sodium", "Sodium"),
            ],
        };
        let r = resolve(&fake, &["sodium", "iris"], &[], "26.3")
            .await
            .unwrap();
        assert_eq!(r.mods.len(), 1);
        assert_eq!(
            r.unavailable,
            vec![Unavailable {
                title: "Iris".into(),
                message: "Iris isn't available for 26.3 yet, so it was skipped. Shaders are off."
                    .into(),
            }]
        );
    }

    #[tokio::test]
    async fn a_mod_whose_dependency_is_missing_is_dropped_too() {
        let fake = Fake {
            versions: HashMap::from([
                (
                    "iris".to_string(),
                    vec![version(
                        "YL57xq9U",
                        "i1",
                        "release",
                        "2026-01-01T00:00:00Z",
                        &["26.3"],
                        &["AANobbMI"],
                    )],
                ),
                ("AANobbMI".to_string(), vec![]),
            ]),
            projects: vec![
                project("YL57xq9U", "iris", "Iris"),
                project("AANobbMI", "sodium", "Sodium"),
            ],
        };
        let r = resolve(&fake, &["iris"], &[], "26.3").await.unwrap();
        assert!(r.mods.is_empty());
        let messages: Vec<&str> = r.unavailable.iter().map(|u| u.message.as_str()).collect();
        assert!(messages.contains(
            &"Sodium isn't available for 26.3 yet, so it was skipped. Rendering speedups are off."
        ));
        assert!(messages.contains(&"Iris needs Sodium, which isn't available for 26.3, so Iris was skipped too. Shaders are off."));
    }

    #[tokio::test]
    async fn turning_a_dependency_off_turns_its_dependents_off() {
        let fake = iris_and_sodium("1.21.4");
        let r = resolve(&fake, &["iris"], &["sodium"], "1.21.4")
            .await
            .unwrap();
        assert!(r.mods.is_empty());
        assert_eq!(
            r.unavailable,
            vec![Unavailable {
                title: "Iris Shaders".into(),
                message:
                    "Iris Shaders needs Sodium, which you turned off, so Iris Shaders is off too."
                        .into(),
            }]
        );
    }

    #[tokio::test]
    async fn resolved_mods_remember_their_slug() {
        let fake = iris_and_sodium("1.21.4");
        let r = resolve(&fake, &["iris", "sodium"], &[], "1.21.4")
            .await
            .unwrap();
        let slugs: Vec<&str> = r.mods.iter().map(|m| m.slug.as_str()).collect();
        assert_eq!(slugs, vec!["iris", "sodium"]);
    }

    #[tokio::test]
    async fn dependencies_given_only_as_a_version_are_followed() {
        let mut fake = iris_and_sodium("26.3");
        if let Some(iris) = fake.versions.get_mut("iris") {
            iris[0].dependencies = vec![Dependency {
                project_id: None,
                version_id: Some("s1".into()),
                dependency_type: "required".into(),
            }];
        }
        let r = resolve(&fake, &["iris"], &[], "26.3").await.unwrap();
        let titles: Vec<&str> = r.mods.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, vec!["Iris Shaders", "Sodium"]);
        assert_eq!(r.mods[0].requires, vec!["AANobbMI".to_string()]);
    }

    #[tokio::test]
    async fn incompatible_mods_are_not_installed_together() {
        let mut fake = iris_and_sodium("26.3");
        if let Some(iris) = fake.versions.get_mut("iris") {
            iris[0].dependencies = vec![Dependency {
                project_id: Some("AANobbMI".into()),
                version_id: None,
                dependency_type: "incompatible".into(),
            }];
        }
        let r = resolve(&fake, &["sodium", "iris"], &[], "26.3")
            .await
            .unwrap();
        let titles: Vec<&str> = r.mods.iter().map(|m| m.title.as_str()).collect();
        assert_eq!(titles, vec!["Sodium"]);
        assert_eq!(
            r.unavailable[0].message,
            "Iris Shaders can't run alongside Sodium, so Iris Shaders was skipped. Shaders are off."
        );
    }

    #[tokio::test]
    async fn test_builds_only_are_named_as_such() {
        let mut fake = iris_and_sodium("26.3");
        if let Some(iris) = fake.versions.get_mut("iris") {
            iris[0].version_type = "beta".into();
        }
        let r = resolve(&fake, &["iris"], &[], "26.3").await.unwrap();
        assert_eq!(
            r.unavailable[0].message,
            "Iris Shaders only has test builds for 26.3 so far, so it was skipped. Shaders are off."
        );
    }

    #[tokio::test]
    async fn a_project_gone_from_modrinth_is_skipped_not_fatal() {
        struct Gone;
        impl Source for Gone {
            async fn versions(&self, project: &str, _game: &str) -> Result<Vec<Version>> {
                Err(Error::Status {
                    url: project.into(),
                    status: 404,
                })
            }
            async fn projects(&self, _ids: &[String]) -> Result<Vec<Project>> {
                Ok(Vec::new())
            }
            async fn version(&self, id: &str) -> Result<Version> {
                Err(Error::Status {
                    url: id.into(),
                    status: 404,
                })
            }
        }
        let r = resolve(&Gone, &["lithium"], &[], "26.3").await.unwrap();
        assert!(r.mods.is_empty());
        assert_eq!(
            r.unavailable[0].message,
            "lithium is no longer on Modrinth, so it was skipped. Game logic speedups are off."
        );
    }

    #[test]
    fn a_malformed_sha512_is_never_picked() {
        let mut v = version(
            "p",
            "x",
            "release",
            "2026-01-01T00:00:00Z",
            &["1.21.4"],
            &[],
        );
        v.files[0].hashes.sha512 = Some("abc".into());
        assert!(pick(&[v], "1.21.4").is_none());
    }

    #[test]
    fn mods_can_be_named_by_slug_or_title() {
        assert_eq!(default_mod("Iris").map(|m| m.slug), Some("iris"));
        assert_eq!(default_mod("optifine").map(|m| m.slug), None);
        assert_eq!(default_mod("iris shaders").map(|m| m.slug), Some("iris"));
        assert_eq!(default_mod(" Sodium ").map(|m| m.slug), Some("sodium"));
        assert!(default_mod("fabric-api").is_some_and(|m| m.required));
    }
}
