use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::mojang::libraries::ResolvedLibrary;
use crate::mojang::version::{Argument, VersionJson};
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Forge,
}

impl Loader {
    pub const ALL: [Loader; 3] = [Loader::Vanilla, Loader::Fabric, Loader::Forge];

    pub fn slug(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Fabric => "fabric",
            Self::Forge => "forge",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Vanilla => "Vanilla",
            Self::Fabric => "Fabric",
            Self::Forge => "Forge",
        }
    }

    pub fn has_mods(self) -> bool {
        self != Self::Vanilla
    }
}

impl fmt::Display for Loader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.title())
    }
}

impl FromStr for Loader {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .into_iter()
            .find(|l| l.slug().eq_ignore_ascii_case(s))
            .ok_or_else(|| {
                Error::Unsupported(format!("unknown loader {s}, use vanilla, fabric or forge"))
            })
    }
}

#[derive(Clone, Debug)]
pub struct LaunchProfile {
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub version_name: String,
    pub main_class: String,
    pub jvm: Vec<Argument>,
    pub game: Vec<Argument>,
    pub legacy_game: Option<String>,
    pub libraries: Vec<ResolvedLibrary>,
}

impl LaunchProfile {
    pub fn vanilla(version: &VersionJson) -> Self {
        Self {
            loader: Loader::Vanilla,
            loader_version: None,
            version_name: version.id.clone(),
            main_class: version.main_class.clone(),
            jvm: Vec::new(),
            game: Vec::new(),
            legacy_game: None,
            libraries: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaders_round_trip_through_their_slugs() {
        for loader in Loader::ALL {
            assert_eq!(loader.slug().parse::<Loader>().unwrap(), loader);
            let json = serde_json::to_string(&loader).unwrap();
            assert_eq!(json, format!("\"{}\"", loader.slug()));
        }
        assert_eq!("Forge".parse::<Loader>().unwrap(), Loader::Forge);
        assert!("quilt".parse::<Loader>().is_err());
        assert!(!Loader::Vanilla.has_mods() && Loader::Forge.has_mods());
    }
}
