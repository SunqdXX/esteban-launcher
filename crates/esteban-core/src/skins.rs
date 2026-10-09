use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::IoContext;
use crate::hash::sha256_hex;
use crate::instance::Instance;
use crate::loader::Loader;
use crate::modrinth::lock::Skipped;
use crate::modrinth::resolve::primary_file;
use crate::modrinth::{Modrinth, ResolvedMod};
use crate::net::Net;
use crate::paths::Paths;
use crate::{Error, Result, fsx};

pub const SKIN_MOD_SLUG: &str = "customskinloader";
pub const SKIN_MOD_TITLE: &str = "CustomSkinLoader";
pub const SKIN_MOD_DIR: &str = "CustomSkinLoader";
const WRITTEN: &str = ".esteban-launcher.json";
pub const MAX_SKIN_BYTES: usize = 1024 * 1024;
pub const PREVIEW_WIDTH: usize = 16;
pub const PREVIEW_HEIGHT: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Model {
    Classic,
    Slim,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub id: String,
    pub name: String,
    pub model: Model,
    pub sha256: String,
    pub width: u32,
    pub height: u32,
    pub added: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    skins: Vec<Skin>,
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    rgba: Vec<u8>,
}

impl Image {
    fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width as usize + x) * 4;
        match self.rgba.get(i..i + 4) {
            Some(p) => [p[0], p[1], p[2], p[3]],
            None => [0, 0, 0, 0],
        }
    }
}

pub fn decode(bytes: &[u8]) -> Result<Image> {
    if bytes.len() > MAX_SKIN_BYTES {
        return Err(Error::Unsupported(
            "That file is too big for a skin (over 1 MB).".into(),
        ));
    }
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(Error::Unsupported("That file isn't a PNG.".into()));
    }
    let bad = |e: png::DecodingError| Error::Unsupported(format!("That PNG can't be read: {e}"));
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(bad)?;
    let (width, height) = (reader.info().width, reader.info().height);
    if width != 64 || (height != 64 && height != 32) {
        return Err(Error::Unsupported(format!(
            "A skin is 64x64 (or the old 64x32), this one is {width}x{height}."
        )));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Unsupported("That PNG can't be read.".into()))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(bad)?;
    let data = &buffer[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => {
            return Err(Error::Unsupported("That PNG can't be read.".into()));
        }
    };
    Ok(Image {
        width,
        height,
        rgba,
    })
}

pub fn guess_model(image: &Image) -> Model {
    if image.height != 64 {
        return Model::Classic;
    }
    let empty = (20..32).all(|y| (54..56).all(|x| image.pixel(x, y)[3] == 0));
    if empty { Model::Slim } else { Model::Classic }
}

fn over(base: [u8; 4], top: [u8; 4]) -> [u8; 4] {
    if top[3] == 0 {
        return base;
    }
    if top[3] == 255 || base[3] == 0 {
        return top;
    }
    let a = u32::from(top[3]);
    let mix = |t: u8, b: u8| ((u32::from(t) * a + u32::from(b) * (255 - a)) / 255) as u8;
    [
        mix(top[0], base[0]),
        mix(top[1], base[1]),
        mix(top[2], base[2]),
        255,
    ]
}

pub fn preview(image: &Image, model: Model) -> Vec<String> {
    let mut grid = vec![[0u8; 4]; PREVIEW_WIDTH * PREVIEW_HEIGHT];
    let modern = image.height == 64;
    let arm = if model == Model::Slim { 3 } else { 4 };
    let mut paint = |dx: usize,
                     dy: usize,
                     sx: usize,
                     sy: usize,
                     w: usize,
                     h: usize,
                     mirror: bool,
                     overlay: bool| {
        for y in 0..h {
            for x in 0..w {
                let from_x = if mirror { sx + w - 1 - x } else { sx + x };
                let p = image.pixel(from_x, sy + y);
                let at = (dy + y) * PREVIEW_WIDTH + dx + x;
                let painted = if overlay {
                    over(grid[at], p)
                } else {
                    [p[0], p[1], p[2], if p[3] == 0 { 0 } else { 255 }]
                };
                grid[at] = painted;
            }
        }
    };
    let right_arm_x = 4 - arm;
    paint(4, 0, 8, 8, 8, 8, false, false);
    paint(4, 0, 40, 8, 8, 8, false, true);
    paint(4, 8, 20, 20, 8, 12, false, false);
    paint(right_arm_x, 8, 44, 20, arm, 12, false, false);
    paint(4, 20, 4, 20, 4, 12, false, false);
    if modern {
        paint(4, 8, 20, 36, 8, 12, false, true);
        paint(right_arm_x, 8, 44, 36, arm, 12, false, true);
        paint(12, 8, 36, 52, arm, 12, false, false);
        paint(12, 8, 52, 52, arm, 12, false, true);
        paint(4, 20, 4, 36, 4, 12, false, true);
        paint(8, 20, 20, 52, 4, 12, false, false);
        paint(8, 20, 4, 52, 4, 12, false, true);
    } else {
        paint(12, 8, 44, 20, arm, 12, true, false);
        paint(8, 20, 4, 20, 4, 12, true, false);
    }
    grid.chunks(PREVIEW_WIDTH)
        .map(|row| row.iter().map(hex::encode).collect())
        .collect()
}

