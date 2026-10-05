#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = tauri::Builder::default().run(tauri::generate_context!()) {
        eprintln!("{} failed to start: {error}", esteban_core::PRODUCT);
        std::process::exit(1);
    }
}
