#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod progress;

use esteban_core::net::Net;
use esteban_core::paths::Paths;
use esteban_core::profile::Settings;

fn state() -> esteban_core::Result<commands::AppState> {
    let home: std::path::PathBuf = match std::env::var_os("ESTEBAN_HOME") {
        Some(dir) => dir.into(),
        None => Paths::default_base()?,
    };
    let paths = tauri::async_runtime::block_on(Settings::paths(home.clone()))?;
    Ok(commands::AppState {
        home,
        paths: std::sync::RwLock::new(paths),
        net: Net::launcher()?,
        busy: tokio::sync::Mutex::new(()),
    })
}

fn main() {
    let state = match state() {
        Ok(state) => state,
        Err(error) => {
            eprintln!("{} failed to start: {error}", esteban_core::PRODUCT);
            std::process::exit(1);
        }
    };
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::overview,
            commands::select,
            commands::accept_hacks_warning,
            commands::instance_status,
            commands::set_mod,
            commands::install,
            commands::open_folder,
            commands::launcher_settings,
            commands::set_memory,
            commands::set_jvm_args,
            commands::check_java,
            commands::set_java,
            commands::pick_folder,
            commands::pick_java,
            commands::set_data_dir,
            commands::packs,
            commands::about,
            commands::open_link,
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("{} failed to start: {error}", esteban_core::PRODUCT);
        std::process::exit(1);
    }
}
