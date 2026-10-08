use std::collections::BTreeSet;
use std::fmt;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::download::Download;
use crate::error::IoContext;
use crate::esteban::{self, HACKS_MOD_ID};
use crate::fabric::token;
use crate::hash::{Hash, sha256_hex};
use crate::modrinth::{ModLock, default_mod};
use crate::paths::Paths;
use crate::{Error, Result, fsx};

pub const HACKS_WARNING: &str = "Most servers ban this. You are responsible for where you use it.";

pub const NEW_INSTANCE_OPTIONS: &[(&str, &str)] = &[("guiScale", "3")];

pub const CUSTOM_BACKGROUNDS: &str = "custom-backgrounds";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProfileKind {
    Clean,
    Hacks,
}

impl ProfileKind {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Hacks => "hacks",
        }
    }
}

impl fmt::Display for ProfileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Clean => "Esteban",
            Self::Hacks => "Esteban + Hacks",
        })
    }
}

impl FromStr for ProfileKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "clean" => Ok(Self::Clean),
            "hacks" => Ok(Self::Hacks),
            other => Err(Error::Unsupported(format!(
                "unknown profile {other}, use clean or hacks"
            ))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Instance {
    pub kind: ProfileKind,
    pub game_version: String,
    pub dir: PathBuf,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InstanceMeta {
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub disabled: BTreeSet<String>,
}

impl Instance {
    pub fn new(paths: &Paths, kind: ProfileKind, game_version: &str) -> Result<Self> {
        let game_version = token(game_version)?.to_string();
        let dir = paths
            .instances()
            .join(format!("{}-{game_version}", kind.slug()));
        Ok(Self {
            kind,
            game_version,
            dir,
        })
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.dir.join("mods")
    }

    pub fn natives_dir(&self) -> PathBuf {
        self.dir.join("natives")
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

    pub fn lock_path(&self) -> PathBuf {
        self.dir.join("mods.lock.json")
    }

    pub fn meta_path(&self) -> PathBuf {
        self.dir.join("instance.json")
    }

    pub async fn read_meta(&self) -> Result<InstanceMeta> {
        let path = self.meta_path();
        match tokio::fs::read(&path).await {
            Ok(bytes) => crate::error::json(&bytes, "the instance settings"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(InstanceMeta::default()),
            Err(e) => Err(e).at(&path),
        }
    }

    pub async fn write_meta(&self, meta: &InstanceMeta) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(meta).map_err(|source| Error::Json {
            what: "the instance settings".into(),
            source,
        })?;
        fsx::write_atomic(&self.meta_path(), &bytes).await
    }

    pub async fn set_mod_enabled(&self, name: &str, enabled: bool) -> Result<&'static str> {
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
        let mut meta = self.read_meta().await?;
        if enabled {
            meta.disabled.remove(known.slug);
        } else {
            meta.disabled.insert(known.slug.to_string());
        }
        self.write_meta(&meta).await?;
        Ok(known.title)
    }

    pub async fn read_lock(&self) -> Result<Option<ModLock>> {
        match ModLock::read(&self.lock_path()).await {
            Err(Error::Json { .. }) => {
                tracing::warn!(path = %self.lock_path().display(), "the mod lock file is damaged, looking the mods up again");
                Ok(None)
            }
            other => other,
        }
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

    pub fn extra_downloads(&self) -> Vec<Download> {
        match self.kind {
            ProfileKind::Clean => Vec::new(),
            ProfileKind::Hacks => esteban::hacks_for(&self.game_version)
                .map(|artifact| Download {
                    url: artifact.url.to_string(),
                    dest: self.mods_dir().join(artifact.filename),
                    hash: Hash::sha256(artifact.sha256),
                    size: Some(artifact.size),
                    executable: false,
                })
                .into_iter()
                .collect(),
        }
    }

    pub async fn guard(&self) -> Result<()> {
        if self.kind != ProfileKind::Clean {
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
                    "{} contains the hacks mod, which must never be in the clean profile. Remove it and launch again.",
                    file_name(&path)
                )));
            }
        }
        Ok(())
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub hacks_warning_accepted: bool,
}

impl Settings {
    pub async fn load(paths: &Paths) -> Result<Self> {
        let path = paths.settings_file();
        match tokio::fs::read(&path).await {
            Ok(bytes) => crate::error::json(&bytes, "the launcher settings"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).at(&path),
        }
    }

