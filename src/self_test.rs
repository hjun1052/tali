use crate::doctor;
use crate::store::Store;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;

#[derive(Debug, Serialize)]
pub struct SelfTestReport {
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub status: SelfTestStatus,
    pub checks: Vec<SelfTestCheck>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SelfTestStatus {
    Passed,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct SelfTestCheck {
    pub name: String,
    pub status: SelfTestStatus,
    pub detail: String,
}

pub fn run(store: &Store) -> SelfTestReport {
    let started_at = Utc::now();
    let mut checks = Vec::new();
    checks.push(check_doctor());
    checks.extend(check_store_layout(store));
    checks.push(check_cache_write(store));
    checks.push(check_completion_generation());
    let status = if checks
        .iter()
        .all(|check| check.status == SelfTestStatus::Passed)
    {
        SelfTestStatus::Passed
    } else {
        SelfTestStatus::Failed
    };

    SelfTestReport {
        started_at,
        ended_at: Utc::now(),
        status,
        checks,
    }
}

pub fn print_report(report: &SelfTestReport) {
    println!("Tali self-test");
    println!("Status: {:?}", report.status);
    for check in &report.checks {
        println!("- {}: {:?} ({})", check.name, check.status, check.detail);
    }
}

pub fn print_json(report: &SelfTestReport) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}

fn check_doctor() -> SelfTestCheck {
    let info = doctor::capture();
    if info.os.is_empty() || info.architecture.is_empty() {
        failed("doctor", "platform information is incomplete")
    } else {
        passed(
            "doctor",
            format!(
                "{} / {} / tali {}",
                info.os, info.architecture, info.tali_version
            ),
        )
    }
}

fn check_store_layout(store: &Store) -> Vec<SelfTestCheck> {
    [
        ("manifests_dir", store.manifests_dir()),
        ("runs_dir", store.runs_dir()),
        ("logs_dir", store.logs_dir()),
        ("cache_dir", store.cache_dir()),
        ("secrets_dir", store.secrets_dir()),
    ]
    .into_iter()
    .map(|(name, path)| {
        if path.is_dir() {
            passed(name, path.display().to_string())
        } else {
            failed(name, format!("missing directory {}", path.display()))
        }
    })
    .collect()
}

fn check_cache_write(store: &Store) -> SelfTestCheck {
    let path = store.cache_dir().join("self-test.tmp");
    match write_and_remove(&path) {
        Ok(()) => passed("cache_write", path.display().to_string()),
        Err(error) => failed("cache_write", error.to_string()),
    }
}

fn check_completion_generation() -> SelfTestCheck {
    let mut command = crate::cli::command();
    let mut output = Vec::new();
    clap_complete::generate(
        clap_complete::Shell::Bash,
        &mut command,
        "tali",
        &mut output,
    );
    if output.is_empty() {
        failed("completion_generation", "generated completion was empty")
    } else {
        passed("completion_generation", format!("{} bytes", output.len()))
    }
}

fn write_and_remove(path: &std::path::Path) -> Result<()> {
    fs::write(path, b"ok").with_context(|| format!("failed to write {}", path.display()))?;
    fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))?;
    Ok(())
}

fn passed(name: impl Into<String>, detail: impl Into<String>) -> SelfTestCheck {
    SelfTestCheck {
        name: name.into(),
        status: SelfTestStatus::Passed,
        detail: detail.into(),
    }
}

fn failed(name: impl Into<String>, detail: impl Into<String>) -> SelfTestCheck {
    SelfTestCheck {
        name: name.into(),
        status: SelfTestStatus::Failed,
        detail: detail.into(),
    }
}
