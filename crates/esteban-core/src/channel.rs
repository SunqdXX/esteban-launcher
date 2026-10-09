use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::esteban::{HACKS_MOD_ID, HUD_MOD_ID};
use crate::loader::Loader;
use crate::net::Net;
use crate::paths::Paths;
use crate::signing::{self, Purpose, Verified};
use crate::{Error, Result, fsx};

pub const CHANNEL_SCHEMA: u32 = 2;

const BUNDLED: &str = include_str!("../../../config/versions.json");

const JAR_HOST: &str = "https://github.com/SunqdXX/esteban/releases/download/";

pub const CHANNEL_URL: &str =
    "https://github.com/SunqdXX/esteban/releases/latest/download/versions.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Channel {
    pub schema: u32,
    pub sequence: u64,
    pub issued: String,
    pub expires: String,
    pub builds: Vec<Build>,
    #[serde(default)]
    pub pins: BTreeMap<String, String>,
    #[serde(default)]
    pub retired_hacks_sha256: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Build {
    pub mc: String,
    pub loader: BuildLoader,
    pub hacked: bool,
    pub jars: Vec<ChannelJar>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BuildLoader {
    pub kind: Loader,
    pub min: String,
    pub tested: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelJar {
    pub id: String,
    pub version: String,
    pub file: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

impl Channel {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let channel: Self = crate::error::json(bytes, "the Esteban version list")?;
        channel.check()?;
        Ok(channel)
    }

    fn check(&self) -> Result<()> {
        let bad = |why: String| Err(Error::Guard(format!("The Esteban version list {why}.")));
        if self.schema != CHANNEL_SCHEMA {
            return bad(format!(
                "has schema {}, this launcher reads {CHANNEL_SCHEMA}",
                self.schema
            ));
        }
        if !is_timestamp(&self.issued) || !is_timestamp(&self.expires) {
            return bad("has dates that aren't YYYY-MM-DDTHH:MM:SSZ".into());
        }
        if self.expires <= self.issued {
            return bad("expires before it was issued".into());
        }
        let mut seen = BTreeSet::new();
        for build in &self.builds {
            let label = format!(
                "{} {}{}",
                build.loader.kind.title(),
                build.mc,
                if build.hacked { " Hacked" } else { "" }
            );
            if !seen.insert((build.mc.as_str(), build.loader.kind, build.hacked)) {
                return bad(format!("lists {label} twice"));
            }
            if build.hacked && build.loader.kind != Loader::Fabric {
                return bad(format!("has a Hacked build outside Fabric ({label})"));
            }
            if build.loader.kind != Loader::Fabric && !build.jars.is_empty() {
                return bad(format!("lists Fabric jars for {label}"));
            }
            for jar in &build.jars {
                if jar.id == HACKS_MOD_ID && !build.hacked {
                    return bad(format!("puts the hacks mod into {label}"));
                }
                if jar.id != HACKS_MOD_ID && jar.id != HUD_MOD_ID {
                    return bad(format!("names an unknown jar {} in {label}", jar.id));
                }
                if !is_sha256(&jar.sha256) {
                    return bad(format!("has a broken sha256 for {}", jar.file));
                }
                let safe_name = jar.file.ends_with(".jar")
                    && !jar.file.starts_with('.')
                    && !jar.file.contains(['/', '\\']);
                if !safe_name {
                    return bad(format!("has an unsafe file name {}", jar.file));
                }
                if !jar.url.starts_with(JAR_HOST) || !jar.url.ends_with(&format!("/{}", jar.file)) {
                    return bad(format!(
                        "points {} somewhere other than the Esteban releases",
                        jar.file
                    ));
                }
            }
        }
        if let Some(hash) = self.retired_hacks_sha256.iter().find(|h| !is_sha256(h)) {
            return bad(format!("has a broken retired hash {hash}"));
        }
        Ok(())
    }

    pub fn build(&self, mc: &str, loader: Loader, hacked: bool) -> Option<&Build> {
        self.builds
            .iter()
            .find(|b| b.mc == mc && b.loader.kind == loader && b.hacked == hacked)
    }

    pub fn jars(&self) -> impl Iterator<Item = (&Build, &ChannelJar)> {
        self.builds
            .iter()
            .flat_map(|b| b.jars.iter().map(move |j| (b, j)))
    }

    pub fn is_known_hacks_jar(&self, sha256: &str) -> bool {
        self.jars()
            .filter(|(_, j)| j.id == HACKS_MOD_ID)
            .map(|(_, j)| j.sha256.as_str())
            .chain(self.retired_hacks_sha256.iter().map(String::as_str))
            .any(|known| known.eq_ignore_ascii_case(sha256))
    }
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

pub fn is_timestamp(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 20
        && b.iter().enumerate().all(|(i, c)| match i {
            4 | 7 => *c == b'-',
            10 => *c == b'T',
            13 | 16 => *c == b':',
            19 => *c == b'Z',
            _ => c.is_ascii_digit(),
        })
}

pub fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    utc_from_secs(secs)
}

pub fn utc_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

static BUNDLED_CHANNEL: LazyLock<Option<Arc<Channel>>> =
    LazyLock::new(|| match Channel::parse(BUNDLED.as_bytes()) {
        Ok(channel) => Some(Arc::new(channel)),
        Err(e) => {
            tracing::error!(error = %e, "the bundled Esteban version list is broken");
            None
        }
    });

static ACTIVE: RwLock<Option<Arc<Channel>>> = RwLock::new(None);

pub fn bundled() -> Option<Arc<Channel>> {
    BUNDLED_CHANNEL.clone()
}

pub fn current() -> Option<Arc<Channel>> {
    let active = match ACTIVE.read() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    active.or_else(bundled)
}

fn set_current(channel: Arc<Channel>) {
    match ACTIVE.write() {
        Ok(mut guard) => guard.replace(channel),
        Err(poisoned) => poisoned.into_inner().replace(channel),
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Bundled,
    Github,
    Saved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelReport {
    pub source: Source,
    pub sequence: u64,
    pub issued: String,
    pub expires: String,
    pub key_id: Option<String>,
    pub keys: usize,
    pub notice: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    #[serde(default)]
    highest: u64,
}

fn state_path(paths: &Paths) -> PathBuf {
    paths.home().join("channel-state.json")
}

fn saved_list(paths: &Paths) -> PathBuf {
    paths.cache().join("channel").join("versions.json")
}

fn saved_signature(paths: &Paths) -> PathBuf {
    paths.cache().join("channel").join("versions.json.minisig")
}

async fn read_state(paths: &Paths) -> State {
    match tokio::fs::read(state_path(paths)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => State::default(),
    }
}

async fn write_state(paths: &Paths, state: &State) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(state).map_err(|source| Error::Json {
        what: "the channel state".into(),
        source,
    })?;
    fsx::write_atomic(&state_path(paths), &bytes).await
}

async fn read_saved(paths: &Paths) -> Option<(Vec<u8>, String)> {
    let list = tokio::fs::read(saved_list(paths)).await.ok()?;
    let signature = tokio::fs::read_to_string(saved_signature(paths))
        .await
        .ok()?;
    Some((list, signature))
}

pub fn accept(
    bytes: &[u8],
    signature: &str,
    floor: u64,
    now: &str,
    keys: &[String],
) -> Result<(Channel, Verified)> {
    let verified = signing::verify_with(keys, Purpose::Channel, bytes, signature)?;
    let channel = Channel::parse(bytes)?;
    if channel.sequence < floor {
        return Err(Error::Guard(format!(
            "The Esteban version list on GitHub is older (sequence {}) than one this launcher already trusted ({floor}), so it was ignored.",
            channel.sequence
        )));
    }
    if channel.expires.as_str() <= now {
        return Err(Error::Guard(format!(
            "The Esteban version list expired on {}, so it was ignored.",
            &channel.expires[..10]
        )));
    }
    Ok((channel, verified))
}

enum Fetched {
    Got(Vec<u8>, String),
    NotPublished,
    Unreachable,
}

async fn fetch(net: &Net, url: &str) -> Fetched {
    let list = match net.bytes(url).await {
        Ok(bytes) => bytes.to_vec(),
        Err(Error::Status { status: 404, .. }) => return Fetched::NotPublished,
        Err(e) => {
            tracing::info!(error = %e, "the Esteban version list couldn't be fetched");
            return Fetched::Unreachable;
        }
    };
    let signature = match net.bytes(&format!("{url}.minisig")).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(Error::Status { status: 404, .. }) => String::new(),
        Err(_) => return Fetched::Unreachable,
    };
    Fetched::Got(list, signature)
}

pub async fn refresh(net: &Net, paths: &Paths) -> Result<ChannelReport> {
    let (channel, report) = refresh_with(
        net,
        paths,
        CHANNEL_URL,
        signing::built_in(Purpose::Channel),
        &utc_now(),
    )
    .await?;
    set_current(channel);
    Ok(report)
}

pub async fn refresh_with(
    net: &Net,
    paths: &Paths,
    url: &str,
    keys: &[String],
    now: &str,
) -> Result<(Arc<Channel>, ChannelReport)> {
    let built_in = bundled()
        .ok_or_else(|| Error::Guard("The built-in Esteban version list is broken.".into()))?;
    let mut state = read_state(paths).await;
    let floor = state.highest.max(built_in.sequence);
    let mut notice = None;
    let mut chosen: Option<(Arc<Channel>, Source, Verified)> = None;
    if !keys.is_empty() {
        match fetch(net, url).await {
            Fetched::Got(bytes, signature) => match accept(&bytes, &signature, floor, now, keys) {
                Ok((channel, verified)) => {
                    fsx::write_atomic(&saved_list(paths), &bytes).await?;
                    fsx::write_atomic(&saved_signature(paths), signature.as_bytes()).await?;
                    chosen = Some((Arc::new(channel), Source::Github, verified));
                }
                Err(e) => notice = Some(e.to_string()),
            },
            Fetched::NotPublished | Fetched::Unreachable => {}
        }
        if chosen.is_none()
            && let Some((bytes, signature)) = read_saved(paths).await
        {
            match accept(&bytes, &signature, floor, now, keys) {
                Ok((channel, verified)) => {
                    chosen = Some((Arc::new(channel), Source::Saved, verified));
                }
                Err(e) => tracing::warn!(error = %e, "the saved Esteban version list isn't usable"),
            }
        }
    }
    let (channel, source, key_id) = match chosen {
        Some((channel, source, verified)) => {
            if channel.sequence > state.highest {
                state.highest = channel.sequence;
                write_state(paths, &state).await?;
            }
            (channel, source, Some(verified.key_id))
        }
        None => (built_in, Source::Bundled, None),
    };
    let report = ChannelReport {
        source,
        sequence: channel.sequence,
        issued: channel.issued.clone(),
        expires: channel.expires.clone(),
        key_id,
        keys: keys.len(),
        notice,
    };
    Ok((channel, report))
}

pub fn known_hacks_jar(sha256: &str) -> bool {
    bundled().is_some_and(|c| c.is_known_hacks_jar(sha256))
        || current().is_some_and(|c| c.is_known_hacks_jar(sha256))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Channel {
        Channel::parse(BUNDLED.as_bytes()).unwrap()
    }

    fn with(edit: impl FnOnce(&mut serde_json::Value)) -> Result<Channel> {
        let mut value: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
        edit(&mut value);
        Channel::parse(&serde_json::to_vec(&value).unwrap())
    }

    #[test]
    fn the_bundled_list_parses_and_covers_every_pinned_version_both_ways() {
        let channel = sample();
        assert!(bundled().is_some());
        for v in crate::versions::PINNED_VERSIONS {
            let normal = channel.build(v.id, Loader::Fabric, false).unwrap();
            let ids: Vec<&str> = normal.jars.iter().map(|j| j.id.as_str()).collect();
            assert_eq!(ids, vec![HUD_MOD_ID]);
            let hacked = channel.build(v.id, Loader::Fabric, true).unwrap();
            let ids: Vec<&str> = hacked.jars.iter().map(|j| j.id.as_str()).collect();
            assert_eq!(ids, vec![HUD_MOD_ID, HACKS_MOD_ID]);
            assert!(channel.build(v.id, Loader::Forge, false).is_none());
        }
        assert_eq!(channel.builds.len(), 10);
        assert_eq!(channel.retired_hacks_sha256.len(), 5);
    }

    #[test]
    fn hacks_hashes_include_current_and_retired_but_never_the_hud() {
        let channel = sample();
        let (_, hacks) = channel.jars().find(|(_, j)| j.id == HACKS_MOD_ID).unwrap();
        let (_, hud) = channel.jars().find(|(_, j)| j.id == HUD_MOD_ID).unwrap();
        assert!(channel.is_known_hacks_jar(&hacks.sha256.to_uppercase()));
        assert!(!channel.is_known_hacks_jar(&hud.sha256));
        assert!(channel.is_known_hacks_jar(&channel.retired_hacks_sha256[0]));
    }

    #[test]
    fn a_normal_build_carrying_the_hacks_mod_is_refused() {
        let result = with(|v| {
            let hacks = v["builds"][1]["jars"][1].clone();
            v["builds"][0]["jars"].as_array_mut().unwrap().push(hacks);
        });
        assert!(matches!(result, Err(Error::Guard(m)) if m.contains("puts the hacks mod")));
    }

    #[test]
    fn broken_lists_are_refused() {
        type Edit = fn(&mut serde_json::Value);
        let cases: [Edit; 10] = [
            |v| v["schema"] = 3.into(),
            |v| v["builds"][0]["loader"]["kind"] = "forge".into(),
            |v| v["builds"][1]["loader"]["kind"] = "forge".into(),
            |v| v["builds"][0]["jars"][0]["sha256"] = "abc".into(),
            |v| v["builds"][0]["jars"][0]["id"] = "other".into(),
            |v| v["builds"][0]["jars"][0]["file"] = "../x.jar".into(),
            |v| {
                v["builds"][0]["jars"][0]["url"] =
                    "https://evil.example/esteban-hud-1.4.0+26.3.jar".into();
            },
            |v| v["retiredHacksSha256"][0] = "nope".into(),
            |v| v["surprise"] = true.into(),
            |v| {
                let first = v["builds"][0].clone();
                v["builds"].as_array_mut().unwrap().push(first);
            },
        ];
        for (index, edit) in cases.into_iter().enumerate() {
            assert!(with(edit).is_err(), "case {index} was accepted");
        }
    }

    #[test]
    fn dates_come_out_in_the_list_format() {
        assert_eq!(utc_from_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_from_secs(1_791_553_507), "2026-10-09T13:45:07Z");
        assert_eq!(utc_from_secs(1_709_251_199), "2024-02-29T23:59:59Z");
        assert_eq!(utc_from_secs(951_868_800), "2000-03-01T00:00:00Z");
        assert!(is_timestamp(&utc_now()));
        assert!(!is_timestamp("2026-10-09 13:45:07Z") && !is_timestamp("2026-10-09T13:45:07"));
    }

    mod remote {
        use wiremock::matchers::path;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        use super::*;
        use crate::net::Policy;
        use crate::signing::tests::TestKey;

        const NOW: &str = "2026-10-09T12:00:00Z";

        fn list(sequence: u64, issued: &str, expires: &str) -> Vec<u8> {
            let mut value: serde_json::Value = serde_json::from_str(BUNDLED).unwrap();
            value["sequence"] = sequence.into();
            value["issued"] = issued.into();
            value["expires"] = expires.into();
            serde_json::to_vec_pretty(&value).unwrap()
        }

        fn fresh(sequence: u64) -> Vec<u8> {
            list(sequence, "2026-10-09T00:00:00Z", "2099-01-01T00:00:00Z")
        }

        async fn serve(body: Option<&[u8]>, signature: Option<&str>) -> MockServer {
            let server = MockServer::start().await;
            let list = match body {
                Some(b) => ResponseTemplate::new(200).set_body_bytes(b.to_vec()),
                None => ResponseTemplate::new(404),
            };
            let sig = match signature {
                Some(s) => ResponseTemplate::new(200).set_body_string(s.to_string()),
                None => ResponseTemplate::new(404),
            };
            Mock::given(path("/versions.json"))
                .respond_with(list)
                .mount(&server)
                .await;
            Mock::given(path("/versions.json.minisig"))
                .respond_with(sig)
                .mount(&server)
                .await;
            server
        }

        #[tokio::test]
        async fn only_signed_newer_unexpired_lists_are_trusted() {
            let dir = tempfile::tempdir().unwrap();
            let paths = Paths::new(dir.path());
            let net = Net::new(Policy::local_test()).unwrap();
            let key = TestKey::new();
            let keys = vec![key.public.clone()];
            let url = |server: &MockServer| format!("{}/versions.json", server.uri());

            let five = fresh(5);
            let server = serve(Some(&five), Some(&key.sign(&five, "seq 5"))).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&server), &keys, NOW)
                .await
                .unwrap();
            assert_eq!(report.source, Source::Github);
            assert_eq!(channel.sequence, 5);
            assert!(report.notice.is_none());
            assert_eq!(report.key_id, signing::key_id(&key.public));
            assert!(saved_list(&paths).is_file() && saved_signature(&paths).is_file());
            assert_eq!(read_state(&paths).await.highest, 5);

            let gone = serve(None, None).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&gone), &keys, NOW)
                .await
                .unwrap();
            assert_eq!((report.source, channel.sequence), (Source::Saved, 5));

            let three = fresh(3);
            let old = serve(Some(&three), Some(&key.sign(&three, "seq 3"))).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&old), &keys, NOW)
                .await
                .unwrap();
            assert_eq!(channel.sequence, 5);
            assert!(report.notice.unwrap().contains("older (sequence 3)"));

            let stale = list(9, "2019-01-01T00:00:00Z", "2020-01-01T00:00:00Z");
            let expired = serve(Some(&stale), Some(&key.sign(&stale, "seq 9"))).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&expired), &keys, NOW)
                .await
                .unwrap();
            assert_eq!(channel.sequence, 5);
            assert!(report.notice.unwrap().contains("expired on 2020-01-01"));

            let seven = fresh(7);
            let stranger = TestKey::new();
            let forged = serve(Some(&seven), Some(&stranger.sign(&seven, "seq 7"))).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&forged), &keys, NOW)
                .await
                .unwrap();
            assert_eq!(channel.sequence, 5);
            assert!(report.notice.unwrap().contains("isn't signed by a key"));

            let unsigned = serve(Some(&seven), None).await;
            let (channel, report) = refresh_with(&net, &paths, &url(&unsigned), &keys, NOW)
                .await
                .unwrap();
            assert_eq!(channel.sequence, 5);
            assert!(report.notice.is_some());
            assert_eq!(read_state(&paths).await.highest, 5);
        }

        #[tokio::test]
        #[ignore = "needs a list signed with the owner's channel key in ESTEBAN_SIGNED_LIST"]
        async fn a_list_signed_with_the_real_channel_key() {
            let Some(folder) = std::env::var_os("ESTEBAN_SIGNED_LIST") else {
                return;
            };
            let folder = PathBuf::from(folder);
            let body = std::fs::read(folder.join("versions.json")).unwrap();
            let signature = std::fs::read_to_string(folder.join("versions.json.minisig")).unwrap();
            let keys = signing::built_in(Purpose::Channel);
            assert!(!keys.is_empty(), "no channel key is built in");
            let net = Net::new(Policy::local_test()).unwrap();
            let now = utc_now();
            let run = |body: Vec<u8>, signature: Option<String>| {
                let net = net.clone();
                let now = now.clone();
                async move {
                    let dir = tempfile::tempdir().unwrap();
                    let paths = Paths::new(dir.path());
                    let server = serve(Some(&body), signature.as_deref()).await;
                    let url = format!("{}/versions.json", server.uri());
                    refresh_with(&net, &paths, &url, keys, &now)
                        .await
                        .unwrap()
                        .1
                }
            };

            let real = run(body.clone(), Some(signature.clone())).await;
            println!("REAL-KEY signed as published: {real:?}");
            assert_eq!(real.source, Source::Github);
            assert!(real.notice.is_none());
            assert_eq!(real.key_id, signing::key_id(&keys[0]));

            let mut tampered = body.clone();
            let at = tampered
                .windows(8)
                .position(|w| w == b"\"sha256\"")
                .unwrap()
                + 12;
            tampered[at] = if tampered[at] == b'0' { b'1' } else { b'0' };
            let changed = run(tampered, Some(signature.clone())).await;
            println!("REAL-KEY one hash character changed: {changed:?}");
            assert_eq!(changed.source, Source::Bundled);
            assert!(changed.notice.unwrap().contains("isn't signed by a key"));

            let unsigned = run(body.clone(), None).await;
            println!("REAL-KEY no signature file: {unsigned:?}");
            assert_eq!(unsigned.source, Source::Bundled);
            assert!(unsigned.notice.is_some());

            let stranger = TestKey::new();
            let forged = run(
                body.clone(),
                Some(stranger.sign(&body, "esteban-launcher channel")),
            )
            .await;
            println!("REAL-KEY signed by another key: {forged:?}");
            assert_eq!(forged.source, Source::Bundled);
            assert!(forged.notice.unwrap().contains("isn't signed by a key"));
        }

        #[tokio::test]
        async fn without_a_built_in_key_only_the_compiled_in_list_is_used() {
            let dir = tempfile::tempdir().unwrap();
            let paths = Paths::new(dir.path());
            let net = Net::new(Policy::local_test()).unwrap();
            let key = TestKey::new();
            let five = fresh(5);
            let server = serve(Some(&five), Some(&key.sign(&five, "seq 5"))).await;
            let url = format!("{}/versions.json", server.uri());
            let (channel, report) = refresh_with(&net, &paths, &url, &[], NOW).await.unwrap();
            assert_eq!(report.source, Source::Bundled);
            assert_eq!(channel.sequence, bundled().unwrap().sequence);
            assert!(report.notice.is_none() && report.key_id.is_none());
            assert!(!saved_list(&paths).exists());
        }
    }
}
