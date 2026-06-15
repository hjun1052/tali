use crate::condition::{self, ConditionContext};
use crate::doctor;
use crate::input::{collect_inputs, secret_values};
use crate::interpolate::{interpolate, mask_secrets};
use crate::logs::{self, RunLog, RunStatus, StepLog, StepStatus};
use crate::manifest::{Manifest, Step};
use crate::safety::{display_path, safe_path};
use crate::store::{ManifestSource, Store};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rand::{distributions::Alphanumeric, Rng};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;

#[derive(Debug, Clone, Default)]
pub struct RunnerOptions {
    pub yes: bool,
    pub dry_run: bool,
    pub provided_inputs: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResult {
    pub run_id: Option<String>,
    pub status: RunStatus,
    pub run_dir: Option<PathBuf>,
}

struct StepExecutionContext<'a> {
    source: &'a ManifestSource,
    values: &'a HashMap<String, String>,
    secrets: &'a [String],
    run_dir: &'a Path,
    stdout_path: &'a Path,
    stderr_path: &'a Path,
    events_path: &'a Path,
}

struct ShellExecutionContext<'a> {
    secrets: &'a [String],
    stdout_path: &'a Path,
    stderr_path: &'a Path,
    events_path: &'a Path,
    step_index: usize,
}

pub fn print_plan(manifest: &Manifest, will_do: bool) {
    println!("Manifest: {}", manifest.name);
    if let Some(description) = &manifest.description {
        println!("{description}");
    }
    if manifest.allow_outside_cwd {
        println!("Warning: this manifest allows file operations outside the working directory.");
    }
    println!("{}", if will_do { "Will do:" } else { "Plan:" });
    for (index, step) in manifest.steps.iter().enumerate() {
        println!("{}. {}", index + 1, step.plan_line());
        if let Some(when) = step.when() {
            println!("   When: {when}");
        }
    }
    if !manifest.inputs.is_empty() {
        println!("Inputs required:");
        for input in &manifest.inputs {
            if input.secret {
                println!("- {} [secret]", input.name);
            } else {
                println!("- {}", input.name);
            }
        }
    }
}

pub fn inspect_manifest(manifest: &Manifest) {
    println!("Name: {}", manifest.name);
    if let Some(description) = &manifest.description {
        println!("Description: {description}");
    }
    println!("Required inputs:");
    if manifest.inputs.is_empty() {
        println!("- none");
    } else {
        for input in &manifest.inputs {
            let required = if input.required {
                "required"
            } else {
                "optional"
            };
            let secret = if input.secret { ", secret" } else { "" };
            println!("- {} ({required}{secret})", input.name);
        }
    }

    println!("Steps:");
    for (index, step) in manifest.steps.iter().enumerate() {
        println!("{}. {}", index + 1, step.plan_line());
        if let Some(when) = step.when() {
            println!("   When: {when}");
        }
    }

    println!("Files that may be written:");
    let mut wrote_any = false;
    for step in &manifest.steps {
        if let Some(path) = step.path_for_inspect() {
            println!("- {path}");
            wrote_any = true;
        }
    }
    if !wrote_any {
        println!("- none");
    }

    println!("Commands that may be executed:");
    let mut command_any = false;
    for step in &manifest.steps {
        if let Some(command) = step.command_for_inspect() {
            println!("- {command}");
            command_any = true;
        }
    }
    if !command_any {
        println!("- none");
    }
}

pub fn dry_run_manifest(manifest: &Manifest) {
    print_plan(manifest, true);
    print_writable_files(manifest, "Files that may be written:");
    print_commands(manifest, "Commands that may be executed:");
}

