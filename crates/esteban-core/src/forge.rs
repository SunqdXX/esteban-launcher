use std::collections::{BTreeMap, HashMap};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::catalog;
use crate::download::{self, Download};
use crate::error::IoContext;
use crate::fabric::token;
use crate::hash::Hash;
use crate::instance::{Instance, InstanceFile};
use crate::java::JavaRuntime;
use crate::loader::{LaunchProfile, Loader};
use crate::mojang::libraries::{ResolvedLibrary, library_key, maven_path};
use crate::mojang::manifest::VersionManifest;
use crate::mojang::version::{Arguments, Library, VersionJson};
use crate::net::Net;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::system::Platform;
use crate::{Error, Result, fsx};

pub const MAVEN: &str = "https://maven.minecraftforge.net/";
const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";
const PROCESSOR_TIMEOUT: Duration = Duration::from_secs(900);

pub struct Setup<'a> {
    pub net: &'a Net,
    pub paths: &'a Paths,
    pub instance: &'a Instance,
    pub platform: &'a Platform,
    pub manifest: &'a VersionManifest,
    pub vanilla: &'a VersionJson,
    pub client_jar: &'a Path,
    pub java: &'a JavaRuntime,
    pub progress: &'a dyn Progress,
}

pub struct Prepared {
    pub profile: LaunchProfile,
    pub client_jar: PathBuf,
}

pub fn allowed(manifest: &VersionManifest, mc: &str) -> Result<()> {
    let releases: Vec<&str> = manifest
        .versions
        .iter()
        .filter(|v| v.kind == "release")
        .map(|v| v.id.as_str())
        .collect();
    let at = |id: &str| releases.iter().position(|r| *r == id);
    let (Some(here), Some(first), Some(last)) =
        (at(mc), at(catalog::FORGE_FIRST), at(catalog::FORGE_LAST))
    else {
        return Err(Error::Forge(format!("Forge isn't offered for {mc}.")));
    };
    if here < last {
        return Err(Error::Forge(format!(
            "Forge stops at {} here, so {mc} can use Vanilla or Fabric.",
            catalog::FORGE_LAST
        )));
    }
    if here > first {
        return Err(Error::Forge(format!(
            "Forge starts at {}, so {mc} is Vanilla only.",
            catalog::FORGE_FIRST
        )));
    }
    Ok(())
}

pub async fn prepare(setup: &Setup<'_>, file: &InstanceFile, update: bool) -> Result<Prepared> {
    let mc = setup.instance.game_version.as_str();
    allowed(setup.manifest, mc)?;
    let version = match (&file.loader.version, file.loader.pinned, update) {
        (Some(chosen), true, _) | (Some(chosen), false, false) => chosen.clone(),
        _ => catalog::loader_versions(setup.net, setup.paths, mc, Loader::Forge)
            .await?
            .default
            .ok_or_else(|| Error::Forge(format!("Forge has no build for {mc}.")))?,
    };
    let full = format!("{mc}-{}", token(&version)?);
    token(&full)?;
    let installer = fetch_installer(setup, &full).await?;
    let bytes = tokio::fs::read(&installer).await.at(&installer)?;
    let mut zip = Zip::open(bytes, &installer)?;
    let profile: serde_json::Value = crate::error::json(
        &zip.read("install_profile.json")?,
        "Forge's install profile",
    )?;
    let prepared = if profile.get("spec").is_some() {
        modern(setup, &mut zip, &installer, profile, &version).await?
    } else if profile.get("versionInfo").is_some() {
        legacy(setup, &mut zip, profile, &version).await?
    } else {
        return Err(Error::Forge(format!(
            "The Forge installer for {full} has a layout this launcher doesn't know."
        )));
    };
    Ok(prepared)
}

