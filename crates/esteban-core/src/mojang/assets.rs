use std::collections::BTreeMap;

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
    progress: &dyn Progress,
) -> Result<Stats> {
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
    if parsed.is_virtual || parsed.map_to_resources {
        return Err(Error::Unsupported(
            "this version uses the legacy asset layout, which is not supported".into(),
        ));
    }
    let objects = objects_dir_items(paths, &parsed);
    stats.add(download::ensure_all(net, objects, 32, progress, "assets").await?);
    Ok(stats)
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
}
