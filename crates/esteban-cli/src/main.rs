mod progress;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use esteban_core::account::{Secret, Session};
use esteban_core::catalog;
use esteban_core::install::{InstallOptions, Installed, install};
use esteban_core::instance::Instance;
use esteban_core::launch::{self, QuickPlay, Smoke};
use esteban_core::loader::Loader;
use esteban_core::modrinth::DEFAULT_MODS;
use esteban_core::net::Net;
use esteban_core::packs::{self, Imported, Linked};
use esteban_core::paths::Paths;
use esteban_core::profile::{HACKS_WARNING, Settings};
use esteban_core::{DISCLAIMER, Error};

use crate::progress::CliProgress;

#[derive(Parser)]
#[command(name = "esteban-cli", version, about = "Esteban Launcher on the command line", after_help = DISCLAIMER)]
struct Cli {
    #[arg(
        long,
        env = "ESTEBAN_HOME",
        global = true,
        help = "Where game files and instances live"
    )]
    base_dir: Option<PathBuf>,
    #[arg(short, long, global = true, help = "Show debug logs")]
    verbose: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(about = "Download and verify everything an instance needs")]
    Install(Target),
    #[command(about = "Install if needed, then start the game")]
    Launch(LaunchArgs),
    #[command(about = "Print the exact command a launch would run, with tokens hidden")]
    Plan(LaunchArgs),
    #[command(subcommand, about = "See or change which mods an instance uses")]
    Mods(ModsCommand),
    #[command(
        subcommand,
        about = "Use shader packs, resource packs and screenshots from another game folder"
    )]
    Packs(PacksCommand),
    #[command(about = "Print the folder of an instance")]
    Path(Selector),
    #[command(about = "List every release and which loaders it can use")]
    Versions,
    #[command(about = "List the loader builds for a version, newest first")]
    Loaders(Selector),
    #[command(about = "Pin a loader build for an instance, or go back to the latest stable one")]
    LoaderVersion(LoaderPick),
}

#[derive(Args, Clone)]
struct LoaderPick {
    #[command(flatten)]
    selector: Selector,
    #[arg(
        value_name = "BUILD",
        help = "A build from the loaders list, or latest-stable"
    )]
    build: String,
}

#[derive(Subcommand)]
enum PacksCommand {
    #[command(
        about = "Point the instance's pack folders at another game folder, nothing gets copied"
    )]
    Link(PackSource),
    #[command(about = "Copy packs and screenshots in, never overwriting a file")]
    Import(PackSource),
}

#[derive(Args, Clone)]
struct PackSource {
    #[command(flatten)]
    selector: Selector,
    #[arg(
        long,
        value_name = "DIR",
        help = "Game folder that holds shaderpacks, resourcepacks and screenshots, for example ~/.minecraft"
    )]
    from: PathBuf,
}

#[derive(Subcommand)]
enum ModsCommand {
    #[command(about = "Show the mods of an instance and why any were skipped")]
    List(Selector),
    #[command(about = "Turn a mod back on, then install")]
    Enable(Toggle),
    #[command(about = "Turn a mod off, then install")]
    Disable(Toggle),
}

#[derive(Args, Clone)]
struct Selector {
    #[arg(
        long = "version",
        value_name = "VERSION",
        help = "Game version, for example 1.21.4"
    )]
    game_version: String,
    #[arg(long, default_value = "fabric", value_parser = parse_loader, help = "vanilla, fabric or forge")]
    loader: Loader,
    #[arg(
        long,
        help = "The Hacked instance (only the Esteban versions on Fabric)"
    )]
    hacked: bool,
}

impl Selector {
    fn instance(&self, paths: &Paths) -> Result<Instance, Failure> {
        Ok(Instance::new(
            paths,
            &self.game_version,
            self.loader,
            self.hacked,
        )?)
    }
}

#[derive(Args, Clone)]
struct Toggle {
    #[command(flatten)]
    target: Target,
    #[arg(
        value_name = "MOD",
        help = "sodium, lithium, ferrite-core, immediatelyfast or iris"
    )]
    name: String,
}

#[derive(Args, Clone)]
struct Target {
    #[command(flatten)]
    selector: Selector,
    #[arg(long, help = "Pick up newer loader and mod versions")]
    update: bool,
    #[arg(long, help = "Confirm the hacks warning (only needed once)")]
    accept_hacks: bool,
}

