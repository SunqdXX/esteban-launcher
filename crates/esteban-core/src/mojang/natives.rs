use std::io::Read;
use std::path::{Path, PathBuf};

use super::libraries::NativeJar;
use crate::error::IoContext;
use crate::{Error, Result};

pub async fn extract(jars: &[NativeJar], dest: &Path) -> Result<usize> {
    if jars.is_empty() {
        return Ok(0);
    }
    let jobs: Vec<(PathBuf, Vec<String>)> = jars
        .iter()
        .map(|j| (j.path.clone(), j.exclude.clone()))
        .collect();
    let dest = dest.to_path_buf();
    tokio::task::spawn_blocking(move || extract_blocking(&jobs, &dest))
        .await
        .map_err(|e| Error::Launch(format!("unpacking the native libraries stopped: {e}")))?
}

fn extract_blocking(jobs: &[(PathBuf, Vec<String>)], dest: &Path) -> Result<usize> {
    match std::fs::remove_dir_all(dest) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e).at(dest),
    }
    std::fs::create_dir_all(dest).at(dest)?;
    let mut written = 0;
    for (jar, exclude) in jobs {
        let file = std::fs::File::open(jar).at(jar)?;
        let zip_error = |source| Error::Zip {
            path: jar.clone(),
            source,
        };
        let mut archive = zip::ZipArchive::new(file).map_err(zip_error)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(zip_error)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            if exclude
                .iter()
                .any(|prefix| name.starts_with(prefix.as_str()))
            {
                continue;
            }
            let Some(relative) = entry.enclosed_name() else {
                return Err(Error::Unsupported(format!(
                    "{} has an unsafe path inside: {name}",
                    jar.display()
                )));
            };
            let target = dest.join(relative);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).at(parent)?;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).at(jar)?;
            std::fs::write(&target, &bytes).at(&target)?;
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;
    use crate::download::Download;
    use crate::hash::Hash;

    fn zip_with(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut out);
            for (name, data) in entries {
                if name.ends_with('/') {
                    writer
                        .add_directory(*name, zip::write::SimpleFileOptions::default())
                        .unwrap();
                } else {
                    writer
                        .start_file(*name, zip::write::SimpleFileOptions::default())
                        .unwrap();
                    writer.write_all(data).unwrap();
                }
            }
            writer.finish().unwrap();
        }
        out.into_inner()
    }

    fn native(path: &Path, exclude: &[&str]) -> NativeJar {
        NativeJar {
            path: path.to_path_buf(),
            download: Download {
                url: String::new(),
                dest: path.to_path_buf(),
                hash: Hash::sha1("00"),
                size: None,
                executable: false,
            },
            exclude: exclude.iter().map(|e| (*e).to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn natives_are_unpacked_without_excluded_entries_and_stale_files_go() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("lwjgl-natives.jar");
        std::fs::write(
            &jar,
            zip_with(&[
                ("META-INF/", b""),
                ("META-INF/MANIFEST.MF", b"m"),
                ("liblwjgl64.so", b"so"),
                ("sub/libopenal64.so", b"al"),
            ]),
        )
        .unwrap();
        let dest = dir.path().join("natives");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.so"), b"old").unwrap();
        let count = extract(&[native(&jar, &["META-INF/"])], &dest)
            .await
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(std::fs::read(dest.join("liblwjgl64.so")).unwrap(), b"so");
        assert_eq!(
            std::fs::read(dest.join("sub/libopenal64.so")).unwrap(),
            b"al"
        );
        assert!(!dest.join("META-INF").exists());
        assert!(!dest.join("stale.so").exists());
        assert_eq!(extract(&[], &dest).await.unwrap(), 0);
        assert!(dest.join("liblwjgl64.so").exists());
    }

    #[tokio::test]
    async fn entries_that_climb_out_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("evil.jar");
        std::fs::write(&jar, zip_with(&[("../escape.so", b"x")])).unwrap();
        let dest = dir.path().join("natives");
        assert!(matches!(
            extract(&[native(&jar, &[])], &dest).await,
            Err(Error::Unsupported(_))
        ));
        assert!(!dir.path().join("escape.so").exists());
    }
}
