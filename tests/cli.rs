use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::tempdir;

fn tali() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tali"))
}

fn run_tali(args: &[&str], data_dir: &Path, cwd: &Path) -> Output {
    tali()
        .args(args)
        .env("TALI_DATA_DIR", data_dir)
        .env("TALI_NO_UPDATE_CHECK", "1")
        .current_dir(cwd)
        .output()
        .expect("failed to run tali")
}

fn run_tali_with_env(args: &[&str], data_dir: &Path, cwd: &Path, env: &[(&str, &str)]) -> Output {
    let mut command = tali();
    command
        .args(args)
        .env("TALI_DATA_DIR", data_dir)
        .env("TALI_NO_UPDATE_CHECK", "1")
        .current_dir(cwd);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("failed to run tali")
}

fn run_tali_allowing_update_check(
    args: &[&str],
    data_dir: &Path,
    cwd: &Path,
    env: &[(&str, &str)],
) -> Output {
    let mut command = tali();
    command
        .args(args)
        .env("TALI_DATA_DIR", data_dir)
        .current_dir(cwd);
    for (key, value) in env {
        command.env(key, value);
    }
    command.output().expect("failed to run tali")
}

fn wait_for_path(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {}", path.display());
}

fn write_run_json(data_dir: &Path, run_id: &str, timestamp: &str) {
    let run_dir = data_dir.join("runs").join(run_id);
    fs::create_dir_all(&run_dir).unwrap();
    let run = serde_json::json!({
        "run_id": run_id,
        "manifest_id": null,
        "manifest_name": "cleanup-test",
        "total_steps": 0,
        "started_at": timestamp,
        "ended_at": timestamp,
        "status": "success",
        "platform": {
            "os": "test",
            "architecture": "test",
            "current_directory": "/tmp",
            "shell": null,
            "path": null,
            "tali_version": env!("CARGO_PKG_VERSION"),
            "rustc_version": null,
            "tools": {}
        },
        "steps": [],
        "failed_step_index": null,
        "stdout_log": "",
        "stderr_log": "",
        "events_log": ""
    });
    fs::write(
        run_dir.join("run.json"),
        serde_json::to_string_pretty(&run).unwrap(),
    )
    .unwrap();
}

#[test]
fn add_shortcut_run_and_ai_logs_mask_secrets() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let manifest = temp.path().join("setup.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "cli-smoke"

[[inputs]]
name = "token"
prompt = "Token"
secret = true
required = true

[[steps]]
name = "Write token file"
type = "write_file"
path = "out/token.txt"
content = "{{token}}"

[[steps]]
name = "Echo token"
type = "shell"
cmd = "echo {{token}}"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(String::from_utf8_lossy(&add.stdout).contains("ID: 01"));

    let inspect = run_tali(&["inspect", "01", "--json"], &data_dir, temp.path());
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspect_json: Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(inspect_json["name"], "cli-smoke");

    let run = run_tali_with_env(
        &["01", "--yes", "--input-env", "token=TALI_TEST_TOKEN"],
        &data_dir,
        &project,
        &[("TALI_TEST_TOKEN", "top-secret-value")],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("********"));
    assert!(!stdout.contains("top-secret-value"));
    assert_eq!(
        fs::read_to_string(project.join("out").join("token.txt")).unwrap(),
        "top-secret-value"
    );

    let logs = run_tali(&["logs", "latest", "--for-ai"], &data_dir, &project);
    assert!(
        logs.status.success(),
        "{}",
        String::from_utf8_lossy(&logs.stderr)
    );
    let logs_text = String::from_utf8_lossy(&logs.stdout);
    assert!(logs_text.contains("cli-smoke"));
    assert!(logs_text.contains("********"));
    assert!(!logs_text.contains("top-secret-value"));
    let logs_json: Value = serde_json::from_slice(&logs.stdout).unwrap();
    assert_eq!(logs_json["status"], "success");
}

#[test]
fn add_json_returns_agent_friendly_run_command() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let manifest = temp.path().join("setup.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "agent-added"

[[steps]]
type = "mkdir"
path = "config"
"#,
    )
    .unwrap();

    let add = run_tali(
        &["add", manifest.to_str().unwrap(), "--json"],
        &data_dir,
        temp.path(),
    );
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_json: Value = serde_json::from_slice(&add.stdout).unwrap();
    assert_eq!(add_json["id"], "01");
    assert_eq!(add_json["name"], "agent-added");
    assert_eq!(add_json["run"], "tali 01");
}

