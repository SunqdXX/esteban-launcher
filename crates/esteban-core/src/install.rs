use std::collections::{BTreeSet, HashSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::channel;
use crate::download::{self, Download, Stats};
use crate::error::IoContext;
use crate::esteban;
use crate::fabric;
use crate::forge;
use crate::hash::Hash;
use crate::instance::{Instance, InstanceFile, Jar};
use crate::java::{self, JavaRuntime};
use crate::launch::jvm;
use crate::loader::{LaunchProfile, Loader};
use crate::modcheck::{self, Environment, Jar as CheckedJar};
use crate::modrinth::lock::Skipped;
use crate::modrinth::{self, DEFAULT_MODS, ModLock, Modrinth, ResolvedMod, Unavailable};
use crate::mojang::manifest::VersionManifest;
use crate::mojang::rules::Context;
use crate::mojang::version::VersionJson;
use crate::mojang::{assets, libraries, natives};
use crate::net::Net;
use crate::paths::Paths;
use crate::profile::{HACKS_WARNING, NEW_INSTANCE_OPTIONS, Settings};
use crate::progress::Progress;
use crate::skins;
use crate::system::{self, Platform};
use crate::{Error, Result, fsx};

#[derive(Clone, Copy, Debug, Default)]
pub struct InstallOptions {
    pub update: bool,
    pub first_run_options: &'static [(&'static str, &'static str)],
}

#[derive(Clone, Debug)]
pub struct Installed {
    pub instance: Instance,
    pub platform: Platform,
    pub version: VersionJson,
    pub loader: LaunchProfile,
    pub classpath: Vec<PathBuf>,
    pub java: JavaRuntime,
    pub assets_root: PathBuf,
    pub game_assets: PathBuf,
    pub libraries_root: PathBuf,
    pub mods: Vec<ResolvedMod>,
    pub extras: Vec<String>,
    pub unavailable: Vec<Unavailable>,
    pub disabled: Vec<String>,
    pub unmanaged: Vec<String>,
    pub default_max_heap_mb: u64,
    pub stats: Stats,
}

impl Installed {
    pub fn loader_label(&self) -> String {
        match &self.loader.loader_version {
            Some(version) => format!("{} {version}", self.loader.loader.title()),
            None => self.loader.loader.title().to_string(),
        }
    }
}

pub async fn install(
    net: &Net,
    paths: &Paths,
    instance: &Instance,
    options: InstallOptions,
    progress: &dyn Progress,
) -> Result<Installed> {
    if instance.hacked && !Settings::load(paths).await?.hacks_warning_accepted {
        return Err(Error::Guard(format!(
            "Hacked has not been accepted yet. {HACKS_WARNING}"
        )));
    }
    let platform = Platform::current()?;
    let game = instance.game_version.clone();
    let features = BTreeSet::new();
    let ctx = Context {
        platform: &platform,
        features: &features,
    };
    let mut stats = Stats::default();

    progress.notice("reading Mojang's version list");
    let manifest = VersionManifest::fetch(net).await?;
    let entry = manifest.find(&game)?;
    let (version, raw): (VersionJson, _) = net
        .verified_json(&entry.url, "the version file", &Hash::sha1(&entry.sha1))
        .await?;
    let version_dir = paths.version_dir(fabric::token(&version.id)?);
    fsx::write_atomic(&version_dir.join(format!("{}.json", version.id)), &raw).await?;

    let client_jar = version_dir.join(format!("{}.jar", version.id));
    stats.add(
        download::ensure_all(
            net,
            vec![Download {
                url: version.downloads.client.url.clone(),
                dest: client_jar.clone(),
                hash: Hash::sha1(&version.downloads.client.sha1),
                size: Some(version.downloads.client.size),
                executable: false,
            }],
            1,
            progress,
            "game",
        )
        .await?,
    );

    if !instance.has_options().await? {
        match data_version(&client_jar).await? {
            Some(data_version) => {
                let entries: Vec<(&str, &str)> = NEW_INSTANCE_OPTIONS
                    .iter()
                    .chain(options.first_run_options)
                    .copied()
                    .collect();
                if instance.seed_options(data_version, &entries).await? {
                    progress.notice(
                        "New instance: GUI scale starts at 3. Change it in game under Options, Video Settings.",
                    );
                }
            }
            None => {
                tracing::debug!(jar = %client_jar.display(), "the client jar has no data version, leaving options.txt to the game");
            }
        }
    }

    let (java, java_stats) = java::ensure(
        net,
        paths,
        &platform,
        &version.java_version.component,
        progress,
    )
    .await?;
    stats.add(java_stats);

    let mut file = instance.read_file().await?;
    let mut game_jar = client_jar.clone();
    let loader = match instance.loader {
        Loader::Vanilla => LaunchProfile::vanilla(&version),
        Loader::Fabric => {
            if crate::versions::is_pinned(&game) {
                let report = channel::refresh(net, paths).await?;
                if let Some(notice) = &report.notice {
                    progress.notice(notice);
                }
            }
            instance.ensure_custom_backgrounds().await?;
            let loader_version = match (&file.loader.version, file.loader.pinned, options.update) {
                (Some(chosen), true, _) | (Some(chosen), false, false) => chosen.clone(),
                _ => fabric::latest_stable_loader(net, &game).await?,
            };
            let profile = fabric::profile(net, &game, &loader_version).await?;
            let resolved = fabric::libraries(net, paths, &profile).await?;
            fabric::launch_profile(profile, &loader_version, resolved)
        }
        Loader::Forge => {
            let setup = forge::Setup {
                net,
                paths,
                instance,
                platform: &platform,
                manifest: &manifest,
                vanilla: &version,
                client_jar: &client_jar,
                java: &java,
                progress,
            };
            let prepared = forge::prepare(&setup, &file, options.update).await?;
            game_jar = prepared.client_jar;
            prepared.profile
        }
    };

    let loader_keys: HashSet<&str> = loader.libraries.iter().map(|l| l.key.as_str()).collect();
    let vanilla = libraries::resolve(&version.libraries, paths, &ctx)?;
    let vanilla_libraries: Vec<_> = vanilla
        .classpath
        .into_iter()
        .filter(|l| !loader_keys.contains(l.key.as_str()))
        .collect();
    let all_libraries = loader.libraries.iter().chain(&vanilla_libraries);
    let loader_downloads: &[_] = if instance.loader == Loader::Forge {
        &[]
    } else {
        &loader.libraries
    };
    stats.add(
        download::ensure_all(
            net,
            loader_downloads
                .iter()
                .chain(&vanilla_libraries)
                .map(|l| l.download.clone())
                .chain(vanilla.natives.iter().map(|n| n.download.clone()))
                .collect(),
            16,
            progress,
            "libraries",
        )
        .await?,
    );
    let mut classpath: Vec<PathBuf> = all_libraries.map(|l| l.path.clone()).collect();
    classpath.push(game_jar);

    let (asset_stats, game_assets) =
        assets::ensure(net, paths, &version.asset_index, &instance.dir, progress).await?;
    stats.add(asset_stats);

    if vanilla.natives.is_empty() {
        fsx::create_dir(&instance.natives_dir()).await?;
    } else {
        natives::extract(&vanilla.natives, &instance.natives_dir()).await?;
    }

    let previous = file.lock();
    let mut mods = if instance.loader == Loader::Fabric {
        let env = Environment {
            game: game.clone(),
            java_major: java
                .version
                .split(['.', '+', '-'])
                .next()
                .unwrap_or_default()
                .to_string(),
            loader: loader.loader_version.clone().unwrap_or_default(),
        };
        fabric_mods(net, instance, &file, &env, options, progress).await?
    } else {
        ModsOutcome::default()
    };
    if instance.loader.has_mods() {
        fsx::create_dir(&instance.mods_dir()).await?;
        let last = previous
            .as_ref()
            .and_then(|l| l.mods.iter().find(|m| m.slug == skins::SKIN_MOD_SLUG));
        let found = match skins::skin_mod(net, instance.loader, &game, last, options.update).await {
            Ok(found) => found,
            Err(e) if is_network(&e) => last.cloned(),
            Err(e) => return Err(e),
        };
        match found {
            Some(skin_mod) => {
                download::ensure_all(
                    net,
                    vec![Download {
                        url: skin_mod.url.clone(),
                        dest: instance.mods_dir().join(jar_name(&skin_mod.filename)?),
                        hash: Hash::sha512(&skin_mod.sha512),
                        size: Some(skin_mod.size),
                        executable: false,
                    }],
                    1,
                    progress,
                    "skin mod",
                )
                .await?;
                mods.mods.push(skin_mod);
            }
            None => {
                let skipped = skins::skin_mod_unavailable(&game);
                progress.notice(&skipped.message);
                mods.unavailable.push(Unavailable {
                    title: skipped.title,
                    message: skipped.message,
                });
            }
        }
    }
    if let Some(previous) = &previous {
        let keep: HashSet<&str> = mods
            .mods
            .iter()
            .map(|m| m.filename.as_str())
            .chain(mods.extras.iter().map(String::as_str))
            .collect();
        for old in previous.filenames().filter(|name| !keep.contains(name)) {
            let path = instance.mods_dir().join(jar_name(old)?);
            if let Err(e) = tokio::fs::remove_file(&path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), error = %e, "could not remove an outdated mod");
            }
        }
    }

    file.loader.version.clone_from(&loader.loader_version);
    file.installed = true;
    file.jars = mods
        .mods
        .iter()
        .map(Jar::from_mod)
        .chain(
            mods.extras
                .iter()
                .filter_map(|name| esteban::by_filename(name))
                .map(|a| Jar::from_artifact(&a)),
        )
        .collect();
    file.skipped = mods
        .unavailable
        .iter()
        .map(|u| Skipped {
            title: u.title.clone(),
            message: u.message.clone(),
        })
        .collect();
    file.resolved_without.clone_from(&mods.disabled);
    instance.write_file(&file).await?;
    let unmanaged = instance.unmanaged_jars(file.lock().as_ref()).await?;

    instance.guard().await?;

    let (_, default_max_heap_mb) = jvm::heap_mb(system::total_memory_bytes());
    Ok(Installed {
        instance: instance.clone(),
        platform,
        version,
        loader,
        classpath,
        java,
        assets_root: paths.assets(),
        game_assets,
        libraries_root: paths.libraries(),
        mods: mods.mods,
        extras: mods.extras,
        unavailable: mods.unavailable,
        disabled: mods.disabled,
        unmanaged,
        default_max_heap_mb,
        stats,
    })
}