#[derive(Args, Clone)]
struct LaunchArgs {
    #[command(flatten)]
    target: Target,
    #[arg(
        long,
        value_name = "HOST",
        conflicts_with = "world",
        help = "Join a server right away"
    )]
    server: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Open a singleplayer world right away"
    )]
    world: Option<String>,
    #[arg(long, value_name = "MB", help = "Maximum memory for the game")]
    memory: Option<u64>,
    #[arg(
        long,
        value_name = "SECONDS",
        help = "Stop once the main menu is reached, fail if it takes longer"
    )]
    smoke_test: Option<u64>,
    #[arg(
        long,
        value_name = "TEXT",
        requires = "smoke_test",
        help = "Log line that means the menu is up (default: Sound engine started)"
    )]
    smoke_marker: Vec<String>,
}

fn parse_loader(value: &str) -> Result<Loader, String> {
    value.parse().map_err(|e: Error| e.to_string())
}

enum Failure {
    Error(Error),
    NeedsConsent,
    GameCrashed,
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Error(error)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let filter = if cli.verbose { "debug" } else { "warn" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .with_writer(std::io::stderr)
        .init();

    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Error(error)) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
        Err(Failure::NeedsConsent) => ExitCode::from(2),
        Err(Failure::GameCrashed) => ExitCode::from(3),
    }
}

async fn run(cli: Cli) -> Result<(), Failure> {
    let home = match cli.base_dir {
        Some(dir) => dir,
        None => Paths::default_base()?,
    };
    let paths = Settings::paths(home).await?;
    let net = Net::launcher()?;
    match cli.command {
        Command::Install(target) => {
            let installed = prepare(&net, &paths, &target, &[]).await?;
            summarize(&installed);
            Ok(())
        }
        Command::Launch(args) => launch(&net, &paths, args, false).await,
        Command::Plan(args) => launch(&net, &paths, args, true).await,
        Command::Mods(ModsCommand::List(selector)) => list_mods(&paths, &selector).await,
        Command::Mods(ModsCommand::Enable(toggle)) => toggle_mod(&net, &paths, &toggle, true).await,
        Command::Mods(ModsCommand::Disable(toggle)) => {
            toggle_mod(&net, &paths, &toggle, false).await
        }
        Command::Packs(PacksCommand::Link(source)) => link_packs(&paths, &source).await,
        Command::Packs(PacksCommand::Import(source)) => import_packs(&paths, &source).await,
        Command::Versions => list_versions(&net, &paths).await,
        Command::Loaders(selector) => list_loaders(&net, &paths, &selector).await,
        Command::LoaderVersion(pick) => pick_loader(&net, &paths, &pick).await,
        Command::Path(selector) => {
            let instance = selector.instance(&paths)?;
            println!("{}", instance.dir.display());
            if !instance.dir.exists() {
                eprintln!("not installed yet");
            }
            Ok(())
        }
    }
}

async fn list_versions(net: &Net, paths: &Paths) -> Result<(), Failure> {
    let catalog = catalog::load(net, paths).await?;
    if catalog.offline {
        eprintln!("offline, showing the last saved lists");
    }
    for release in &catalog.releases {
        let mark = |offer: &catalog::Offer| if offer.available { "yes" } else { "-" };
        let tag = if release.tag.is_empty() {
            String::new()
        } else {
            format!("  {}", release.tag)
        };
        println!(
            "{:<10} {}  fabric {:<3}  forge {:<3}{tag}",
            release.id,
            release.date,
            mark(&release.fabric),
            mark(&release.forge)
        );
    }
    Ok(())
}

async fn list_loaders(net: &Net, paths: &Paths, selector: &Selector) -> Result<(), Failure> {
    let instance = selector.instance(paths)?;
    let choices =
        catalog::loader_versions(net, paths, &instance.game_version, instance.loader).await?;
    let file = instance.read_file().await?;
    if let Some(note) = &choices.note {
        eprintln!("{note}");
    }
    for v in &choices.versions {
        let mut marks = Vec::new();
        if choices.default.as_ref() == Some(&v.version) {
            marks.push("default");
        }
        if v.stable {
            marks.push("stable");
        }
        if file.loader.pinned && file.loader.version.as_ref() == Some(&v.version) {
            marks.push("pinned");
        }
        println!("{}  {}", v.version, marks.join(", "));
    }
    if choices.versions.is_empty() {
        eprintln!("{} has no loader builds to pick from.", instance.label());
    }
    Ok(())
}

async fn pick_loader(net: &Net, paths: &Paths, pick: &LoaderPick) -> Result<(), Failure> {
    let instance = pick.selector.instance(paths)?;
    let version = if pick.build == "latest-stable" {
        None
    } else {
        let choices =
            catalog::loader_versions(net, paths, &instance.game_version, instance.loader).await?;
        if !choices.versions.iter().any(|v| v.version == pick.build) {
            return Err(Failure::Error(Error::Unsupported(format!(
                "{} isn't a {} build for {}. Run loaders to see the list.",
                pick.build,
                instance.loader.title(),
                instance.game_version
            ))));
        }
        Some(pick.build.clone())
    };
    instance.set_loader_version(version.clone()).await?;
    match version {
        Some(v) => eprintln!(
            "{} will use {} {v}.",
            instance.label(),
            instance.loader.title()
        ),
        None => eprintln!(
            "{} will use the latest stable {} build.",
            instance.label(),
            instance.loader.title()
        ),
    }
    Ok(())
}

