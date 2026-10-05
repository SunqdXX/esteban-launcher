use std::collections::HashSet;
use std::path::{Path, PathBuf};

use futures::{StreamExt, stream};
use reqwest::StatusCode;
use tokio::io::AsyncWriteExt;

use crate::error::IoContext;
use crate::fsx;
use crate::hash::Hash;
use crate::net::Net;
use crate::progress::Progress;
use crate::{Error, Result};

const HASH_ATTEMPTS: usize = 3;

#[derive(Clone, Debug)]
pub struct Download {
    pub url: String,
    pub dest: PathBuf,
    pub hash: Hash,
    pub size: Option<u64>,
    pub executable: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub fetched_files: usize,
    pub fetched_bytes: u64,
    pub cached_files: usize,
}

impl Stats {
    pub fn add(&mut self, other: Stats) {
        self.fetched_files += other.fetched_files;
        self.fetched_bytes += other.fetched_bytes;
        self.cached_files += other.cached_files;
    }
}

pub async fn ensure_all(
    net: &Net,
    items: Vec<Download>,
    concurrency: usize,
    progress: &dyn Progress,
    stage: &str,
) -> Result<Stats> {
    let mut seen = HashSet::new();
    let items: Vec<Download> = items
        .into_iter()
        .filter(|d| seen.insert(d.dest.clone()))
        .collect();
    let total = items.iter().filter_map(|d| d.size).sum();
    progress.stage(stage, items.len(), total);

    let mut stats = Stats::default();
    let mut jobs = stream::iter(items.iter().map(|d| ensure_one(net, d, progress)))
        .buffer_unordered(concurrency.max(1));
    while let Some(result) = jobs.next().await {
        match result? {
            Some(bytes) => {
                stats.fetched_files += 1;
                stats.fetched_bytes += bytes;
            }
            None => stats.cached_files += 1,
        }
    }
    Ok(stats)
}

pub async fn ensure_one(
    net: &Net,
    item: &Download,
    progress: &dyn Progress,
) -> Result<Option<u64>> {
    if is_valid(item).await? {
        progress.advance(item.size.unwrap_or(0));
        progress.file_done();
        return Ok(None);
    }
    if let Some(parent) = item.dest.parent() {
        fsx::create_dir(parent).await?;
    }
    let part = fsx::part_path(&item.dest);
    let mut last_error = None;
    for _ in 0..HASH_ATTEMPTS {
        let fetched = fetch_to_part(net, item, &part, progress).await?;
        match check_part(item, &part).await {
            Ok(()) => {
                tokio::fs::rename(&part, &item.dest).await.at(&item.dest)?;
                if item.executable {
                    fsx::set_executable(&item.dest).await?;
                }
                progress.file_done();
                return Ok(Some(fetched));
            }
            Err(e) => {
                remove_quietly(&part).await;
                tracing::warn!(url = %item.url, error = %e, "download failed verification, retrying");
                last_error = Some(e);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Launch(format!("could not download {}", item.url))))
}

async fn is_valid(item: &Download) -> Result<bool> {
    match tokio::fs::metadata(&item.dest).await {
        Ok(meta) => {
            if item.size.is_some_and(|size| meta.len() != size) {
                return Ok(false);
            }
            item.hash.file_matches(&item.dest).await
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).at(&item.dest),
    }
}

async fn check_part(item: &Download, part: &Path) -> Result<()> {
    let data = tokio::fs::read(part).await.at(part)?;
    if let Some(size) = item.size
        && data.len() as u64 != size
    {
        return Err(Error::SizeMismatch {
            what: item.url.clone(),
            expected: size,
            actual: data.len() as u64,
        });
    }
    item.hash.verify(&data, &item.url)
}

async fn fetch_to_part(
    net: &Net,
    item: &Download,
    part: &Path,
    progress: &dyn Progress,
) -> Result<u64> {
    let url = net.url(&item.url)?;
    let existing = tokio::fs::metadata(part)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    let mut response = net.send(&url, (existing > 0).then_some(existing)).await?;
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        return Ok(0);
    }
    let append = existing > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    let mut file = if append {
        progress.advance(existing);
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part)
            .await
            .at(part)?
    } else {
        tokio::fs::File::create(part).await.at(part)?
    };
    let mut written = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|source| Error::Http {
        url: url.to_string(),
        source,
    })? {
        file.write_all(&chunk).await.at(part)?;
        written += chunk.len() as u64;
        progress.advance(chunk.len() as u64);
    }
    file.flush().await.at(part)?;
    Ok(written)
}