#[derive(Default)]
struct ModsOutcome {
    mods: Vec<ResolvedMod>,
    extras: Vec<String>,
    unavailable: Vec<Unavailable>,
    disabled: Vec<String>,
}

async fn fabric_mods(
    net: &Net,
    instance: &Instance,
    file: &InstanceFile,
    env: &Environment,
    options: InstallOptions,
    progress: &dyn Progress,
) -> Result<ModsOutcome> {
    let game = env.game.as_str();
    fsx::create_dir(&instance.mods_dir()).await?;
    let disabled: Vec<String> = file.disabled.iter().cloned().collect();
    let wanted: Vec<&str> = DEFAULT_MODS
        .iter()
        .map(|m| m.slug)
        .filter(|slug| !file.disabled.contains(*slug))
        .collect();
    let off: Vec<&str> = disabled.iter().map(String::as_str).collect();
    let previous = file.lock();
    let previous = previous.map(|mut lock| {
        lock.mods.retain(|m| m.slug != skins::SKIN_MOD_SLUG);
        lock
    });
    let same_choice = |lock: &&ModLock| lock.game_version == game && lock.disabled == disabled;
    let reuse = previous
        .as_ref()
        .filter(|_| !options.update)
        .filter(same_choice);
    let (mut mods, mut unavailable) = match reuse {
        Some(lock) => from_lock(lock),
        None => {
            progress.notice("looking up mods on Modrinth");
            let looked_up = modrinth::resolve(&Modrinth::new(net), &wanted, &off, game).await;
            settle(looked_up, previous.as_ref().filter(same_choice), progress)?
        }
    };
    for skipped in &unavailable {
        progress.notice(&skipped.message);
    }

    let mods_dir = instance.mods_dir();
    let mut mod_downloads = Vec::new();
    for m in &mods {
        mod_downloads.push(Download {
            url: m.url.clone(),
            dest: mods_dir.join(jar_name(&m.filename)?),
            hash: Hash::sha512(&m.sha512),
            size: Some(m.size),
            executable: false,
        });
    }
    let artifacts = instance.extra_artifacts();
    if esteban::hud_for(game).is_none() && crate::versions::is_pinned(game) {
        progress.notice(&format!(
            "The Esteban HUD isn't published for {game} yet, so it's missing."
        ));
    }
    if instance.hacked && esteban::hacks_for(game).is_none() {
        progress.notice(&format!(
            "Esteban isn't published for {game} yet, so the hacks mod is missing."
        ));
    }
    let mut extra_names: Vec<String> = artifacts.iter().map(|a| a.filename.to_string()).collect();
    mod_downloads.extend(instance.extra_downloads());
    download::ensure_all(net, mod_downloads, 8, progress, "mods").await?;

    let managed: Vec<(String, String, String)> = mods
        .iter()
        .map(|m| (m.filename.clone(), m.title.clone(), m.slug.clone()))
        .chain(artifacts.iter().map(|a| {
            (
                a.filename.to_string(),
                a.title().to_string(),
                a.mod_id().to_string(),
            )
        }))
        .collect();
    let mut infos = Vec::new();
    let mut unreadable = false;
    for (file, title, _) in &managed {
        match modcheck::read(&mods_dir.join(jar_name(file)?), title).await? {
            CheckedJar::Mod(info) => infos.push(*info),
            CheckedJar::Plain => {}
            CheckedJar::Unreadable => unreadable = true,
        }
    }
    for problem in modcheck::check(&infos, env, unreadable) {
        let path = mods_dir.join(jar_name(&problem.file)?);
        if let Err(e) = tokio::fs::remove_file(&path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            return Err(e).at(&path);
        }
        let slug = managed
            .iter()
            .find(|(file, _, _)| *file == problem.file)
            .map(|(_, _, slug)| slug.as_str())
            .unwrap_or_default();
        let message = format!("{}{}", problem.message, modrinth::consequence(slug));
        progress.notice(&message);
        mods.retain(|m| m.filename != problem.file);
        extra_names.retain(|n| *n != problem.file);
        unavailable.push(Unavailable {
            title: problem.title,
            message,
        });
    }

    Ok(ModsOutcome {
        mods,
        extras: extra_names,
        unavailable,
        disabled,
    })
}

