use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::download::Download;
use crate::error::IoContext;
use crate::esteban::{self, HACKS_MOD_ID};
use crate::fabric::token;
use crate::hash::{Hash, sha256_hex};
use crate::loader::Loader;
use crate::modrinth::lock::Skipped;
use crate::modrinth::{ModLock, ResolvedMod, default_mod};
use crate::paths::Paths;
use crate::versions::{PINNED_VERSIONS, is_pinned};
use crate::{Error, Result, fsx};

pub const INSTANCE_SCHEMA: u32 = 2;

pub const CUSTOM_BACKGROUNDS: &str = "custom-backgrounds";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instance {
    pub game_version: String,
    pub loader: Loader,
    pub hacked: bool,
    pub dir: PathBuf,
}

pub fn hacked_allowed(game_version: &str, loader: Loader) -> bool {
    loader == Loader::Fabric && is_pinned(game_version)
}

pub fn folder_name(game_version: &str, loader: Loader, hacked: bool) -> String {
    let suffix = if hacked { "-hacked" } else { "" };
    format!("{}-{game_version}{suffix}", loader.slug())
}

impl Instance {
    pub fn new(paths: &Paths, game_version: &str, loader: Loader, hacked: bool) -> Result<Self> {
        let game_version = token(game_version)?.to_string();
        if hacked && !hacked_allowed(&game_version, loader) {
            let pinned: Vec<&str> = PINNED_VERSIONS.iter().map(|v| v.id).collect();
            return Err(Error::Guard(format!(
                "Hacked is only for {} on Fabric, not {} {game_version}.",
                pinned.join(", "),
                loader.title()
            )));
        }
        let dir = paths
            .instances()
            .join(folder_name(&game_version, loader, hacked));
        Ok(Self {
            game_version,
            loader,
            hacked,
            dir,
        })
    }

    pub fn label(&self) -> String {
        let mut label = format!("{} {}", self.loader.title(), self.game_version);
        if self.hacked {
            label.push_str(" Hacked");
        }
        label
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.dir.join("mods")
    }

    pub fn natives_dir(&self) -> PathBuf {
        self.dir.join("natives")
    }

    pub fn resources_dir(&self) -> PathBuf {
        self.dir.join("resources")
    }

    pub fn crash_reports_dir(&self) -> PathBuf {
        self.dir.join("crash-reports")
    }

    pub fn custom_backgrounds_dir(&self) -> PathBuf {
        self.dir.join(CUSTOM_BACKGROUNDS)
    }

    pub async fn ensure_custom_backgrounds(&self) -> Result<()> {
        fsx::create_dir(&self.custom_backgrounds_dir()).await
    }

    pub fn options_path(&self) -> PathBuf {
        self.dir.join("options.txt")
    }

