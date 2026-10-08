use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GameVersion {
    pub id: &'static str,
    pub tag: &'static str,
}

pub const GAME_VERSIONS: &[GameVersion] = &[
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

pub fn is_supported(id: &str) -> bool {
    GAME_VERSIONS.iter().any(|v| v.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esteban::hacks_for;

    #[test]
    fn every_listed_version_has_a_pinned_hacks_jar() {
        for v in GAME_VERSIONS {
            assert!(hacks_for(v.id).is_some(), "{} has no hacks jar", v.id);
        }
    }

    #[test]
    fn the_default_is_listed_and_tagged_latest() {
        assert!(is_supported(DEFAULT_GAME_VERSION));
        assert_eq!(GAME_VERSIONS[0].id, DEFAULT_GAME_VERSION);
        assert_eq!(GAME_VERSIONS[0].tag, "latest");
        assert!(!is_supported("1.20.1"));
    }
}