    pub async fn save(&self, paths: &Paths) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|source| Error::Json {
            what: "the launcher settings".into(),
            source,
        })?;
        fsx::write_atomic(&paths.settings_file(), &bytes).await
    }
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

    #[tokio::test]
    async fn custom_backgrounds_folder_is_created_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let instance = Instance::new(&Paths::new(dir.path()), ProfileKind::Clean, "26.3").unwrap();
        instance.ensure_custom_backgrounds().await.unwrap();
        let folder = instance.dir.join("custom-backgrounds");
        assert!(folder.is_dir());
        std::fs::write(folder.join("mine.png"), b"png").unwrap();
        instance.ensure_custom_backgrounds().await.unwrap();
        assert_eq!(std::fs::read(folder.join("mine.png")).unwrap(), b"png");
    }

    async fn clean_instance() -> (tempfile::TempDir, Instance) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let instance = Instance::new(&paths, ProfileKind::Clean, "1.21.4").unwrap();
        tokio::fs::create_dir_all(instance.mods_dir())
            .await
            .unwrap();
        (dir, instance)
    }

    #[test]
    fn clean_profile_never_gets_the_hacks_jar() {
        let paths = Paths::new("/base");
        let clean = Instance::new(&paths, ProfileKind::Clean, "1.21.4").unwrap();
        assert!(clean.extra_downloads().is_empty());
        let hacks = Instance::new(&paths, ProfileKind::Hacks, "1.21.4").unwrap();
        let extras = hacks.extra_downloads();
        assert_eq!(extras.len(), 1);
        assert!(extras[0].dest.starts_with(hacks.mods_dir()));
    }

    #[test]
    fn instance_names_cannot_escape_the_base_dir() {
        let paths = Paths::new("/base");
        assert!(Instance::new(&paths, ProfileKind::Clean, "../x").is_err());
    }

    #[tokio::test]
    async fn guard_allows_normal_mods() {
        let (_dir, instance) = clean_instance().await;
        let sodium = jar(&[("fabric.mod.json", br#"{"id":"sodium"}"#)]);
        std::fs::write(instance.mods_dir().join("sodium.jar"), sodium).unwrap();
        assert!(instance.guard().await.is_ok());
    }

    #[tokio::test]
    async fn guard_refuses_the_hacks_mod_by_id() {
        let (_dir, instance) = clean_instance().await;
        let renamed = jar(&[("fabric.mod.json", br#"{"id":"esteban","version":"9"}"#)]);
        std::fs::write(instance.mods_dir().join("innocent-name.jar"), renamed).unwrap();
        assert!(matches!(instance.guard().await, Err(Error::Guard(_))));
    }

    #[tokio::test]
    async fn guard_finds_the_hacks_mod_nested_in_another_jar() {
        let (_dir, instance) = clean_instance().await;
        let inner = jar(&[("fabric.mod.json", br#"{"id":"esteban"}"#)]);
        let outer = jar(&[
            ("fabric.mod.json", br#"{"id":"bundle"}"#),
            ("META-INF/jars/inner.jar", &inner),
        ]);
        std::fs::write(instance.mods_dir().join("bundle.jar"), outer).unwrap();
        assert!(matches!(instance.guard().await, Err(Error::Guard(_))));
    }

    #[tokio::test]
    async fn hacks_profile_is_not_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let instance = Instance::new(&paths, ProfileKind::Hacks, "1.21.4").unwrap();
        tokio::fs::create_dir_all(instance.mods_dir())
            .await
            .unwrap();
        let hacks = jar(&[("fabric.mod.json", br#"{"id":"esteban"}"#)]);
        std::fs::write(instance.mods_dir().join("esteban.jar"), hacks).unwrap();
        assert!(instance.guard().await.is_ok());
    }

    #[tokio::test]
    async fn toggles_persist_and_fabric_api_stays_on() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let instance = Instance::new(&paths, ProfileKind::Clean, "1.21.4").unwrap();
        assert_eq!(
            instance.set_mod_enabled("Iris", false).await.unwrap(),
            "Iris Shaders"
        );
        assert!(
            instance
                .read_meta()
                .await
                .unwrap()
                .disabled
                .contains("iris")
        );
        instance.set_mod_enabled("iris", true).await.unwrap();
        assert!(instance.read_meta().await.unwrap().disabled.is_empty());
        assert!(matches!(
            instance.set_mod_enabled("fabric-api", false).await,
            Err(Error::Mods(_))
        ));
        assert!(matches!(
            instance.set_mod_enabled("optifine", false).await,
            Err(Error::Mods(_))
        ));
    }

    #[tokio::test]
    async fn jars_the_launcher_did_not_put_there_are_listed() {
        let (_dir, instance) = clean_instance().await;
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
    async fn a_damaged_lock_file_is_looked_up_again() {
        let (_dir, instance) = clean_instance().await;
        std::fs::write(instance.lock_path(), b"{broken").unwrap();
        assert!(instance.read_lock().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_new_instance_gets_the_data_version_and_gui_scale() {
        let (_dir, instance) = clean_instance().await;
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
        let (_dir, instance) = clean_instance().await;
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
