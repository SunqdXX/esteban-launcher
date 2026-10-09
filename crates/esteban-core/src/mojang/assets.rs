use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::version::AssetIndexRef;
use crate::download::{self, Download, Stats};
use crate::error::IoContext;
use crate::hash::Hash;
use crate::net::Net;
use crate::paths::Paths;
use crate::progress::Progress;
use crate::{Error, Result};

const OBJECT_HOST: &str = "https://resources.download.minecraft.net";

#[derive(Debug, Deserialize)]
struct AssetIndex {
    objects: BTreeMap<String, AssetObject>,
    #[serde(default, rename = "virtual")]
    is_virtual: bool,
    #[serde(default)]
    map_to_resources: bool,
}

#[derive(Debug, Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

pub async fn ensure(
    net: &Net,
    paths: &Paths,
    index: &AssetIndexRef,
    game_dir: &Path,
    progress: &dyn Progress,
) -> Result<(Stats, PathBuf)> {
    let index_path = paths.asset_indexes().join(format!("{}.json", index.id));
    let mut stats = download::ensure_all(
        net,
        vec![Download {
            url: index.url.clone(),
            dest: index_path.clone(),
            hash: Hash::sha1(&index.sha1),
            size: Some(index.size),
            executable: false,
        }],
        1,
        progress,
        "asset index",
    )
    .await?;
    let bytes = tokio::fs::read(&index_path).await.at(&index_path)?;
    let parsed: AssetIndex = crate::error::json(&bytes, "the asset index")?;
    let objects = objects_dir_items(paths, &parsed);
    stats.add(download::ensure_all(net, objects, 32, progress, "assets").await?);
    let root = if parsed.map_to_resources {
        game_dir.join("resources")
    } else if parsed.is_virtual {
        paths.assets().join("virtual").join(&index.id)
    } else {
        return Ok((stats, paths.assets()));
    };
    let placed = place_legacy(paths, &parsed, &root).await?;
    tracing::debug!(placed, root = %root.display(), "old asset layout ready");
    Ok((stats, root))
}

async fn place_legacy(paths: &Paths, index: &AssetIndex, root: &Path) -> Result<usize> {
    let mut placed = 0;
    for (name, object) in &index.objects {
        let relative = safe_relative(name)?;
        if object.hash.len() <= 2 {
            continue;
        }
        let source = paths
            .asset_objects()
            .join(&object.hash[..2])
            .join(&object.hash);
        let target = root.join(relative);
        if let Ok(meta) = tokio::fs::metadata(&target).await
            && meta.len() == object.size
        {
            continue;
        }
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await.at(parent)?;
        }
        match tokio::fs::remove_file(&target).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).at(&target),
        }
        if tokio::fs::hard_link(&source, &target).await.is_err() {
            tokio::fs::copy(&source, &target).await.at(&target)?;
        }
        placed += 1;
    }
    Ok(placed)
}

fn safe_relative(name: &str) -> Result<&Path> {
    let path = Path::new(name);
    let ok = !name.is_empty()
        && path
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    if ok {
        Ok(path)
    } else {
        Err(Error::Unsupported(format!(
            "the asset index has an unsafe name: {name}"
        )))
    }
}

fn objects_dir_items(paths: &Paths, index: &AssetIndex) -> Vec<Download> {
    index
        .objects
        .values()
        .filter(|o| o.hash.len() > 2)
        .map(|o| {
            let prefix = &o.hash[..2];
            Download {
                url: format!("{OBJECT_HOST}/{prefix}/{}", o.hash),
                dest: paths.asset_objects().join(prefix).join(&o.hash),
                hash: Hash::sha1(&o.hash),
                size: Some(o.size),
                executable: false,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objects_use_the_hash_prefix_layout() {
        let index: AssetIndex = serde_json::from_str(
            r#"{"objects":{"minecraft/sounds/x.ogg":{"hash":"b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356","size":9101}}}"#,
        )
        .unwrap();
        let paths = Paths::new("/base");
        let items = objects_dir_items(&paths, &index);
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].url,
            "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
        assert!(
            items[0]
                .dest
                .ends_with("assets/objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356")
        );
    }

    #[tokio::test]
    async fn old_layouts_get_the_files_under_their_names() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let hash = "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356";
        let object = paths.asset_objects().join("b6").join(hash);
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"ogg!").unwrap();
        let index: AssetIndex = serde_json::from_str(&format!(
            r#"{{"virtual":true,"objects":{{"sound/random/click.ogg":{{"hash":"{hash}","size":4}}}}}}"#
        ))
        .unwrap();
        let root = dir.path().join("virtual/legacy");
        assert_eq!(place_legacy(&paths, &index, &root).await.unwrap(), 1);
        assert_eq!(
            std::fs::read(root.join("sound/random/click.ogg")).unwrap(),
            b"ogg!"
        );
        assert_eq!(place_legacy(&paths, &index, &root).await.unwrap(), 0);
        let evil: AssetIndex = serde_json::from_str(&format!(
            r#"{{"objects":{{"../../x.ogg":{{"hash":"{hash}","size":4}}}}}}"#
        ))
        .unwrap();
        assert!(place_legacy(&paths, &evil, &root).await.is_err());
        assert!(safe_relative("/etc/passwd").is_err());
        assert!(safe_relative("a/b.png").is_ok());
    }
}
