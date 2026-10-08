use esteban_core::install::{InstallOptions, install as install_instance};
use esteban_core::net::Net;
use esteban_core::paths::Paths;
use esteban_core::profile::{HACKS_WARNING, Instance, ProfileKind, Settings};
use esteban_core::status::{InstanceStatus, status};
use esteban_core::versions::{DEFAULT_GAME_VERSION, GAME_VERSIONS, GameVersion, is_supported};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;

use crate::progress::UiProgress;

pub struct AppState {
    pub paths: Paths,
    pub net: Net,
    pub busy: Mutex<()>,
}

type Reply<T> = Result<T, String>;

fn human(error: esteban_core::Error) -> String {
    error.to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    versions: &'static [GameVersion],
    game_version: String,
    profile: ProfileKind,
    hacks_warning_accepted: bool,
    hacks_warning: &'static str,
    disclaimer: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSummary {
    loader_version: String,
    java_version: String,
    mods: usize,
    fetched_files: usize,
    fetched_bytes: u64,
    skipped: Vec<String>,
}

fn instance(state: &AppState, game_version: &str, profile: ProfileKind) -> Reply<Instance> {
    if !is_supported(game_version) {
        return Err(format!(
            "{game_version} isn't one of the versions this launcher supports."
        ));
    }
    Instance::new(&state.paths, profile, game_version).map_err(human)
}

#[tauri::command]
pub async fn overview(state: State<'_, AppState>) -> Reply<Overview> {
    let settings = Settings::load(&state.paths).await.map_err(human)?;
    let game_version = settings
        .game_version
        .filter(|v| is_supported(v))
        .unwrap_or_else(|| DEFAULT_GAME_VERSION.to_string());
    let profile = match settings.profile {
        Some(ProfileKind::Hacks) if settings.hacks_warning_accepted => ProfileKind::Hacks,
        _ => ProfileKind::Clean,
    };
    Ok(Overview {
        versions: GAME_VERSIONS,
        game_version,
        profile,
        hacks_warning_accepted: settings.hacks_warning_accepted,
        hacks_warning: HACKS_WARNING,
        disclaimer: esteban_core::DISCLAIMER,
    })
}

#[tauri::command]
pub async fn select(
    state: State<'_, AppState>,
    game_version: String,
    profile: ProfileKind,
) -> Reply<()> {
    instance(&state, &game_version, profile)?;
    let mut settings = Settings::load(&state.paths).await.map_err(human)?;
    settings.game_version = Some(game_version);
    settings.profile = Some(profile);
    settings.save(&state.paths).await.map_err(human)
}

#[tauri::command]
pub async fn accept_hacks_warning(state: State<'_, AppState>) -> Reply<()> {
    let mut settings = Settings::load(&state.paths).await.map_err(human)?;
    settings.hacks_warning_accepted = true;
    settings.save(&state.paths).await.map_err(human)
}

#[tauri::command]
pub async fn instance_status(
    state: State<'_, AppState>,
    game_version: String,
    profile: ProfileKind,
) -> Reply<InstanceStatus> {
    let instance = instance(&state, &game_version, profile)?;
    status(&instance).await.map_err(human)
}

#[tauri::command]
pub async fn set_mod(
    state: State<'_, AppState>,
    game_version: String,
    profile: ProfileKind,
    slug: String,
    enabled: bool,
) -> Reply<InstanceStatus> {
    let instance = instance(&state, &game_version, profile)?;
    instance
        .set_mod_enabled(&slug, enabled)
        .await
        .map_err(human)?;
    status(&instance).await.map_err(human)
}

#[tauri::command]
pub async fn install(
    app: AppHandle,
    state: State<'_, AppState>,
    game_version: String,
    profile: ProfileKind,
) -> Reply<InstallSummary> {
    instance(&state, &game_version, profile)?;
    if profile == ProfileKind::Hacks {
        let settings = Settings::load(&state.paths).await.map_err(human)?;
        if !settings.hacks_warning_accepted {
            return Err("Accept the Hacked warning first.".into());
        }
    }
    let Ok(_busy) = state.busy.try_lock() else {
        return Err("Already installing. Wait for it to finish.".into());
    };
    let net = state.net.clone();
    let paths = state.paths.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let progress = UiProgress::new(app);
        let result = tauri::async_runtime::block_on(install_instance(
            &net,
            &paths,
            profile,
            &game_version,
            InstallOptions {
                update: false,
                first_run_options: &[],
            },
            &progress,
        ));
        progress.finish();
        let installed = result.map_err(human)?;
        Ok(InstallSummary {
            loader_version: installed.loader_version,
            java_version: installed.java.version,
            mods: installed.mods.len() + installed.extras.len(),
            fetched_files: installed.stats.fetched_files,
            fetched_bytes: installed.stats.fetched_bytes,
            skipped: installed
                .unavailable
                .into_iter()
                .map(|u| u.message)
                .collect(),
        })
    })
    .await
    .map_err(|e| format!("The install stopped unexpectedly: {e}"))?
}

#[tauri::command]
pub async fn open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    game_version: String,
    profile: ProfileKind,
) -> Reply<()> {
    let instance = instance(&state, &game_version, profile)?;
    if !instance.dir.is_dir() {
        return Err("Not installed yet, so there is no folder to open.".into());
    }
    app.opener()
        .open_path(instance.dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Could not open {}: {e}", instance.dir.display()))
}
