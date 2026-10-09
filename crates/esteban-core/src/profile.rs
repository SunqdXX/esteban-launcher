use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::IoContext;
use crate::loader::Loader;
use crate::paths::Paths;
use crate::{Error, Result, fsx};

pub const HACKS_WARNING: &str = "Most servers ban this. You are responsible for where you use it.";

pub const NEW_INSTANCE_OPTIONS: &[(&str, &str)] = &[("guiScale", "3")];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub hacks_warning_accepted: bool,
    #[serde(default)]
    pub game_version: Option<String>,
    #[serde(default)]
    pub loader: Option<Loader>,
    #[serde(default)]
    pub hacked: bool,
    #[serde(default, skip_serializing)]
    profile: Option<String>,
    #[serde(default)]
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub java_path: Option<PathBuf>,
}

impl Settings {
    pub async fn paths(home: PathBuf) -> Result<Paths> {
        let probe = Paths::new(home.clone());
        let settings = Self::load(&probe).await?;
        let paths = match settings.data_dir {
            Some(data) => Paths::with_data(home, data),
            None => probe,
        };
        for (from, to) in crate::instance::migrate_folders(&paths).await? {
            tracing::info!(%from, %to, "renamed an instance folder to the new layout");
        }
        Ok(paths)
    }

    pub fn launch_options(
        &self,
        quick_play: Option<crate::launch::QuickPlay>,
        memory_override: Option<u64>,
    ) -> crate::launch::LaunchOptions {
        crate::launch::LaunchOptions {
            quick_play,
            max_heap_mb: memory_override.or(self.memory_mb),
            extra_jvm_args: self.jvm_args.clone(),
            java: self.java_path.clone(),
        }
    }

    pub async fn load(paths: &Paths) -> Result<Self> {
        let path = paths.settings_file();
        let mut settings: Self = match tokio::fs::read(&path).await {
            Ok(bytes) => crate::error::json(&bytes, "the launcher settings")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e).at(&path),
        };
        if settings.profile.take().as_deref() == Some("hacks") {
            settings.hacked = true;
            settings.loader.get_or_insert(Loader::Fabric);
        }
        Ok(settings)
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
    use super::*;

    #[tokio::test]
    async fn the_data_folder_moves_but_settings_stay_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let data = dir.path().join("data");
        let settings = Settings {
            data_dir: Some(data.clone()),
            memory_mb: Some(6144),
            jvm_args: vec!["-XX:+UseZGC".into()],
            ..Settings::default()
        };
        settings.save(&Paths::new(&home)).await.unwrap();
        let resolved = Settings::paths(home.clone()).await.unwrap();
        assert_eq!(resolved.home(), home);
        assert_eq!(resolved.settings_file(), home.join("settings.json"));
        assert_eq!(resolved.instances(), data.join("instances"));
        let options = settings.launch_options(None, None);
        assert_eq!(options.max_heap_mb, Some(6144));
        assert_eq!(options.extra_jvm_args, vec!["-XX:+UseZGC".to_string()]);
        assert_eq!(
            settings.launch_options(None, Some(2048)).max_heap_mb,
            Some(2048)
        );
    }

    #[tokio::test]
    async fn old_settings_keep_hacked_and_old_folders_get_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        std::fs::write(
            home.join("settings.json"),
            br#"{"hacks_warning_accepted":true,"game_version":"26.3","profile":"hacks"}"#,
        )
        .unwrap();
        std::fs::create_dir_all(home.join("instances/hacks-26.3/saves/w")).unwrap();
        let paths = Settings::paths(home.clone()).await.unwrap();
        assert!(
            paths
                .instances()
                .join("fabric-26.3-hacked/saves/w")
                .is_dir()
        );
        let settings = Settings::load(&paths).await.unwrap();
        assert!(settings.hacked && settings.hacks_warning_accepted);
        assert_eq!(settings.loader, Some(Loader::Fabric));
        settings.save(&paths).await.unwrap();
        let text = std::fs::read_to_string(home.join("settings.json")).unwrap();
        assert!(!text.contains("profile") && text.contains("\"hacked\": true"));
        let clean: Settings = serde_json::from_str(r#"{"profile":"clean"}"#).unwrap();
        assert!(!clean.hacked);
    }
}
