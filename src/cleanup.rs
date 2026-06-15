use crate::logs::read_run_log;
use crate::store::Store;
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CleanupOptions {
    pub older_than: String,
    pub dry_run: bool,
    pub yes: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CleanupKind {
    Run,
    Cache,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupCandidate {
    pub kind: CleanupKind,
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanupReport {
    pub older_than: String,
    pub cutoff: DateTime<Utc>,
    pub dry_run: bool,
    pub deleted: bool,
    pub candidates: Vec<CleanupCandidate>,
    pub deleted_count: usize,
    pub skipped_running_runs: Vec<String>,
    pub bytes_to_free: u64,
    pub bytes_freed: u64,
}

pub fn cleanup(store: &Store, options: CleanupOptions) -> Result<CleanupReport> {
    let age = parse_age(&options.older_than)?;
    let cutoff = Utc::now() - age;
    let running = store.latest_running_run_id()?.unwrap_or_default();
    let mut skipped_running_runs = Vec::new();
    let candidates = collect_candidates(store, cutoff, &running, &mut skipped_running_runs)?;
    let bytes_to_free = candidates.iter().map(|candidate| candidate.bytes).sum();
    let should_delete = options.yes && !options.dry_run;
    let mut bytes_freed = 0;
    let mut deleted_count = 0;
    let deleted_run_ids: Vec<String> = candidates
        .iter()
        .filter(|candidate| candidate.kind == CleanupKind::Run)
        .map(|candidate| candidate.id.clone())
        .collect();

    if should_delete {
        for candidate in &candidates {
            let path = PathBuf::from(&candidate.path);
            remove_entry(&path).with_context(|| format!("failed to delete {}", path.display()))?;
            bytes_freed += candidate.bytes;
            deleted_count += 1;
        }
        refresh_latest_pointer(store, &deleted_run_ids)?;
    }

    Ok(CleanupReport {
        older_than: options.older_than,
        cutoff,
        dry_run: !should_delete,
        deleted: should_delete,
        candidates,
        deleted_count,
        skipped_running_runs,
        bytes_to_free,
        bytes_freed,
    })
}

pub fn print_report(report: &CleanupReport) {
    let run_count = report
        .candidates
        .iter()
        .filter(|candidate| candidate.kind == CleanupKind::Run)
        .count();
    let cache_count = report
        .candidates
        .iter()
        .filter(|candidate| candidate.kind == CleanupKind::Cache)
        .count();

    if report.deleted {
        println!("Cleanup complete:");
        println!("Deleted run directories: {run_count}");
        println!("Deleted cache entries: {cache_count}");
        println!("Freed: {}", format_bytes(report.bytes_freed));
    } else {
        println!("Cleanup preview:");
        println!("Runs older than {}: {run_count}", report.older_than);
        println!(
            "Cache entries older than {}: {cache_count}",
            report.older_than
        );
        println!(
            "Estimated space to free: {}",
            format_bytes(report.bytes_to_free)
        );
        if !report.skipped_running_runs.is_empty() {
            println!(
                "Skipped running runs: {}",
                report.skipped_running_runs.join(", ")
            );
        }
        println!();
        println!("Nothing deleted. Run with:");
        println!("tali cleanup --older-than {} --yes", report.older_than);
    }
}

fn collect_candidates(
    store: &Store,
    cutoff: DateTime<Utc>,
    running_run_id: &str,
    skipped_running_runs: &mut Vec<String>,
) -> Result<Vec<CleanupCandidate>> {
    let mut candidates = Vec::new();
    collect_run_candidates(
        store,
        cutoff,
        running_run_id,
        skipped_running_runs,
        &mut candidates,
    )?;
    collect_cache_candidates(store, cutoff, &mut candidates)?;
    candidates.sort_by(|a, b| {
        a.timestamp
            .cmp(&b.timestamp)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(candidates)
}

fn collect_run_candidates(
    store: &Store,
    cutoff: DateTime<Utc>,
    running_run_id: &str,
    skipped_running_runs: &mut Vec<String>,
    candidates: &mut Vec<CleanupCandidate>,
) -> Result<()> {
    let runs_dir = store.runs_dir();
    for entry in read_children(&runs_dir)? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if id == running_run_id {
            skipped_running_runs.push(id);
            continue;
        }
        let timestamp = run_timestamp(&path)?;
        if timestamp >= cutoff {
            continue;
        }
        candidates.push(CleanupCandidate {
            kind: CleanupKind::Run,
            id,
            bytes: entry_size(&path)?,
            path: path.display().to_string(),
            timestamp,
        });
    }
    Ok(())
}

fn collect_cache_candidates(
    store: &Store,
    cutoff: DateTime<Utc>,
    candidates: &mut Vec<CleanupCandidate>,
) -> Result<()> {
    let cache_dir = store.cache_dir();
    for entry in read_children(&cache_dir)? {
        let path = entry.path();
        let timestamp = filesystem_timestamp(&path)?;
        if timestamp >= cutoff {
            continue;
        }
        candidates.push(CleanupCandidate {
            kind: CleanupKind::Cache,
            id: entry.file_name().to_string_lossy().to_string(),
            bytes: entry_size(&path)?,
            path: path.display().to_string(),
            timestamp,
        });
    }
    Ok(())
}

fn read_children(path: &Path) -> Result<Vec<fs::DirEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read entry under {}", path.display()))
}

fn run_timestamp(run_dir: &Path) -> Result<DateTime<Utc>> {
    match read_run_log(run_dir) {
        Ok(run) => Ok(run.ended_at.unwrap_or(run.started_at)),
        Err(_) => filesystem_timestamp(run_dir),
    }
}

fn filesystem_timestamp(path: &Path) -> Result<DateTime<Utc>> {
    let modified = fs::metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?
        .modified()
        .with_context(|| format!("failed to read modified time for {}", path.display()))?;
    Ok(DateTime::<Utc>::from(modified))
}

fn entry_size(path: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if metadata.is_symlink() {
        return Ok(0);
    }
    if metadata.is_dir() {
        let mut total = 0;
        for entry in
            fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))?
        {
            total += entry_size(&entry?.path())?;
        }
        return Ok(total);
    }
    Ok(0)
}

fn remove_entry(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    if metadata.is_dir() && !metadata.is_symlink() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn refresh_latest_pointer(store: &Store, deleted_run_ids: &[String]) -> Result<()> {
    if deleted_run_ids.is_empty() {
        return Ok(());
    }
    let Ok(latest) = store.latest_run_id() else {
        return Ok(());
    };
    if !deleted_run_ids.iter().any(|run_id| run_id == &latest) {
        return Ok(());
    }
    if let Some(new_latest) = newest_remaining_run(store)? {
        store.set_latest_run(&new_latest)?;
    } else {
        let latest_path = store.logs_dir().join("latest");
        if latest_path.exists() {
            fs::remove_file(&latest_path)
                .with_context(|| format!("failed to remove {}", latest_path.display()))?;
        }
    }
    Ok(())
}

fn newest_remaining_run(store: &Store) -> Result<Option<String>> {
    let mut newest: Option<(String, DateTime<Utc>)> = None;
    for entry in read_children(&store.runs_dir())? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        let timestamp = run_timestamp(&path)?;
        if newest
            .as_ref()
            .is_none_or(|(_, current_timestamp)| timestamp > *current_timestamp)
        {
            newest = Some((id, timestamp));
        }
    }
    Ok(newest.map(|(id, _)| id))
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

fn parse_age(value: &str) -> Result<Duration> {
    let value = value.trim();
    if value.is_empty() {
        bail!("age cannot be empty");
    }
    let split_at = value
        .find(|ch: char| !ch.is_ascii_digit())
        .unwrap_or(value.len());
    let (amount, unit) = value.split_at(split_at);
    if amount.is_empty() {
        bail!("age must start with a number");
    }
    let amount: i64 = amount.parse().context("age number is invalid")?;
    if amount < 0 {
        bail!("age cannot be negative");
    }
    match unit {
        "" | "d" | "day" | "days" => Ok(Duration::days(amount)),
        "h" | "hour" | "hours" => Ok(Duration::hours(amount)),
        "m" | "min" | "mins" | "minute" | "minutes" => Ok(Duration::minutes(amount)),
        "s" | "sec" | "secs" | "second" | "seconds" => Ok(Duration::seconds(amount)),
        _ => bail!("unsupported age unit '{unit}'; use s, m, h, or d"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_age_units() {
        assert_eq!(parse_age("30d").unwrap(), Duration::days(30));
        assert_eq!(parse_age("12h").unwrap(), Duration::hours(12));
        assert_eq!(parse_age("15m").unwrap(), Duration::minutes(15));
        assert_eq!(parse_age("20s").unwrap(), Duration::seconds(20));
        assert!(parse_age("old").is_err());
    }

    #[test]
    fn formats_byte_counts() {
        assert_eq!(format_bytes(42), "42 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }
}