    pub async fn has_options(&self) -> Result<bool> {
        let path = self.options_path();
        match tokio::fs::symlink_metadata(&path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e).at(&path),
        }
    }

    pub async fn seed_options(&self, data_version: u32, entries: &[(&str, &str)]) -> Result<bool> {
        if self.has_options().await? {
            return Ok(false);
        }
        let mut text = format!("version:{data_version}\n");
        for (key, value) in entries {
            text.push_str(&format!("{key}:{value}\n"));
        }
        fsx::write_atomic(&self.options_path(), text.as_bytes()).await?;
        Ok(true)
    }

    pub fn meta_path(&self) -> PathBuf {
        self.dir.join("instance.json")
    }

    pub fn legacy_lock_path(&self) -> PathBuf {
        self.dir.join("mods.lock.json")
    }

    pub async fn read_file(&self) -> Result<InstanceFile> {
        let path = self.meta_path();
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e).at(&path),
        };
        let mut v1 = InstanceFileV1::default();
        if let Some(bytes) = bytes {
            let value: serde_json::Value = crate::error::json(&bytes, "the instance settings")?;
            if value.get("schema").is_some() {
                let mut file: InstanceFile =
                    serde_json::from_value(value).map_err(|source| Error::Json {
                        what: "the instance settings".into(),
                        source,
                    })?;
                if file.schema > INSTANCE_SCHEMA {
                    return Err(Error::Unsupported(format!(
                        "{} was written by a newer launcher. Update the launcher to use it.",
                        path.display()
                    )));
                }
                file.mc.clone_from(&self.game_version);
                file.loader.kind = self.loader;
                file.hacked = self.hacked;
                return Ok(file);
            }
            v1 = serde_json::from_value(value).map_err(|source| Error::Json {
                what: "the instance settings".into(),
                source,
            })?;
        }
        let lock = match ModLock::read(&self.legacy_lock_path()).await {
            Err(Error::Json { .. }) => {
                tracing::warn!(path = %self.legacy_lock_path().display(), "the old mod lock file is damaged, looking the mods up again");
                None
            }
            other => other?,
        };
        Ok(InstanceFile::from_v1(self, v1, lock))
    }

    pub async fn write_file(&self, file: &InstanceFile) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(file).map_err(|source| Error::Json {
            what: "the instance settings".into(),
            source,
        })?;
        fsx::write_atomic(&self.meta_path(), &bytes).await?;
        let legacy = self.legacy_lock_path();
        match tokio::fs::remove_file(&legacy).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).at(&legacy),
        }
    }

    pub async fn read_lock(&self) -> Result<Option<ModLock>> {
        Ok(self.read_file().await?.lock())
    }

    pub async fn set_mod_enabled(&self, name: &str, enabled: bool) -> Result<&'static str> {
        if self.loader != Loader::Fabric {
            return Err(Error::Mods(format!(
                "{} doesn't use the launcher's performance mods.",
                self.label()
            )));
        }
        let Some(known) = default_mod(name) else {
            return Err(Error::Mods(format!(
                "{name} is not one of the launcher's mods. Pick one of: {}",
                crate::modrinth::DEFAULT_MODS
                    .iter()
                    .map(|m| m.slug)
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        };
        if known.required && !enabled {
            return Err(Error::Mods(format!(
                "{} is needed by the other mods and can't be turned off.",
                known.title
            )));
        }
        let mut file = self.read_file().await?;
        if enabled {
            file.disabled.remove(known.slug);
        } else {
            file.disabled.insert(known.slug.to_string());
        }
        self.write_file(&file).await?;
        Ok(known.title)
    }

    pub async fn set_skin(&self, skin: Option<String>) -> Result<InstanceFile> {
        let mut file = self.read_file().await?;
        file.skin = skin;
        self.write_file(&file).await?;
        Ok(file)
    }

    pub async fn set_loader_version(&self, version: Option<String>) -> Result<InstanceFile> {
        if self.loader == Loader::Vanilla && version.is_some() {
            return Err(Error::Unsupported(
                "Vanilla has no loader version to pick.".into(),
            ));
        }
        if let Some(v) = &version {
            token(v)?;
        }
        let mut file = self.read_file().await?;
        file.loader.pinned = version.is_some();
        file.loader.version = version;
        self.write_file(&file).await?;
        Ok(file)
    }

    pub async fn unmanaged_jars(&self, lock: Option<&ModLock>) -> Result<Vec<String>> {
        let mods = self.mods_dir();
        let mut entries = match tokio::fs::read_dir(&mods).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e).at(&mods),
        };
        let managed: BTreeSet<&str> = lock.map(|l| l.filenames().collect()).unwrap_or_default();
        let mut out = Vec::new();
        while let Some(entry) = entries.next_entry().await.at(&mods)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".jar") && !managed.contains(name.as_str()) {
                out.push(name);
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn extra_artifacts(&self) -> Vec<esteban::Artifact> {
        if self.loader != Loader::Fabric {
            return Vec::new();
        }
        let hud = esteban::hud_for(&self.game_version);
        let hacks = if self.hacked {
            esteban::hacks_for(&self.game_version)
        } else {
            None
        };
        hud.into_iter().chain(hacks).collect()
    }

    pub fn extra_downloads(&self) -> Vec<Download> {
        self.extra_artifacts()
            .into_iter()
            .map(|artifact| Download {
                url: artifact.url.to_string(),
                dest: self.mods_dir().join(artifact.filename),
                hash: Hash::sha256(artifact.sha256),
                size: Some(artifact.size),
                executable: false,
            })
            .collect()
    }

    pub async fn guard(&self) -> Result<()> {
        if self.hacked {
            return Ok(());
        }
        let mods = self.mods_dir();
        let mut entries = match tokio::fs::read_dir(&mods).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e).at(&mods),
        };
        while let Some(entry) = entries.next_entry().await.at(&mods)? {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "jar") {
                continue;
            }
            let bytes = tokio::fs::read(&path).await.at(&path)?;
            if esteban::is_known_hacks_jar(&sha256_hex(&bytes))
                || contains_hacks_mod(&bytes, &path, 0)?
            {
                return Err(Error::Guard(format!(
                    "{} contains the hacks mod, which must never be outside a Hacked instance. Remove it and launch again.",
                    file_name(&path)
                )));
            }
        }
        Ok(())
    }
}

