use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GameVersion {
    pub id: &'static str,
    pub tag: &'static str,
}

pub const PINNED_VERSIONS: &[GameVersion] = &[
    GameVersion {
        id: "26.3",
        tag: "latest",
    },
    GameVersion {
        id: "26.2",
        tag: "",
    },
    GameVersion {
        id: "26.1.2",
        tag: "",
    },
    GameVersion {
        id: "26.1",
        tag: "",
    },
    GameVersion {
        id: "1.21.4",
        tag: "2b2t",
    },
];

pub const DEFAULT_GAME_VERSION: &str = "26.3";

pub fn is_pinned(id: &str) -> bool {
    PINNED_VERSIONS.iter().any(|v| v.id == id)
}

pub fn pinned_tag(id: &str) -> &'static str {
    PINNED_VERSIONS
        .iter()
        .find(|v| v.id == id)
        .map_or("", |v| v.tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esteban::hacks_for;

    #[test]
    fn every_pinned_version_has_a_pinned_hacks_jar() {
        for v in PINNED_VERSIONS {
            assert!(hacks_for(v.id).is_some(), "{} has no hacks jar", v.id);
        }
    }

    #[test]
    fn the_default_is_listed_and_tagged_latest() {
        assert!(is_pinned(DEFAULT_GAME_VERSION));
        assert_eq!(PINNED_VERSIONS[0].id, DEFAULT_GAME_VERSION);
        assert_eq!(pinned_tag(DEFAULT_GAME_VERSION), "latest");
        assert_eq!(pinned_tag("1.21.4"), "2b2t");
        assert!(!is_pinned("1.20.1"));
        assert_eq!(pinned_tag("1.20.1"), "");
    }
}
