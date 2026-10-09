use std::path::Path;

use sha1::Digest;

use crate::error::IoContext;
use crate::{Error, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Hash {
    Sha1(String),
    Sha256(String),
    Sha512(String),
    Sha1Any(Vec<String>),
}

impl Hash {
    pub fn sha1(hex: &str) -> Self {
        Self::Sha1(hex.to_ascii_lowercase())
    }

    pub fn sha256(hex: &str) -> Self {
        Self::Sha256(hex.to_ascii_lowercase())
    }

    pub fn sha512(hex: &str) -> Self {
        Self::Sha512(hex.to_ascii_lowercase())
    }

    pub fn sha1_any(hexes: &[String]) -> Self {
        Self::Sha1Any(hexes.iter().map(|h| h.to_ascii_lowercase()).collect())
    }

    pub fn algorithm(&self) -> &'static str {
        match self {
            Self::Sha1(_) | Self::Sha1Any(_) => "sha1",
            Self::Sha256(_) => "sha256",
            Self::Sha512(_) => "sha512",
        }
    }

    pub fn expected(&self) -> &str {
        match self {
            Self::Sha1(h) | Self::Sha256(h) | Self::Sha512(h) => h,
            Self::Sha1Any(list) => list.first().map_or("", String::as_str),
        }
    }

    fn accepts(&self, actual: &str) -> bool {
        match self {
            Self::Sha1Any(list) => list.iter().any(|h| h == actual),
            _ => actual == self.expected(),
        }
    }

    pub fn digest(&self, data: &[u8]) -> String {
        match self {
            Self::Sha1(_) | Self::Sha1Any(_) => hex::encode(sha1::Sha1::digest(data)),
            Self::Sha256(_) => hex::encode(sha2::Sha256::digest(data)),
            Self::Sha512(_) => hex::encode(sha2::Sha512::digest(data)),
        }
    }

    pub fn verify(&self, data: &[u8], what: &str) -> Result<()> {
        let actual = self.digest(data);
        if self.accepts(&actual) {
            Ok(())
        } else {
            Err(Error::HashMismatch {
                what: what.to_string(),
                algorithm: self.algorithm(),
                expected: self.expected().to_string(),
                actual,
            })
        }
    }

    pub async fn file_matches(&self, path: &Path) -> Result<bool> {
        match tokio::fs::read(path).await {
            Ok(data) => Ok(self.accepts(&self.digest(&data))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e).at(path),
        }
    }
}

pub fn is_hex(value: &str, len: usize) -> bool {
    value.len() == len && value.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(sha2::Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests() {
        assert_eq!(
            Hash::sha1("").digest(b"abc"),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn verify_accepts_good_and_rejects_bad_or_truncated() {
        let good = Hash::sha1("A9993E364706816ABA3E25717850C26C9CD0D89D");
        assert!(good.verify(b"abc", "x").is_ok());
        assert!(matches!(
            good.verify(b"abd", "x"),
            Err(Error::HashMismatch { .. })
        ));
        assert!(matches!(
            good.verify(b"ab", "x"),
            Err(Error::HashMismatch { .. })
        ));
    }

    #[test]
    fn any_of_several_sha1s_is_accepted_and_nothing_else() {
        let any = Hash::sha1_any(&[
            "0000000000000000000000000000000000000000".into(),
            "A9993E364706816ABA3E25717850C26C9CD0D89D".into(),
        ]);
        assert!(any.verify(b"abc", "x").is_ok());
        assert!(any.verify(b"abd", "x").is_err());
        assert!(Hash::sha1_any(&[]).verify(b"abc", "x").is_err());
        assert_eq!(any.algorithm(), "sha1");
    }

    #[tokio::test]
    async fn missing_file_does_not_match() {
        let dir = tempfile::tempdir().unwrap();
        let h = Hash::sha256(&sha256_hex(b"x"));
        assert!(!h.file_matches(&dir.path().join("nope")).await.unwrap());
    }
}
