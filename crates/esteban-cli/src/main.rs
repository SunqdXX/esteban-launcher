mod progress;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use esteban_core::account::{Secret, Session};
use esteban_core::install::{InstallOptions, Installed, install};
use esteban_core::launch::{self, QuickPlay, Smoke};
use esteban_core::modrinth::DEFAULT_MODS;
use esteban_core::net::Net;
use esteban_core::packs::{self, Imported, Linked};
use esteban_core::paths::Paths;
use esteban_core::profile::{HACKS_WARNING, Instance, ProfileKind, Settings};
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
    #[command(about = "Download and verify everything a profile needs")]
    Install(Target),
    #[command(about = "Install if needed, then start the game")]
    Launch(LaunchArgs),
    #[command(about = "Print the exact command a launch would run, with tokens hidden")]
    Plan(LaunchArgs),
    #[command(subcommand, about = "See or change which mods a profile uses")]
    Mods(ModsCommand),
    #[command(
        subcommand,
        about = "Use shader packs, resource packs and screenshots from another game folder"
    )]
    Packs(PacksCommand),
    #[command(about = "Print the folder of a profile")]
    Path(Selector),
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
    #[command(about = "Show the mods of a profile and why any were skipped")]
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
    #[arg(long, default_value = "clean", value_parser = parse_profile, help = "clean or hacks")]
    profile: ProfileKind,
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
    #[arg(
        long = "version",
        value_name = "VERSION",
        help = "Game version, for example 1.21.4"
    )]
    game_version: String,
    #[arg(long, default_value = "clean", value_parser = parse_profile, help = "clean or hacks")]
    profile: ProfileKind,
    #[arg(long, help = "Pick up newer Fabric and mod versions")]
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
}

fn parse_profile(value: &str) -> Result<ProfileKind, String> {
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
        Command::Path(selector) => {
            let instance = Instance::new(&paths, selector.profile, &selector.game_version)?;
            println!("{}", instance.dir.display());
            if !instance.dir.exists() {
                eprintln!("not installed yet");
            }
            Ok(())
        }
    }
}

async fn link_packs(paths: &Paths, source: &PackSource) -> Result<(), Failure> {
    let instance = Instance::new(
        paths,
        source.selector.profile,
        &source.selector.game_version,
    )?;
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
    let instance = Instance::new(
        paths,
        source.selector.profile,
        &source.selector.game_version,
    )?;
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
    let instance = Instance::new(paths, selector.profile, &selector.game_version)?;
    let Some(lock) = instance.read_lock().await? else {
        eprintln!(
            "{} {} is not installed yet. Run install first.",
            selector.profile, selector.game_version
        );
        return Ok(());
    };
    let meta = instance.read_meta().await?;
    println!("{} {}", instance.kind, instance.game_version);
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
    for slug in &meta.disabled {
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
    let instance = Instance::new(paths, target.profile, &target.game_version)?;
    let title = instance.set_mod_enabled(&toggle.name, enabled).await?;
    eprintln!(
        "{title} is {} for {} {}.",
        if enabled { "on" } else { "off" },
        target.profile,
        target.game_version
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
    if target.profile == ProfileKind::Hacks {
        let mut settings = Settings::load(paths).await?;
        if !settings.hacks_warning_accepted {
            if !target.accept_hacks {
                eprintln!("Esteban + Hacks adds the hacks mod on top of the clean profile.");
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
        target.profile,
        &target.game_version,
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
        "{} {} ready: Fabric {}, Java {}, {} mods{}",
        installed.instance.kind,
        installed.instance.game_version,
        installed.loader_version,
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

    let smoke = args.smoke_test.map(|seconds| Smoke {
        markers: vec!["Sound engine started".to_string()],
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
