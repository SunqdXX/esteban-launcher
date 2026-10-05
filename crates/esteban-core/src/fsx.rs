use std::path::{Path, PathBuf};

use crate::Result;
use crate::error::IoContext;

pub fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

pub async fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await.at(parent)?;
    }
    let tmp = part_path(dest);
    tokio::fs::write(&tmp, bytes).await.at(&tmp)?;
    tokio::fs::rename(&tmp, dest).await.at(dest)?;
    Ok(())
}

pub async fn create_dir(path: &Path) -> Result<()> {
    tokio::fs::create_dir_all(path).await.at(path)
}

#[cfg(unix)]
pub async fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = tokio::fs::metadata(path).await.at(path)?;
    let mut perms = meta.permissions();
    perms.set_mode(perms.mode() | 0o755);
    tokio::fs::set_permissions(path, perms).await.at(path)
}

#[cfg(not(unix))]
pub async fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_file_sits_next_to_the_target() {
        assert_eq!(
            part_path(Path::new("/a/b/client.jar")),
            PathBuf::from("/a/b/client.jar.part")
        );
    }

    #[tokio::test]
    async fn atomic_write_leaves_no_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x").join("file.json");
        write_atomic(&dest, b"{}").await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"{}");
        assert!(!part_path(&dest).exists());
    }
}
