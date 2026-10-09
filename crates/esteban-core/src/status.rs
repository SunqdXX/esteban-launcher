use std::path::PathBuf;

use serde::Serialize;

use crate::Result;
use crate::instance::Instance;
use crate::loader::Loader;
use crate::modrinth::DEFAULT_MODS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRow {
    pub slug: String,
    pub title: String,
    pub version: Option<String>,
    pub wanted: bool,
    pub installed: bool,
    pub required: bool,
    pub managed: bool,
    pub skipped: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraJar {
    pub file: String,
    pub title: String,
    pub hacks: bool,
}

impl ExtraJar {
    fn from_file(file: &str) -> Self {
        let hacks = !file.starts_with("esteban-hud-");
        Self {
            file: file.to_string(),
            title: if hacks {
                "Esteban hacks mod"
            } else {
                "Esteban HUD"
            }
            .to_string(),
            hacks,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceStatus {
    pub loader: Loader,
    pub hacked: bool,
    pub game_version: String,
    pub installed: bool,
    pub loader_version: Option<String>,
    pub loader_pinned: bool,
    pub skin: Option<String>,
    pub mods: Vec<ModRow>,
    pub extras: Vec<ExtraJar>,
    pub unmanaged: Vec<String>,
    pub folder: PathBuf,
}

pub async fn status(instance: &Instance) -> Result<InstanceStatus> {
    let file = instance.read_file().await?;
    let lock = file.lock();
    let defaults: &[crate::modrinth::DefaultMod] = if instance.loader == Loader::Fabric {
        DEFAULT_MODS
    } else {
        &[]
    };
    let mut mods = Vec::with_capacity(defaults.len());
    for known in defaults {
        let resolved = lock.as_ref().and_then(|l| {
            l.mods
                .iter()
                .find(|m| m.slug == known.slug || m.title == known.title)
        });
        let skipped = lock.as_ref().and_then(|l| {
            l.skipped
                .iter()
                .find(|s| s.title == known.title || s.title == known.slug)
                .map(|s| s.message.clone())
        });
        mods.push(ModRow {
            slug: known.slug.to_string(),
            title: known.title.to_string(),
            version: resolved.map(|m| m.version_number.clone()),
            wanted: known.required || !file.disabled.contains(known.slug),
            installed: resolved.is_some(),
            required: known.required,
            managed: true,
            skipped,
        });
    }
    if let Some(lock) = &lock {
        for m in &lock.mods {
            if defaults
                .iter()
                .any(|d| d.slug == m.slug || d.title == m.title)
            {
                continue;
            }
            mods.push(ModRow {
                slug: m.slug.clone(),
                title: m.title.clone(),
                version: Some(m.version_number.clone()),
                wanted: true,
                installed: true,
                required: false,
                managed: false,
                skipped: None,
            });
        }
    }
    let unmanaged = instance.unmanaged_jars(lock.as_ref()).await?;
    Ok(InstanceStatus {
        loader: instance.loader,
        hacked: instance.hacked,
        game_version: instance.game_version.clone(),
        installed: lock.is_some(),
        loader_version: file.loader.version.clone(),
        loader_pinned: file.loader.pinned,
        skin: file.skin.clone(),
        mods,
        extras: lock
            .map(|l| l.extras.iter().map(|f| ExtraJar::from_file(f)).collect())
            .unwrap_or_default(),
        unmanaged,
        folder: instance.dir.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::InstanceFile;
    use crate::modrinth::lock::{ModLock, Skipped};
    use crate::modrinth::resolve::ResolvedMod;
    use crate::paths::Paths;

    fn resolved(slug: &str, title: &str, version: &str) -> ResolvedMod {
        ResolvedMod {
            slug: slug.into(),
            project_id: slug.into(),
            title: title.into(),
            version_id: version.into(),
            version_number: version.into(),
            filename: format!("{slug}-{version}.jar"),
            url: String::new(),
            sha512: String::new(),
            size: 0,
            requires: Vec::new(),
        }
    }

    #[tokio::test]
    async fn a_missing_instance_lists_every_default_mod_as_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let instance =
            Instance::new(&Paths::new(dir.path()), "26.3", Loader::Fabric, false).unwrap();
        let s = status(&instance).await.unwrap();
        assert!(!s.installed);
        assert_eq!(s.mods.len(), DEFAULT_MODS.len());
        assert!(s.mods.iter().all(|m| m.wanted && !m.installed && m.managed));
        assert!(s.unmanaged.is_empty() && s.extras.is_empty());
    }

    #[tokio::test]
    async fn rows_show_versions_toggles_skips_and_extra_jars() {
        let dir = tempfile::tempdir().unwrap();
        let instance =
            Instance::new(&Paths::new(dir.path()), "26.1", Loader::Fabric, true).unwrap();
        std::fs::create_dir_all(instance.mods_dir()).unwrap();
        let lock = ModLock {
            game_version: "26.1".into(),
            mods: vec![
                resolved("fabric-api", "Fabric API", "0.145.1"),
                resolved("sodium", "Sodium", "0.8.9"),
                resolved("lithium", "Lithium", "0.20.0"),
                resolved("indium", "Indium", "1.0.0"),
            ],
            extras: vec![
                "esteban-hud-1.4.0+26.1.jar".into(),
                "esteban-1.4.0+26.1.jar".into(),
            ],
            skipped: vec![Skipped {
                title: "Iris Shaders".into(),
                message: "Iris Shaders isn't available for 26.1 yet, so it was skipped.".into(),
            }],
            disabled: vec!["lithium".into()],
        };
        lock.write(&instance.legacy_lock_path()).await.unwrap();
        std::fs::write(
            instance.meta_path(),
            br#"{"loader_version":"0.19.3","disabled":["lithium","fabric-api"]}"#,
        )
        .unwrap();
        let converted: InstanceFile = instance.read_file().await.unwrap();
        assert!(converted.installed);
        std::fs::write(instance.mods_dir().join("mine.jar"), b"jar").unwrap();
        std::fs::write(instance.mods_dir().join("sodium-0.8.9.jar"), b"jar").unwrap();

        let s = status(&instance).await.unwrap();
        assert!(s.installed);
        assert_eq!(s.loader_version.as_deref(), Some("0.19.3"));
        let row = |slug: &str| s.mods.iter().find(|m| m.slug == slug).unwrap().clone();
        assert_eq!(row("sodium").version.as_deref(), Some("0.8.9"));
        assert!(row("sodium").wanted && row("sodium").installed);
        assert!(!row("lithium").wanted && row("lithium").installed);
        assert!(row("fabric-api").wanted && row("fabric-api").required);
        assert!(!row("iris").installed);
        assert!(
            row("iris")
                .skipped
                .as_deref()
                .unwrap()
                .contains("isn't available for 26.1")
        );
        assert!(!row("indium").managed && row("indium").installed);
        assert_eq!(s.extras.len(), 2);
        assert!(!s.extras[0].hacks && s.extras[0].title == "Esteban HUD");
        assert!(s.extras[1].hacks && s.extras[1].file == "esteban-1.4.0+26.1.jar");
        assert_eq!(s.unmanaged, vec!["mine.jar".to_string()]);
        assert!(s.hacked && s.loader == Loader::Fabric && !s.loader_pinned);
    }

    #[tokio::test]
    async fn vanilla_and_forge_list_no_performance_mods() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        for (version, loader) in [("1.8.9", Loader::Vanilla), ("1.20.1", Loader::Forge)] {
            let instance = Instance::new(&paths, version, loader, false).unwrap();
            let s = status(&instance).await.unwrap();
            assert!(s.mods.is_empty() && !s.installed && s.loader == loader);
        }
    }
}
