pub mod account;
pub mod download;
pub mod error;
pub mod esteban;
pub mod fabric;
pub mod fsx;
pub mod hash;
pub mod install;
pub mod java;
pub mod launch;
pub mod modcheck;
pub mod modrinth;
pub mod mojang;
pub mod net;
pub mod paths;
pub mod profile;
pub mod progress;
pub mod system;

pub use error::{Error, Result};

pub const PRODUCT: &str = "Esteban Launcher";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const LAUNCHER_BRAND: &str = "esteban-launcher";
pub const DISCLAIMER: &str =
    "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclaimer_is_the_required_text() {
        assert_eq!(
            DISCLAIMER,
            "Not an official Minecraft product. Not approved by or associated with Mojang or Microsoft."
        );
    }

    #[test]
    fn product_name_does_not_use_the_game_name() {
        assert!(!PRODUCT.to_lowercase().contains("minecraft"));
    }
}
