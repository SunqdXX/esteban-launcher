fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "overview",
        "select",
        "accept_hacks_warning",
        "instance_status",
        "set_mod",
        "install",
        "open_folder",
    ]);
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
