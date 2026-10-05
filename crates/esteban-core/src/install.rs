use std::collections::{BTreeSet, HashSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::download::{self, Download, Stats};
use crate::error::IoContext;
use crate::esteban::HACKS_MOD_ID;
use crate::fabric::{self, FabricProfile};
use crate::hash::Hash;
use crate::java::{self, JavaRuntime};
use crate::launch::jvm;
use crate::modcheck::{self, Environment, Jar};
use crate::modrinth::lock::Skipped;
use crate::modrinth::{self, DEFAULT_MODS, ModLock, Modrinth, ResolvedMod, Unavailable};
use crate::mojang::manifest::VersionManifest;
use crate::mojang::rules::Context;
use crate::mojang::version::VersionJson;
use crate::mojang::{assets, libraries};
use crate::net::Net;
use crate::paths::Paths;
use crate::profile::{HACKS_WARNING, Instance, NEW_INSTANCE_OPTIONS, ProfileKind, Settings};
use crate::progress::Progress;
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
    pub fabric: FabricProfile,
    pub loader_version: String,
    pub classpath: Vec<PathBuf>,
    pub java: JavaRuntime,
    pub assets_root: PathBuf,
    pub libraries_root: PathBuf,
    pub mods: Vec<ResolvedMod>,
    pub extras: Vec<String>,
    pub unavailable: Vec<Unavailable>,
    pub disabled: Vec<String>,
    pub unmanaged: Vec<String>,
    pub default_max_heap_mb: u64,
    pub stats: Stats,
}