async fn link_packs(paths: &Paths, source: &PackSource) -> Result<(), Failure> {
    let instance = source.selector.instance(paths)?;
    let mut refused = false;
    for (name, outcome) in packs::link(&instance, &source.from).await? {
        match outcome {
            Linked::Linked(to) => println!("{name}: linked to {}", to.display()),
            Linked::AlreadyLinked(to) => println!("{name}: already linked to {}", to.display()),
            Linked::Relinked { from, to } => println!(
                "{name}: linked to {} (was {}, that folder is untouched)",
                to.display(),
                from.display()
            ),
            Linked::NotInSource => println!("{name}: not in {}, skipped", source.from.display()),
            Linked::HasFiles => {
                refused = true;
                println!(
                    "{name}: already has files here, so it was left alone. Use packs import to copy instead."
                );
            }
        }
    }
    println!("Folder: {}", instance.dir.display());
    if refused {
        return Err(Failure::Error(Error::Unsupported(
            "some folders were left alone".into(),
        )));
    }
    Ok(())
}

async fn import_packs(paths: &Paths, source: &PackSource) -> Result<(), Failure> {
    let instance = source.selector.instance(paths)?;
    for (name, outcome) in packs::import(&instance, &source.from).await? {
        match outcome {
            Imported::Copied {
                copied,
                kept,
                skipped_links,
            } => {
                let links = if skipped_links > 0 {
                    format!(", skipped {skipped_links} links")
                } else {
                    String::new()
                };
                println!("{name}: copied {copied}, kept {kept} that were already there{links}");
            }
            Imported::IsLinked(to) => println!(
                "{name}: linked to {}, so there is nothing to copy",
                to.display()
            ),
            Imported::NotInSource => println!("{name}: not in {}, skipped", source.from.display()),
        }
    }
    println!("Folder: {}", instance.dir.display());
    Ok(())
}

async fn list_mods(paths: &Paths, selector: &Selector) -> Result<(), Failure> {
    let instance = selector.instance(paths)?;
    let file = instance.read_file().await?;
    let Some(lock) = file.lock() else {
        eprintln!(
            "{} is not installed yet. Run install first.",
            instance.label()
        );
        return Ok(());
    };
    println!("{}", instance.label());
    let width = lock.mods.iter().map(|m| m.title.len()).max().unwrap_or(0);
    for m in &lock.mods {
        let note = if DEFAULT_MODS
            .iter()
            .any(|d| d.required && (d.slug == m.slug || d.title == m.title))
        {
            "  required"
        } else {
            ""
        };
        println!("  on    {:width$}  {}{note}", m.title, m.version_number);
    }
    for extra in &lock.extras {
        let what = if extra.starts_with("esteban-hud-") {
            "Esteban HUD"
        } else {
            "hacks mod"
        };
        println!("  on    {extra}  {what}");
    }
    for slug in &file.disabled {
        let title = DEFAULT_MODS
            .iter()
            .find(|d| d.slug == slug)
            .map_or(slug.as_str(), |d| d.title);
        println!("  off   {title:width$}  turn on with: mods enable {slug}");
    }
    if !lock.skipped.is_empty() {
        println!("Skipped:");
        for skipped in &lock.skipped {
            println!("  {}", skipped.message);
        }
    }
    let unmanaged = instance.unmanaged_jars(Some(&lock)).await?;
    if !unmanaged.is_empty() {
        println!("Added by you, not checked by the launcher:");
        for name in unmanaged {
            println!("  {name}");
        }
    }
    println!("Folder: {}", instance.dir.display());
    Ok(())
}

async fn toggle_mod(
    net: &Net,
    paths: &Paths,
    toggle: &Toggle,
    enabled: bool,
) -> Result<(), Failure> {
    let target = &toggle.target;
    let instance = target.selector.instance(paths)?;
    let title = instance.set_mod_enabled(&toggle.name, enabled).await?;
    eprintln!(
        "{title} is {} for {}.",
        if enabled { "on" } else { "off" },
        instance.label()
    );
    let installed = prepare(net, paths, target, &[]).await?;
    summarize(&installed);
    Ok(())
}

const SMOKE_OPTIONS: &[(&str, &str)] = &[("onboardAccessibility", "false")];

