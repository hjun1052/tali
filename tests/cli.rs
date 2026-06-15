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
        .current_dir(cwd)
        .output()
        .expect("failed to run tali")
}

fn run_tali_with_env(args: &[&str], data_dir: &Path, cwd: &Path, env: &[(&str, &str)]) -> Output {
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