#[derive(Deserialize)]
struct JarVersion {
    world_version: u32,
}

async fn data_version(client_jar: &Path) -> Result<Option<u32>> {
    let bytes = tokio::fs::read(client_jar).await.at(client_jar)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|source| Error::Zip {
        path: client_jar.to_path_buf(),
        source,
    })?;
    let Ok(mut entry) = archive.by_name("version.json") else {
        return Ok(None);
    };
    let mut text = Vec::new();
    entry.read_to_end(&mut text).at(client_jar)?;
    Ok(serde_json::from_slice::<JarVersion>(&text)
        .ok()
        .map(|v| v.world_version))
}

type Mods = (Vec<ResolvedMod>, Vec<Unavailable>);

fn from_lock(lock: &ModLock) -> Mods {
    (
        lock.mods.clone(),
        lock.skipped
            .iter()
            .map(|s| Unavailable {
                title: s.title.clone(),
                message: s.message.clone(),
            })
            .collect(),
    )
}

fn settle(
    looked_up: Result<modrinth::Resolution>,
    fallback: Option<&ModLock>,
    progress: &dyn Progress,
) -> Result<Mods> {
    match looked_up {
        Ok(resolution) => Ok((resolution.mods, resolution.unavailable)),
        Err(e) if is_network(&e) => match fallback {
            Some(lock) => {
                progress.notice(&format!(
                    "Modrinth didn't answer ({e}), so the mods from last time are kept."
                ));
                Ok(from_lock(lock))
            }
            None => Err(Error::Mods(format!(
                "Couldn't reach Modrinth to look up mods ({e}). Check your connection and try again."
            ))),
        },
        Err(e) => Err(e),
    }
}

