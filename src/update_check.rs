use crate::store::Store;
use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::process::{Command, Stdio};

const DEFAULT_REPO: &str = "hjun1052/tali";
const CHECK_INTERVAL_HOURS: i64 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheckStatus {
    UpToDate,
    UpdateAvailable { latest_version: String },
    Unknown,
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateCache {
    checked_at: DateTime<Utc>,
    latest_version: Option<String>,
    notified_at: Option<DateTime<Utc>>,
    notified_version: Option<String>,
}

pub fn check_now(store: &Store) -> Result<UpdateCheckStatus> {
    if disabled() {
        return Ok(UpdateCheckStatus::Disabled);
    }
    let latest_version = fetch_latest_version().ok();
    write_cache(store, latest_version.as_deref())?;
    Ok(status_for_latest(latest_version.as_deref()))
}

pub fn maybe_check_and_print(store: &Store) {
    if disabled() {
        return;
    }
    let Ok(Some(cache)) = read_cache(store) else {
        if let Ok(status) = check_now(store) {
            maybe_print_available(store, &status);
        }
        return;
    };
    if Utc::now() - cache.checked_at < Duration::hours(CHECK_INTERVAL_HOURS) {
        maybe_print_available(store, &status_for_latest(cache.latest_version.as_deref()));
        return;
    }
    if let Ok(status) = check_now(store) {
        maybe_print_available(store, &status);
    }
}

pub fn print_manual_status(status: &UpdateCheckStatus) {
    match status {
        UpdateCheckStatus::UpdateAvailable { latest_version } => {
            println!("Update available: tali {latest_version}");
            println!("Run:");
            println!("tali update");
        }
        UpdateCheckStatus::UpToDate => {
            println!("Tali is up to date.");
        }
        UpdateCheckStatus::Unknown => {
            println!("Could not check for updates.");
        }
        UpdateCheckStatus::Disabled => {
            println!("Update checks are disabled.");
        }
    }
}

fn maybe_print_available(store: &Store, status: &UpdateCheckStatus) {
    if let UpdateCheckStatus::UpdateAvailable { latest_version } = status {
        if !should_notify(store, latest_version) {
            return;
        }
        println!();
        println!("Update available: tali {latest_version}");
        println!("Run:");
        println!("tali update");
        let _ = mark_notified(store, latest_version);
    }
}

fn disabled() -> bool {
    env_flag("TALI_NO_UPDATE_CHECK")
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn fetch_latest_version() -> Result<String> {
    if let Ok(version) = env::var("TALI_UPDATE_CHECK_LATEST_VERSION") {
        return Ok(normalize_version(&version).to_string());
    }
    let repo = env::var("TALI_UPDATE_CHECK_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    let url = env::var("TALI_UPDATE_CHECK_URL")
        .unwrap_or_else(|_| format!("https://api.github.com/repos/{repo}/releases/latest"));
    let output = fetch_url(&url).context("failed to fetch latest release metadata")?;
    parse_latest_version(&output).context("latest release metadata did not include a version")
}

fn fetch_url(url: &str) -> Result<String> {
    if cfg!(windows) {
        let script = concat!(
            "$ProgressPreference = 'SilentlyContinue'; ",
            "(Invoke-RestMethod -TimeoutSec 2 -Uri $env:TALI_UPDATE_CHECK_URL).tag_name"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .env("TALI_UPDATE_CHECK_URL", url)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .context("failed to run powershell")?;
        if output.status.success() {
            return Ok(String::from_utf8_lossy(&output.stdout).to_string());
        }
    } else {
        if command_exists("curl") {
            let output = Command::new("curl")
                .args([
                    "-fsSL",
                    "--max-time",
                    "2",
                    "-H",
                    "User-Agent: tali-update-check",
                    url,
                ])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .context("failed to run curl")?;
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
        }
        if command_exists("wget") {
            let output = Command::new("wget")
                .args(["-qO-", "--timeout=2", url])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .context("failed to run wget")?;
            if output.status.success() {
                return Ok(String::from_utf8_lossy(&output.stdout).to_string());
            }
        }
    }
    anyhow::bail!("no update metadata fetcher succeeded")
}

fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn parse_latest_version(output: &str) -> Result<String> {
    let trimmed = output.trim();
    if trimmed.starts_with('{') {
        let value: serde_json::Value = serde_json::from_str(trimmed)?;
        if let Some(tag_name) = value.get("tag_name").and_then(|value| value.as_str()) {
            return Ok(normalize_version(tag_name).to_string());
        }
    }
    let first_line = trimmed.lines().next().unwrap_or_default().trim();
    if first_line.is_empty() {
        anyhow::bail!("empty latest version");
    }
    Ok(normalize_version(first_line).to_string())
}

fn status_for_latest(latest_version: Option<&str>) -> UpdateCheckStatus {
    let Some(latest_version) = latest_version else {
        return UpdateCheckStatus::Unknown;
    };
    if version_greater(latest_version, env!("CARGO_PKG_VERSION")) {
        UpdateCheckStatus::UpdateAvailable {
            latest_version: latest_version.to_string(),
        }
    } else {
        UpdateCheckStatus::UpToDate
    }
}

fn normalize_version(version: &str) -> &str {
    version.trim().trim_start_matches('v')
}

fn version_greater(candidate: &str, current: &str) -> bool {
    parse_version(candidate) > parse_version(current)
}

fn parse_version(version: &str) -> Vec<u64> {
    normalize_version(version)
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .take(3)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

fn cache_path(store: &Store) -> std::path::PathBuf {
    store.cache_dir().join("update-check.json")
}

fn read_cache(store: &Store) -> Result<Option<UpdateCache>> {
    let path = cache_path(store);
    if !path.exists() {
        return Ok(None);
    }
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(Some(serde_json::from_str(&content)?))
}

fn write_cache(store: &Store, latest_version: Option<&str>) -> Result<()> {
    let previous = read_cache(store).ok().flatten();
    let cache = UpdateCache {
        checked_at: Utc::now(),
        latest_version: latest_version.map(str::to_string),
        notified_at: previous.as_ref().and_then(|cache| cache.notified_at),
        notified_version: previous.and_then(|cache| cache.notified_version),
    };
    let path = cache_path(store);
    let json = serde_json::to_string_pretty(&cache)?;
    fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn should_notify(store: &Store, latest_version: &str) -> bool {
    let Ok(Some(cache)) = read_cache(store) else {
        return true;
    };
    if cache.notified_version.as_deref() != Some(latest_version) {
        return true;
    }
    let Some(notified_at) = cache.notified_at else {
        return true;
    };
    Utc::now() - notified_at >= Duration::hours(CHECK_INTERVAL_HOURS)
}

fn mark_notified(store: &Store, latest_version: &str) -> Result<()> {
    let Some(mut cache) = read_cache(store)? else {
        return Ok(());
    };
    cache.notified_at = Some(Utc::now());
    cache.notified_version = Some(latest_version.to_string());
    let path = cache_path(store);
    let json = serde_json::to_string_pretty(&cache)?;
    fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_github_latest_release_json() {
        let version = parse_latest_version(r#"{"tag_name":"v0.1.5"}"#).unwrap();
        assert_eq!(version, "0.1.5");
    }

    #[test]
    fn parses_plain_tag_output() {
        let version = parse_latest_version("v0.2.0\n").unwrap();
        assert_eq!(version, "0.2.0");
    }

    #[test]
    fn compares_versions() {
        assert!(version_greater("0.1.5", "0.1.4"));
        assert!(version_greater("1.0.0", "0.9.9"));
        assert!(!version_greater("0.1.4", "0.1.4"));
        assert!(!version_greater("0.1.3", "0.1.4"));
    }

    #[test]
    fn writes_update_cache() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();

        write_cache(&store, Some("9.9.9")).unwrap();
        let cache = read_cache(&store).unwrap().unwrap();

        assert_eq!(cache.latest_version.as_deref(), Some("9.9.9"));
    }

    #[test]
    fn suppresses_repeated_update_notifications_for_same_version() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        write_cache(&store, Some("9.9.9")).unwrap();

        assert!(should_notify(&store, "9.9.9"));
        mark_notified(&store, "9.9.9").unwrap();
        assert!(!should_notify(&store, "9.9.9"));
        assert!(should_notify(&store, "9.9.10"));
    }
}
