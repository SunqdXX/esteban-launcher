use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrashSummary {
    pub headline: String,
    pub detail: Option<String>,
    pub report: Option<PathBuf>,
}

pub fn summarize(
    instance_dir: &Path,
    since: SystemTime,
    exit_code: Option<i32>,
) -> Option<CrashSummary> {
    if let Some(report) = newest(&instance_dir.join("crash-reports"), since, |n| {
        n.ends_with(".txt")
    }) {
        let text = std::fs::read_to_string(&report).unwrap_or_default();
        let (headline, detail) = parse_crash_report(&text);
        return Some(CrashSummary {
            headline,
            detail,
            report: Some(report),
        });
    }
    if let Some(report) = newest(instance_dir, since, |n| {
        n.starts_with("hs_err_pid") && n.ends_with(".log")
    }) {
        let text = std::fs::read_to_string(&report).unwrap_or_default();
        let detail = text
            .lines()
            .skip_while(|l| !l.starts_with("# Problematic frame:"))
            .nth(1)
            .map(|l| l.trim_start_matches('#').trim().to_string());
        return Some(CrashSummary {
            headline: "The Java runtime crashed (native crash).".into(),
            detail,
            report: Some(report),
        });
    }
    match exit_code {
        Some(0) => None,
        Some(code) => Some(CrashSummary {
            headline: format!("The game stopped with exit code {code}."),
            detail: None,
            report: None,
        }),
        None => Some(CrashSummary {
            headline: "The game was stopped by a signal.".into(),
            detail: None,
            report: None,
        }),
    }
}

pub fn parse_crash_report(text: &str) -> (String, Option<String>) {
    let headline = text
        .lines()
        .find_map(|l| l.strip_prefix("Description: "))
        .map(|d| format!("The game crashed: {}", d.trim()))
        .unwrap_or_else(|| "The game crashed.".to_string());
    let detail = text
        .lines()
        .skip_while(|l| !l.starts_with("Description: "))
        .skip(1)
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string);
    (headline, detail)
}

fn newest(dir: &Path, since: SystemTime, matches: impl Fn(&str) -> bool) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| matches(&e.file_name().to_string_lossy()))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            (modified >= since).then_some((modified, e.path()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_description_and_the_exception() {
        let text = "---- Minecraft Crash Report ----\n// joke\n\nTime: x\nDescription: Rendering overlay\n\njava.lang.NullPointerException: boom\n\tat a.b(c)\n";
        let (headline, detail) = parse_crash_report(text);
        assert_eq!(headline, "The game crashed: Rendering overlay");
        assert_eq!(
            detail.as_deref(),
            Some("java.lang.NullPointerException: boom")
        );
    }

    #[test]
    fn clean_exit_is_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        assert!(summarize(dir.path(), SystemTime::now(), Some(0)).is_none());
        let s = summarize(dir.path(), SystemTime::now(), Some(1)).unwrap();
        assert_eq!(s.headline, "The game stopped with exit code 1.");
    }

    #[test]
    fn finds_a_new_crash_report() {
        let dir = tempfile::tempdir().unwrap();
        let since = SystemTime::now() - std::time::Duration::from_secs(5);
        std::fs::create_dir_all(dir.path().join("crash-reports")).unwrap();
        std::fs::write(
            dir.path().join("crash-reports").join("crash-1-client.txt"),
            "Description: Initializing game\n\njava.lang.RuntimeException: mod x\n",
        )
        .unwrap();
        let s = summarize(dir.path(), since, Some(255)).unwrap();
        assert_eq!(s.headline, "The game crashed: Initializing game");
        assert_eq!(
            s.detail.as_deref(),
            Some("java.lang.RuntimeException: mod x")
        );
        assert!(s.report.is_some());
    }
}
