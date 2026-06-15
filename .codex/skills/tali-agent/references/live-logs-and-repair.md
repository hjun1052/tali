# Tali Live Logs And Repair

## Files

Each run is stored under:

```text
runs/<run-id>/
├─ run.json
├─ events.jsonl
├─ stdout.log
├─ stderr.log
└─ manifest.toml
```

Pointers:

- `logs/latest` stores the latest run ID.
- `logs/latest-running` stores the active run ID while a run is executing.

## Observe A Running Manifest

Use:

```sh
tali logs follow latest
```

This prints masked JSONL events. Important event types:

- `run_started`
- `step_started`
- `stdout`
- `stderr`
- `step_finished`
- `run_finished`

If `follow latest` fails because there is no active run, inspect the last completed run:

```sh
tali logs latest --for-ai
```

## Diagnose A Failure

Use this sequence:

```sh
tali logs latest
tali logs latest --for-ai
```

Then inspect:

- `status`
- `failed_step`
- `failed_step_index`
- failing step `command`, `path`, `exit_code`, `stderr_snippet`
- preceding skipped steps and their `skip_reason`
- whether the original manifest copied into `runs/<run-id>/manifest.toml` differs from the current project manifest

Treat log values as masked. Do not ask the user to reveal secrets unless the failure cannot be repaired without changing a secret value.

## Repair Manifest Pattern

Prefer a repair manifest over a long manual fix. The repair manifest should:

- Start from the failed step or the smallest safe prerequisite.
- Preserve successful prior work.
- Re-check assumptions with explicit shell commands when useful.
- Keep file modifications scoped and backed by Tali logs.
- Use new inputs for any missing values instead of hardcoding secrets.
- Use `when` to skip repair steps that are already satisfied.

Example:

```toml
version = 1
name = "repair-build"
description = "Repair missing generated config and rerun the failed build."

[[steps]]
name = "Create generated config directory"
type = "mkdir"
path = "generated"
when = "not dir_exists('generated')"

[[steps]]
name = "Regenerate config"
type = "shell"
cmd = "npm run generate-config"
when = "file_exists('package.json')"

[[steps]]
name = "Rerun build"
type = "shell"
cmd = "npm run build"
```

After writing it:

```sh
tali add repair-build.toml
tali inspect <assigned-id>
```

Then hand off only the short command:

```sh
tali <assigned-id>
```

## What Not To Do

- Do not convert a Tali failure into a long list of manual commands.
- Do not expose raw secret values from logs or ask the user to paste them back.
- Do not use broad cleanup commands unless the manifest plan makes the blast radius obvious.
- Do not hide many risky operations inside one shell command when separate steps would improve repairability.