pub fn run_manifest(
    store: &Store,
    source: &ManifestSource,
    options: RunnerOptions,
) -> Result<RunResult> {
    if options.dry_run {
        dry_run_manifest(&source.manifest);
        return Ok(RunResult {
            run_id: None,
            status: RunStatus::Aborted,
            run_dir: None,
        });
    }

    print_plan(&source.manifest, false);
    if !options.yes && !confirm()? {
        println!("Aborted.");
        return Ok(RunResult {
            run_id: None,
            status: RunStatus::Aborted,
            run_dir: None,
        });
    }

    let input_values = collect_inputs(&source.manifest.inputs, &options.provided_inputs)?;
    let secrets = secret_values(&source.manifest.inputs, &input_values);
    let run_id = generate_run_id();
    let run_dir = store.create_run_dir(&run_id)?;
    let stdout_path = run_dir.join("stdout.log");
    let stderr_path = run_dir.join("stderr.log");
    let events_path = run_dir.join("events.jsonl");
    fs::File::create(&stdout_path)
        .with_context(|| format!("failed to create {}", stdout_path.display()))?;
    fs::File::create(&stderr_path)
        .with_context(|| format!("failed to create {}", stderr_path.display()))?;
    fs::File::create(&events_path)
        .with_context(|| format!("failed to create {}", events_path.display()))?;
    fs::copy(&source.path, run_dir.join("manifest.toml"))?;

    let mut run = RunLog {
        run_id: run_id.clone(),
        manifest_id: source.global.as_ref().map(|entry| entry.id.clone()),
        manifest_name: source.manifest.name.clone(),
        total_steps: source.manifest.steps.len(),
        started_at: Utc::now(),
        ended_at: None,
        status: RunStatus::Running,
        platform: doctor::capture(),
        steps: Vec::new(),
        failed_step_index: None,
        stdout_log: display_path(&stdout_path),
        stderr_log: display_path(&stderr_path),
        events_log: display_path(&events_path),
    };
    store.set_latest_run(&run_id)?;
    store.set_latest_running_run(&run_id)?;
    logs::append_event(
        &events_path,
        logs::run_started_event(&run_id, &source.manifest.name),
    )?;
    logs::write_run_log(&run_dir, &run)?;

    for (zero_index, step) in source.manifest.steps.iter().enumerate() {
        let index = zero_index + 1;
        if let Some(when) = step.when() {
            match condition::evaluate(
                when,
                &ConditionContext {
                    project_root: &source.project_root,
                    values: &input_values,
                    allow_outside_cwd: source.manifest.allow_outside_cwd,
                },
            ) {
                Ok(true) => {}
                Ok(false) => {
                    println!(
                        "Skipping step {}/{}: {}",
                        index,
                        source.manifest.steps.len(),
                        step.name().unwrap_or_else(|| step.kind())
                    );
                    let step_log = skipped_step_log(step, index, when, &secrets);
                    logs::append_event(&events_path, logs::step_finished_event(&step_log))?;
                    run.steps.push(step_log);
                    logs::write_run_log(&run_dir, &run)?;
                    continue;
                }
                Err(error) => {
                    let now = Utc::now();
                    let step_log = StepLog {
                        index,
                        name: step.name().map(str::to_string),
                        step_type: step.kind().to_string(),
                        started_at: now,
                        ended_at: Some(now),
                        status: StepStatus::Failed,
                        command: shell_command_for_log(step, &input_values, &secrets),
                        path: file_path_for_log(step, &input_values, &secrets),
                        when: Some(mask_secrets(when, &secrets)),
                        skip_reason: None,
                        exit_code: None,
                        stdout_snippet: None,
                        stderr_snippet: Some(mask_secrets(&error.to_string(), &secrets)),
                        backup_path: None,
                    };
                    append_line(
                        &stderr_path,
                        &step_log.stderr_snippet.clone().unwrap_or_default(),
                    )?;
                    run.failed_step_index = Some(index);
                    run.status = RunStatus::Failed;
                    logs::append_event(&events_path, logs::step_finished_event(&step_log))?;
                    run.steps.push(step_log);
                    logs::write_run_log(&run_dir, &run)?;
                    break;
                }
            }
        }

        println!(
            "Running step {}/{}: {}",
            index,
            source.manifest.steps.len(),
            step.name().unwrap_or_else(|| step.kind())
        );
        logs::append_event(
            &events_path,
            logs::step_started_event(index, step.name(), step.kind()),
        )?;
        logs::write_run_log(&run_dir, &run)?;

        match execute_step(
            step,
            index,
            &StepExecutionContext {
                source,
                values: &input_values,
                secrets: &secrets,
                run_dir: &run_dir,
                stdout_path: &stdout_path,
                stderr_path: &stderr_path,
                events_path: &events_path,
            },
        ) {
            Ok(step_log) if step_log.status == StepStatus::Success => {
                logs::append_event(&events_path, logs::step_finished_event(&step_log))?;
                run.steps.push(step_log);
                logs::write_run_log(&run_dir, &run)?;
            }
            Ok(step_log) => {
                run.failed_step_index = Some(index);
                run.status = RunStatus::Failed;
                logs::append_event(&events_path, logs::step_finished_event(&step_log))?;
                run.steps.push(step_log);
                logs::write_run_log(&run_dir, &run)?;
                break;
            }
            Err(error) => {
                let now = Utc::now();
                let step_log = StepLog {
                    index,
                    name: step.name().map(str::to_string),
                    step_type: step.kind().to_string(),
                    started_at: now,
                    ended_at: Some(now),
                    status: StepStatus::Failed,
                    command: shell_command_for_log(step, &input_values, &secrets),
                    path: file_path_for_log(step, &input_values, &secrets),
                    when: step.when().map(|when| mask_secrets(when, &secrets)),
                    skip_reason: None,
                    exit_code: None,
                    stdout_snippet: None,
                    stderr_snippet: Some(mask_secrets(&error.to_string(), &secrets)),
                    backup_path: None,
                };
                append_line(
                    &stderr_path,
                    &step_log.stderr_snippet.clone().unwrap_or_default(),
                )?;
                run.failed_step_index = Some(index);
                run.status = RunStatus::Failed;
                logs::append_event(&events_path, logs::step_finished_event(&step_log))?;
                run.steps.push(step_log);
                logs::write_run_log(&run_dir, &run)?;
                break;
            }
        }
    }

    if run.status == RunStatus::Running {
        run.status = RunStatus::Success;
    }
    run.ended_at = Some(Utc::now());
    logs::append_event(&events_path, logs::run_finished_event(&run_id, &run.status))?;
    logs::write_run_log(&run_dir, &run)?;
    store.set_latest_run(&run_id)?;
    store.clear_latest_running_run(&run_id)?;

    match run.status {
        RunStatus::Success => {
            println!("Run succeeded.");
            println!("Logs saved to:");
            println!("{}", run_dir.display());
        }
        RunStatus::Failed => {
            print_failure(&run, &run_dir);
        }
        RunStatus::Aborted | RunStatus::Running => {}
    }

    Ok(RunResult {
        run_id: Some(run_id),
        status: run.status,
        run_dir: Some(run_dir),
    })
}