fn is_network(error: &Error) -> bool {
    matches!(
        error,
        Error::Http { .. } | Error::Status { .. } | Error::Json { .. }
    )
}

fn jar_name(name: &str) -> Result<&str> {
    let ok = name.ends_with(".jar")
        && !name.contains(['/', '\\'])
        && !name.starts_with('.')
        && name.len() <= 200;
    if ok {
        Ok(name)
    } else {
        Err(Error::Unsupported(format!(
            "refusing an unsafe mod file name: {name}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_file_names_cannot_escape_the_mods_folder() {
        assert!(jar_name("sodium-fabric-0.6.13+mc1.21.4.jar").is_ok());
        assert!(jar_name("../../evil.jar").is_err());
        assert!(jar_name("a\\b.jar").is_err());
        assert!(jar_name("..jar").is_err());
        assert!(jar_name("readme.txt").is_err());
    }

    fn down() -> Result<modrinth::Resolution> {
        Err(Error::Status {
            url: "https://api.modrinth.com/v2/projects".into(),
            status: 503,
        })
    }

    #[test]
    fn modrinth_down_keeps_the_last_mods() {
        let lock = ModLock {
            game_version: "1.21.4".into(),
            mods: Vec::new(),
            extras: Vec::new(),
            skipped: vec![Skipped {
                title: "Iris".into(),
                message: "kept".into(),
            }],
            disabled: Vec::new(),
        };
        let (_, unavailable) = settle(down(), Some(&lock), &crate::progress::Silent).unwrap();
        assert_eq!(unavailable[0].message, "kept");
    }

    #[test]
    fn modrinth_down_on_a_first_install_says_so_plainly() {
        match settle(down(), None, &crate::progress::Silent) {
            Err(Error::Mods(message)) => {
                assert!(message.starts_with("Couldn't reach Modrinth to look up mods"));
            }
            other => panic!("expected a plain mods error, got {:?}", other.map(|_| ())),
        }
    }

    #[test]
    fn other_failures_are_not_hidden_by_the_fallback() {
        let broken = Err(Error::Unsupported("x".into()));
        assert!(matches!(
            settle(broken, None, &crate::progress::Silent),
            Err(Error::Unsupported(_))
        ));
    }

    fn jar_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            for (name, data) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(data).unwrap();
            }
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    #[tokio::test]
    async fn the_data_version_comes_from_the_client_jar() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("1.21.4.jar");
        std::fs::write(
            &jar,
            jar_with(&[("version.json", br#"{"id":"1.21.4","world_version":4189}"#)]),
        )
        .unwrap();
        assert_eq!(data_version(&jar).await.unwrap(), Some(4189));
        let bare = dir.path().join("bare.jar");
        std::fs::write(&bare, jar_with(&[("a.class", b"x")])).unwrap();
        assert_eq!(data_version(&bare).await.unwrap(), None);
    }

    #[tokio::test]
    async fn hacks_need_explicit_acceptance_first() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let net = Net::launcher().unwrap();
        let instance = Instance::new(&paths, "1.21.4", Loader::Fabric, true).unwrap();
        let result = install(
            &net,
            &paths,
            &instance,
            InstallOptions::default(),
            &crate::progress::Silent,
        )
        .await;
        match result {
            Err(Error::Guard(message)) => assert!(message.contains(HACKS_WARNING)),
            other => panic!("expected the hacks warning, got {:?}", other.map(|_| ())),
        }
    }
}
