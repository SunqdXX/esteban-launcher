use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::error::IoContext;
use crate::profile::Instance;
use crate::{Error, Result, fsx};

pub const PACK_FOLDERS: &[&str] = &["shaderpacks", "resourcepacks", "screenshots"];

pub fn known_sources() -> Vec<PathBuf> {
    let Some(dirs) = directories::BaseDirs::new() else {
        return Vec::new();
    };
    let candidates = if cfg!(windows) {
        vec![dirs.data_dir().join(".minecraft")]
    } else if cfg!(target_os = "macos") {
        vec![dirs.data_dir().join("minecraft")]
    } else {
        vec![dirs.home_dir().join(".minecraft")]
    };
    candidates.into_iter().filter(|p| p.is_dir()).collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Linked {
    Linked(PathBuf),
    AlreadyLinked(PathBuf),
    Relinked { from: PathBuf, to: PathBuf },
    NotInSource,
    HasFiles,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Imported {
    Copied {
        copied: usize,
        kept: usize,
        skipped_links: usize,
    },
    IsLinked(PathBuf),
    NotInSource,
}

async fn source_folder(from: &Path, name: &str) -> Result<Option<PathBuf>> {
    let folder = from.join(name);
    match tokio::fs::metadata(&folder).await {
        Ok(meta) if meta.is_dir() => Ok(Some(tokio::fs::canonicalize(&folder).await.at(&folder)?)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).at(&folder),
    }
}

async fn refuse_inside_instance(instance: &Instance, source: &Path) -> Result<()> {
    fsx::create_dir(&instance.dir).await?;
    let own = tokio::fs::canonicalize(&instance.dir)
        .await
        .at(&instance.dir)?;
    if source.starts_with(&own) {
        return Err(Error::Unsupported(format!(
            "{} is inside this instance, pick the folder of another game or launcher",
            source.display()
        )));
    }
    Ok(())
}

pub async fn link(instance: &Instance, from: &Path) -> Result<Vec<(&'static str, Linked)>> {
    let mut out = Vec::new();
    for name in PACK_FOLDERS {
        let Some(source) = source_folder(from, name).await? else {
            out.push((*name, Linked::NotInSource));
            continue;
        };
        refuse_inside_instance(instance, &source).await?;
        let dest = instance.dir.join(name);
        let outcome = match tokio::fs::symlink_metadata(&dest).await {
            Err(e) if e.kind() == ErrorKind::NotFound => {
                make_link(&source, &dest).await?;
                Linked::Linked(source)
            }
            Err(e) => return Err(e).at(&dest),
            Ok(meta) if meta.file_type().is_symlink() => {
                let current = tokio::fs::canonicalize(&dest).await.ok();
                if current.as_deref() == Some(source.as_path()) {
                    Linked::AlreadyLinked(source)
                } else {
                    let from = tokio::fs::read_link(&dest).await.at(&dest)?;
                    remove_link(&dest).await?;
                    make_link(&source, &dest).await?;
                    Linked::Relinked { from, to: source }
                }
            }
            Ok(meta) if meta.is_dir() && is_empty(&dest).await? => {
                tokio::fs::remove_dir(&dest).await.at(&dest)?;
                make_link(&source, &dest).await?;
                Linked::Linked(source)
            }
            Ok(_) => Linked::HasFiles,
        };
        out.push((*name, outcome));
    }
    Ok(out)
}

pub async fn import(instance: &Instance, from: &Path) -> Result<Vec<(&'static str, Imported)>> {
    let mut out = Vec::new();
    for name in PACK_FOLDERS {
        let Some(source) = source_folder(from, name).await? else {
            out.push((*name, Imported::NotInSource));
            continue;
        };
        refuse_inside_instance(instance, &source).await?;
        let dest = instance.dir.join(name);
        if let Ok(meta) = tokio::fs::symlink_metadata(&dest).await
            && meta.file_type().is_symlink()
        {
            let target = tokio::fs::read_link(&dest).await.at(&dest)?;
            out.push((*name, Imported::IsLinked(target)));
            continue;
        }
        out.push((*name, copy_tree(&source, &dest).await?));
    }
    Ok(out)
}

async fn is_empty(dir: &Path) -> Result<bool> {
    let mut entries = tokio::fs::read_dir(dir).await.at(dir)?;
    Ok(entries.next_entry().await.at(dir)?.is_none())
}

async fn copy_tree(source: &Path, dest: &Path) -> Result<Imported> {
    let mut copied = 0;
    let mut kept = 0;
    let mut skipped_links = 0;
    let mut stack = vec![(source.to_path_buf(), dest.to_path_buf())];
    while let Some((from_dir, to_dir)) = stack.pop() {
        fsx::create_dir(&to_dir).await?;
        let mut entries = tokio::fs::read_dir(&from_dir).await.at(&from_dir)?;
        while let Some(entry) = entries.next_entry().await.at(&from_dir)? {
            let kind = entry.file_type().await.at(&entry.path())?;
            let target = to_dir.join(entry.file_name());
            if kind.is_symlink() {
                skipped_links += 1;
            } else if kind.is_dir() {
                stack.push((entry.path(), target));
            } else if copy_new(&entry.path(), &target).await? {
                copied += 1;
            } else {
                kept += 1;
            }
        }
    }
    Ok(Imported::Copied {
        copied,
        kept,
        skipped_links,
    })
}

async fn copy_new(from: &Path, to: &Path) -> Result<bool> {
    if tokio::fs::symlink_metadata(to).await.is_ok() {
        return Ok(false);
    }
    let part = fsx::part_path(to);
    tokio::fs::copy(from, &part).await.at(&part)?;
    let placed = match tokio::fs::hard_link(&part, to).await {
        Ok(()) => true,
        Err(e) if e.kind() == ErrorKind::AlreadyExists => false,
        Err(e) => {
            if tokio::fs::symlink_metadata(to).await.is_ok() {
                false
            } else {
                tokio::fs::rename(&part, to).await.at(to)?;
                tracing::debug!(error = %e, "hard link not supported here, renamed instead");
                return Ok(true);
            }
        }
    };
    tokio::fs::remove_file(&part).await.at(&part)?;
    Ok(placed)
}

#[cfg(unix)]
async fn make_link(source: &Path, dest: &Path) -> Result<()> {
    tokio::fs::symlink(source, dest).await.at(dest)
}

#[cfg(windows)]
async fn make_link(source: &Path, dest: &Path) -> Result<()> {
    junction::create(source, dest).at(dest)
}

#[cfg(unix)]
async fn remove_link(dest: &Path) -> Result<()> {
    tokio::fs::remove_file(dest).await.at(dest)
}

#[cfg(windows)]
async fn remove_link(dest: &Path) -> Result<()> {
    tokio::fs::remove_dir(dest).await.at(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    use crate::profile::ProfileKind;

    struct Setup {
        _dir: tempfile::TempDir,
        source: PathBuf,
        instance: Instance,
    }

    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("lunar");
        std::fs::create_dir_all(source.join("shaderpacks/unzipped/shaders")).unwrap();
        std::fs::write(source.join("shaderpacks/photon.zip"), b"zip").unwrap();
        std::fs::write(
            source.join("shaderpacks/unzipped/shaders/final.fsh"),
            b"glsl",
        )
        .unwrap();
        std::fs::create_dir_all(source.join("resourcepacks")).unwrap();
        std::fs::write(source.join("resourcepacks/pack.zip"), b"pack").unwrap();
        let paths = Paths::new(dir.path().join("base"));
        let instance = Instance::new(&paths, ProfileKind::Clean, "1.21.4").unwrap();
        Setup {
            _dir: dir,
            source,
            instance,
        }
    }

    #[tokio::test]
    async fn link_points_missing_folders_at_the_source() {
        let s = setup();
        let result = link(&s.instance, &s.source).await.unwrap();
        let shaders = std::fs::canonicalize(s.source.join("shaderpacks")).unwrap();
        assert_eq!(result[0], ("shaderpacks", Linked::Linked(shaders)));
        assert!(matches!(result[1], ("resourcepacks", Linked::Linked(_))));
        assert_eq!(result[2], ("screenshots", Linked::NotInSource));
        assert!(s.instance.dir.join("shaderpacks/photon.zip").exists());
        assert!(!s.instance.dir.join("screenshots").exists());
    }

    #[tokio::test]
    async fn linking_twice_changes_nothing() {
        let s = setup();
        link(&s.instance, &s.source).await.unwrap();
        let again = link(&s.instance, &s.source).await.unwrap();
        assert!(matches!(
            again[0],
            ("shaderpacks", Linked::AlreadyLinked(_))
        ));
    }

    #[tokio::test]
    async fn a_folder_with_files_is_never_replaced() {
        let s = setup();
        let own = s.instance.dir.join("shaderpacks");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("mine.zip"), b"mine").unwrap();
        let result = link(&s.instance, &s.source).await.unwrap();
        assert_eq!(result[0], ("shaderpacks", Linked::HasFiles));
        assert_eq!(std::fs::read(own.join("mine.zip")).unwrap(), b"mine");
        assert!(!std::fs::symlink_metadata(&own).unwrap().is_symlink());
    }

    #[tokio::test]
    async fn an_empty_folder_is_swapped_for_the_link() {
        let s = setup();
        std::fs::create_dir_all(s.instance.dir.join("shaderpacks")).unwrap();
        let result = link(&s.instance, &s.source).await.unwrap();
        assert!(matches!(result[0], ("shaderpacks", Linked::Linked(_))));
    }

    #[tokio::test]
    async fn relinking_moves_only_the_link_and_keeps_the_old_source() {
        let s = setup();
        link(&s.instance, &s.source).await.unwrap();
        let other = s.source.parent().unwrap().join("other");
        std::fs::create_dir_all(other.join("shaderpacks")).unwrap();
        let result = link(&s.instance, &other).await.unwrap();
        assert!(matches!(
            result[0],
            ("shaderpacks", Linked::Relinked { .. })
        ));
        assert!(s.source.join("shaderpacks/photon.zip").exists());
    }

    #[tokio::test]
    async fn import_copies_without_overwriting() {
        let s = setup();
        let own = s.instance.dir.join("shaderpacks");
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join("photon.zip"), b"mine").unwrap();
        let result = import(&s.instance, &s.source).await.unwrap();
        assert_eq!(
            result[0],
            (
                "shaderpacks",
                Imported::Copied {
                    copied: 1,
                    kept: 1,
                    skipped_links: 0
                }
            )
        );
        assert_eq!(std::fs::read(own.join("photon.zip")).unwrap(), b"mine");
        assert_eq!(
            std::fs::read(own.join("unzipped/shaders/final.fsh")).unwrap(),
            b"glsl"
        );
        assert!(!fsx::part_path(&own.join("unzipped/shaders/final.fsh")).exists());
        assert_eq!(
            std::fs::read(s.source.join("shaderpacks/photon.zip")).unwrap(),
            b"zip"
        );
    }

    #[tokio::test]
    async fn import_never_writes_through_a_link() {
        let s = setup();
        link(&s.instance, &s.source).await.unwrap();
        let other = s.source.parent().unwrap().join("other");
        std::fs::create_dir_all(other.join("shaderpacks")).unwrap();
        std::fs::write(other.join("shaderpacks/new.zip"), b"new").unwrap();
        let result = import(&s.instance, &other).await.unwrap();
        assert!(matches!(result[0], ("shaderpacks", Imported::IsLinked(_))));
        assert!(!s.source.join("shaderpacks/new.zip").exists());
    }

    #[tokio::test]
    async fn the_instance_cannot_be_its_own_source() {
        let s = setup();
        std::fs::create_dir_all(s.instance.dir.join("resourcepacks/x")).unwrap();
        assert!(matches!(
            link(&s.instance, &s.instance.dir).await,
            Err(Error::Unsupported(_))
        ));
    }
}
