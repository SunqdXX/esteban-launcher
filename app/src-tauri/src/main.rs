#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod progress;

use esteban_core::net::Net;
use esteban_core::paths::Paths;

fn state() -> esteban_core::Result<commands::AppState> {
    let base = match std::env::var_os("ESTEBAN_HOME") {
        Some(dir) => dir.into(),
        None => Paths::default_base()?,
    };
    Ok(commands::AppState {
        paths: Paths::new(base),
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
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("{} failed to start: {error}", esteban_core::PRODUCT);
        std::process::exit(1);
    }
}
