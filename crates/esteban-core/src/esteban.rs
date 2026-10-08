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
}

const PINNED: &[Artifact] = &[
    Artifact {
        kind: ArtifactKind::Hacks,
        game_version: "1.21.4",
        filename: "esteban-1.4.0+1.21.4.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-1.4.0+1.21.4.jar",
        sha256: "d56ef9204bfc48cf716fd59d4f0e46b6bfdbb292568ce72c803b293cbb0f0164",
        size: 91566,
    },
    Artifact {
        kind: ArtifactKind::Hacks,
        game_version: "26.1",
        filename: "esteban-1.4.0+26.1.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-1.4.0+26.1.jar",
        sha256: "b97659c5bba4f313545c440c54c7dbd5e546fbcf4a1cdf4ac8675b5e5861088e",
        size: 90792,
    },
    Artifact {
        kind: ArtifactKind::Hacks,
        game_version: "26.1.2",
        filename: "esteban-1.4.0+26.1.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-1.4.0+26.1.2.jar",
        sha256: "5136b2acdc53a6221595a2fa1575d589374e3b35c36fc1387da6629da8475175",
        size: 90801,
    },
    Artifact {
        kind: ArtifactKind::Hacks,
        game_version: "26.2",
        filename: "esteban-1.4.0+26.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-1.4.0+26.2.jar",
        sha256: "55df8e6b5aaa50bb4966921fcc1ee626cf5c404f33ae2f4454236996f37139f9",
        size: 90828,
    },
    Artifact {
        kind: ArtifactKind::Hacks,
        game_version: "26.3",
        filename: "esteban-1.4.0+26.3.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-1.4.0+26.3.jar",
        sha256: "2b7030411d486663aa51bf407661a5bf2da3c1d77ecf3fbd871b3cb586b31065",
        size: 90910,
    },
    Artifact {
        kind: ArtifactKind::Hud,
        game_version: "1.21.4",
        filename: "esteban-hud-1.4.0+1.21.4.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-hud-1.4.0+1.21.4.jar",
        sha256: "1ac7e7d38b28924239536f58a2400abef6b7fe9ba3141809775211889eafe27d",
        size: 8122450,
    },
    Artifact {
        kind: ArtifactKind::Hud,
        game_version: "26.1",
        filename: "esteban-hud-1.4.0+26.1.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-hud-1.4.0+26.1.jar",
        sha256: "754488006d74a7939396c6c0066a7fb679cac36e2a5f6f04e8a8e610050204fb",
        size: 8120222,
    },
    Artifact {
        kind: ArtifactKind::Hud,
        game_version: "26.1.2",
        filename: "esteban-hud-1.4.0+26.1.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-hud-1.4.0+26.1.2.jar",
        sha256: "cd4d742618b79a59e7a2688625622fb513ede873095819850ffd454392ce3610",
        size: 8120232,
    },
    Artifact {
        kind: ArtifactKind::Hud,
        game_version: "26.2",
        filename: "esteban-hud-1.4.0+26.2.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-hud-1.4.0+26.2.jar",
        sha256: "fbd3d5619624d49e4ce099d4f7b6818ad0d852650674841e6991e47189372061",
        size: 8120351,
    },
    Artifact {
        kind: ArtifactKind::Hud,
        game_version: "26.3",
        filename: "esteban-hud-1.4.0+26.3.jar",
        url: "https://github.com/SunqdXX/esteban/releases/download/v1.4.0/esteban-hud-1.4.0+26.3.jar",
        sha256: "3554e7a96ce401f3437ddd1ad81ceb9892ab23e197cac431e02fa442218bf1d8",
        size: 8120476,
    },
];

const RETIRED_HACKS_SHA256: &[&str] = &[
    "cea75e8065ff0af2b20dcf8668e8ebd891b02999f376d75ac8415e4af1e63531",
    "dd6c83c573dee145e3613d1017e607710b5a382e2df777aa53c744ac3af4d71b",
    "a58c4a1677511eb926604f8b13ffd5cccd2f1b22e00a2af764b2244efb6e494e",
    "d9d3851fa732eb7bc3c8b5218d3b47850849858c8b01b6d057dbeebd1cfb7c5f",
    "12f31834ec685ab6008650c2666a109f595eaddef83ae79837639bc44bb9139d",
];

fn find(kind: ArtifactKind, game_version: &str) -> Option<Artifact> {
    PINNED
        .iter()
        .copied()
        .find(|a| a.kind == kind && a.game_version == game_version)
}

pub fn hacks_for(game_version: &str) -> Option<Artifact> {
    find(ArtifactKind::Hacks, game_version)
}

pub fn hud_for(game_version: &str) -> Option<Artifact> {
    find(ArtifactKind::Hud, game_version)
}

pub fn is_known_hacks_jar(sha256: &str) -> bool {
    PINNED
        .iter()
        .filter(|a| a.kind == ArtifactKind::Hacks)
        .map(|a| a.sha256)
        .chain(RETIRED_HACKS_SHA256.iter().copied())
        .any(|known| known.eq_ignore_ascii_case(sha256))
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