fn execute_step(step: &Step, index: usize, context: &StepExecutionContext<'_>) -> Result<StepLog> {
    let started_at = Utc::now();
    match step {
        Step::Shell {
            name,
            cmd,
            cwd,
            env,
            ..
        } => {
            let command = interpolate(cmd, context.values)?;
            let cwd = match cwd {
                Some(cwd) => safe_path(
                    &context.source.project_root,
                    &interpolate(cwd, context.values)?,
                    context.source.manifest.allow_outside_cwd,
                )?,
                None => context.source.project_root.clone(),
            };
            let env = interpolate_env(env, context.values)?;
            let masked_command = mask_secrets(&command, context.secrets);
            let result = run_shell(
                &command,
                &cwd,
                &env,
                &ShellExecutionContext {
                    secrets: context.secrets,
                    stdout_path: context.stdout_path,
                    stderr_path: context.stderr_path,
                    events_path: context.events_path,
                    step_index: index,
                },
            )?;
            Ok(StepLog {
                index,
                name: name.clone(),
                step_type: "shell".to_string(),
                started_at,
                ended_at: Some(Utc::now()),
                status: if result.exit_code == 0 {
                    StepStatus::Success
                } else {
                    StepStatus::Failed
                },
                command: Some(masked_command),
                path: None,
                when: step.when().map(|when| mask_secrets(when, context.secrets)),
                skip_reason: None,
                exit_code: Some(result.exit_code),
                stdout_snippet: result.stdout_snippet,
                stderr_snippet: result.stderr_snippet,
                backup_path: None,
            })
        }
        Step::WriteFile {
            name,
            path,
            content,
            overwrite,
            create_dirs,
            ..
        } => {
            let path_text = interpolate(path, context.values)?;
            let target = safe_path(
                &context.source.project_root,
                &path_text,
                context.source.manifest.allow_outside_cwd,
            )?;
            if target.exists() && !overwrite {
                bail!("refusing to overwrite existing file {}", target.display());
            }
            let backup_path = backup_existing(&target, context.run_dir, index)?;
            if *create_dirs {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
            }
            let rendered = interpolate(content, context.values)?;
            fs::write(&target, rendered)?;
            Ok(StepLog {
                index,
                name: name.clone(),
                step_type: "write_file".to_string(),
                started_at,
                ended_at: Some(Utc::now()),
                status: StepStatus::Success,
                command: None,
                path: Some(mask_secrets(&display_path(&target), context.secrets)),
                when: step.when().map(|when| mask_secrets(when, context.secrets)),
                skip_reason: None,
                exit_code: None,
                stdout_snippet: None,
                stderr_snippet: None,
                backup_path: backup_path.map(|path| display_path(&path)),
            })
        }
        Step::Mkdir { name, path, .. } => {
            let path_text = interpolate(path, context.values)?;
            let target = safe_path(
                &context.source.project_root,
                &path_text,
                context.source.manifest.allow_outside_cwd,
            )?;
            fs::create_dir_all(&target)?;
            Ok(StepLog {
                index,
                name: name.clone(),
                step_type: "mkdir".to_string(),
                started_at,
                ended_at: Some(Utc::now()),
                status: StepStatus::Success,
                command: None,
                path: Some(mask_secrets(&display_path(&target), context.secrets)),
                when: step.when().map(|when| mask_secrets(when, context.secrets)),
                skip_reason: None,
                exit_code: None,
                stdout_snippet: None,
                stderr_snippet: None,
                backup_path: None,
            })
        }
        Step::Copy {
            name,
            from,
            to,
            overwrite,
            ..
        } => {
            let from_text = interpolate(from, context.values)?;
            let to_text = interpolate(to, context.values)?;
            let source_path = safe_path(
                &context.source.project_root,
                &from_text,
                context.source.manifest.allow_outside_cwd,
            )?;
            let target = safe_path(
                &context.source.project_root,
                &to_text,
                context.source.manifest.allow_outside_cwd,
            )?;
            if target.exists() && !overwrite {
                bail!("refusing to overwrite existing file {}", target.display());
            }
            let backup_path = backup_existing(&target, context.run_dir, index)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &target)?;
            Ok(StepLog {
                index,
                name: name.clone(),
                step_type: "copy".to_string(),
                started_at,
                ended_at: Some(Utc::now()),
                status: StepStatus::Success,
                command: None,
                path: Some(mask_secrets(&display_path(&target), context.secrets)),
                when: step.when().map(|when| mask_secrets(when, context.secrets)),
                skip_reason: None,
                exit_code: None,
                stdout_snippet: None,
                stderr_snippet: None,
                backup_path: backup_path.map(|path| display_path(&path)),
            })
        }
    }
}

