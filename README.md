# Tali

Tali is an AI-friendly command manifest runner. Instead of making a coding agent print a long sequence of setup commands, the agent can write a TOML manifest and tell the user to run a short command such as:

```sh
tali 03
```

Tali is a runtime, not a planner. The manifest author decides what should happen. Tali shows the plan, asks for approval, collects inputs and secrets, executes steps predictably, masks secrets, and stores complete run logs for later repair.

## Install

macOS/Linux:

```sh
curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/hjun1052/tali/releases/latest/download/install.ps1 | iex
```

Installer environment variables:

- `TALI_VERSION=0.1.0` installs a specific release tag.
- `TALI_INSTALL_DIR=/path/to/bin` changes the install directory.
- `TALI_REPO=owner/repo` installs from a fork or renamed official repository.
- `TALI_BASE_URL=https://host/path` overrides the release asset base URL.
- `TALI_INSTALL_SKILL=0` skips automatic agent skill installation.
- `TALI_SKILL_DIRS=/path/a:/path/b` installs the bundled `$tali-agent` skill into explicit skill directories.

From source:

```sh
cargo build --release
cargo install --path .
```

Release builds are produced by GitHub Actions when a `v*` tag is pushed. Each release includes macOS, Linux, and Windows archives plus SHA-256 checksum files. Archives contain the `tali` binary, installer scripts, the bundled `$tali-agent` skill, README, install guide, release guide, license, security policy, changelog, and generated shell completion files.
See `INSTALL.md` for archive verification and shell completion setup.
See `RELEASE.md` for the maintainer release checklist.

The declared minimum supported Rust version is 1.85.

```sh
git tag v0.1.0
git push origin v0.1.0
```

## Storage

Tali uses platform app data directories:

- macOS: `~/Library/Application Support/tali/`
- Linux: `$XDG_DATA_HOME/tali/` or `~/.local/share/tali/`
- Windows: `%APPDATA%\tali\`

Inside that directory Tali creates:

```text
manifests/
runs/
logs/
cache/
secrets/
```

The `secrets/` directory exists for future encrypted persistent secrets. The MVP only supports prompt-time secret input and log masking.

Project-local manifests are also supported. Tali searches the current directory and then walks upward until it finds a matching `.tali/<name>.toml` file:

```text
.tali/setup.toml
.tali/build.toml
.tali/deploy.toml
```

Run them by name, for example:

```sh
tali setup
```

## Example AI Workflow

1. An AI agent writes `setup.toml`.
2. The user adds it:

```sh
tali add setup.toml
```

3. Tali assigns a short ID:

```text
Added manifest:
ID: 03
Name: nextjs-blog
Run:
tali 03
```

4. The user runs:

```sh
tali 03
```

If the run fails, share:

```sh
tali logs latest --for-ai
```

An AI agent can inspect the structured, masked log summary and create a repair manifest.
While a run is still active, an agent can also follow masked live events:

```sh
tali logs follow latest
```

## Manifest Example

```toml
version = 1
name = "nextjs-blog"
description = "Install dependencies, create .env, and start the dev server."

[[inputs]]
name = "database_url"
prompt = "Database URL"
secret = true
required = true

[[inputs]]
name = "project_name"
prompt = "Project name"
secret = false
required = true
default = "my-app"

[[steps]]
name = "Install dependencies"
type = "shell"
cmd = "npm install"

[[steps]]
name = "Write env file"
type = "write_file"
path = ".env"
content = """
DATABASE_URL={{database_url}}
PROJECT_NAME={{project_name}}
"""

