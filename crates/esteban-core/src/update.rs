use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::net::Net;
use crate::signing::{self, Purpose};
use crate::system::parse_version;
use crate::{Error, Result, VERSION};

pub const UPDATE_URL: &str =
    "https://github.com/SunqdXX/esteban-launcher/releases/latest/download/latest.json";

const DOWNLOAD_PREFIX: &str = "https://github.com/SunqdXX/";

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub version: String,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub pub_date: String,
    pub platforms: BTreeMap<String, PlatformEntry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PlatformEntry {
    pub url: String,
    #[serde(default)]
    pub signature: String,
}

fn numbers(version: &str) -> Option<Vec<u64>> {
    parse_version(version.trim_start_matches('v')).filter(|n| n.len() == 3)
}

pub fn parse(bytes: &[u8]) -> Result<Manifest> {
    let manifest: Manifest = crate::error::json(bytes, "the launcher update info")?;
    let bad = |why: String| Err(Error::Guard(format!("The launcher update info {why}.")));
    if numbers(&manifest.version).is_none() {
        return bad(format!(
            "has a version that isn't x.y.z: {}",
            manifest.version
        ));
    }
    if manifest.platforms.is_empty() {
        return bad("lists no downloads".into());
    }
    for (platform, entry) in &manifest.platforms {
        if !entry.url.starts_with(DOWNLOAD_PREFIX) {
            return bad(format!(
                "points {platform} somewhere other than the launcher's GitHub releases"
            ));
        }
    }
    Ok(manifest)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (numbers(candidate), numbers(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UpdateCheck {
    NoKey,
    NothingPublished,
    Offline,
    UpToDate {
        latest: String,
    },
    #[serde(rename_all = "camelCase")]
    Available {
        version: String,
        notes: String,
        key_id: String,
    },
    Refused {
        reason: String,
    },
}

pub async fn check(net: &Net) -> UpdateCheck {
    check_with(
        net,
        UPDATE_URL,
        signing::built_in(Purpose::Updater),
        VERSION,
    )
    .await
}

pub async fn check_with(net: &Net, url: &str, keys: &[String], current: &str) -> UpdateCheck {
    if keys.is_empty() {
        return UpdateCheck::NoKey;
    }
    let bytes = match net.bytes(url).await {
        Ok(bytes) => bytes,
        Err(Error::Status { status: 404, .. }) => return UpdateCheck::NothingPublished,
        Err(_) => return UpdateCheck::Offline,
    };
    let signature = match net.bytes(&format!("{url}.minisig")).await {
        Ok(text) => String::from_utf8_lossy(&text).into_owned(),
        Err(Error::Status { status: 404, .. }) => String::new(),
        Err(_) => return UpdateCheck::Offline,
    };
    let verified = match signing::verify_with(keys, Purpose::Updater, &bytes, &signature) {
        Ok(verified) => verified,
        Err(e) => {
            return UpdateCheck::Refused {
                reason: e.to_string(),
            };
        }
    };
    let manifest = match parse(&bytes) {
        Ok(manifest) => manifest,
        Err(e) => {
            return UpdateCheck::Refused {
                reason: e.to_string(),
            };
        }
    };
    if is_newer(&manifest.version, current) {
        UpdateCheck::Available {
            version: manifest.version,
            notes: manifest.notes,
            key_id: verified.key_id,
        }
    } else {
        UpdateCheck::UpToDate {
            latest: manifest.version,
        }
    }
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::net::Policy;
    use crate::signing::tests::TestKey;

    const LATEST: &str = r#"{"version":"0.2.0","notes":"Faster installs","pub_date":"2026-11-01T00:00:00Z",
        "platforms":{"linux-x86_64":{"url":"https://github.com/SunqdXX/esteban-launcher/releases/download/v0.2.0/esteban.AppImage","signature":"x"}}}"#;

    #[test]
    fn manifests_need_a_version_and_downloads_on_our_releases() {
        assert_eq!(parse(LATEST.as_bytes()).unwrap().version, "0.2.0");
        let bad = [
            LATEST.replace("0.2.0\"", "soon\""),
            LATEST.replace("https://github.com/SunqdXX/", "https://evil.example/"),
            r#"{"version":"0.2.0","platforms":{}}"#.to_string(),
        ];
        for text in bad {
            assert!(parse(text.as_bytes()).is_err(), "{text} was accepted");
        }
        assert!(is_newer("0.2.0", "0.1.0") && is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.0", "0.1.0") && !is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("x", "0.1.0"));
    }

    async fn serve(body: &str, signature: Option<&str>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(path("/latest.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body.to_string()))
            .mount(&server)
            .await;
        let sig = match signature {
            Some(s) => ResponseTemplate::new(200).set_body_string(s.to_string()),
            None => ResponseTemplate::new(404),
        };
        Mock::given(path("/latest.json.minisig"))
            .respond_with(sig)
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn only_a_signed_newer_version_is_reported() {
        let key = TestKey::new();
        let net = Net::new(Policy::local_test()).unwrap();
        let keys = vec![key.public.clone()];
        let signed = key.sign(LATEST.as_bytes(), "esteban-launcher updater version:0.2.0");
        let server = serve(LATEST, Some(&signed)).await;
        let url = format!("{}/latest.json", server.uri());
        assert!(matches!(
            check_with(&net, &url, &keys, "0.1.0").await,
            UpdateCheck::Available { version, .. } if version == "0.2.0"
        ));
        assert_eq!(
            check_with(&net, &url, &keys, "0.2.0").await,
            UpdateCheck::UpToDate {
                latest: "0.2.0".into()
            }
        );
        assert_eq!(
            check_with(&net, &url, &[], "0.1.0").await,
            UpdateCheck::NoKey
        );
        let stranger = TestKey::new();
        assert!(matches!(
            check_with(&net, &url, std::slice::from_ref(&stranger.public), "0.1.0").await,
            UpdateCheck::Refused { .. }
        ));

        let forged = LATEST.replace("0.2.0", "9.9.9");
        let tampered = serve(&forged, Some(&signed)).await;
        let url = format!("{}/latest.json", tampered.uri());
        assert!(matches!(
            check_with(&net, &url, &keys, "0.1.0").await,
            UpdateCheck::Refused { .. }
        ));
        let unsigned = serve(LATEST, None).await;
        let url = format!("{}/latest.json", unsigned.uri());
        assert!(matches!(
            check_with(&net, &url, &keys, "0.1.0").await,
            UpdateCheck::Refused { .. }
        ));
        let empty = MockServer::start().await;
        let url = format!("{}/latest.json", empty.uri());
        assert_eq!(
            check_with(&net, &url, &keys, "0.1.0").await,
            UpdateCheck::NothingPublished
        );
    }
}
