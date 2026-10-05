use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use crate::download::{self, Download, Stats};
use crate::fsx;
use crate::hash::Hash;
use crate::net::Net;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::system::Platform;
use crate::{Error, Result};

pub const RUNTIME_INDEX: &str = "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

type RuntimeIndex = BTreeMap<String, BTreeMap<String, Vec<RuntimeEntry>>>;

#[derive(Debug, Deserialize)]
struct RuntimeEntry {
    manifest: ManifestRef,
    version: RuntimeVersion,
}

#[derive(Debug, Deserialize)]
struct ManifestRef {
    sha1: String,
    url: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeVersion {
    name: String,
}

#[derive(Debug, Deserialize)]
struct RuntimeManifest {
    files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RuntimeFile {
    File {
        downloads: FileDownloads,
        #[serde(default)]
        executable: bool,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
struct FileDownloads {
    raw: RawFile,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    sha1: String,
    size: u64,
    url: String,
}

#[derive(Clone, Debug)]
pub struct JavaRuntime {
    pub component: String,
    pub version: String,
    pub home: PathBuf,
    pub executable: PathBuf,
}

pub async fn ensure(
    net: &Net,
    paths: &Paths,
    platform: &Platform,
    component: &str,
    progress: &dyn Progress,
) -> Result<(JavaRuntime, Stats)> {
    let key = platform.runtime_key()?;
    let index: RuntimeIndex = net.json(RUNTIME_INDEX, "the Java runtime index").await?;
    let entry = index
        .get(key)
        .and_then(|components| components.get(component))
        .and_then(|entries| entries.first())
        .ok_or_else(|| {
            Error::Unsupported(format!("Mojang has no {component} Java runtime for {key}"))
        })?;
    let (manifest, _): (RuntimeManifest, _) = net
        .verified_json(
            &entry.manifest.url,
            "the Java runtime manifest",
            &Hash::sha1(&entry.manifest.sha1),
        )
        .await?;

    let home = paths.runtime().join(component);
    let mut files = Vec::new();
    let mut links = Vec::new();
    for (relative, file) in &manifest.files {
        let path = contained(&home, relative)?;
        match file {
            RuntimeFile::Directory => fsx::create_dir(&path).await?,
            RuntimeFile::File {
                downloads,
                executable,
            } => files.push(Download {
                url: downloads.raw.url.clone(),
                dest: path,
                hash: Hash::sha1(&downloads.raw.sha1),
                size: Some(downloads.raw.size),
                executable: *executable,
            }),
            RuntimeFile::Link { target } => {
                let parent = Path::new(relative).parent().unwrap_or(Path::new(""));
                contained(&home, &parent.join(target).to_string_lossy())?;
                links.push((path, PathBuf::from(target)));
            }
        }
    }
    let stats = download::ensure_all(net, files, 16, progress, "java runtime").await?;
    for (path, target) in links {
        make_link(&path, &target).await?;
    }
    let executable = home.join(platform.java_binary());
    if !executable.exists() {
        return Err(Error::Unsupported(format!(
            "the Java runtime installed but {} is missing",
            executable.display()
        )));
    }
    Ok((
        JavaRuntime {
            component: component.to_string(),
            version: entry.version.name.clone(),
            home,
            executable,
        },
        stats,
    ))
}

fn contained(base: &Path, relative: &str) -> Result<PathBuf> {
    let mut depth: i64 = 0;
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => depth -= 1,
            Component::RootDir | Component::Prefix(_) => depth = -1,
        }
        if depth < 0 {
            return Err(Error::Unsupported(format!(
                "the Java runtime manifest points outside its folder: {relative}"
            )));
        }
    }
    Ok(base.join(relative))
}

#[cfg(unix)]
async fn make_link(path: &Path, target: &Path) -> Result<()> {
    use crate::error::IoContext;

    if let Ok(existing) = tokio::fs::read_link(path).await {
        if existing == target {
            return Ok(());
        }
        tokio::fs::remove_file(path).await.at(path)?;
    }
    if let Some(parent) = path.parent() {
        fsx::create_dir(parent).await?;
    }
    tokio::fs::symlink(target, path).await.at(path)
}

#[cfg(not(unix))]
async fn make_link(_path: &Path, _target: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_paths_must_stay_inside_the_runtime() {
        let base = Path::new("/rt");
        assert!(contained(base, "bin/java").is_ok());
        assert!(contained(base, "legal/java.base/../java.base/LICENSE").is_ok());
        assert!(contained(base, "../escape").is_err());
        assert!(contained(base, "lib/../../escape").is_err());
        assert!(contained(base, "/etc/passwd").is_err());
    }

    #[test]
    fn manifest_entries_parse() {
        let manifest: RuntimeManifest = serde_json::from_str(
            r#"{"files":{
                "bin":{"type":"directory"},
                "bin/java":{"type":"file","executable":true,"downloads":{"raw":{"sha1":"a","size":1,"url":"https://piston-data.mojang.com/x"},"lzma":{"sha1":"b","size":1,"url":"https://piston-data.mojang.com/y"}}},
                "legal/a/LICENSE":{"type":"link","target":"../java.base/LICENSE"}
            }}"#,
        )
        .unwrap();
        assert!(matches!(
            manifest.files.get("bin/java"),
            Some(RuntimeFile::File {
                executable: true,
                ..
            })
        ));
        assert!(matches!(
            manifest.files.get("legal/a/LICENSE"),
            Some(RuntimeFile::Link { .. })
        ));
    }
}