#[test]
fn skill_install_writes_bundled_tali_agent_skill() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let skills_dir = temp.path().join("skills");

    let install = run_tali(
        &["skill", "install", skills_dir.to_str().unwrap(), "--json"],
        &data_dir,
        temp.path(),
    );
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );
    let install_json: Value = serde_json::from_slice(&install.stdout).unwrap();
    assert_eq!(install_json["skill"], "tali-agent");
    assert!(skills_dir.join("tali-agent").join("SKILL.md").exists());
    assert!(skills_dir
        .join("tali-agent")
        .join("references")
        .join("manifest-authoring.md")
        .exists());
    assert!(
        fs::read_to_string(skills_dir.join("tali-agent").join("SKILL.md"))
            .unwrap()
            .contains("Tali Agent")
    );
}

#[test]
fn project_manifest_resolves_from_nested_directory() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    let nested = project.join("a").join("b");
    fs::create_dir_all(project.join(".tali")).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        project.join(".tali").join("setup.toml"),
        r#"
version = 1
name = "project-setup"

[[steps]]
type = "write_file"
path = "project-root.txt"
content = "ok"
"#,
    )
    .unwrap();

    let run = run_tali(&["setup", "--yes"], &data_dir, &nested);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("project-root.txt")).unwrap(),
        "ok"
    );
    assert!(!nested.join("project-root.txt").exists());
}

#[test]
fn shared_project_manifest_resolves_from_nested_directory() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    let nested = project.join("a").join("b");
    fs::create_dir_all(project.join(".tali").join("share")).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        project.join(".tali").join("share").join("setup.toml"),
        r#"
version = 1
name = "shared-project-setup"

[[steps]]
type = "write_file"
path = "shared-root.txt"
content = "shared"
"#,
    )
    .unwrap();

    let run = run_tali(&["setup", "--yes"], &data_dir, &nested);

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        fs::read_to_string(project.join("shared-root.txt")).unwrap(),
        "shared"
    );
}

#[test]
fn private_project_manifest_warns_when_tali_is_not_ignored_but_does_not_modify_with_yes() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(project.join(".tali")).unwrap();
    fs::write(
        project.join(".tali").join("setup.toml"),
        r#"
version = 1
name = "private-project-setup"

[[steps]]
type = "write_file"
path = "out.txt"
content = "ok"
"#,
    )
    .unwrap();

    let run = run_tali(&["setup", "--yes"], &data_dir, &project);

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("is not ignored by git"));
    assert!(stdout.contains(".tali/share/*.toml"));
    assert!(!project.join(".gitignore").exists());
}

#[test]
fn private_project_manifest_is_quiet_when_tali_is_already_ignored() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(project.join(".tali")).unwrap();
    fs::write(project.join(".gitignore"), ".tali/\n").unwrap();
    fs::write(
        project.join(".tali").join("setup.toml"),
        r#"
version = 1
name = "ignored-project-setup"

[[steps]]
type = "write_file"
path = "out.txt"
content = "ok"
"#,
    )
    .unwrap();

    let run = run_tali(&["setup", "--yes"], &data_dir, &project);

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!stdout.contains("is not ignored by git"));
}

#[test]
fn failed_run_returns_non_zero_and_records_failure() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let manifest = temp.path().join("fail.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "cli-fail"

[[steps]]
name = "Fail"
type = "shell"
cmd = "exit 13"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let run = run_tali(&["01", "--yes"], &data_dir, temp.path());
    assert!(!run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("Run failed."));
    assert!(stdout.contains("Exit code: 13"));

    let logs = run_tali(&["logs", "latest", "--json"], &data_dir, temp.path());
    assert!(
        logs.status.success(),
        "{}",
        String::from_utf8_lossy(&logs.stderr)
    );
    let logs_json: Value = serde_json::from_slice(&logs.stdout).unwrap();
    assert_eq!(logs_json["status"], "failed");
    assert_eq!(logs_json["failed_step_index"], 1);
}