async fn fetch_installer(setup: &Setup<'_>, full: &str) -> Result<PathBuf> {
    let relative = format!("net/minecraftforge/forge/{full}/forge-{full}-installer.jar");
    let url = format!("{MAVEN}{relative}");
    let hash = sidecar(setup.net, &url).await?;
    let dest = setup.paths.libraries().join(&relative);
    download::ensure_all(
        setup.net,
        vec![Download {
            url,
            dest: dest.clone(),
            hash,
            size: None,
            executable: false,
        }],
        1,
        setup.progress,
        "Forge installer",
    )
    .await?;
    Ok(dest)
}

async fn sidecar(net: &Net, url: &str) -> Result<Hash> {
    for (extension, length) in [("sha512", 128), ("sha256", 64), ("sha1", 40)] {
        let Ok(text) = net.bytes(&format!("{url}.{extension}")).await else {
            continue;
        };
        let text = String::from_utf8_lossy(&text);
        let Some(first) = text.split_whitespace().next() else {
            continue;
        };
        if first.len() == length && first.chars().all(|c| c.is_ascii_hexdigit()) {
            return Ok(match extension {
                "sha512" => Hash::sha512(first),
                "sha256" => Hash::sha256(first),
                _ => Hash::sha1(first),
            });
        }
    }
    Err(Error::Forge(format!(
        "{url} has no checksum published next to it, so it can't be checked and isn't used."
    )))
}

struct Zip {
    archive: zip::ZipArchive<Cursor<Vec<u8>>>,
    path: PathBuf,
}

impl Zip {
    fn open(bytes: Vec<u8>, path: &Path) -> Result<Self> {
        let archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|source| Error::Zip {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(Self {
            archive,
            path: path.to_path_buf(),
        })
    }

    fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let name = name.trim_start_matches('/');
        let mut entry = self.archive.by_name(name).map_err(|source| Error::Zip {
            path: self.path.clone(),
            source,
        })?;
        let mut out = Vec::new();
        entry.read_to_end(&mut out).at(&self.path)?;
        Ok(out)
    }

