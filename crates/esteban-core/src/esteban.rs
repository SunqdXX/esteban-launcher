pub const HACKS_MOD_ID: &str = "esteban";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HacksArtifact {
    pub game_version: &'static str,
    pub filename: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

const PINNED_HACKS: &[HacksArtifact] = &[
    HacksArtifact {
        game_version: "1.21.4",
        filename: "esteban-1.3.0+1.21.4.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.3.0/esteban-1.3.0+1.21.4.jar",
        sha256: "cea75e8065ff0af2b20dcf8668e8ebd891b02999f376d75ac8415e4af1e63531",
        size: 83502,
    },
    HacksArtifact {
        game_version: "26.1",
        filename: "esteban-1.3.0+26.1.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.3.0/esteban-1.3.0+26.1.jar",
        sha256: "dd6c83c573dee145e3613d1017e607710b5a382e2df777aa53c744ac3af4d71b",
        size: 82260,
    },
    HacksArtifact {
        game_version: "26.1.2",
        filename: "esteban-1.3.0+26.1.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.3.0/esteban-1.3.0+26.1.2.jar",
        sha256: "a58c4a1677511eb926604f8b13ffd5cccd2f1b22e00a2af764b2244efb6e494e",
        size: 82261,
    },
    HacksArtifact {
        game_version: "26.2",
        filename: "esteban-1.3.0+26.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.3.0/esteban-1.3.0+26.2.jar",
        sha256: "d9d3851fa732eb7bc3c8b5218d3b47850849858c8b01b6d057dbeebd1cfb7c5f",
        size: 82257,
    },
    HacksArtifact {
        game_version: "26.3",
        filename: "esteban-1.3.0+26.3.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.3.0/esteban-1.3.0+26.3.jar",
        sha256: "12f31834ec685ab6008650c2666a109f595eaddef83ae79837639bc44bb9139d",
        size: 82316,
    },
];

pub fn hacks_for(game_version: &str) -> Option<HacksArtifact> {
    PINNED_HACKS
        .iter()
        .copied()
        .find(|a| a.game_version == game_version)
}

pub fn is_known_hacks_jar(sha256: &str) -> bool {
    PINNED_HACKS
        .iter()
        .any(|a| a.sha256.eq_ignore_ascii_case(sha256))
}