pub async fn migrate_folders(paths: &Paths) -> Result<Vec<(String, String)>> {
    let root = paths.instances();
    let mut entries = match tokio::fs::read_dir(&root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).at(&root),
    };
    let mut moved = Vec::new();
    while let Some(entry) = entries.next_entry().await.at(&root)? {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(target) = old_folder_target(&name) else {
            continue;
        };
        if !entry.file_type().await.at(&entry.path())?.is_dir() {
            continue;
        }
        let to = root.join(&target);
        if tokio::fs::symlink_metadata(&to).await.is_ok() {
            tracing::warn!(from = %name, to = %target, "not renaming an old instance folder, the new name is taken");
            continue;
        }
        tokio::fs::rename(entry.path(), &to).await.at(&to)?;
        moved.push((name, target));
    }
    moved.sort();
    Ok(moved)
}

fn old_folder_target(name: &str) -> Option<String> {
    let (version, hacked) = match (name.strip_prefix("clean-"), name.strip_prefix("hacks-")) {
        (Some(version), _) => (version, false),
        (None, Some(version)) => (version, true),
        (None, None) => return None,
    };
    token(version).ok()?;
    Some(folder_name(version, Loader::Fabric, hacked))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoaderChoice {
    pub kind: Loader,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JarSource {
    Modrinth,
    Esteban,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Jar {
    pub file: String,
    pub source: JarSource,
    pub id: String,
    pub title: String,
    pub version: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

impl Jar {
    pub fn from_mod(m: &ResolvedMod) -> Self {
        Self {
            file: m.filename.clone(),
            source: JarSource::Modrinth,
            id: m.slug.clone(),
            title: m.title.clone(),
            version: m.version_number.clone(),
            url: m.url.clone(),
            sha512: Some(m.sha512.clone()),
            sha256: None,
            size: m.size,
            project_id: Some(m.project_id.clone()),
            version_id: Some(m.version_id.clone()),
            requires: m.requires.clone(),
        }
    }

    pub fn to_mod(&self) -> Option<ResolvedMod> {
        if self.source != JarSource::Modrinth {
            return None;
        }
        Some(ResolvedMod {
            slug: self.id.clone(),
            project_id: self.project_id.clone()?,
            title: self.title.clone(),
            version_id: self.version_id.clone()?,
            version_number: self.version.clone(),
            filename: self.file.clone(),
            url: self.url.clone(),
            sha512: self.sha512.clone()?,
            size: self.size,
            requires: self.requires.clone(),
        })
    }

    pub fn from_artifact(artifact: &esteban::Artifact) -> Self {
        Self {
            file: artifact.filename.to_string(),
            source: JarSource::Esteban,
            id: artifact.mod_id().to_string(),
            title: artifact.title().to_string(),
            version: artifact.version().to_string(),
            url: artifact.url.to_string(),
            sha512: None,
            sha256: Some(artifact.sha256.to_string()),
            size: artifact.size,
            project_id: None,
            version_id: None,
            requires: Vec::new(),
        }
    }

    fn from_old_extra(file: &str) -> Self {
        if let Some(artifact) = esteban::by_filename(file) {
            return Self::from_artifact(&artifact);
        }
        let hud = file.starts_with("esteban-hud-");
        Self {
            file: file.to_string(),
            source: JarSource::Esteban,
            id: if hud {
                esteban::HUD_MOD_ID
            } else {
                HACKS_MOD_ID
            }
            .to_string(),
            title: if hud { "Esteban HUD" } else { "Esteban" }.to_string(),
            version: String::new(),
            url: String::new(),
            sha512: None,
            sha256: None,
            size: 0,
            project_id: None,
            version_id: None,
            requires: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceFile {
    pub schema: u32,
    pub mc: String,
    pub loader: LoaderChoice,
    pub hacked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    #[serde(default)]
    pub disabled: BTreeSet<String>,
    #[serde(default)]
    pub installed: bool,
    #[serde(default)]
    pub jars: Vec<Jar>,
    #[serde(default)]
    pub skipped: Vec<Skipped>,
    #[serde(default)]
    pub resolved_without: Vec<String>,
}

#[derive(Default, Deserialize)]
struct InstanceFileV1 {
    #[serde(default)]
    loader_version: Option<String>,
    #[serde(default)]
    disabled: BTreeSet<String>,
}

impl InstanceFile {
    pub fn new(instance: &Instance) -> Self {
        Self {
            schema: INSTANCE_SCHEMA,
            mc: instance.game_version.clone(),
            loader: LoaderChoice {
                kind: instance.loader,
                version: None,
                pinned: false,
            },
            hacked: instance.hacked,
            skin: None,
            disabled: BTreeSet::new(),
            installed: false,
            jars: Vec::new(),
            skipped: Vec::new(),
            resolved_without: Vec::new(),
        }
    }

    fn from_v1(instance: &Instance, v1: InstanceFileV1, lock: Option<ModLock>) -> Self {
        let mut file = Self::new(instance);
        file.loader.version = v1.loader_version;
        file.disabled = v1.disabled;
        if let Some(lock) = lock {
            file.installed = true;
            file.jars = lock
                .mods
                .iter()
                .map(Jar::from_mod)
                .chain(lock.extras.iter().map(|f| Jar::from_old_extra(f)))
                .collect();
            file.skipped = lock.skipped;
            file.resolved_without = lock.disabled;
        }
        file
    }

    pub fn lock(&self) -> Option<ModLock> {
        self.installed.then(|| ModLock {
            game_version: self.mc.clone(),
            mods: self.jars.iter().filter_map(Jar::to_mod).collect(),
            extras: self
                .jars
                .iter()
                .filter(|j| j.source == JarSource::Esteban)
                .map(|j| j.file.clone())
                .collect(),
            skipped: self.skipped.clone(),
            disabled: self.resolved_without.clone(),
        })
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct FabricModJson {
    id: String,
}

fn contains_hacks_mod(bytes: &[u8], path: &Path, depth: u8) -> Result<bool> {
    let zip_error = |source| Error::Zip {
        path: path.to_path_buf(),
        source,
    };
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(zip_error)?;
    let mut nested = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(zip_error)?;
        let name = file.name().to_string();
        if name == "fabric.mod.json" {
            let mut text = Vec::new();
            file.read_to_end(&mut text).at(path)?;
            if let Ok(meta) = serde_json::from_slice::<FabricModJson>(&text)
                && meta.id == HACKS_MOD_ID
            {
                return Ok(true);
            }
        } else if depth < 2 && name.ends_with(".jar") {
            let mut inner = Vec::new();
            file.read_to_end(&mut inner).at(path)?;
            nested.push(inner);
        }
    }
    for inner in nested {
        if contains_hacks_mod(&inner, path, depth + 1)? {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;
    use crate::profile::NEW_INSTANCE_OPTIONS;

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

    fn resolved(slug: &str) -> ResolvedMod {
        ResolvedMod {
            slug: slug.into(),
            project_id: format!("{slug}-id"),
            title: slug.to_uppercase(),
            version_id: "v1".into(),
            version_number: "1.0".into(),
            filename: format!("{slug}.jar"),
            url: format!("https://cdn.modrinth.com/{slug}.jar"),
            sha512: "a".repeat(128),
            size: 7,
            requires: vec!["fabric-api".into()],
        }
    }

    async fn instance_at(
        dir: &tempfile::TempDir,
        version: &str,
        loader: Loader,
        hacked: bool,
    ) -> Instance {
        let instance = Instance::new(&Paths::new(dir.path()), version, loader, hacked).unwrap();
        tokio::fs::create_dir_all(instance.mods_dir())
            .await
            .unwrap();
        instance
    }

    #[test]
    fn folders_are_named_by_loader_version_and_hacked() {
        let paths = Paths::new("/base");
        let fabric = Instance::new(&paths, "1.21.4", Loader::Fabric, true).unwrap();
        assert!(fabric.dir.ends_with("instances/fabric-1.21.4-hacked"));
        assert_eq!(fabric.label(), "Fabric 1.21.4 Hacked");
        let forge = Instance::new(&paths, "1.20.1", Loader::Forge, false).unwrap();
        assert!(forge.dir.ends_with("instances/forge-1.20.1"));
        let vanilla = Instance::new(&paths, "1.8.9", Loader::Vanilla, false).unwrap();
        assert!(vanilla.dir.ends_with("instances/vanilla-1.8.9"));
    }

    #[test]
    fn hacked_needs_a_pinned_version_on_fabric() {
        let paths = Paths::new("/base");
        assert!(Instance::new(&paths, "26.3", Loader::Fabric, true).is_ok());
        for (version, loader) in [
            ("1.20.1", Loader::Fabric),
            ("1.21.4", Loader::Forge),
            ("1.21.4", Loader::Vanilla),
        ] {
            assert!(matches!(
                Instance::new(&paths, version, loader, true),
                Err(Error::Guard(_))
            ));
        }
        assert!(!hacked_allowed("1.20.1", Loader::Fabric));
        assert!(hacked_allowed("1.21.4", Loader::Fabric));
    }

    #[test]
    fn instance_names_cannot_escape_the_base_dir() {
        let paths = Paths::new("/base");
        assert!(Instance::new(&paths, "../x", Loader::Vanilla, false).is_err());
    }

    #[test]
    fn only_fabric_on_a_pinned_version_gets_esteban_jars() {
        let paths = Paths::new("/base");
        for version in PINNED_VERSIONS {
            let clean = Instance::new(&paths, version.id, Loader::Fabric, false).unwrap();
            let kinds: Vec<_> = clean.extra_artifacts().iter().map(|a| a.kind).collect();
            assert_eq!(kinds, vec![esteban::ArtifactKind::Hud]);
            assert!(
                clean
                    .extra_artifacts()
                    .iter()
                    .all(|a| !esteban::is_known_hacks_jar(a.sha256))
            );
            let hacked = Instance::new(&paths, version.id, Loader::Fabric, true).unwrap();
            let kinds: Vec<_> = hacked.extra_artifacts().iter().map(|a| a.kind).collect();
            assert_eq!(
                kinds,
                vec![esteban::ArtifactKind::Hud, esteban::ArtifactKind::Hacks]
            );
            assert!(
                hacked
                    .extra_downloads()
                    .iter()
                    .all(|d| d.dest.starts_with(hacked.mods_dir()))
            );
            for loader in [Loader::Vanilla, Loader::Forge] {
                let other = Instance::new(&paths, version.id, loader, false).unwrap();
                assert!(other.extra_artifacts().is_empty());
            }
        }
        let unpinned = Instance::new(&paths, "1.20.1", Loader::Fabric, false).unwrap();
        assert!(unpinned.extra_artifacts().is_empty());
    }

    #[tokio::test]
    async fn custom_backgrounds_folder_is_created_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "26.3", Loader::Fabric, false).await;
        instance.ensure_custom_backgrounds().await.unwrap();
        let folder = instance.dir.join("custom-backgrounds");
        assert!(folder.is_dir());
        std::fs::write(folder.join("mine.png"), b"png").unwrap();
        instance.ensure_custom_backgrounds().await.unwrap();
        assert_eq!(std::fs::read(folder.join("mine.png")).unwrap(), b"png");
    }

    #[tokio::test]
    async fn guard_allows_normal_mods() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        let sodium = jar(&[("fabric.mod.json", br#"{"id":"sodium"}"#)]);
        std::fs::write(instance.mods_dir().join("sodium.jar"), sodium).unwrap();
        assert!(instance.guard().await.is_ok());
    }

    #[tokio::test]
    async fn guard_refuses_the_hacks_mod_in_every_loader_that_is_not_hacked() {
        let dir = tempfile::tempdir().unwrap();
        for (version, loader) in [
            ("1.21.4", Loader::Fabric),
            ("1.20.1", Loader::Forge),
            ("1.8.9", Loader::Vanilla),
        ] {
            let instance = instance_at(&dir, version, loader, false).await;
            let renamed = jar(&[("fabric.mod.json", br#"{"id":"esteban","version":"9"}"#)]);
            std::fs::write(instance.mods_dir().join("innocent-name.jar"), renamed).unwrap();
            assert!(matches!(instance.guard().await, Err(Error::Guard(_))));
        }
    }

    #[tokio::test]
    async fn guard_finds_the_hacks_mod_nested_in_another_jar() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        let inner = jar(&[("fabric.mod.json", br#"{"id":"esteban"}"#)]);
        let outer = jar(&[
            ("fabric.mod.json", br#"{"id":"bundle"}"#),
            ("META-INF/jars/inner.jar", &inner),
        ]);
        std::fs::write(instance.mods_dir().join("bundle.jar"), outer).unwrap();
        assert!(matches!(instance.guard().await, Err(Error::Guard(_))));
    }

    #[tokio::test]
    async fn hacked_instances_are_not_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, true).await;
        let hacks = jar(&[("fabric.mod.json", br#"{"id":"esteban"}"#)]);
        std::fs::write(instance.mods_dir().join("esteban.jar"), hacks).unwrap();
        assert!(instance.guard().await.is_ok());
    }

    #[tokio::test]
    async fn toggles_persist_and_fabric_api_stays_on() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        assert_eq!(
            instance.set_mod_enabled("Iris", false).await.unwrap(),
            "Iris Shaders"
        );
        let file = instance.read_file().await.unwrap();
        assert!(file.disabled.contains("iris"));
        assert_eq!(file.schema, INSTANCE_SCHEMA);
        instance.set_mod_enabled("iris", true).await.unwrap();
        assert!(instance.read_file().await.unwrap().disabled.is_empty());
        assert!(matches!(
            instance.set_mod_enabled("fabric-api", false).await,
            Err(Error::Mods(_))
        ));
        assert!(matches!(
            instance.set_mod_enabled("optifine", false).await,
            Err(Error::Mods(_))
        ));
        let forge = instance_at(&dir, "1.20.1", Loader::Forge, false).await;
        assert!(matches!(
            forge.set_mod_enabled("iris", false).await,
            Err(Error::Mods(_))
        ));
    }

    #[tokio::test]
    async fn a_picked_loader_version_is_pinned_until_reset() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.20.1", Loader::Forge, false).await;
        let file = instance
            .set_loader_version(Some("47.4.10".into()))
            .await
            .unwrap();
        assert!(file.loader.pinned);
        assert_eq!(
            instance
                .read_file()
                .await
                .unwrap()
                .loader
                .version
                .as_deref(),
            Some("47.4.10")
        );
        let file = instance.set_loader_version(None).await.unwrap();
        assert!(!file.loader.pinned && file.loader.version.is_none());
        assert!(
            instance
                .set_loader_version(Some("../x".into()))
                .await
                .is_err()
        );
        let vanilla = instance_at(&dir, "1.8.9", Loader::Vanilla, false).await;
        assert!(vanilla.set_loader_version(Some("1".into())).await.is_err());
    }

    #[tokio::test]
    async fn jars_the_launcher_did_not_put_there_are_listed() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        std::fs::write(instance.mods_dir().join("sodium.jar"), b"x").unwrap();
        std::fs::write(instance.mods_dir().join("mine.jar"), b"x").unwrap();
        std::fs::write(instance.mods_dir().join("notes.txt"), b"x").unwrap();
        let lock = ModLock {
            game_version: "1.21.4".into(),
            mods: Vec::new(),
            extras: vec!["sodium.jar".into()],
            skipped: Vec::new(),
            disabled: Vec::new(),
        };
        assert_eq!(
            instance.unmanaged_jars(Some(&lock)).await.unwrap(),
            vec!["mine.jar".to_string()]
        );
    }

    #[tokio::test]
    async fn a_damaged_old_lock_file_is_looked_up_again() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        std::fs::write(instance.legacy_lock_path(), b"{broken").unwrap();
        assert!(instance.read_lock().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn old_instance_files_are_read_as_schema_two_and_the_old_lock_goes_on_write() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, true).await;
        std::fs::write(
            instance.meta_path(),
            br#"{"loader_version":"0.19.5","disabled":["iris"]}"#,
        )
        .unwrap();
        let hud = esteban::hud_for("1.21.4").unwrap();
        let old = ModLock {
            game_version: "1.21.4".into(),
            mods: vec![resolved("sodium")],
            extras: vec![hud.filename.into(), "esteban-1.3.0+1.21.4.jar".into()],
            skipped: vec![Skipped {
                title: "Iris Shaders".into(),
                message: "off".into(),
            }],
            disabled: vec!["iris".into()],
        };
        old.write(&instance.legacy_lock_path()).await.unwrap();

        let file = instance.read_file().await.unwrap();
        assert_eq!(file.schema, 2);
        assert_eq!(file.mc, "1.21.4");
        assert_eq!(file.loader.kind, Loader::Fabric);
        assert_eq!(file.loader.version.as_deref(), Some("0.19.5"));
        assert!(file.hacked && file.installed);
        assert_eq!(file.jars.len(), 3);
        assert_eq!(file.jars[1].sha256.as_deref(), Some(hud.sha256));
        assert!(file.jars[2].sha256.is_none());
        let lock = file.lock().unwrap();
        assert_eq!(lock.mods, vec![resolved("sodium")]);
        assert_eq!(lock.extras, old.extras);
        assert_eq!(lock.disabled, vec!["iris".to_string()]);

        instance.write_file(&file).await.unwrap();
        assert!(!instance.legacy_lock_path().exists());
        assert_eq!(instance.read_file().await.unwrap(), file);
        let text = std::fs::read_to_string(instance.meta_path()).unwrap();
        assert!(text.contains("\"schema\": 2") && text.contains("\"resolvedWithout\""));
    }

    #[tokio::test]
    async fn a_file_from_a_newer_launcher_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.8.9", Loader::Vanilla, false).await;
        std::fs::write(
            instance.meta_path(),
            br#"{"schema":3,"mc":"1.8.9","loader":{"kind":"vanilla"},"hacked":false}"#,
        )
        .unwrap();
        assert!(matches!(
            instance.read_file().await,
            Err(Error::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn old_folders_are_renamed_once_and_never_over_a_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let root = paths.instances();
        for name in [
            "clean-1.21.4",
            "hacks-1.21.4",
            "hacks-26.3",
            "fabric-26.3-hacked",
        ] {
            std::fs::create_dir_all(root.join(name)).unwrap();
        }
        std::fs::create_dir_all(root.join("hacks-1.21.4/saves/New World")).unwrap();
        std::fs::write(root.join("clean-notes"), b"not a folder").unwrap();
        let moved = migrate_folders(&paths).await.unwrap();
        assert_eq!(
            moved,
            vec![
                ("clean-1.21.4".to_string(), "fabric-1.21.4".to_string()),
                (
                    "hacks-1.21.4".to_string(),
                    "fabric-1.21.4-hacked".to_string()
                ),
            ]
        );
        assert!(root.join("fabric-1.21.4-hacked/saves/New World").is_dir());
        assert!(root.join("hacks-26.3").is_dir());
        assert!(root.join("clean-notes").is_file());
        assert!(migrate_folders(&paths).await.unwrap().is_empty());
        assert!(
            migrate_folders(&Paths::new(dir.path().join("nothing")))
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn a_new_instance_gets_the_data_version_and_gui_scale() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        let extra = [("onboardAccessibility", "false")];
        let entries: Vec<(&str, &str)> =
            NEW_INSTANCE_OPTIONS.iter().copied().chain(extra).collect();
        assert!(instance.seed_options(4189, &entries).await.unwrap());
        assert_eq!(
            std::fs::read_to_string(instance.options_path()).unwrap(),
            "version:4189\nguiScale:3\nonboardAccessibility:false\n"
        );
    }

    #[tokio::test]
    async fn existing_options_are_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let instance = instance_at(&dir, "1.21.4", Loader::Fabric, false).await;
        let mine = "version:4189\nguiScale:4\nfov:0.5\n";
        std::fs::write(instance.options_path(), mine).unwrap();
        assert!(
            !instance
                .seed_options(4189, NEW_INSTANCE_OPTIONS)
                .await
                .unwrap()
        );
        assert_eq!(
            std::fs::read_to_string(instance.options_path()).unwrap(),
            mine
        );
    }
}
