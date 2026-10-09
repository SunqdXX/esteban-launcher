use std::sync::LazyLock;

use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

use crate::{Error, Result};

const KEYS: &str = include_str!("../../../config/keys.json");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Channel,
    Updater,
}

impl Purpose {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Channel => "channel",
            Self::Updater => "updater",
        }
    }

    pub fn what(self) -> &'static str {
        match self {
            Self::Channel => "The Esteban version list",
            Self::Updater => "The launcher update info",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Keys {
    #[serde(default)]
    channel: Vec<String>,
    #[serde(default)]
    updater: Vec<String>,
}

static BUILT_IN: LazyLock<Keys> = LazyLock::new(|| match serde_json::from_str(KEYS) {
    Ok(keys) => keys,
    Err(e) => {
        tracing::error!(error = %e, "config/keys.json is broken, no signing key is trusted");
        Keys::default()
    }
});

pub fn built_in(purpose: Purpose) -> &'static [String] {
    match purpose {
        Purpose::Channel => &BUILT_IN.channel,
        Purpose::Updater => &BUILT_IN.updater,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    pub key_id: String,
    pub trusted_comment: String,
}

pub fn key_id(public_key: &str) -> Option<String> {
    let bytes = base64_decode(public_key.trim())?;
    if bytes.len() != 42 || bytes[..2] != *b"Ed" {
        return None;
    }
    let mut id = [0u8; 8];
    id.copy_from_slice(&bytes[2..10]);
    Some(format!("{:016X}", u64::from_le_bytes(id)))
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let body = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(body.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0;
    for c in body.bytes() {
        buffer = (buffer << 6) | value(c)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

pub fn verify(purpose: Purpose, data: &[u8], signature: &str) -> Result<Verified> {
    verify_with(built_in(purpose), purpose, data, signature)
}

pub fn verify_with(
    keys: &[String],
    purpose: Purpose,
    data: &[u8],
    signature: &str,
) -> Result<Verified> {
    if keys.is_empty() {
        return Err(Error::Guard(format!(
            "{} can't be checked: no {} key is built into this launcher yet.",
            purpose.what(),
            purpose.slug()
        )));
    }
    if signature.trim().is_empty() {
        return Err(Error::Guard(format!(
            "{} has no signature, so it was ignored.",
            purpose.what()
        )));
    }
    let signature = Signature::decode(signature)
        .map_err(|_| Error::Guard(format!("{} has a broken signature file.", purpose.what())))?;
    for key in keys {
        let Ok(public) = PublicKey::from_base64(key.trim()) else {
            tracing::warn!(
                purpose = purpose.slug(),
                "a built-in public key doesn't parse"
            );
            continue;
        };
        if public.verify(data, &signature, false).is_ok() {
            return Ok(Verified {
                key_id: key_id(key).unwrap_or_default(),
                trusted_comment: signature.trusted_comment().to_string(),
            });
        }
    }
    Err(Error::Guard(format!(
        "{} isn't signed by a key this launcher trusts, so it was ignored.",
        purpose.what()
    )))
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Cursor;

    use super::*;

    pub(crate) struct TestKey {
        pub public: String,
        secret: minisign::SecretKey,
        pk: minisign::PublicKey,
    }

    impl TestKey {
        pub(crate) fn new() -> Self {
            let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
            Self {
                public: pair.pk.to_base64(),
                secret: pair.sk,
                pk: pair.pk,
            }
        }

        pub(crate) fn sign(&self, data: &[u8], comment: &str) -> String {
            minisign::sign(
                Some(&self.pk),
                &self.secret,
                Cursor::new(data),
                Some(comment),
                None,
            )
            .unwrap()
            .into_string()
        }
    }

    #[test]
    fn the_built_in_key_list_parses_and_every_key_is_valid() {
        let parsed: Keys = serde_json::from_str(KEYS).unwrap();
        for key in parsed.channel.iter().chain(&parsed.updater) {
            assert!(PublicKey::from_base64(key).is_ok(), "{key} doesn't parse");
            assert!(key_id(key).is_some());
        }
    }

    #[test]
    fn a_good_signature_passes_and_reports_the_key_and_comment() {
        let key = TestKey::new();
        let data = b"{\"schema\":2}";
        let signature = key.sign(data, "esteban-launcher channel sequence:7");
        let verified = verify_with(
            std::slice::from_ref(&key.public),
            Purpose::Channel,
            data,
            &signature,
        )
        .unwrap();
        assert_eq!(
            verified.trusted_comment,
            "esteban-launcher channel sequence:7"
        );
        assert_eq!(verified.key_id.len(), 16);
        assert_eq!(Some(verified.key_id), key_id(&key.public));
    }

    #[test]
    fn tampered_data_other_keys_and_junk_are_refused() {
        let key = TestKey::new();
        let other = TestKey::new();
        let data = b"versions";
        let signature = key.sign(data, "c");
        let refused = |keys: &[String], data: &[u8], signature: &str| {
            matches!(
                verify_with(keys, Purpose::Channel, data, signature),
                Err(Error::Guard(_))
            )
        };
        assert!(refused(
            std::slice::from_ref(&key.public),
            b"versionz",
            &signature
        ));
        assert!(refused(
            std::slice::from_ref(&other.public),
            data,
            &signature
        ));
        assert!(refused(
            std::slice::from_ref(&key.public),
            data,
            "not a signature"
        ));
        assert!(matches!(
            verify_with(std::slice::from_ref(&key.public), Purpose::Channel, data, "  "),
            Err(Error::Guard(m)) if m.contains("has no signature")
        ));
        assert!(refused(&[], data, &signature));
        let swapped = signature.replace("trusted comment: c", "trusted comment: d");
        assert!(refused(std::slice::from_ref(&key.public), data, &swapped));
        assert!(
            verify_with(
                &[other.public.clone(), "junk".into(), key.public.clone()],
                Purpose::Updater,
                data,
                &signature
            )
            .is_ok()
        );
    }

    #[test]
    fn with_no_built_in_key_nothing_remote_is_trusted() {
        if built_in(Purpose::Channel).is_empty() {
            assert!(
                matches!(verify(Purpose::Channel, b"x", "y"), Err(Error::Guard(m)) if m.contains("no channel key"))
            );
        }
        assert_eq!(key_id("bm9wZQ=="), None);
    }
}