    fn has(&self, name: &str) -> bool {
        self.archive
            .index_for_name(name.trim_start_matches('/'))
            .is_some()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeVersion {
    id: String,
    main_class: String,
    #[serde(default)]
    arguments: Option<Arguments>,
    #[serde(default)]
    minecraft_arguments: Option<String>,
    #[serde(default)]
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
struct ModernProfile {
    #[serde(default)]
    json: Option<String>,
    #[serde(default)]
    data: BTreeMap<String, SideValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
}

#[derive(Debug, Deserialize)]
struct SideValue {
    client: String,
}

#[derive(Debug, Deserialize)]
struct Processor {
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: BTreeMap<String, String>,
    #[serde(default)]
    sides: Option<Vec<String>>,
}

async fn modern(
    setup: &Setup<'_>,
    zip: &mut Zip,
    installer: &Path,
    profile: serde_json::Value,
    version: &str,
) -> Result<Prepared> {
    let profile: ModernProfile = serde_json::from_value(profile).map_err(|source| Error::Json {
        what: "Forge's install profile".into(),
        source,
    })?;
    let json_name = profile.json.as_deref().unwrap_or("/version.json");
    let forge: ForgeVersion = crate::error::json(&zip.read(json_name)?, "Forge's version file")?;
    token(&forge.id)?;

    let libraries_root = setup.paths.libraries();
    let mut downloads = Vec::new();
    for library in profile.libraries.iter().chain(&forge.libraries) {
        if let Some(download) = library_download(zip, library, &libraries_root).await? {
            downloads.push(download);
        }
    }
    download::ensure_all(setup.net, downloads, 16, setup.progress, "Forge libraries").await?;

    let work = setup.paths.cache().join("forge").join(&forge.id);
    let mut data: HashMap<String, String> = HashMap::new();
    data.insert("SIDE".into(), "client".into());
    data.insert(
        "MINECRAFT_JAR".into(),
        setup.client_jar.to_string_lossy().into_owned(),
    );
    data.insert(
        "MINECRAFT_VERSION".into(),
        setup.instance.game_version.clone(),
    );
    data.insert("ROOT".into(), work.to_string_lossy().into_owned());
    data.insert("INSTALLER".into(), installer.to_string_lossy().into_owned());
    data.insert(
        "LIBRARY_DIR".into(),
        libraries_root.to_string_lossy().into_owned(),
    );
    for (key, value) in &profile.data {
        let resolved = data_value(zip, &value.client, &work, &libraries_root).await?;
        data.insert(key.clone(), resolved);
    }

    let expected = expected_outputs(&profile.data, &data);
    if all_match(&expected).await? {
        tracing::debug!(id = %forge.id, "Forge's patched files are already in place");
    } else {
        let steps: Vec<&Processor> = profile
            .processors
            .iter()
            .filter(|p| {
                p.sides
                    .as_ref()
                    .is_none_or(|s| s.iter().any(|side| side == "client"))
            })
            .collect();
        for (index, processor) in steps.iter().enumerate() {
            setup.progress.notice(&format!(
                "setting up Forge, step {} of {}",
                index + 1,
                steps.len()
            ));
            run_processor(setup, processor, &data, &libraries_root, &work).await?;
        }
        for (path, sha1) in &expected {
            check_sha1(path, sha1).await?;
        }
    }

    let runtime = runtime_libraries(&forge.libraries, &libraries_root)?;
    let client_jar = named_client_jar(setup, &forge.id).await?;
    Ok(Prepared {
        profile: LaunchProfile {
            loader: Loader::Forge,
            loader_version: Some(version.to_string()),
            version_name: forge.id,
            main_class: forge.main_class,
            jvm: forge
                .arguments
                .as_ref()
                .map(|a| a.jvm.clone())
                .unwrap_or_default(),
            game: forge
                .arguments
                .as_ref()
                .map(|a| a.game.clone())
                .unwrap_or_default(),
            legacy_game: forge.minecraft_arguments,
            libraries: runtime,
        },
        client_jar,
    })
}

async fn library_download(
    zip: &mut Zip,
    library: &Library,
    libraries_root: &Path,
) -> Result<Option<Download>> {
    let Some(artifact) = library.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
        return Ok(None);
    };
    let path = libraries_root.join(&artifact.path);
    if artifact.url.is_empty() {
        let inside = format!("maven/{}", artifact.path);
        if zip.has(&inside) {
            let bytes = zip.read(&inside)?;
            if !artifact.sha1.is_empty() {
                Hash::sha1(&artifact.sha1).verify(&bytes, &library.name)?;
            }
            fsx::write_atomic(&path, &bytes).await?;
        }
        return Ok(None);
    }
    if artifact.sha1.is_empty() {
        return Err(Error::Forge(format!(
            "Forge lists {} without a checksum.",
            library.name
        )));
    }
    Ok(Some(Download {
        url: artifact.url.clone(),
        dest: path,
        hash: Hash::sha1(&artifact.sha1),
        size: (artifact.size > 0).then_some(artifact.size),
        executable: false,
    }))
}

async fn data_value(
    zip: &mut Zip,
    raw: &str,
    work: &Path,
    libraries_root: &Path,
) -> Result<String> {
    if let Some(coordinate) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Ok(libraries_root
            .join(maven_path(coordinate)?)
            .to_string_lossy()
            .into_owned());
    }
    if let Some(literal) = raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        return Ok(literal.to_string());
    }
    if raw.starts_with('/') {
        let relative = raw.trim_start_matches('/');
        let safe = !relative.is_empty()
            && Path::new(relative)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !safe {
            return Err(Error::Forge(format!(
                "Forge's installer names an unsafe file: {raw}"
            )));
        }
        let target = work.join(relative);
        let bytes = zip.read(relative)?;
        fsx::write_atomic(&target, &bytes).await?;
        return Ok(target.to_string_lossy().into_owned());
    }
    Ok(raw.to_string())
}

