fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "overview",
        "select",
        "accept_hacks_warning",
        "instance_status",
        "set_mod",
        "install",
        "open_folder",
        "launcher_settings",
        "set_memory",
        "set_jvm_args",
        "check_java",
        "set_java",
        "pick_folder",
        "pick_java",
        "set_data_dir",
        "packs",
        "about",
        "open_link",
    ]);
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