async fn prepare(
    net: &Net,
    paths: &Paths,
    target: &Target,
    first_run_options: &'static [(&'static str, &'static str)],
) -> Result<Installed, Failure> {
    let instance = target.selector.instance(paths)?;
    if instance.hacked {
        let mut settings = Settings::load(paths).await?;
        if !settings.hacks_warning_accepted {
            if !target.accept_hacks {
                eprintln!("Hacked adds the hacks mod on top of the normal Fabric instance.");
                eprintln!("{HACKS_WARNING}");
                eprintln!(
                    "Run the same command again with --accept-hacks to confirm. You only do this once."
                );
                return Err(Failure::NeedsConsent);
            }
            settings.hacks_warning_accepted = true;
            settings.save(paths).await?;
        }
    }
    let progress = CliProgress::new();
    let installed = install(
        net,
        paths,
        &instance,
        InstallOptions {
            update: target.update,
            first_run_options,
        },
        &progress,
    )
    .await;
    progress.finish();
    Ok(installed?)
}

fn summarize(installed: &Installed) {
    let stats = installed.stats;
    eprintln!(
        "{} ready: {}, Java {}, {} mods{}",
        installed.instance.label(),
        installed.loader_label(),
        installed.java.version,
        installed.mods.len() + installed.extras.len(),
        if stats.fetched_files == 0 {
            " (everything was already verified)".to_string()
        } else {
            format!(
                " (downloaded {} files, {:.1} MB)",
                stats.fetched_files,
                stats.fetched_bytes as f64 / 1_048_576.0
            )
        }
    );
    for m in &installed.mods {
        eprintln!("  {} {}", m.title, m.version_number);
    }
    for extra in &installed.extras {
        eprintln!("  {extra}");
    }
    if !installed.disabled.is_empty() {
        eprintln!("  turned off: {}", installed.disabled.join(", "));
    }
    for skipped in &installed.unavailable {
        eprintln!("  skipped: {}", skipped.message);
    }
    if !installed.unmanaged.is_empty() {
        eprintln!(
            "  added by you, not checked by the launcher: {}",
            installed.unmanaged.join(", ")
        );
    }
}

fn session() -> Result<Session, Failure> {
    Err(Failure::Error(Error::Account(
        "Microsoft sign-in is not built yet, so the game can't be launched. install and plan work."
            .into(),
    )))
}

fn preview_session() -> Session {
    Session {
        username: "<account>".into(),
        uuid: "<uuid>".into(),
        access_token: Secret::new("<token>"),
        xuid: "<xuid>".into(),
        user_type: "msa".into(),
        client_id: "<client-id>".into(),
    }
}

async fn launch(net: &Net, paths: &Paths, args: LaunchArgs, dry_run: bool) -> Result<(), Failure> {
    let session = if dry_run {
        preview_session()
    } else {
        session()?
    };
    let first_run = if args.smoke_test.is_some() {
        SMOKE_OPTIONS
    } else {
        &[]
    };
    let installed = prepare(net, paths, &args.target, first_run).await?;
    summarize(&installed);
    let quick_play = match (&args.server, &args.world) {
        (Some(server), _) => Some(QuickPlay::Multiplayer(server.clone())),
        (None, Some(world)) => Some(QuickPlay::Singleplayer(world.clone())),
        (None, None) => None,
    };
    let options = Settings::load(paths)
        .await?
        .launch_options(quick_play, args.memory);
    let plan = launch::build(&installed, &session, &options, &installed.platform)?;
    if dry_run {
        println!("{}", plan.command_line());
        return Ok(());
    }

    let markers = if args.smoke_marker.is_empty() {
        vec!["Sound engine started".to_string()]
    } else {
        args.smoke_marker.clone()
    };
    let smoke = args.smoke_test.map(|seconds| Smoke {
        markers,
        timeout: Duration::from_secs(seconds),
        settle: Duration::from_secs(15),
    });
    eprintln!("starting the game");
    let outcome = launch::run(&plan, |line| println!("{line}"), smoke).await?;

    if let Some(smoke) = &outcome.smoke {
        eprintln!(
            "smoke test: reached the main menu with {} mods loaded",
            smoke.loaded_mods.len()
        );
        for entry in &smoke.loaded_mods {
            eprintln!("  {entry}");
        }
    }
    if let Some(crash) = &outcome.crash {
        eprintln!("{}", crash.headline);
        if let Some(detail) = &crash.detail {
            eprintln!("  {detail}");
        }
        if let Some(report) = &crash.report {
            eprintln!("  full report: {}", report.display());
        }
        return Err(Failure::GameCrashed);
    }
    Ok(())
}
