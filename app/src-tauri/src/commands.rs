use std::path::PathBuf;
use std::sync::RwLock;

use esteban_core::catalog::{self, Catalog, LoaderVersions};
use esteban_core::install::{InstallOptions, install as install_instance};
use esteban_core::instance::{self, Instance};
use esteban_core::launch::jvm;
use esteban_core::loader::Loader;
use esteban_core::net::Net;
use esteban_core::packs::{self, Imported, Linked};
use esteban_core::paths::Paths;
use esteban_core::profile::{HACKS_WARNING, Settings};
use esteban_core::skins::{self, Model, Skin};
use esteban_core::status::{InstanceStatus, status};
use esteban_core::versions::{DEFAULT_GAME_VERSION, GameVersion, PINNED_VERSIONS};
use esteban_core::{java, system};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::Mutex;

use crate::progress::UiProgress;

pub struct AppState {
    pub home: PathBuf,
    pub paths: RwLock<Paths>,
    pub net: Net,
    pub busy: Mutex<()>,
    pub catalog: Mutex<Option<Catalog>>,
}

impl AppState {
    fn paths(&self) -> Paths {
        match self.paths.read() {
            Ok(paths) => paths.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    fn set_paths(&self, paths: Paths) {
        match self.paths.write() {
            Ok(mut current) => *current = paths,
            Err(poisoned) => *poisoned.into_inner() = paths,
        }
    }
}

type Reply<T> = Result<T, String>;

fn human(error: esteban_core::Error) -> String {
    error.to_string()
}

#[derive(Deserialize, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    game_version: String,
    loader: Loader,
    hacked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pinned: &'static [GameVersion],
    game_version: String,
    loader: Loader,
    hacked: bool,
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

fn instance(state: &AppState, selection: &Selection) -> Reply<Instance> {
    Instance::new(
        &state.paths(),
        &selection.game_version,
        selection.loader,
        selection.hacked,
    )
    .map_err(human)
}

async fn load_settings(state: &AppState) -> Reply<Settings> {
    Settings::load(&state.paths()).await.map_err(human)
}

async fn save_settings(state: &AppState, settings: &Settings) -> Reply<()> {
    settings.save(&state.paths()).await.map_err(human)
}

#[tauri::command]
pub async fn overview(state: State<'_, AppState>) -> Reply<Overview> {
    let settings = load_settings(&state).await?;
    let game_version = settings
        .game_version
        .unwrap_or_else(|| DEFAULT_GAME_VERSION.to_string());
    let loader = settings.loader.unwrap_or(Loader::Fabric);
    let hacked = settings.hacked
        && settings.hacks_warning_accepted
        && esteban_core::instance::hacked_allowed(&game_version, loader);
    Ok(Overview {
        pinned: PINNED_VERSIONS,
        game_version,
        loader,
        hacked,
        hacks_warning_accepted: settings.hacks_warning_accepted,
        hacks_warning: HACKS_WARNING,
        disclaimer: esteban_core::DISCLAIMER,
    })
}

#[tauri::command]
pub async fn select(state: State<'_, AppState>, selection: Selection) -> Reply<()> {
    instance(&state, &selection)?;
    let mut settings = load_settings(&state).await?;
    settings.game_version = Some(selection.game_version);
    settings.loader = Some(selection.loader);
    settings.hacked = selection.hacked;
    save_settings(&state, &settings).await
}

#[tauri::command]
pub async fn accept_hacks_warning(state: State<'_, AppState>) -> Reply<()> {
    let mut settings = load_settings(&state).await?;
    settings.hacks_warning_accepted = true;
    save_settings(&state, &settings).await
}

#[tauri::command]
pub async fn instance_status(
    state: State<'_, AppState>,
    selection: Selection,
) -> Reply<InstanceStatus> {
    let instance = instance(&state, &selection)?;
    status(&instance).await.map_err(human)
}

#[tauri::command]
pub async fn set_mod(
    state: State<'_, AppState>,
    selection: Selection,
    slug: String,
    enabled: bool,
) -> Reply<InstanceStatus> {
    let instance = instance(&state, &selection)?;
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
    selection: Selection,
) -> Reply<InstallSummary> {
    let target = instance(&state, &selection)?;
    if target.hacked {
        let settings = load_settings(&state).await?;
        if !settings.hacks_warning_accepted {
            return Err("Accept the Hacked warning first.".into());
        }
    }
    let Ok(_busy) = state.busy.try_lock() else {
        return Err("Already installing. Wait for it to finish.".into());
    };
    let net = state.net.clone();
    let paths = state.paths();
    tauri::async_runtime::spawn_blocking(move || {
        let progress = UiProgress::new(app);
        let result = tauri::async_runtime::block_on(install_instance(
            &net,
            &paths,
            &target,
            InstallOptions {
                update: false,
                first_run_options: &[],
            },
            &progress,
        ));
        progress.finish();
        let installed = result.map_err(human)?;
        Ok(InstallSummary {
            loader_version: installed.loader_label(),
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
    selection: Selection,
) -> Reply<()> {
    let instance = instance(&state, &selection)?;
    if !instance.dir.is_dir() {
        return Err("Not installed yet, so there is no folder to open.".into());
    }
    app.opener()
        .open_path(instance.dir.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Could not open {}: {e}", instance.dir.display()))
}

#[tauri::command]
pub async fn catalog(state: State<'_, AppState>, refresh: bool) -> Reply<Catalog> {
    let mut cached = state.catalog.lock().await;
    if let Some(known) = cached.as_ref().filter(|c| !refresh && !c.offline) {
        return Ok(known.clone());
    }
    let fresh = catalog::load(&state.net, &state.paths())
        .await
        .map_err(human)?;
    cached.replace(fresh.clone());
    Ok(fresh)
}

#[tauri::command]
pub async fn loader_versions(
    state: State<'_, AppState>,
    selection: Selection,
) -> Reply<LoaderVersions> {
    catalog::loader_versions(
        &state.net,
        &state.paths(),
        &selection.game_version,
        selection.loader,
    )
    .await
    .map_err(human)
}

#[tauri::command]
pub async fn set_loader_version(
    state: State<'_, AppState>,
    selection: Selection,
    version: Option<String>,
) -> Reply<InstanceStatus> {
    let instance = instance(&state, &selection)?;
    if let Some(v) = &version {
        let choices = catalog::loader_versions(
            &state.net,
            &state.paths(),
            &instance.game_version,
            instance.loader,
        )
        .await
        .map_err(human)?;
        if !choices.versions.iter().any(|c| &c.version == v) {
            return Err(format!(
                "{v} isn't a {} build for {}.",
                instance.loader.title(),
                instance.game_version
            ));
        }
    }
    instance.set_loader_version(version).await.map_err(human)?;
    status(&instance).await.map_err(human)
}

#[tauri::command]
pub async fn instances(state: State<'_, AppState>) -> Reply<Vec<InstanceStatus>> {
    let mut out = Vec::new();
    for found in instance::list(&state.paths()).await.map_err(human)? {
        out.push(status(&found).await.map_err(human)?);
    }
    Ok(out)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinCard {
    #[serde(flatten)]
    skin: Skin,
    preview: Vec<String>,
}

async fn card(paths: &Paths, skin: Skin) -> Reply<SkinCard> {
    let image = skins::image(paths, &skin.id).await.map_err(human)?;
    let preview = skins::preview(&image, skin.model);
    Ok(SkinCard { skin, preview })
}

#[tauri::command]
pub async fn skins(state: State<'_, AppState>) -> Reply<Vec<SkinCard>> {
    let paths = state.paths();
    let mut out = Vec::new();
    for skin in skins::list(&paths).await.map_err(human)? {
        out.push(card(&paths, skin).await?);
    }
    Ok(out)
}

#[tauri::command]
pub async fn import_skin(app: AppHandle, state: State<'_, AppState>) -> Reply<Option<SkinCard>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Pick a skin")
            .add_filter("Skin", &["png"])
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("The file picker stopped: {e}"))?;
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    let paths = state.paths();
    let skin = skins::import(&paths, &path).await.map_err(human)?;
    Ok(Some(card(&paths, skin).await?))
}

#[tauri::command]
pub async fn update_skin(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    model: Option<Model>,
) -> Reply<SkinCard> {
    let paths = state.paths();
    let skin = skins::update(&paths, &id, name.as_deref(), model)
        .await
        .map_err(human)?;
    card(&paths, skin).await
}

#[tauri::command]
pub async fn remove_skin(state: State<'_, AppState>, id: String) -> Reply<()> {
    skins::remove(&state.paths(), &id).await.map_err(human)
}

#[tauri::command]
pub async fn set_skin(
    state: State<'_, AppState>,
    selection: Selection,
    skin: Option<String>,
) -> Reply<InstanceStatus> {
    let paths = state.paths();
    let instance = instance(&state, &selection)?;
    if let Some(id) = &skin
        && skins::find(&paths, id).await.map_err(human)?.is_none()
    {
        return Err("That skin isn't in the list anymore.".into());
    }
    instance.set_skin(skin).await.map_err(human)?;
    status(&instance).await.map_err(human)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    memory_mb: Option<u64>,
    auto_memory_mb: u64,
    min_memory_mb: u64,
    max_memory_mb: u64,
    total_memory_mb: u64,
    jvm_args: String,
    java_path: Option<String>,
    data_dir: String,
    default_data_dir: String,
    pack_sources: Vec<String>,
}

fn join_args(args: &[String]) -> String {
    args.iter()
        .map(|a| {
            if a.contains(char::is_whitespace) {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[tauri::command]
pub async fn launcher_settings(state: State<'_, AppState>) -> Reply<LauncherSettings> {
    let settings = load_settings(&state).await?;
    let total = system::total_memory_bytes();
    let (_, auto) = jvm::heap_mb(total);
    Ok(LauncherSettings {
        memory_mb: settings.memory_mb,
        auto_memory_mb: auto,
        min_memory_mb: jvm::MIN_HEAP_MB,
        max_memory_mb: jvm::heap_limit_mb(total),
        total_memory_mb: total / (1024 * 1024),
        jvm_args: join_args(&settings.jvm_args),
        java_path: settings.java_path.map(|p| p.display().to_string()),
        data_dir: state.paths().base().display().to_string(),
        default_data_dir: state.home.display().to_string(),
        pack_sources: packs::known_sources()
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    })
}

#[tauri::command]
pub async fn set_memory(state: State<'_, AppState>, memory_mb: Option<u64>) -> Reply<()> {
    if let Some(mb) = memory_mb {
        let limit = jvm::heap_limit_mb(system::total_memory_bytes());
        if !(jvm::MIN_HEAP_MB..=limit).contains(&mb) {
            return Err(format!("Pick between {} and {limit} MB.", jvm::MIN_HEAP_MB));
        }
    }
    let mut settings = load_settings(&state).await?;
    settings.memory_mb = memory_mb;
    save_settings(&state, &settings).await
}

#[tauri::command]
pub async fn set_jvm_args(state: State<'_, AppState>, text: String) -> Reply<String> {
    let args = jvm::parse_extra_args(&text).map_err(human)?;
    let mut settings = load_settings(&state).await?;
    settings.jvm_args = args;
    save_settings(&state, &settings).await?;
    Ok(join_args(&settings.jvm_args))
}

#[tauri::command]
pub async fn check_java(path: String) -> Reply<String> {
    java::probe(&PathBuf::from(path)).await.map_err(human)
}

#[tauri::command]
pub async fn set_java(state: State<'_, AppState>, path: Option<String>) -> Reply<Option<String>> {
    let version = match &path {
        Some(p) => Some(java::probe(&PathBuf::from(p)).await.map_err(human)?),
        None => None,
    };
    let mut settings = load_settings(&state).await?;
    settings.java_path = path.map(PathBuf::from);
    save_settings(&state, &settings).await?;
    Ok(version)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Reply<Option<String>> {
    let picked =
        tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
            .await
            .map_err(|e| format!("The folder picker stopped: {e}"))?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string()))
}

#[tauri::command]
pub async fn pick_java(app: AppHandle) -> Reply<Option<String>> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_title("Pick a java executable")
            .blocking_pick_file()
    })
    .await
    .map_err(|e| format!("The file picker stopped: {e}"))?;
    Ok(picked
        .and_then(|p| p.into_path().ok())
        .map(|p| p.display().to_string()))
}

#[tauri::command]
pub async fn set_data_dir(state: State<'_, AppState>, path: Option<String>) -> Reply<String> {
    let Ok(_busy) = state.busy.try_lock() else {
        return Err("Wait for the install to finish first.".into());
    };
    let target = path.as_ref().map(PathBuf::from);
    if let Some(dir) = &target {
        if !dir.is_absolute() {
            return Err("Pick a full folder path.".into());
        }
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| format!("Can't use {}: {e}", dir.display()))?;
        let probe = dir.join(".esteban-write-test");
        tokio::fs::write(&probe, b"ok")
            .await
            .map_err(|e| format!("Can't write to {}: {e}", dir.display()))?;
        tokio::fs::remove_file(&probe)
            .await
            .map_err(|e| format!("Can't clean up in {}: {e}", dir.display()))?;
    }
    let home = Paths::new(state.home.clone());
    let mut settings = Settings::load(&home).await.map_err(human)?;
    settings.data_dir = target.filter(|d| *d != state.home);
    settings.save(&home).await.map_err(human)?;
    let paths = Settings::paths(state.home.clone()).await.map_err(human)?;
    let shown = paths.base().display().to_string();
    state.set_paths(paths);
    Ok(shown)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackLine {
    folder: &'static str,
    text: String,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum PackAction {
    Link,
    Import,
}

#[tauri::command]
pub async fn packs(
    state: State<'_, AppState>,
    action: PackAction,
    from: String,
    selection: Selection,
) -> Reply<Vec<PackLine>> {
    let instance = instance(&state, &selection)?;
    let from = PathBuf::from(from);
    if !from.is_dir() {
        return Err(format!("{} isn't a folder.", from.display()));
    }
    let lines = match action {
        PackAction::Link => packs::link(&instance, &from)
            .await
            .map_err(human)?
            .into_iter()
            .map(|(folder, outcome)| PackLine {
                folder,
                text: match outcome {
                    Linked::Linked(to) => format!("linked to {}", to.display()),
                    Linked::AlreadyLinked(to) => format!("already linked to {}", to.display()),
                    Linked::Relinked { to, .. } => format!("now linked to {}", to.display()),
                    Linked::NotInSource => "not in that folder, skipped".into(),
                    Linked::HasFiles => {
                        "already has files here, left alone (use Copy instead)".into()
                    }
                },
            })
            .collect(),
        PackAction::Import => packs::import(&instance, &from)
            .await
            .map_err(human)?
            .into_iter()
            .map(|(folder, outcome)| PackLine {
                folder,
                text: match outcome {
                    Imported::Copied {
                        copied,
                        kept,
                        skipped_links,
                    } => {
                        let mut text = format!("copied {copied} files");
                        if kept > 0 {
                            text.push_str(&format!(", kept {kept} you already had"));
                        }
                        if skipped_links > 0 {
                            text.push_str(&format!(", skipped {skipped_links} links"));
                        }
                        text
                    }
                    Imported::IsLinked(to) => {
                        format!("linked to {}, nothing to copy", to.display())
                    }
                    Imported::NotInSource => "not in that folder, skipped".into(),
                },
            })
            .collect(),
    };
    Ok(lines)
}

const DONATE: &str = include_str!("../../../config/donate.json");
const GITHUB: &str = "https://github.com/SunqdXX/esteban-launcher";
const ESTEBAN: &str = "https://github.com/SunqdXX/esteban";

#[derive(Deserialize, Default)]
struct Donate {
    #[serde(default)]
    discord: String,
    #[serde(default)]
    btc: String,
    #[serde(default)]
    xmr: String,
    #[serde(default)]
    usdc: String,
    #[serde(default)]
    usdc_network: String,
}

fn donate() -> Donate {
    serde_json::from_str(DONATE).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Coin {
    name: &'static str,
    note: String,
    address: String,
    qr: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct About {
    version: &'static str,
    disclaimer: &'static str,
    discord: bool,
    coins: Vec<Coin>,
}

fn qr_rows(text: &str) -> Vec<String> {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else {
        return Vec::new();
    };
    let width = code.width();
    code.to_colors()
        .chunks(width)
        .map(|row| {
            row.iter()
                .map(|c| if *c == qrcode::Color::Dark { '1' } else { '0' })
                .collect()
        })
        .collect()
}

fn coin(name: &'static str, note: String, address: &str) -> Coin {
    let address = address.trim().to_string();
    Coin {
        name,
        note,
        qr: if address.is_empty() {
            Vec::new()
        } else {
            qr_rows(&address)
        },
        address,
    }
}

#[tauri::command]
pub fn about() -> About {
    let config = donate();
    let usdc_note = if config.usdc_network.trim().is_empty() {
        String::new()
    } else {
        format!("on {}", config.usdc_network.trim())
    };
    About {
        version: esteban_core::VERSION,
        disclaimer: esteban_core::DISCLAIMER,
        discord: !config.discord.trim().is_empty(),
        coins: vec![
            coin("Bitcoin", String::new(), &config.btc),
            coin("Monero", String::new(), &config.xmr),
            coin("USDC", usdc_note, &config.usdc),
        ],
    }
}

#[tauri::command]
pub fn open_link(app: AppHandle, which: String) -> Reply<()> {
    let config = donate();
    let url = match which.as_str() {
        "github" => GITHUB.to_string(),
        "esteban" => ESTEBAN.to_string(),
        "discord"
            if config.discord.starts_with("https://discord.gg/")
                || config.discord.starts_with("https://discord.com/invite/") =>
        {
            config.discord.clone()
        }
        _ => return Err("That link isn't available.".into()),
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| format!("Could not open the link: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_rows_are_a_square_with_finder_patterns() {
        let rows = qr_rows("bc1qexampleexampleexampleexample");
        assert!(rows.len() >= 21);
        assert!(rows.iter().all(|r| r.len() == rows.len()));
        assert!(rows[0].starts_with("1111111"));
        assert!(rows[0].ends_with("1111111"));
    }

    #[test]
    fn empty_addresses_have_no_qr_and_the_shipped_config_parses() {
        let empty = coin("Bitcoin", String::new(), "   ");
        assert!(empty.address.is_empty() && empty.qr.is_empty());
        let parsed: Donate = serde_json::from_str(DONATE).unwrap();
        assert!(parsed.discord.is_empty() || parsed.discord.starts_with("https://"));
    }

    #[test]
    fn jvm_args_round_trip_with_quotes() {
        let args = vec!["-XX:+UseZGC".to_string(), "-Dname=a b".to_string()];
        let text = join_args(&args);
        assert_eq!(text, "-XX:+UseZGC \"-Dname=a b\"");
        assert_eq!(
            jvm::parse_extra_args(&text).unwrap(),
            vec!["-XX:+UseZGC", "-Dname=a b"]
        );
    }
}
