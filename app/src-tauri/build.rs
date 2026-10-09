use std::path::Path;
use std::time::SystemTime;

const FRONTEND_SOURCES: &[&str] = &["src", "public", "index.html", "vite.config.ts"];

fn newest(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.is_dir() {
        std::fs::read_dir(path)
            .ok()?
            .flatten()
            .filter_map(|entry| newest(&entry.path()))
            .max()
    } else {
        meta.modified().ok()
    }
}

fn check_frontend() {
    if std::env::var_os("CARGO_FEATURE_CUSTOM_PROTOCOL").is_none() {
        return;
    }
    let app = Path::new("..");
    println!("cargo:rerun-if-changed=../dist");
    for source in FRONTEND_SOURCES {
        println!("cargo:rerun-if-changed=../{source}");
    }
    let how = "Run `npm run build` in app/ first, or build with `npm run tauri build`.";
    let Some(built) = newest(&app.join("dist").join("index.html")) else {
        eprintln!("app/dist is missing, so this build would have no screens. {how}");
        std::process::exit(1);
    };
    let edited = FRONTEND_SOURCES
        .iter()
        .filter_map(|source| newest(&app.join(source)))
        .max();
    if edited.is_some_and(|edited| edited > built) {
        let message =
            "app/dist is older than the frontend sources, so this build would show an old UI.";
        if std::env::var("PROFILE").is_ok_and(|profile| profile == "release") {
            eprintln!("{message} {how}");
            std::process::exit(1);
        }
        println!("cargo:warning={message} {how}");
    }
}

fn main() {
    check_frontend();
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
        "catalog",
        "loader_versions",
        "set_loader_version",
        "instances",
        "skins",
        "import_skin",
        "update_skin",
        "remove_skin",
        "set_skin",
        "release_status",
        "install_update",
        "restart_app",
    ]);
    if let Err(error) =
        tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
    {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