fn skipped_step_log(step: &Step, index: usize, when: &str, secrets: &[String]) -> StepLog {
    let now = Utc::now();
    StepLog {
        index,
        name: step.name().map(str::to_string),
        step_type: step.kind().to_string(),
        started_at: now,
        ended_at: Some(now),
        status: StepStatus::Skipped,
        command: None,
        path: None,
        when: Some(mask_secrets(when, secrets)),
        skip_reason: Some("when evaluated to false".to_string()),
        exit_code: None,
        stdout_snippet: None,
        stderr_snippet: None,
        backup_path: None,
    }
}

struct ShellResult {
    exit_code: i32,
    stdout_snippet: Option<String>,
    stderr_snippet: Option<String>,
}

fn run_shell(
    command: &str,
    cwd: &Path,
    env: &BTreeMap<String, String>,
    context: &ShellExecutionContext<'_>,
) -> Result<ShellResult> {
    let mut process = shell_command(command);
    process
        .current_dir(cwd)
        .envs(env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn command in {}", cwd.display()))?;

    let stdout = child.stdout.take().context("failed to capture stdout")?;
    let stderr = child.stderr.take().context("failed to capture stderr")?;
    let stdout_log = context.stdout_path.to_path_buf();
    let stderr_log = context.stderr_path.to_path_buf();
    let stdout_events = context.events_path.to_path_buf();
    let stderr_events = context.events_path.to_path_buf();
    let stdout_secrets = context.secrets.to_vec();
    let stderr_secrets = context.secrets.to_vec();
    let step_index = context.step_index;

    let stdout_handle = thread::spawn(move || {
        stream_output(
            stdout,
            &stdout_log,
            Some(&stdout_events),
            &stdout_secrets,
            false,
            step_index,
        )
    });
    let stderr_handle = thread::spawn(move || {
        stream_output(
            stderr,
            &stderr_log,
            Some(&stderr_events),
            &stderr_secrets,
            true,
            step_index,
        )
    });

    let status = child.wait()?;
    let stdout_snippet = stdout_handle
        .join()
        .map_err(|_| anyhow!("stdout reader thread panicked"))??;
    let stderr_snippet = stderr_handle
        .join()
        .map_err(|_| anyhow!("stderr reader thread panicked"))??;

    Ok(ShellResult {
        exit_code: status.code().unwrap_or(-1),
        stdout_snippet,
        stderr_snippet,
    })
}

fn shell_command(command: &str) -> Command {
    if cfg!(target_os = "windows") {
        let shell = if command_available("pwsh") {
            "pwsh"
        } else {
            "powershell"
        };
        let mut process = Command::new(shell);
        process.args(["-NoProfile", "-Command", command]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-lc", command]);
        process
    }
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn stream_output<R: std::io::Read + Send + 'static>(
    reader: R,
    log_path: &Path,
    event_path: Option<&Path>,
    secrets: &[String],
    stderr: bool,
    step_index: usize,
) -> Result<Option<String>> {
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    let mut snippets = Vec::new();
    let reader = BufReader::new(reader);

    for line in reader.lines() {
        let line = line?;
        let masked = mask_secrets(&line, secrets);
        if stderr {
            eprintln!("{masked}");
        } else {
            println!("{masked}");
        }
        writeln!(log, "{masked}")?;
        if let Some(event_path) = event_path {
            let stream = if stderr { "stderr" } else { "stdout" };
            logs::append_event(event_path, logs::stream_event(step_index, stream, &masked))?;
        }
        if snippets.join("\n").len() < 4000 {
            snippets.push(masked);
        }
    }

    Ok((!snippets.is_empty()).then(|| snippets.join("\n")))
}

fn interpolate_env(
    env: &BTreeMap<String, String>,
    values: &HashMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    env.iter()
        .map(|(key, value)| Ok((key.clone(), interpolate(value, values)?)))
        .collect()
}

fn confirm() -> Result<bool> {
    print!("Okay to proceed? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}

fn generate_run_id() -> String {
    let random: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    format!("run-{}-{random}", Utc::now().format("%Y%m%d%H%M%S"))
}

fn backup_existing(target: &Path, run_dir: &Path, step_index: usize) -> Result<Option<PathBuf>> {
    if !target.exists() || !target.is_file() {
        return Ok(None);
    }

    let backup_dir = run_dir
        .join("backups")
        .join(format!("step-{step_index:02}"));
    fs::create_dir_all(&backup_dir)?;
    let filename = target
        .file_name()
        .map(|name| name.to_owned())
        .unwrap_or_else(|| "file".into());
    let backup_path = backup_dir.join(filename);
    fs::copy(target, &backup_path)?;
    Ok(Some(backup_path))
}

fn shell_command_for_log(
    step: &Step,
    values: &HashMap<String, String>,
    secrets: &[String],
) -> Option<String> {
    match step {
        Step::Shell { cmd, .. } => interpolate(cmd, values)
            .ok()
            .map(|command| mask_secrets(&command, secrets)),
        _ => None,
    }
}

fn file_path_for_log(
    step: &Step,
    values: &HashMap<String, String>,
    secrets: &[String],
) -> Option<String> {
    let raw = match step {
        Step::WriteFile { path, .. } | Step::Mkdir { path, .. } => path,
        Step::Copy { to, .. } => to,
        Step::Shell { .. } => return None,
    };
    interpolate(raw, values)
        .ok()
        .map(|path| mask_secrets(&path, secrets))
}

fn append_line(path: &Path, line: &str) -> Result<()> {
    let mut log = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(log, "{line}")?;
    Ok(())
}

fn print_failure(run: &RunLog, run_dir: &Path) {
    println!("Run failed.");
    if let Some(index) = run.failed_step_index {
        println!("Failed step: {} / {}", index, run.total_steps);
        if let Some(step) = run.steps.iter().find(|step| step.index == index) {
            println!("Step name: {}", step.name.as_deref().unwrap_or("<unnamed>"));
            if let Some(exit_code) = step.exit_code {
                println!("Exit code: {exit_code}");
            }
        }
    }
    println!("Logs saved to:");
    println!("{}", run_dir.display());
    println!("For AI repair, share:");
    println!("tali logs latest");
}

fn print_writable_files(manifest: &Manifest, heading: &str) {
    println!("{heading}");
    let mut wrote_any = false;
    for step in &manifest.steps {
        if let Some(path) = step.path_for_inspect() {
            println!("- {path}");
            wrote_any = true;
        }
    }
    if !wrote_any {
        println!("- none");
    }
}

fn print_commands(manifest: &Manifest, heading: &str) {
    println!("{heading}");
    let mut command_any = false;
    for step in &manifest.steps {
        if let Some(command) = step.command_for_inspect() {
            println!("- {command}");
            command_any = true;
        }
    }
    if !command_any {
        println!("- none");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn runs_manifest_and_writes_logs() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("setup.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "write-test"

[[steps]]
name = "Write marker"
type = "write_file"
path = "out/marker.txt"
content = "hello"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::new(),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Success);
        assert_eq!(
            fs::read_to_string(temp.path().join("out/marker.txt")).unwrap(),
            "hello"
        );
        let run_dir = result.run_dir.unwrap();
        assert!(run_dir.join("run.json").exists());
        assert!(run_dir.join("stdout.log").exists());
        assert!(run_dir.join("stderr.log").exists());
        assert!(run_dir.join("events.jsonl").exists());
        assert!(run_dir.join("manifest.toml").exists());
        assert_eq!(store.latest_run_id().unwrap(), result.run_id.unwrap());
        assert!(store.latest_running_run_id().unwrap().is_none());
    }

    #[test]
    fn masks_secret_in_command_output_and_run_json() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("secret.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "secret-test"

[[inputs]]
name = "token"
prompt = "Token"
secret = true
required = true

[[steps]]
name = "Echo token"
type = "shell"
cmd = "echo {{token}}"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::from([(
                    "token".to_string(),
                    "super-secret-token".to_string(),
                )]),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Success);
        let run_dir = result.run_dir.unwrap();
        let run_json = fs::read_to_string(run_dir.join("run.json")).unwrap();
        let stdout = fs::read_to_string(run_dir.join("stdout.log")).unwrap();
        let events = fs::read_to_string(run_dir.join("events.jsonl")).unwrap();
        assert!(!run_json.contains("super-secret-token"));
        assert!(!stdout.contains("super-secret-token"));
        assert!(!events.contains("super-secret-token"));
        assert!(run_json.contains("********"));
        assert!(stdout.contains("********"));
        assert!(events.contains("********"));
    }

    #[test]
    fn stops_on_first_shell_failure() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("fail.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "fail-test"

[[steps]]
name = "Fail"
type = "shell"
cmd = "exit 7"

[[steps]]
name = "Should not run"
type = "write_file"
path = "after.txt"
content = "bad"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::new(),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Failed);
        assert!(!temp.path().join("after.txt").exists());
        let run = logs::read_run_log(&result.run_dir.unwrap()).unwrap();
        assert_eq!(run.failed_step_index, Some(1));
        assert_eq!(run.total_steps, 2);
        assert_eq!(run.steps.len(), 1);
    }

    #[test]
    fn skips_step_when_condition_is_false() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("conditional.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "conditional-test"

[[inputs]]
name = "target"
prompt = "Target"
required = true

[[steps]]
name = "Skip prod"
type = "write_file"
when = "input_equals('target', 'prod')"
path = "prod.txt"
content = "bad"

[[steps]]
name = "Write preview"
type = "write_file"
when = "input_equals('target', 'preview')"
path = "preview.txt"
content = "ok"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::from([("target".to_string(), "preview".to_string())]),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Success);
        assert!(!temp.path().join("prod.txt").exists());
        assert_eq!(
            fs::read_to_string(temp.path().join("preview.txt")).unwrap(),
            "ok"
        );

        let run = logs::read_run_log(&result.run_dir.unwrap()).unwrap();
        assert_eq!(run.steps.len(), 2);
        assert_eq!(run.steps[0].status, StepStatus::Skipped);
        assert_eq!(
            run.steps[0].skip_reason.as_deref(),
            Some("when evaluated to false")
        );
        assert_eq!(run.steps[1].status, StepStatus::Success);
    }

    #[test]
    fn dry_run_does_not_write_files() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        let manifest_path = temp.path().join("dry.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "dry-test"

[[steps]]
type = "write_file"
path = "dry.txt"
content = "bad"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: true,
                provided_inputs: HashMap::new(),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Aborted);
        assert!(!temp.path().join("dry.txt").exists());
    }

    #[test]
    fn refuses_overwrite_when_disabled() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        fs::write(temp.path().join("existing.txt"), "old").unwrap();
        let manifest_path = temp.path().join("overwrite.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "overwrite-test"

[[steps]]
type = "write_file"
path = "existing.txt"
content = "new"
overwrite = false
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::new(),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Failed);
        assert_eq!(
            fs::read_to_string(temp.path().join("existing.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn backs_up_file_before_overwrite() {
        let temp = tempdir().unwrap();
        let store = Store::from_data_dir(temp.path().join("store")).unwrap();
        fs::write(temp.path().join("existing.txt"), "old").unwrap();
        let manifest_path = temp.path().join("backup.toml");
        fs::write(
            &manifest_path,
            r#"
version = 1
name = "backup-test"

[[steps]]
type = "write_file"
path = "existing.txt"
content = "new"
"#,
        )
        .unwrap();

        let entry = store.add_manifest(&manifest_path).unwrap();
        let source = store.resolve_manifest(&entry.id, temp.path()).unwrap();
        let result = run_manifest(
            &store,
            &source,
            RunnerOptions {
                yes: true,
                dry_run: false,
                provided_inputs: HashMap::new(),
            },
        )
        .unwrap();

        assert_eq!(result.status, RunStatus::Success);
        let run = logs::read_run_log(&result.run_dir.unwrap()).unwrap();
        let backup_path = run.steps[0].backup_path.as_ref().unwrap();
        assert_eq!(fs::read_to_string(backup_path).unwrap(), "old");
        assert_eq!(
            fs::read_to_string(temp.path().join("existing.txt")).unwrap(),
            "new"
        );
    }
}