fn substitute(arg: &str, data: &HashMap<String, String>, libraries_root: &Path) -> Result<String> {
    if let Some(coordinate) = arg.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Ok(libraries_root
            .join(maven_path(coordinate)?)
            .to_string_lossy()
            .into_owned());
    }
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let key = &after[..end];
                match data.get(key) {
                    Some(value) => out.push_str(value),
                    None => {
                        return Err(Error::Forge(format!(
                            "Forge's setup asks for {{{key}}}, which its installer doesn't define."
                        )));
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    Ok(out)
}

fn expected_outputs(
    raw: &BTreeMap<String, SideValue>,
    data: &HashMap<String, String>,
) -> Vec<(PathBuf, String)> {
    raw.keys()
        .filter_map(|key| {
            let base = key.strip_suffix("_SHA")?;
            let sha = data.get(key)?;
            let path = data.get(base)?;
            (sha.len() == 40).then(|| (PathBuf::from(path), sha.to_ascii_lowercase()))
        })
        .collect()
}

async fn all_match(expected: &[(PathBuf, String)]) -> Result<bool> {
    if expected.is_empty() {
        return Ok(false);
    }
    for (path, sha1) in expected {
        match tokio::fs::read(path).await {
            Ok(bytes) if Hash::sha1(sha1).digest(&bytes) == *sha1 => {}
            Ok(_) => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e).at(path),
        }
    }
    Ok(true)
}

async fn check_sha1(path: &Path, sha1: &str) -> Result<()> {
    let bytes = tokio::fs::read(path).await.at(path)?;
    Hash::sha1(sha1).verify(&bytes, &path.display().to_string())
}

fn main_class(jar: &Path) -> Result<String> {
    let bytes = std::fs::read(jar).at(jar)?;
    let mut zip = Zip::open(bytes, jar)?;
    let manifest = String::from_utf8_lossy(&zip.read("META-INF/MANIFEST.MF")?).into_owned();
    let mut joined = String::new();
    for line in manifest.lines() {
        match line.strip_prefix(' ') {
            Some(continued) => joined.push_str(continued),
            None => {
                joined.push('\n');
                joined.push_str(line);
            }
        }
    }
    joined
        .lines()
        .find_map(|l| l.strip_prefix("Main-Class:"))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .ok_or_else(|| Error::Forge(format!("{} has no Main-Class", jar.display())))
}

async fn run_processor(
    setup: &Setup<'_>,
    processor: &Processor,
    data: &HashMap<String, String>,
    libraries_root: &Path,
    work: &Path,
) -> Result<()> {
    let jar = libraries_root.join(maven_path(&processor.jar)?);
    let outputs: Vec<(PathBuf, String)> = processor
        .outputs
        .iter()
        .map(|(path, sha)| {
            Ok((
                PathBuf::from(substitute(path, data, libraries_root)?),
                substitute(sha, data, libraries_root)?.to_ascii_lowercase(),
            ))
        })
        .collect::<Result<_>>()?;
    if all_match(&outputs).await? {
        return Ok(());
    }
    let main = main_class(&jar)?;
    let mut classpath = vec![jar.to_string_lossy().into_owned()];
    for entry in &processor.classpath {
        classpath.push(
            libraries_root
                .join(maven_path(entry)?)
                .to_string_lossy()
                .into_owned(),
        );
    }
    let args: Vec<String> = processor
        .args
        .iter()
        .map(|a| substitute(a, data, libraries_root))
        .collect::<Result<_>>()?;
    fsx::create_dir(work).await?;
    let mut command = tokio::process::Command::new(&setup.java.executable);
    command
        .arg("-cp")
        .arg(classpath.join(setup.platform.classpath_separator()))
        .arg(&main)
        .args(&args)
        .current_dir(work)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(PROCESSOR_TIMEOUT, command.output())
        .await
        .map_err(|_| Error::Forge(format!("Forge's {} step took too long.", processor.jar)))?
        .at(&setup.java.executable)?;
    if !output.status.success() {
        let text = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = text.lines().rev().take(6).collect();
        return Err(Error::Forge(format!(
            "Forge's {} step failed: {}",
            processor.jar,
            tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
        )));
    }
    for (path, sha1) in &outputs {
        check_sha1(path, sha1).await?;
    }
    Ok(())
}