#[test]
fn inspect_and_logs_show_when_conditions() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let manifest = temp.path().join("conditional.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "cli-conditional"

[[steps]]
name = "Skip missing"
type = "write_file"
when = "file_exists('missing.txt')"
path = "missing-output.txt"
content = "bad"

[[steps]]
name = "Write ok"
type = "write_file"
when = "not file_exists('missing.txt')"
path = "ok.txt"
content = "ok"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let inspect = run_tali(&["inspect", "01"], &data_dir, temp.path());
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspect_stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(inspect_stdout.contains("When: file_exists('missing.txt')"));

    let run = run_tali(&["01", "--yes"], &data_dir, temp.path());
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!temp.path().join("missing-output.txt").exists());
    assert_eq!(
        fs::read_to_string(temp.path().join("ok.txt")).unwrap(),
        "ok"
    );

    let logs = run_tali(&["logs", "latest", "--json"], &data_dir, temp.path());
    assert!(
        logs.status.success(),
        "{}",
        String::from_utf8_lossy(&logs.stderr)
    );
    let logs_json: Value = serde_json::from_slice(&logs.stdout).unwrap();
    assert_eq!(logs_json["steps"][0]["status"], "skipped");
    assert_eq!(
        logs_json["steps"][0]["skip_reason"],
        "when evaluated to false"
    );
    assert_eq!(logs_json["steps"][1]["status"], "success");
}

#[test]
fn inspect_shows_replace_in_file_as_writable_file() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let manifest = temp.path().join("replace.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "cli-replace"

[[steps]]
name = "Fill env"
type = "replace_in_file"
path = ".env"

[steps.replacements]
"__TOKEN__" = "value"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let inspect = run_tali(&["inspect", "01"], &data_dir, temp.path());
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let stdout = String::from_utf8_lossy(&inspect.stdout);
    assert!(stdout.contains("Replace in file: .env"));
    assert!(stdout.contains("- .env"));
}

#[test]
fn logs_follow_latest_streams_live_events() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let manifest = temp.path().join("live.toml");
    let command = if cfg!(windows) {
        "Write-Output one; Start-Sleep -Milliseconds 800; Write-Output two"
    } else {
        "printf 'one\\n'; sleep 1; printf 'two\\n'"
    };
    fs::write(
        &manifest,
        format!(
            r#"
version = 1
name = "cli-live"

[[steps]]
name = "Slow output"
type = "shell"
cmd = "{}"
"#,
            command.replace('\\', "\\\\").replace('"', "\\\"")
        ),
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );

    let mut run = tali()
        .args(["01", "--yes"])
        .env("TALI_DATA_DIR", &data_dir)
        .env("TALI_NO_UPDATE_CHECK", "1")
        .current_dir(temp.path())
        .spawn()
        .expect("failed to spawn tali run");
    wait_for_path(&data_dir.join("logs").join("latest-running"));

    let follow = run_tali(&["logs", "follow", "latest"], &data_dir, temp.path());
    let run_status = run.wait().expect("failed to wait for tali run");
    assert!(run_status.success());
    assert!(
        follow.status.success(),
        "{}",
        String::from_utf8_lossy(&follow.stderr)
    );
    let follow_stdout = String::from_utf8_lossy(&follow.stdout);
    assert!(follow_stdout.contains("\"type\":\"run_started\""));
    assert!(follow_stdout.contains("\"type\":\"step_started\""));
    assert!(follow_stdout.contains("\"type\":\"stdout\""));
    assert!(follow_stdout.contains("\"text\":\"one\""));
    assert!(follow_stdout.contains("\"text\":\"two\""));
    assert!(follow_stdout.contains("\"type\":\"run_finished\""));
}