async fn remove_quietly(path: &Path) {
    if let Err(e) = tokio::fs::remove_file(path).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(path = %path.display(), error = %e, "could not remove a partial download");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::sha256_hex;
    use crate::net::Policy;
    use crate::progress::Silent;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn item(server: &MockServer, dir: &Path, name: &str, body: &[u8]) -> Download {
        Download {
            url: format!("{}/{name}", server.uri()),
            dest: dir.join(name),
            hash: Hash::sha256(&sha256_hex(body)),
            size: Some(body.len() as u64),
            executable: false,
        }
    }

    #[tokio::test]
    async fn downloads_verifies_then_skips_when_cached() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/a.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let net = Net::new(Policy::local_test()).unwrap();
        let d = item(&server, dir.path(), "a.bin", b"hello");
        let first = ensure_all(&net, vec![d.clone()], 4, &Silent, "t")
            .await
            .unwrap();
        assert_eq!(first.fetched_files, 1);
        let second = ensure_all(&net, vec![d.clone()], 4, &Silent, "t")
            .await
            .unwrap();
        assert_eq!(
            second,
            Stats {
                fetched_files: 0,
                fetched_bytes: 0,
                cached_files: 1
            }
        );
        assert_eq!(std::fs::read(&d.dest).unwrap(), b"hello");
    }

    #[tokio::test]
    async fn corrupted_file_is_fetched_again() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/b.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"world".to_vec()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let net = Net::new(Policy::local_test()).unwrap();
        let d = item(&server, dir.path(), "b.bin", b"world");
        std::fs::write(&d.dest, b"wurld").unwrap();
        let stats = ensure_all(&net, vec![d.clone()], 1, &Silent, "t")
            .await
            .unwrap();
        assert_eq!(stats.fetched_files, 1);
        assert_eq!(std::fs::read(&d.dest).unwrap(), b"world");
    }

    #[tokio::test]
    async fn bad_payload_is_never_kept() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/c.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"tampered".to_vec()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let net = Net::new(Policy::local_test()).unwrap();
        let mut d = item(&server, dir.path(), "c.bin", b"original");
        d.size = None;
        let result = ensure_all(&net, vec![d.clone()], 1, &Silent, "t").await;
        assert!(matches!(result, Err(Error::HashMismatch { .. })));
        assert!(!d.dest.exists());
        assert!(!fsx::part_path(&d.dest).exists());
    }

    #[tokio::test]
    async fn resumes_a_partial_download_with_a_range_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/d.bin"))
            .and(header("range", "bytes=3-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(b"def".to_vec()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let net = Net::new(Policy::local_test()).unwrap();
        let d = item(&server, dir.path(), "d.bin", b"abcdef");
        std::fs::write(fsx::part_path(&d.dest), b"abc").unwrap();
        ensure_all(&net, vec![d.clone()], 1, &Silent, "t")
            .await
            .unwrap();
        assert_eq!(std::fs::read(&d.dest).unwrap(), b"abcdef");
    }

    #[tokio::test]
    async fn refuses_hosts_outside_the_policy() {
        let dir = tempfile::tempdir().unwrap();
        let net = Net::launcher().unwrap();
        let d = Download {
            url: "https://example.com/x".into(),
            dest: dir.path().join("x"),
            hash: Hash::sha1("00"),
            size: None,
            executable: false,
        };
        let result = ensure_all(&net, vec![d], 1, &Silent, "t").await;
        assert!(matches!(result, Err(Error::HostNotAllowed(_))));
    }
}