fn runtime_libraries(libraries: &[Library], libraries_root: &Path) -> Result<Vec<ResolvedLibrary>> {
    let mut out = Vec::new();
    for library in libraries {
        let Some(artifact) = library.downloads.as_ref().and_then(|d| d.artifact.as_ref()) else {
            continue;
        };
        let path = libraries_root.join(&artifact.path);
        out.push(ResolvedLibrary {
            key: library_key(&library.name)?,
            path: path.clone(),
            download: Download {
                url: artifact.url.clone(),
                dest: path,
                hash: Hash::sha1(&artifact.sha1),
                size: (artifact.size > 0).then_some(artifact.size),
                executable: false,
            },
        });
    }
    Ok(out)
}

async fn named_client_jar(setup: &Setup<'_>, id: &str) -> Result<PathBuf> {
    let dir = setup.paths.version_dir(token(id)?);
    let target = dir.join(format!("{id}.jar"));
    let source = tokio::fs::read(setup.client_jar)
        .await
        .at(setup.client_jar)?;
    let same = match tokio::fs::read(&target).await {
        Ok(existing) => existing == source,
        Err(_) => false,
    };
    if !same {
        fsx::write_atomic(&target, &source).await?;
    }
    Ok(target)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyProfile {
    install: LegacyInstall,
    version_info: LegacyVersionInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyInstall {
    path: String,
    file_path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyVersionInfo {
    id: String,
    main_class: String,
    minecraft_arguments: String,
    #[serde(default)]
    libraries: Vec<LegacyLibrary>,
}

#[derive(Debug, Deserialize)]
struct LegacyLibrary {
    name: String,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    checksums: Vec<String>,
    #[serde(default)]
    natives: Option<serde_json::Value>,
}

async fn legacy(
    setup: &Setup<'_>,
    zip: &mut Zip,
    profile: serde_json::Value,
    version: &str,
) -> Result<Prepared> {
    let profile: LegacyProfile = serde_json::from_value(profile).map_err(|source| Error::Json {
        what: "Forge's install profile".into(),
        source,
    })?;
    let info = profile.version_info;
    token(&info.id)?;
    let libraries_root = setup.paths.libraries();
    let forge_path = libraries_root.join(maven_path(&profile.install.path)?);
    let forge_jar = zip.read(&profile.install.file_path)?;
    fsx::write_atomic(&forge_path, &forge_jar).await?;

    let vanilla_hashes: HashMap<String, String> = setup
        .vanilla
        .libraries
        .iter()
        .filter_map(|l| {
            let artifact = l.downloads.as_ref()?.artifact.as_ref()?;
            Some((artifact.path.clone(), artifact.sha1.clone()))
        })
        .collect();

    let mut runtime = Vec::new();
    let mut downloads = Vec::new();
    for library in &info.libraries {
        if library.natives.is_some() {
            continue;
        }
        let relative = maven_path(&library.name)?;
        let path = libraries_root.join(&relative);
        runtime.push(ResolvedLibrary {
            key: library_key(&library.name)?,
            path: path.clone(),
            download: Download {
                url: String::new(),
                dest: path.clone(),
                hash: Hash::sha1(""),
                size: None,
                executable: false,
            },
        });
        if library.name == profile.install.path {
            continue;
        }
        let base = library
            .url
            .as_deref()
            .filter(|u| !u.is_empty())
            .map(https_maven)
            .unwrap_or_else(|| MOJANG_LIBRARIES.to_string());
        let url = format!("{}/{relative}", base.trim_end_matches('/'));
        let hash = match vanilla_hashes.get(&relative) {
            Some(sha1) => Hash::sha1(sha1),
            None => legacy_hash(setup.net, &url, &library.checksums).await?,
        };
        downloads.push(Download {
            url,
            dest: path,
            hash,
            size: None,
            executable: false,
        });
    }
    download::ensure_all(setup.net, downloads, 16, setup.progress, "Forge libraries").await?;
    let client_jar = named_client_jar(setup, &info.id).await?;
    Ok(Prepared {
        profile: LaunchProfile {
            loader: Loader::Forge,
            loader_version: Some(version.to_string()),
            version_name: info.id,
            main_class: info.main_class,
            jvm: Vec::new(),
            game: Vec::new(),
            legacy_game: Some(info.minecraft_arguments),
            libraries: runtime,
        },
        client_jar,
    })
}

fn https_maven(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    match trimmed {
        "http://files.minecraftforge.net/maven" | "https://files.minecraftforge.net/maven" => {
            MAVEN.trim_end_matches('/').to_string()
        }
        other => other.replacen("http://", "https://", 1),
    }
}

async fn legacy_hash(net: &Net, url: &str, listed: &[String]) -> Result<Hash> {
    let listed: Vec<String> = listed
        .iter()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| c.len() == 40 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
        .collect();
    if !listed.is_empty() {
        return Ok(Hash::sha1_any(&listed));
    }
    let text = net.bytes(&format!("{url}.sha1")).await.map_err(|_| {
        Error::Forge(format!(
            "{url} has no checksum, from Forge or next to the file, so it isn't used."
        ))
    })?;
    let text = String::from_utf8_lossy(&text).to_ascii_lowercase();
    let published = text
        .split_whitespace()
        .next()
        .filter(|h| h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| Error::Forge(format!("{url} has a broken checksum file.")))?;
    Ok(Hash::sha1(published))
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn manifest(ids: &[&str]) -> VersionManifest {
        let versions: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| serde_json::json!({"id": id, "type": "release", "url": "https://piston-meta.mojang.com/x.json", "sha1": "00", "releaseTime": "2020-01-01T00:00:00+00:00"}))
            .collect();
        serde_json::from_value(
            serde_json::json!({"latest": {"release": ids[0], "snapshot": "x"}, "versions": versions}),
        )
        .unwrap()
    }

    #[test]
    fn forge_is_only_allowed_from_1_6_1_to_1_20_1() {
        let m = manifest(&["1.21.4", "1.20.2", "1.20.1", "1.12.2", "1.6.1", "1.5.2"]);
        assert!(allowed(&m, "1.20.1").is_ok());
        assert!(allowed(&m, "1.12.2").is_ok());
        assert!(allowed(&m, "1.6.1").is_ok());
        assert!(
            matches!(allowed(&m, "1.20.2"), Err(Error::Forge(t)) if t.contains("stops at 1.20.1"))
        );
        assert!(
            matches!(allowed(&m, "1.5.2"), Err(Error::Forge(t)) if t.contains("starts at 1.6.1"))
        );
        assert!(allowed(&m, "9.9").is_err());
    }

    #[test]
    fn processor_args_fill_in_data_and_maven_paths() {
        let root = Path::new("/libs");
        let data = HashMap::from([
            ("SIDE".to_string(), "client".to_string()),
            ("MAPPINGS".to_string(), "/libs/m.txt".to_string()),
        ]);
        assert_eq!(substitute("{SIDE}", &data, root).unwrap(), "client");
        assert_eq!(
            substitute("--output={MAPPINGS}", &data, root).unwrap(),
            "--output=/libs/m.txt"
        );
        assert_eq!(
            substitute(
                "[de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip]",
                &data,
                root
            )
            .unwrap(),
            "/libs/de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip"
        );
        assert_eq!(substitute("--plain", &data, root).unwrap(), "--plain");
        assert!(substitute("{NOPE}", &data, root).is_err());
    }

    #[test]
    fn sha_keys_pair_with_their_files() {
        let raw: BTreeMap<String, SideValue> = [
            ("PATCHED", "[x]"),
            ("PATCHED_SHA", "'x'"),
            ("MC_SLIM", "[y]"),
            ("MC_SLIM_SHA", "'y'"),
            ("BROKEN_SHA", "'z'"),
        ]
        .iter()
        .map(|(k, v)| {
            (
                (*k).to_string(),
                SideValue {
                    client: (*v).to_string(),
                },
            )
        })
        .collect();
        let sha = "a".repeat(40);
        let data = HashMap::from([
            ("PATCHED".to_string(), "/l/patched.jar".to_string()),
            ("PATCHED_SHA".to_string(), sha.to_uppercase()),
            ("MC_SLIM".to_string(), "/l/slim.jar".to_string()),
            ("MC_SLIM_SHA".to_string(), "short".to_string()),
            ("BROKEN_SHA".to_string(), sha.clone()),
        ]);
        assert_eq!(
            expected_outputs(&raw, &data),
            vec![(PathBuf::from("/l/patched.jar"), sha)]
        );
    }

    #[test]
    fn main_class_is_read_from_a_folded_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("tool.jar");
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            writer
                .start_file(
                    "META-INF/MANIFEST.MF",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer
                .write_all(b"Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.binarypatcher.Consol\r\n eTool\r\n\r\n")
                .unwrap();
            writer.finish().unwrap();
        }
        std::fs::write(&jar, out.into_inner()).unwrap();
        assert_eq!(
            main_class(&jar).unwrap(),
            "net.minecraftforge.binarypatcher.ConsoleTool"
        );
    }

    #[test]
    fn old_forge_urls_become_https_on_the_current_maven() {
        assert_eq!(
            https_maven("http://files.minecraftforge.net/maven/"),
            "https://maven.minecraftforge.net"
        );
        assert_eq!(
            https_maven("https://maven.minecraftforge.net/"),
            "https://maven.minecraftforge.net"
        );
        assert_eq!(
            https_maven("http://libraries.minecraft.net/"),
            "https://libraries.minecraft.net"
        );
    }

    #[test]
    fn both_installer_layouts_parse() {
        let legacy: LegacyProfile = serde_json::from_str(
            r#"{"install":{"path":"net.minecraftforge:forge:1.8.9-11.15.1.2318-1.8.9","filePath":"forge-1.8.9-11.15.1.2318-1.8.9-universal.jar","minecraft":"1.8.9"},
                "versionInfo":{"id":"1.8.9-forge1.8.9-11.15.1.2318-1.8.9","mainClass":"net.minecraft.launchwrapper.Launch",
                "minecraftArguments":"--username ${auth_player_name} --tweakClass net.minecraftforge.fml.common.launcher.FMLTweaker",
                "libraries":[{"name":"net.minecraft:launchwrapper:1.12"},{"name":"org.scala-lang:scala-library:2.11.1","url":"https://maven.minecraftforge.net/","checksums":["aa"],"clientreq":true,"serverreq":true}]}}"#,
        )
        .unwrap();
        assert_eq!(legacy.version_info.libraries.len(), 2);
        let modern: ModernProfile = serde_json::from_str(
            r#"{"spec":1,"json":"/version.json","data":{"SIDE_X":{"client":"'a'","server":"'b'"}},
                "processors":[{"sides":["server"],"jar":"a:b:1","args":[]},{"jar":"c:d:1","classpath":["e:f:1"],"args":["--x","{SIDE}"],"outputs":{"{P}":"{P_SHA}"}}],
                "libraries":[]}"#,
        )
        .unwrap();
        assert_eq!(modern.processors.len(), 2);
        assert_eq!(modern.data["SIDE_X"].client, "'a'");
    }
}
