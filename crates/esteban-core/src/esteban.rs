use crate::channel::{self, Build, ChannelJar};
use crate::loader::Loader;

pub const HACKS_MOD_ID: &str = "esteban";
pub const HUD_MOD_ID: &str = "esteban-hud";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactKind {
    Hacks,
    Hud,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub kind: ArtifactKind,
    pub game_version: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    version: &'static str,
}

impl Artifact {
    pub fn title(&self) -> &'static str {
        match self.kind {
            ArtifactKind::Hacks => "Esteban",
            ArtifactKind::Hud => "Esteban HUD",
        }
    }

    pub fn mod_id(&self) -> &'static str {
        match self.kind {
            ArtifactKind::Hacks => HACKS_MOD_ID,
            ArtifactKind::Hud => HUD_MOD_ID,
        }
    }

    pub fn version(&self) -> &'static str {
        self.version
    }

    fn of(build: &'static Build, jar: &'static ChannelJar) -> Option<Self> {
        let kind = match jar.id.as_str() {
            HACKS_MOD_ID => ArtifactKind::Hacks,
            HUD_MOD_ID => ArtifactKind::Hud,
            _ => return None,
        };
        Some(Self {
            kind,
            game_version: &build.mc,
            filename: &jar.file,
            url: &jar.url,
            sha256: &jar.sha256,
            size: jar.size,
            version: &jar.version,
        })
    }
}

fn find(kind: ArtifactKind, game_version: &str, hacked: bool) -> Option<Artifact> {
    let build = channel::bundled()?.build(game_version, Loader::Fabric, hacked)?;
    build
        .jars
        .iter()
        .filter_map(|jar| Artifact::of(build, jar))
        .find(|a| a.kind == kind)
}

pub fn hacks_for(game_version: &str) -> Option<Artifact> {
    find(ArtifactKind::Hacks, game_version, true)
}

pub fn hud_for(game_version: &str) -> Option<Artifact> {
    find(ArtifactKind::Hud, game_version, false)
}

pub fn by_filename(filename: &str) -> Option<Artifact> {
    channel::bundled()?
        .jars()
        .filter(|(_, jar)| jar.file == filename)
        .find_map(|(build, jar)| Artifact::of(build, jar))
}

pub fn is_known_hacks_jar(sha256: &str) -> bool {
    channel::bundled().is_some_and(|c| c.is_known_hacks_jar(sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_version_has_both_jars_with_matching_names() {
        for v in ["1.21.4", "26.1", "26.1.2", "26.2", "26.3"] {
            let hacks = hacks_for(v).unwrap();
            let hud = hud_for(v).unwrap();
            assert_eq!(hacks.filename, format!("esteban-1.4.0+{v}.jar"));
            assert_eq!(hud.filename, format!("esteban-hud-1.4.0+{v}.jar"));
            assert!(hacks.url.ends_with(hacks.filename) && hud.url.ends_with(hud.filename));
            assert_eq!(hacks.sha256.len(), 64);
            assert_eq!(hud.sha256.len(), 64);
            assert_eq!(hacks.version(), "1.4.0");
            assert_eq!(hud.version(), "1.4.0");
            assert_eq!(by_filename(hud.filename), Some(hud));
        }
    }

    #[test]
    fn only_hacks_jars_count_as_hacks_and_old_ones_still_do() {
        for v in ["1.21.4", "26.3"] {
            assert!(is_known_hacks_jar(hacks_for(v).unwrap().sha256));
            assert!(!is_known_hacks_jar(hud_for(v).unwrap().sha256));
        }
        assert!(is_known_hacks_jar(
            "12f31834ec685ab6008650c2666a109f595eaddef83ae79837639bc44bb9139d"
        ));
    }
}