#[test]
fn cleanup_previews_then_deletes_old_runs_and_cache() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    fs::create_dir_all(data_dir.join("runs")).unwrap();
    fs::create_dir_all(data_dir.join("logs")).unwrap();
    fs::create_dir_all(data_dir.join("cache")).unwrap();
    write_run_json(&data_dir, "run-old", "2000-01-01T00:00:00Z");
    write_run_json(&data_dir, "run-new", "2999-01-01T00:00:00Z");
    fs::write(data_dir.join("logs").join("latest"), "run-old").unwrap();
    fs::write(data_dir.join("cache").join("old-cache.txt"), "cache").unwrap();

    let preview = run_tali(
        &["cleanup", "--older-than", "0s", "--json"],
        &data_dir,
        temp.path(),
    );
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview_json: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview_json["deleted"], false);
    assert_eq!(preview_json["dry_run"], true);
    assert!(data_dir.join("runs").join("run-old").exists());
    assert!(data_dir.join("cache").join("old-cache.txt").exists());

    let cleanup = run_tali(
        &["cleanup", "--older-than", "0s", "--yes", "--json"],
        &data_dir,
        temp.path(),
    );
    assert!(
        cleanup.status.success(),
        "{}",
        String::from_utf8_lossy(&cleanup.stderr)
    );
    let cleanup_json: Value = serde_json::from_slice(&cleanup.stdout).unwrap();
    assert_eq!(cleanup_json["deleted"], true);
    assert!(!data_dir.join("runs").join("run-old").exists());
    assert!(data_dir.join("runs").join("run-new").exists());
    assert!(!data_dir.join("cache").join("old-cache.txt").exists());
    assert_eq!(
        fs::read_to_string(data_dir.join("logs").join("latest")).unwrap(),
        "run-new"
    );
}

#[test]
fn completions_command_generates_shell_script() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let output = run_tali(&["completions", "bash"], &data_dir, temp.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_tali"));
    assert!(stdout.contains("completions"));
}

#[test]
fn doctor_json_and_self_test_work() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");

    let doctor = run_tali(&["doctor", "--json"], &data_dir, temp.path());
    assert!(
        doctor.status.success(),
        "{}",
        String::from_utf8_lossy(&doctor.stderr)
    );
    let doctor_json: Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor_json["tali_version"], env!("CARGO_PKG_VERSION"));

    let self_test = run_tali(&["self-test", "--json"], &data_dir, temp.path());
    assert!(
        self_test.status.success(),
        "{}",
        String::from_utf8_lossy(&self_test.stderr)
    );
    let self_test_json: Value = serde_json::from_slice(&self_test.stdout).unwrap();
    assert_eq!(self_test_json["status"], "passed");
    assert!(self_test_json["checks"].as_array().unwrap().len() >= 5);
}

#[test]
fn update_check_reports_available_version_and_writes_cache() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");

    let output = run_tali_allowing_update_check(
        &["update", "--check"],
        &data_dir,
        temp.path(),
        &[("TALI_UPDATE_CHECK_LATEST_VERSION", "9.9.9")],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Update available: tali 9.9.9"));
    let cache = fs::read_to_string(data_dir.join("cache").join("update-check.json")).unwrap();
    assert!(cache.contains("9.9.9"));
}

#[test]
fn passive_update_check_can_be_disabled_for_run() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let manifest = temp.path().join("setup.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "update-disabled"

[[steps]]
type = "write_file"
path = "out.txt"
content = "ok"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let run = run_tali_allowing_update_check(
        &["01", "--yes", "--no-update-check"],
        &data_dir,
        &project,
        &[("TALI_UPDATE_CHECK_LATEST_VERSION", "9.9.9")],
    );

    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(!stdout.contains("Update available"));
}

#[test]
fn failed_run_does_not_print_passive_update_notice() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    let manifest = temp.path().join("fail-update.toml");
    fs::write(
        &manifest,
        r#"
version = 1
name = "fail-update"

[[steps]]
type = "shell"
cmd = "exit 7"
"#,
    )
    .unwrap();

    let add = run_tali(&["add", manifest.to_str().unwrap()], &data_dir, temp.path());
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let run = run_tali_allowing_update_check(
        &["01", "--yes"],
        &data_dir,
        &project,
        &[("TALI_UPDATE_CHECK_LATEST_VERSION", "9.9.9")],
    );

    assert!(!run.status.success());
    let stdout = String::from_utf8_lossy(&run.stdout);
    assert!(stdout.contains("Run failed."));
    assert!(!stdout.contains("Update available"));
}

#[test]
fn help_does_not_create_data_directory() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().join("store");
    let output = run_tali(&["--help"], &data_dir, temp.path());
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!data_dir.exists());
}