pub async fn install(
    net: &Net,
    paths: &Paths,
    kind: ProfileKind,
    game_version: &str,
    options: InstallOptions,
    progress: &dyn Progress,
) -> Result<Installed> {
    if kind == ProfileKind::Hacks && !Settings::load(paths).await?.hacks_warning_accepted {
        return Err(Error::Guard(format!(
            "The hacks profile has not been accepted yet. {HACKS_WARNING}"
        )));
    }
    let platform = Platform::current()?;
    let instance = Instance::new(paths, kind, game_version)?;
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
    if version.minecraft_arguments.is_some() || version.arguments.is_none() {
        return Err(Error::Unsupported(format!(
            "{game} uses the old launch format, which this launcher does not support"
        )));
    }
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
                        "New instance: GUI scale starts at 2. Change it in game under Options, Video Settings.",
                    );
                }
            }
            None => {
                tracing::warn!(jar = %client_jar.display(), "the client jar has no data version, leaving options.txt to the game");
            }
        }
    }

    let mut meta = instance.read_meta().await?;
    let loader_version = match (&meta.loader_version, options.update) {
        (Some(pinned), false) => pinned.clone(),
        _ => fabric::latest_stable_loader(net, &game).await?,
    };
    let fabric_profile = fabric::profile(net, &game, &loader_version).await?;
    let fabric_libraries = fabric::libraries(net, paths, &fabric_profile).await?;
    let fabric_keys: HashSet<&str> = fabric_libraries.iter().map(|l| l.key.as_str()).collect();
    let vanilla_libraries: Vec<_> = libraries::resolve(&version.libraries, paths, &ctx)?
        .into_iter()
        .filter(|l| !fabric_keys.contains(l.key.as_str()))
        .collect();
    let all_libraries = fabric_libraries.iter().chain(&vanilla_libraries);
    stats.add(
        download::ensure_all(
            net,
            all_libraries.clone().map(|l| l.download.clone()).collect(),
            16,
            progress,
            "libraries",
        )
        .await?,
    );
    let mut classpath: Vec<PathBuf> = all_libraries.map(|l| l.path.clone()).collect();
    classpath.push(client_jar);

    stats.add(assets::ensure(net, paths, &version.asset_index, progress).await?);

    let (java, java_stats) = java::ensure(
        net,
        paths,
        &platform,
        &version.java_version.component,
        progress,
    )
    .await?;
    stats.add(java_stats);

    fsx::create_dir(&instance.mods_dir()).await?;
    fsx::create_dir(&instance.natives_dir()).await?;

    let disabled: Vec<String> = meta.disabled.iter().cloned().collect();
    let wanted: Vec<&str> = DEFAULT_MODS
        .iter()
        .map(|m| m.slug)
        .filter(|slug| !meta.disabled.contains(*slug))
        .collect();
    let off: Vec<&str> = disabled.iter().map(String::as_str).collect();
    let previous = instance.read_lock().await?;
    let same_choice = |lock: &&ModLock| lock.game_version == game && lock.disabled == disabled;
    let reuse = previous
        .as_ref()
        .filter(|_| !options.update)
        .filter(same_choice);
    let (mut mods, mut unavailable) = match reuse {
        Some(lock) => from_lock(lock),
        None => {
            progress.notice("looking up mods on Modrinth");
            let looked_up = modrinth::resolve(&Modrinth::new(net), &wanted, &off, &game).await;
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
    let extras = instance.extra_downloads();
    if kind == ProfileKind::Hacks && extras.is_empty() {
        progress.notice(&format!(
            "Esteban isn't published for {game} yet, so the hacks mod is missing."
        ));
    }
    let mut extra_names: Vec<String> = extras
        .iter()
        .filter_map(|d| d.dest.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    mod_downloads.extend(extras);
    stats.add(download::ensure_all(net, mod_downloads, 8, progress, "mods").await?);

    let env = Environment {
        game: game.clone(),
        java_major: java
            .version
            .split(['.', '+', '-'])
            .next()
            .unwrap_or_default()
            .to_string(),
        loader: loader_version.clone(),
    };
    let managed: Vec<(String, String, String)> = mods
        .iter()
        .map(|m| (m.filename.clone(), m.title.clone(), m.slug.clone()))
        .chain(
            extra_names
                .iter()
                .map(|n| (n.clone(), "Esteban".to_string(), HACKS_MOD_ID.to_string())),
        )
        .collect();
    let mut infos = Vec::new();
    let mut unreadable = false;
    for (file, title, _) in &managed {
        match modcheck::read(&mods_dir.join(jar_name(file)?), title).await? {
            Jar::Mod(info) => infos.push(*info),
            Jar::Plain => {}
            Jar::Unreadable => unreadable = true,
        }
    }
    for problem in modcheck::check(&infos, &env, unreadable) {
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

    let lock = ModLock {
        game_version: game.clone(),
        mods: mods.clone(),
        extras: extra_names.clone(),
        skipped: unavailable
            .iter()
            .map(|u| Skipped {
                title: u.title.clone(),
                message: u.message.clone(),
            })
            .collect(),
        disabled: disabled.clone(),
    };
    if let Some(previous) = &previous {
        let keep: HashSet<&str> = lock.filenames().collect();
        for old in previous.filenames().filter(|name| !keep.contains(name)) {
            let path = mods_dir.join(jar_name(old)?);
            if let Err(e) = tokio::fs::remove_file(&path).await
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), error = %e, "could not remove an outdated mod");
            }
        }
    }
    lock.write(&instance.lock_path()).await?;
    let unmanaged = instance.unmanaged_jars(Some(&lock)).await?;
    meta.loader_version = Some(loader_version.clone());
    instance.write_meta(&meta).await?;

    instance.guard().await?;

    let (_, default_max_heap_mb) = jvm::heap_mb(system::total_memory_bytes());
    Ok(Installed {
        instance,
        platform,
        version,
        fabric: fabric_profile,
        loader_version,
        classpath,
        java,
        assets_root: paths.assets(),
        libraries_root: paths.libraries(),
        mods,
        extras: extra_names,
        unavailable,
        disabled,
        unmanaged,
        default_max_heap_mb,
        stats,
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
        let result = install(
            &net,
            &paths,
            ProfileKind::Hacks,
            "1.21.4",
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