fn skins_dir(paths: &Paths) -> PathBuf {
    paths.home().join("skins")
}

fn index_path(paths: &Paths) -> PathBuf {
    skins_dir(paths).join("skins.json")
}

pub fn skin_file(paths: &Paths, id: &str) -> Result<PathBuf> {
    let ok = id.len() == 16 && id.chars().all(|c| c.is_ascii_hexdigit());
    if !ok {
        return Err(Error::Unsupported(format!("not a skin id: {id}")));
    }
    Ok(skins_dir(paths).join(format!("{id}.png")))
}

async fn read_index(paths: &Paths) -> Result<Index> {
    let path = index_path(paths);
    match tokio::fs::read(&path).await {
        Ok(bytes) => crate::error::json(&bytes, "the skin list"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(e) => Err(e).at(&path),
    }
}

async fn write_index(paths: &Paths, index: &Index) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(index).map_err(|source| Error::Json {
        what: "the skin list".into(),
        source,
    })?;
    fsx::write_atomic(&index_path(paths), &bytes).await
}

pub async fn list(paths: &Paths) -> Result<Vec<Skin>> {
    Ok(read_index(paths).await?.skins)
}

pub async fn find(paths: &Paths, id: &str) -> Result<Option<Skin>> {
    Ok(list(paths).await?.into_iter().find(|s| s.id == id))
}

pub async fn image(paths: &Paths, id: &str) -> Result<Image> {
    let path = skin_file(paths, id)?;
    let bytes = tokio::fs::read(&path).await.at(&path)?;
    decode(&bytes)
}

fn clean_name(name: &str) -> Result<String> {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(Error::Unsupported(
            "Give the skin a name of 1 to 40 characters.".into(),
        ));
    }
    Ok(name)
}

pub async fn import(paths: &Paths, file: &Path) -> Result<Skin> {
    let meta = tokio::fs::metadata(file).await.at(file)?;
    if meta.len() > MAX_SKIN_BYTES as u64 {
        return Err(Error::Unsupported(
            "That file is too big for a skin (over 1 MB).".into(),
        ));
    }
    let bytes = tokio::fs::read(file).await.at(file)?;
    let picture = decode(&bytes)?;
    let sha256 = sha256_hex(&bytes);
    let id = sha256[..16].to_string();
    let mut index = read_index(paths).await?;
    if let Some(existing) = index.skins.iter().find(|s| s.id == id) {
        return Ok(existing.clone());
    }
    fsx::write_atomic(&skin_file(paths, &id)?, &bytes).await?;
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = clean_name(&stem).unwrap_or_else(|_| format!("Skin {}", index.skins.len() + 1));
    let skin = Skin {
        id,
        name: name.chars().take(40).collect(),
        model: guess_model(&picture),
        sha256,
        width: picture.width,
        height: picture.height,
        added: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    };
    index.skins.push(skin.clone());
    write_index(paths, &index).await?;
    Ok(skin)
}

pub async fn update(
    paths: &Paths,
    id: &str,
    name: Option<&str>,
    model: Option<Model>,
) -> Result<Skin> {
    let mut index = read_index(paths).await?;
    let Some(skin) = index.skins.iter_mut().find(|s| s.id == id) else {
        return Err(Error::Unsupported("That skin isn't in the list.".into()));
    };
    if let Some(name) = name {
        skin.name = clean_name(name)?;
    }
    if let Some(model) = model {
        skin.model = model;
    }
    let out = skin.clone();
    write_index(paths, &index).await?;
    Ok(out)
}

