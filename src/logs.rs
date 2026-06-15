use crate::doctor::PlatformInfo;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Running,
    Success,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunLog {
    pub run_id: String,
    pub manifest_id: Option<String>,
    pub manifest_name: String,
    pub total_steps: usize,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub platform: PlatformInfo,
    pub steps: Vec<StepLog>,
    pub failed_step_index: Option<usize>,
    pub stdout_log: String,
    pub stderr_log: String,
    #[serde(default)]
    pub events_log: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepLog {
    pub index: usize,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub step_type: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: StepStatus,
    pub command: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub skip_reason: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout_snippet: Option<String>,
    pub stderr_snippet: Option<String>,
    pub backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiRunSummary {
    pub run_id: String,
    pub manifest_id: Option<String>,
    pub manifest_name: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub platform: PlatformInfo,
    pub failed_step: Option<StepLog>,
    pub steps: Vec<StepLog>,
    pub stdout_log: String,
    pub stderr_log: String,
    pub events_log: String,
    pub run_dir: String,
}

pub fn write_run_log(run_dir: &Path, run: &RunLog) -> Result<PathBuf> {
    let path = run_dir.join("run.json");
    let json = serde_json::to_string_pretty(run)?;
    fs::write(&path, json).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(path)
}

pub fn append_event(event_path: &Path, event: serde_json::Value) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(event_path)
        .with_context(|| format!("failed to open {}", event_path.display()))?;
    writeln!(file, "{}", serde_json::to_string(&event)?)
        .with_context(|| format!("failed to write {}", event_path.display()))?;
    Ok(())
}

pub fn run_started_event(run_id: &str, manifest_name: &str) -> serde_json::Value {
    json!({
        "type": "run_started",
        "run_id": run_id,
        "manifest_name": manifest_name,
        "at": Utc::now(),
    })
}

pub fn run_finished_event(run_id: &str, status: &RunStatus) -> serde_json::Value {
    json!({
        "type": "run_finished",
        "run_id": run_id,
        "status": status,
        "at": Utc::now(),
    })
}

pub fn step_started_event(index: usize, name: Option<&str>, step_type: &str) -> serde_json::Value {
    json!({
        "type": "step_started",
        "index": index,
        "name": name,
        "step_type": step_type,
        "at": Utc::now(),
    })
}

pub fn step_finished_event(step: &StepLog) -> serde_json::Value {
    json!({
        "type": "step_finished",
        "index": step.index,
        "name": step.name,
        "step_type": step.step_type,
        "status": step.status,
        "exit_code": step.exit_code,
        "skip_reason": step.skip_reason,
        "at": Utc::now(),
    })
}

pub fn stream_event(index: usize, stream: &str, text: &str) -> serde_json::Value {
    json!({
        "type": stream,
        "index": index,
        "text": text,
        "at": Utc::now(),
    })
}

pub fn follow_events(run_dir: &Path) -> Result<()> {
    let event_path = run_dir.join("events.jsonl");
    let run_path = run_dir.join("run.json");
    let mut offset = 0;

    loop {
        if event_path.exists() {
            let mut file = fs::File::open(&event_path)
                .with_context(|| format!("failed to open {}", event_path.display()))?;
            file.seek(SeekFrom::Start(offset))?;
            let mut chunk = String::new();
            file.read_to_string(&mut chunk)?;
            offset += chunk.len() as u64;
            print!("{chunk}");
            std::io::stdout().flush()?;
        }

        if run_path.exists() {
            match read_run_log(run_dir) {
                Ok(run) if run.status != RunStatus::Running => {
                    if event_path.exists() {
                        let len = fs::metadata(&event_path)?.len();
                        if offset >= len {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => {}
            }
        }

        thread::sleep(Duration::from_millis(250));
    }

    Ok(())
}

pub fn read_run_log(run_dir: &Path) -> Result<RunLog> {
    let path = run_dir.join("run.json");
    let content =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(serde_json::from_str(&content)?)
}

pub fn print_run_summary(run: &RunLog, run_dir: &Path) {
    println!("Run: {}", run.run_id);
    println!("Manifest: {}", run.manifest_name);
    println!("Status: {:?}", run.status);
    println!("Started: {}", run.started_at);
    if let Some(ended_at) = run.ended_at {
        println!("Ended: {ended_at}");
    }
    if let Some(index) = run.failed_step_index {
        if let Some(step) = run.steps.iter().find(|step| step.index == index) {
            println!("Failed step: {} / {}", step.index, run.total_steps);
            println!("Step name: {}", step.name.as_deref().unwrap_or("<unnamed>"));
            if let Some(exit_code) = step.exit_code {
                println!("Exit code: {exit_code}");
            }
        }
    }
    println!("Full log path:");
    println!("{}", run_dir.display());
    if !run.events_log.is_empty() {
        println!("Live events log:");
        println!("{}", run.events_log);
    }
}

pub fn print_run_json(run: &RunLog) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(run)?);
    Ok(())
}

pub fn print_ai_summary(run: &RunLog, run_dir: &Path) -> Result<()> {
    let summary = ai_summary(run, run_dir);
    println!("{}", serde_json::to_string_pretty(&summary)?);
    Ok(())
}

pub fn ai_summary(run: &RunLog, run_dir: &Path) -> AiRunSummary {
    AiRunSummary {
        run_id: run.run_id.clone(),
        manifest_id: run.manifest_id.clone(),
        manifest_name: run.manifest_name.clone(),
        status: run.status.clone(),
        started_at: run.started_at,
        ended_at: run.ended_at,
        platform: run.platform.clone(),
        failed_step: run
            .failed_step_index
            .and_then(|index| run.steps.iter().find(|step| step.index == index).cloned()),
        steps: run.steps.clone(),
        stdout_log: run.stdout_log.clone(),
        stderr_log: run.stderr_log.clone(),
        events_log: run.events_log.clone(),
        run_dir: run_dir.display().to_string(),
    }
}
