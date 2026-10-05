use std::process::Stdio;
use std::time::{Duration, SystemTime};

use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::crash::{self, CrashSummary};
use super::plan::LaunchPlan;
use crate::error::IoContext;
use crate::{Error, Result};

#[derive(Clone, Debug)]
pub struct Smoke {
    pub markers: Vec<String>,
    pub timeout: Duration,
    pub settle: Duration,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmokeResult {
    pub markers_seen: Vec<String>,
    pub loaded_mods: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub exit_code: Option<i32>,
    pub crash: Option<CrashSummary>,
    pub smoke: Option<SmokeResult>,
}

pub async fn run(
    plan: &LaunchPlan,
    mut on_line: impl FnMut(&str) + Send,
    smoke: Option<Smoke>,
) -> Result<Outcome> {
    let started = SystemTime::now();
    let mut child = Command::new(&plan.java)
        .args(&plan.args)
        .current_dir(&plan.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .at(&plan.java)?;

    let (sender, mut lines) = mpsc::unbounded_channel::<String>();
    if let Some(stdout) = child.stdout.take() {
        forward(stdout, sender.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        forward(stderr, sender.clone());
    }
    drop(sender);

    let mut seen: Vec<String> = Vec::new();
    let mut loaded_mods: Vec<String> = Vec::new();
    let mut in_mod_list = false;
    let deadline = smoke
        .as_ref()
        .map(|s| tokio::time::Instant::now() + s.timeout);
    let mut settle_until: Option<tokio::time::Instant> = None;

    let status = loop {
        let wake = settle_until.or(deadline);
        tokio::select! {
            line = lines.recv() => {
                let Some(line) = line else {
                    break child.wait().await.at(&plan.java)?;
                };
                on_line(&line);
                if let Some(smoke) = &smoke {
                    if line.contains("Loading ") && line.contains(" mods:") {
                        in_mod_list = true;
                    } else if in_mod_list {
                        match line.trim_start().strip_prefix("- ") {
                            Some(entry) => loaded_mods.push(entry.to_string()),
                            None if line.starts_with('\t') || line.starts_with("   ") => {}
                            None => in_mod_list = false,
                        }
                    }
                    for marker in &smoke.markers {
                        if !seen.contains(marker) && line.contains(marker.as_str()) {
                            seen.push(marker.clone());
                        }
                    }
                    if settle_until.is_none() && seen.len() == smoke.markers.len() {
                        settle_until = Some(tokio::time::Instant::now() + smoke.settle);
                    }
                }
            }
            status = child.wait() => {
                break status.at(&plan.java)?;
            }
            () = sleep_until(wake) => {
                if settle_until.is_some() {
                    child.kill().await.at(&plan.java)?;
                    return Ok(Outcome {
                        exit_code: None,
                        crash: crash::summarize(&plan.cwd, started, Some(0)),
                        smoke: Some(SmokeResult { markers_seen: seen, loaded_mods }),
                    });
                }
                child.kill().await.at(&plan.java)?;
                return Err(Error::Launch(format!(
                    "the game did not reach the title screen in time (saw {} of the expected log lines)",
                    seen.len()
                )));
            }
        }
    };

    let exit_code = status.code();
    Ok(Outcome {
        exit_code,
        crash: crash::summarize(&plan.cwd, started, exit_code),
        smoke: smoke.map(|_| SmokeResult {
            markers_seen: seen,
            loaded_mods,
        }),
    })
}

async fn sleep_until(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn forward(stream: impl AsyncRead + Unpin + Send + 'static, sender: mpsc::UnboundedSender<String>) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
}