pub async fn remove(paths: &Paths, id: &str) -> Result<()> {
    let mut index = read_index(paths).await?;
    let before = index.skins.len();
    index.skins.retain(|s| s.id != id);
    if index.skins.len() == before {
        return Ok(());
    }
    write_index(paths, &index).await?;
    let path = skin_file(paths, id)?;
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).at(&path),
    }
}

pub fn skin_mod_unavailable(game: &str) -> Skipped {
    Skipped {
        title: SKIN_MOD_TITLE.into(),
        message: format!("Local skins aren't available for {game} yet."),
    }
}

pub async fn skin_mod(
    net: &Net,
    loader: Loader,
    game: &str,
    previous: Option<&ResolvedMod>,
    update: bool,
) -> Result<Option<ResolvedMod>> {
    if loader == Loader::Vanilla {
        return Ok(None);
    }
    if let Some(previous) = previous.filter(|_| !update) {
        return Ok(Some(previous.clone()));
    }
    let versions = Modrinth::new(net)
        .versions_for_loader(SKIN_MOD_SLUG, loader.slug(), game)
        .await?;
    let picked = versions
        .iter()
        .filter(|v| v.version_type == "release")
        .filter(|v| v.loaders.iter().any(|l| l == loader.slug()))
        .filter(|v| v.game_versions.iter().any(|g| g == game))
        .filter(|v| {
            primary_file(v).is_some_and(|f| {
                f.hashes
                    .sha512
                    .as_deref()
                    .is_some_and(|h| crate::hash::is_hex(h, 128))
            })
        })
        .max_by(|a, b| a.date_published.cmp(&b.date_published));
    Ok(picked.and_then(|v| {
        let file = primary_file(v)?;
        Some(ResolvedMod {
            slug: SKIN_MOD_SLUG.into(),
            project_id: v.project_id.clone(),
            title: SKIN_MOD_TITLE.into(),
            version_id: v.id.clone(),
            version_number: v.version_number.clone(),
            filename: file.filename.clone(),
            url: file.url.clone(),
            sha512: file.hashes.sha512.clone()?,
            size: file.size,
            requires: Vec::new(),
        })
    }))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Applied {
    Skin { name: String },
    AccountSkin,
    Unavailable { reason: String },
}

pub fn valid_username(name: &str) -> bool {
    (1..=16).contains(&name.len()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn availability(instance: &Instance, has_skin_mod: bool) -> Option<String> {
    if instance.loader == Loader::Vanilla {
        Some(
            "Vanilla can't show a local skin. Upload it to your account once sign-in is on.".into(),
        )
    } else if !has_skin_mod {
        Some(format!(
            "Local skins aren't available for {} yet.",
            instance.game_version
        ))
    } else {
        None
    }
}

fn skin_mod_config(model: Model) -> serde_json::Value {
    let model = match model {
        Model::Classic => "default",
        Model::Slim => "slim",
    };
    serde_json::json!({
        "loadlist": [
            {
                "name": "LocalSkin",
                "type": "Legacy",
                "checkPNG": false,
                "skin": "LocalSkin/skins/{USERNAME}.png",
                "model": model,
                "cape": "LocalSkin/capes/{USERNAME}.png",
                "elytra": "LocalSkin/elytras/{USERNAME}.png"
            },
            {
                "name": "Mojang",
                "type": "MojangAPI"
            }
        ],
        "enableCape": true,
        "enableLocalProfileCache": false,
        "enableCacheAutoClean": false
    })
}

pub async fn apply(paths: &Paths, instance: &Instance, username: &str) -> Result<Applied> {
    let file = instance.read_file().await?;
    let has_mod = file.jars.iter().any(|j| j.id == SKIN_MOD_SLUG);
    if let Some(reason) = availability(instance, has_mod) {
        return Ok(Applied::Unavailable { reason });
    }
    if !valid_username(username) {
        return Err(Error::Unsupported(format!(
            "{username} isn't a valid player name, so no skin file was written."
        )));
    }
    let chosen = match &file.skin {
        Some(id) => find(paths, id).await?,
        None => None,
    };
    let root = instance.dir.join(SKIN_MOD_DIR);
    let local = root.join("LocalSkin");
    let written_path = local.join(WRITTEN);
    let previous: Vec<String> = match tokio::fs::read(&written_path).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    for name in previous
        .iter()
        .filter(|n| n.strip_suffix(".png").is_some_and(valid_username))
    {
        let path = local.join("skins").join(name);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).at(&path),
        }
    }
    let config = skin_mod_config(chosen.as_ref().map_or(Model::Classic, |s| s.model));
    let bytes = serde_json::to_vec_pretty(&config).map_err(|source| Error::Json {
        what: "the skin mod settings".into(),
        source,
    })?;
    fsx::write_atomic(&root.join("CustomSkinLoader.json"), &bytes).await?;
    let (applied, written) = match chosen {
        Some(skin) => {
            let source = skin_file(paths, &skin.id)?;
            let png = tokio::fs::read(&source).await.at(&source)?;
            let name = format!("{username}.png");
            fsx::write_atomic(&local.join("skins").join(&name), &png).await?;
            (Applied::Skin { name: skin.name }, vec![name])
        }
        None => (Applied::AccountSkin, Vec::new()),
    };
    let list = serde_json::to_vec(&written).map_err(|source| Error::Json {
        what: "the skin file list".into(),
        source,
    })?;
    fsx::write_atomic(&written_path, &list).await?;
    Ok(applied)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn png_of(width: u32, height: u32, paint: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let mut data = Vec::new();
            for y in 0..height {
                for x in 0..width {
                    data.extend_from_slice(&paint(x, y));
                }
            }
            writer.write_image_data(&data).unwrap();
        }
        out
    }

    pub(crate) fn solid(rgb: [u8; 3]) -> Vec<u8> {
        png_of(64, 64, |_, _| [rgb[0], rgb[1], rgb[2], 255])
    }

    fn slim() -> Vec<u8> {
        png_of(64, 64, |x, y| {
            if (54..56).contains(&x) && (20..32).contains(&y) {
                [0, 0, 0, 0]
            } else {
                [10, 20, 30, 255]
            }
        })
    }

    #[test]
    fn only_real_skin_sized_pngs_are_accepted() {
        assert!(decode(&solid([255, 0, 0])).is_ok());
        assert!(decode(&png_of(64, 32, |_, _| [1, 2, 3, 255])).is_ok());
        assert!(decode(&png_of(128, 128, |_, _| [1, 2, 3, 255])).is_err());
        assert!(decode(b"GIF89a not a png").is_err());
        assert!(decode(&vec![0; MAX_SKIN_BYTES + 1]).is_err());
    }

    #[test]
    fn slim_arms_are_spotted() {
        assert_eq!(guess_model(&decode(&slim()).unwrap()), Model::Slim);
        assert_eq!(
            guess_model(&decode(&solid([1, 1, 1])).unwrap()),
            Model::Classic
        );
        let old = decode(&png_of(64, 32, |_, _| [0, 0, 0, 0])).unwrap();
        assert_eq!(guess_model(&old), Model::Classic);
    }

    #[test]
    fn the_preview_is_a_front_view_with_both_layers() {
        let skin = png_of(64, 64, |x, y| {
            if (8..16).contains(&x) && (8..16).contains(&y) {
                [255, 0, 0, 255]
            } else if (40..48).contains(&x) && (8..10).contains(&y) {
                [0, 0, 255, 255]
            } else if (40..48).contains(&x) && (8..16).contains(&y) {
                [0, 0, 0, 0]
            } else {
                [0, 255, 0, 255]
            }
        });
        let rows = preview(&decode(&skin).unwrap(), Model::Classic);
        assert_eq!(rows.len(), PREVIEW_HEIGHT);
        assert!(rows.iter().all(|r| r.len() == PREVIEW_WIDTH * 8));
        let cell = |x: usize, y: usize| rows[y][x * 8..x * 8 + 8].to_string();
        assert_eq!(cell(4, 0), "0000ffff");
        assert_eq!(cell(4, 5), "ff0000ff");
        assert_eq!(cell(0, 0), "00000000");
        assert_eq!(cell(0, 10), "00ff00ff");
        let slim_rows = preview(&decode(&slim()).unwrap(), Model::Slim);
        assert_eq!(slim_rows[10][..8].to_string(), "00000000");
        assert_eq!(slim_rows[10][8..16].to_string(), "0a141eff");
        let old = preview(
            &decode(&png_of(64, 32, |_, _| [9, 9, 9, 255])).unwrap(),
            Model::Classic,
        );
        assert_eq!(old[10][15 * 8..].to_string(), "090909ff");
    }

    #[tokio::test]
    async fn applying_writes_the_locked_config_and_switches_files() {
        use crate::instance::{Jar, JarSource};
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let red_png = dir.path().join("red.png");
        std::fs::write(&red_png, solid([255, 0, 0])).unwrap();
        let blue_png = dir.path().join("blue.png");
        std::fs::write(&blue_png, slim()).unwrap();
        let red = import(&paths, &red_png).await.unwrap();
        let blue = import(&paths, &blue_png).await.unwrap();

        let forge = Instance::new(&paths, "1.20.1", Loader::Forge, false).unwrap();
        assert!(matches!(
            apply(&paths, &forge, "Tester").await.unwrap(),
            Applied::Unavailable { reason } if reason.contains("1.20.1")
        ));
        let mut file = forge.read_file().await.unwrap();
        file.jars.push(Jar {
            file: "csl.jar".into(),
            source: JarSource::Modrinth,
            id: SKIN_MOD_SLUG.into(),
            title: SKIN_MOD_TITLE.into(),
            version: "15.0.1".into(),
            url: String::new(),
            sha512: Some("a".repeat(128)),
            sha256: None,
            size: 1,
            project_id: Some("idMHQ4n2".into()),
            version_id: Some("v".into()),
            requires: Vec::new(),
        });
        file.skin = Some(red.id.clone());
        forge.write_file(&file).await.unwrap();

        let local = forge.dir.join("CustomSkinLoader/LocalSkin/skins");
        assert_eq!(
            apply(&paths, &forge, "Tester").await.unwrap(),
            Applied::Skin { name: "red".into() }
        );
        assert_eq!(
            std::fs::read(local.join("Tester.png")).unwrap(),
            solid([255, 0, 0])
        );
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(forge.dir.join("CustomSkinLoader/CustomSkinLoader.json")).unwrap(),
        )
        .unwrap();
        let names: Vec<&str> = config["loadlist"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["LocalSkin", "Mojang"]);
        assert_eq!(config["loadlist"][0]["model"], "default");

        std::fs::write(local.join("Friend.png"), b"theirs").unwrap();
        file.skin = Some(blue.id.clone());
        forge.write_file(&file).await.unwrap();
        apply(&paths, &forge, "Other_1").await.unwrap();
        assert!(!local.join("Tester.png").exists());
        assert_eq!(std::fs::read(local.join("Other_1.png")).unwrap(), slim());
        assert!(local.join("Friend.png").exists());
        let config: serde_json::Value = serde_json::from_slice(
            &std::fs::read(forge.dir.join("CustomSkinLoader/CustomSkinLoader.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(config["loadlist"][0]["model"], "slim");

        file.skin = None;
        forge.write_file(&file).await.unwrap();
        assert_eq!(
            apply(&paths, &forge, "Other_1").await.unwrap(),
            Applied::AccountSkin
        );
        assert!(!local.join("Other_1.png").exists());
        assert!(apply(&paths, &forge, "../evil").await.is_err());

        let vanilla = Instance::new(&paths, "1.8.9", Loader::Vanilla, false).unwrap();
        assert!(matches!(
            apply(&paths, &vanilla, "Tester").await.unwrap(),
            Applied::Unavailable { reason } if reason.starts_with("Vanilla")
        ));
    }

    #[tokio::test]
    async fn skins_are_imported_once_renamed_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let red = dir.path().join("Red Knight.png");
        std::fs::write(&red, solid([255, 0, 0])).unwrap();
        let skin = import(&paths, &red).await.unwrap();
        assert_eq!(skin.name, "Red Knight");
        assert_eq!(skin.model, Model::Classic);
        assert_eq!(skin.id.len(), 16);
        assert!(skin_file(&paths, &skin.id).unwrap().is_file());
        assert_eq!(import(&paths, &red).await.unwrap(), skin);
        assert_eq!(list(&paths).await.unwrap().len(), 1);

        let renamed = update(&paths, &skin.id, Some("  Crimson "), Some(Model::Slim))
            .await
            .unwrap();
        assert_eq!(renamed.name, "Crimson");
        assert_eq!(renamed.model, Model::Slim);
        assert!(update(&paths, &skin.id, Some("   "), None).await.is_err());
        assert!(
            update(&paths, "0000000000000000", Some("x"), None)
                .await
                .is_err()
        );

        let bad = dir.path().join("big.png");
        std::fs::write(&bad, png_of(32, 32, |_, _| [0, 0, 0, 255])).unwrap();
        assert!(import(&paths, &bad).await.is_err());

        remove(&paths, &skin.id).await.unwrap();
        assert!(list(&paths).await.unwrap().is_empty());
        assert!(!skin_file(&paths, &skin.id).unwrap().exists());
        assert!(skin_file(&paths, "../../etc").is_err());
    }
}