[[steps]]
name = "Start dev server"
type = "shell"
cmd = "npm run dev"
when = "not file_exists('.env.skip-dev')"
```

## Commands

```sh
tali add <path>          # add a global manifest and assign an ID
tali list                # list global manifests
tali run <id-or-name>    # run a manifest
tali <id-or-name>        # shortcut for tali run <id-or-name>
tali inspect <id-or-name>
tali inspect <id-or-name> --json
tali run <id-or-name> --dry-run
tali run <id-or-name> --yes
tali run <id-or-name> --input project_name=my-app
tali run <id-or-name> --input-env openai_key=OPENAI_API_KEY
tali logs latest
tali logs follow latest
tali logs follow <run-id>
tali logs latest --json
tali logs latest --for-ai
tali logs <run-id>
tali doctor
tali doctor --json
tali self-test
tali self-test --json
tali completions zsh
```

Use `--input key=value` for non-interactive runs. Use `--input-env key=ENV_VAR` to read values from the environment. If the matching manifest input is marked `secret = true`, Tali still treats the value as secret and masks it in terminal output and logs. Prefer `--input-env` for secrets because command-line arguments may be visible to the operating system process table or shell history.

Each run stores `run.json`, `events.jsonl`, `stdout.log`, `stderr.log`, and the manifest copy under `runs/<run-id>/`. `events.jsonl` is append-only and records run start, step start, masked stdout/stderr lines, step completion, skipped steps, and run completion. `logs/latest-running` points to the active run while one is executing, and `logs/latest` points to the latest run.

Generate shell completions with:

```sh
tali completions bash
tali completions zsh
tali completions fish
tali completions powershell
tali completions elvish
```

After installing a release archive or crates.io build, verify the installation with:

```sh
tali self-test
tali doctor --json
```

## Step Types

### Shell

```toml
[[steps]]
name = "Deploy"
type = "shell"
cmd = "npm run deploy"
cwd = "."

[steps.env]
OPENAI_API_KEY = "{{openai_key}}"
```

On macOS and Linux, shell commands run with `sh -lc`. On Windows, Tali prefers `pwsh` and falls back to `powershell`, using `-NoProfile -Command`.

### Write File

```toml
[[steps]]
name = "Write env"
type = "write_file"
path = ".env"
content = "OPENAI_API_KEY={{openai_key}}"
overwrite = true
create_dirs = true
```

### Mkdir

```toml
[[steps]]
type = "mkdir"
path = "config"
```

### Copy

```toml
[[steps]]
type = "copy"
from = ".env.example"
to = ".env"
overwrite = false
```

### Conditional Steps

Every step type supports an optional `when` condition. If the condition evaluates to false, Tali skips the step and records it as `skipped` in `run.json`.

```toml
[[steps]]
name = "Install macOS tools"
type = "shell"
cmd = "brew bundle"
when = "os_is('macos') && file_exists('Brewfile')"

[[steps]]
name = "Use preview config"
type = "copy"
from = ".env.preview"
to = ".env"
overwrite = false
when = "input_equals('target', 'preview')"
```

Supported operators:

- `not`
- `&&`
- `||`
- parentheses

Supported condition functions:

- `os_is("macos" | "linux" | "windows")`
- `file_exists("path")`
- `dir_exists("path")`
- `env_exists("NAME")`
- `input_exists("name")`
- `input_equals("name", "value")`

Tali does not add a separate `else` block. Use complementary `when` conditions on separate steps, for example `input_equals('target', 'prod')` and `not input_equals('target', 'prod')`. Path checks are interpolated and use the same outside-working-directory safety rules as file operations unless `allow_outside_cwd = true` is set.

## Security Notes

Tali always shows the plan before execution and requires approval unless `--yes` is passed. Shell commands are not sandboxed and are not deeply analyzed for danger.

By default, file operations cannot use absolute paths, `..` traversal, or symlink paths that resolve outside the working directory. A manifest can opt out with:

```toml
allow_outside_cwd = true
```

Secret inputs use hidden prompts and are masked in commands, environment values, stdout, stderr, and JSON logs. Tali does not infer where secrets belong; manifest interpolation controls that.

Before overwriting files, Tali stores lightweight backups under the run directory so future rollback support can build on the log records.

## Release Checklist

Before publishing a release:

```sh
scripts/release-check.sh
```

Then push a version tag:

```sh
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds archives for Linux, macOS Intel, macOS Apple Silicon, and Windows, then uploads checksum files with the GitHub Release.
Each archive is smoke-tested inside the workflow before upload.
Publish the crates.io package with the manual `Publish crate` workflow after the GitHub Release succeeds.

Release artifacts also receive GitHub artifact attestations. After publishing from a GitHub repository, users can verify an archive with:

```sh
gh attestation verify tali-linux-x86_64.tar.gz --repo OWNER/REPO
shasum -a 256 -c tali-linux-x86_64.tar.gz.sha256
```

To build a local Unix-style archive for a manual smoke test:

```sh
scripts/package-local.sh
```
