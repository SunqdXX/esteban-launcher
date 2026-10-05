use std::time::Duration;

use bytes::Bytes;
use reqwest::{Response, StatusCode, Url, header};
use serde::de::DeserializeOwned;

use crate::hash::Hash;
use crate::{Error, Result};

pub const USER_AGENT: &str = concat!(
    "esteban-launcher/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/SunqdXX/esteban-launcher)"
);

pub const ALLOWED_HOSTS: &[&str] = &[
    "piston-meta.mojang.com",
    "piston-data.mojang.com",
    "libraries.minecraft.net",
    "resources.download.minecraft.net",
    "meta.fabricmc.net",
    "maven.fabricmc.net",
    "api.modrinth.com",
    "cdn.modrinth.com",
    "login.microsoftonline.com",
    "user.auth.xboxlive.com",
    "xsts.auth.xboxlive.com",
    "api.minecraftservices.com",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

const ATTEMPTS: u32 = 4;
const MAX_WAIT: Duration = Duration::from_secs(60);
const LOW_BUDGET: u64 = 5;

#[derive(Clone, Debug)]
pub struct Policy {
    hosts: Vec<String>,
    https_only: bool,
}

impl Policy {
    pub fn launcher() -> Self {
        Self {
            hosts: ALLOWED_HOSTS.iter().map(|h| (*h).to_string()).collect(),
            https_only: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn local_test() -> Self {
        Self {
            hosts: vec!["127.0.0.1".into()],
            https_only: false,
        }
    }

    fn check(&self, url: &Url) -> Result<()> {
        if self.https_only && url.scheme() != "https" {
            return Err(Error::Insecure(url.to_string()));
        }
        let host = url
            .host_str()
            .ok_or_else(|| Error::BadUrl(url.to_string()))?;
        if self.hosts.iter().any(|h| h == host) {
            Ok(())
        } else {
            Err(Error::HostNotAllowed(host.to_string()))
        }
    }
}

#[derive(Clone)]
pub struct Net {
    client: reqwest::Client,
    policy: Policy,
}

impl Net {
    pub fn launcher() -> Result<Self> {
        Self::new(Policy::launcher())
    }

    pub fn new(policy: Policy) -> Result<Self> {
        let redirects = policy.clone();
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(30))
            .https_only(policy.https_only)
            .redirect(reqwest::redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("too many redirects");
                }
                match redirects.check(attempt.url()) {
                    Ok(()) => attempt.follow(),
                    Err(e) => attempt.error(e.to_string()),
                }
            }))
            .build()
            .map_err(|source| Error::Http {
                url: String::new(),
                source,
            })?;
        Ok(Self { client, policy })
    }

    pub fn url(&self, raw: &str) -> Result<Url> {
        let url = Url::parse(raw).map_err(|_| Error::BadUrl(raw.to_string()))?;
        self.policy.check(&url)?;
        Ok(url)
    }

    pub async fn send(&self, url: &Url, range_from: Option<u64>) -> Result<Response> {
        let mut attempt = 0;
        loop {
            attempt += 1;
            let mut request = self.client.get(url.clone());
            if let Some(from) = range_from {
                request = request.header(header::RANGE, format!("bytes={from}-"));
            }
            match request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() || status == StatusCode::RANGE_NOT_SATISFIABLE {
                        pace(&response).await;
                        return Ok(response);
                    }
                    let retryable =
                        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                    if !retryable || attempt >= ATTEMPTS {
                        return Err(Error::Status {
                            url: url.to_string(),
                            status: status.as_u16(),
                        });
                    }
                    tokio::time::sleep(wait_for(&response, attempt)).await;
                }
                Err(source) => {
                    let retryable = source.is_timeout() || source.is_connect();
                    if !retryable || attempt >= ATTEMPTS {
                        return Err(Error::Http {
                            url: url.to_string(),
                            source,
                        });
                    }
                    tokio::time::sleep(backoff(attempt)).await;
                }
            }
        }
    }

    pub async fn bytes(&self, raw: &str) -> Result<Bytes> {
        let url = self.url(raw)?;
        let response = self.send(&url, None).await?;
        response.bytes().await.map_err(|source| Error::Http {
            url: url.to_string(),
            source,
        })
    }

    pub async fn json<T: DeserializeOwned>(&self, raw: &str, what: &str) -> Result<T> {
        crate::error::json(&self.bytes(raw).await?, what)
    }

    pub async fn verified_json<T: DeserializeOwned>(
        &self,
        raw: &str,
        what: &str,
        hash: &Hash,
    ) -> Result<(T, Bytes)> {
        let bytes = self.bytes(raw).await?;
        hash.verify(&bytes, what)?;
        Ok((crate::error::json(&bytes, what)?, bytes))
    }
}

fn header_u64(response: &Response, name: &str) -> Option<u64> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
}

fn backoff(attempt: u32) -> Duration {
    Duration::from_millis(500u64 << attempt.min(6)).min(MAX_WAIT)
}

fn wait_for(response: &Response, attempt: u32) -> Duration {
    let limited = response.status() == StatusCode::TOO_MANY_REQUESTS;
    header_u64(response, "retry-after")
        .or_else(|| {
            limited
                .then(|| header_u64(response, "x-ratelimit-reset"))
                .flatten()
        })
        .map(Duration::from_secs)
        .unwrap_or_else(|| backoff(attempt))
        .min(MAX_WAIT)
}

async fn pace(response: &Response) {
    if let (Some(remaining), Some(reset)) = (
        header_u64(response, "x-ratelimit-remaining"),
        header_u64(response, "x-ratelimit-reset"),
    ) && remaining < LOW_BUDGET
    {
        tracing::info!(remaining, reset, "rate limit budget low, waiting");
        tokio::time::sleep(Duration::from_secs(reset).min(MAX_WAIT)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_rejects_plain_http_and_unknown_hosts() {
        let policy = Policy::launcher();
        let ok = Url::parse("https://piston-meta.mojang.com/x").unwrap();
        assert!(policy.check(&ok).is_ok());
        let http = Url::parse("http://piston-meta.mojang.com/x").unwrap();
        assert!(matches!(policy.check(&http), Err(Error::Insecure(_))));
        let other = Url::parse("https://example.com/x").unwrap();
        assert!(matches!(
            policy.check(&other),
            Err(Error::HostNotAllowed(_))
        ));
        let lookalike = Url::parse("https://piston-meta.mojang.com.evil.net/x").unwrap();
        assert!(matches!(
            policy.check(&lookalike),
            Err(Error::HostNotAllowed(_))
        ));
    }

    #[test]
    fn allowed_hosts_match_the_network_doc() {
        let doc = include_str!("../../../docs/network.md");
        for host in ALLOWED_HOSTS {
            assert!(
                doc.contains(&format!("`{host}`")),
                "{host} missing from docs/network.md"
            );
        }
    }

    #[test]
    fn user_agent_names_the_launcher_and_a_contact() {
        assert!(USER_AGENT.starts_with("esteban-launcher/"));
        assert!(USER_AGENT.contains("github.com/SunqdXX/esteban-launcher"));
    }
}
