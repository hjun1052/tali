# Tali

[![Release](https://img.shields.io/github/v/release/hjun1052/tali?label=release)](https://github.com/hjun1052/tali/releases)
[![CI](https://github.com/hjun1052/tali/actions/workflows/ci.yml/badge.svg)](https://github.com/hjun1052/tali/actions)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

**One command instead of a fragile wall of setup instructions.**

Tali is an AI-friendly command manifest runner. It gives coding agents a safe,
auditable way to hand work to a human:

```sh
tali 03
```

Instead of pasting this into terminal:

```sh
cd app
npm install
cp .env.example .env
npm run db:migrate
npm run deploy
```

an agent writes a Tali manifest, registers it, and tells the user to run one
short command. Tali shows the plan, asks for approval, collects inputs and
secrets, executes the steps, masks sensitive output, and stores complete logs
for repair.

Tali is intentionally boring in the best way: it is a runtime, not a planner.
The agent decides what should happen. Tali runs the manifest predictably.

## Install

macOS and Linux:

```sh
curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/hjun1052/tali/releases/latest/download/install.ps1 | iex
```

The installer verifies checksums, runs `tali self-test`, and installs the
bundled `tali-agent` skill into detected agent skill directories when possible.

Useful installer options:

```sh
TALI_VERSION=0.1.3 curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
TALI_INSTALL_DIR="$HOME/.local/bin" curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
TALI_INSTALL_SKILL=0 curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
TALI_SKILL_DIRS="$HOME/.codex/skills:$HOME/.agents/skills" curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
```

Update later with:

```sh
tali update
```

## What It Feels Like

```sh
$ tali 03
Manifest: nextjs-blog
Plan:
1. Shell: npm install
2. Write file: .env
3. Shell: npm run dev
Inputs required:
- project_name
- database_url [secret]
Okay to proceed? [y/N] y
Project name [my-app]:
Database URL:
Running step 1/3: Install dependencies
...
Run succeeded.
Logs saved to:
/Users/you/Library/Application Support/tali/runs/run-20260615071629-g42VzkQr
```

If something fails:

```text
Run failed.
Failed step: 3 / 3
Step name: Start dev server
Exit code: 1
Logs saved to:
...
For AI repair, share:
tali logs latest
```

## Why Agents Need This

Coding agents are getting good at planning local changes, but the handoff to a
human is still clumsy. The usual pattern is a long checklist of commands,
manual edits, copied secrets, and "if this fails, try..." notes.

Tali turns that into a small contract:

- The agent writes exactly what should happen.
- The user sees the plan before it runs.
- Secrets are prompted at runtime and masked everywhere.
- Every run gets structured logs, stdout, stderr, live events, and a manifest copy.
- A failed run can be inspected by an agent and repaired with a new manifest.

That makes Tali useful for setup scripts, local dev bootstraps, one-off deploys,
project migrations, test runs, environment creation, and anything else where
"please run these 12 commands" would normally appear in chat.

## Quick Start For Agents

Create a manifest:

```toml
version = 1
name = "nextjs-blog"
description = "Install dependencies, write .env, and start dev."

[[inputs]]
name = "database_url"
prompt = "Database URL"
secret = true
required = true

[[inputs]]
name = "project_name"
prompt = "Project name"
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
```

Register it:

```sh
tali add setup.toml --json
```

Tell the user only:

```sh
tali 03
```

Watch progress while the user runs it:

```sh
tali logs follow latest
```

If it fails, collect repair context:

```sh
tali logs latest --for-ai
```

## Manifest Steps

Tali supports five MVP step types.

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

Shell commands run through `sh -lc` on macOS/Linux and PowerShell on Windows
(`pwsh` when available, then `powershell`). Tali streams stdout/stderr to the
user and stores masked logs.

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

Before overwriting a file, Tali stores a lightweight backup under the run
directory so future rollback support has the data it needs.

### Mkdir

```toml
[[steps]]
name = "Create config directory"
type = "mkdir"
path = "config"
```

### Copy

```toml
[[steps]]
name = "Copy env template"
type = "copy"
from = ".env.example"
to = ".env"
overwrite = false
```

### Replace In File

Use `replace_in_file` when a project already has placeholders and Tali should
fill only those positions:

```toml
[[steps]]
name = "Fill API key"
type = "replace_in_file"
path = ".env"
expected_matches = 1

[steps.replacements]
"__OPENAI_API_KEY__" = "{{openai_key}}"
```

Given this existing file:

```env
OPENAI_API_KEY=__OPENAI_API_KEY__
```

Tali prompts for `openai_key`, replaces the placeholder, stores a backup, and
logs only the number of replacements. It does not log the rendered file content.

Fields:

- `path`: required file path
- `replacements`: required table of `placeholder = replacement`
- `require_match`: optional bool, default `true`
- `expected_matches`: optional total replacement count

## Conditions

Steps can be conditional with `when`:

```toml
[[steps]]
name = "Install macOS tools"
type = "shell"
cmd = "brew bundle"
when = "os_is('macos') && file_exists('Brewfile')"
```

Supported helpers:

- `os_is("macos" | "linux" | "windows")`
- `file_exists("path")`
- `dir_exists("path")`
- `env_exists("NAME")`
- `input_exists("name")`
- `input_equals("name", "value")`

Use `not`, `&&`, `||`, and parentheses for boolean logic.

## Commands

```sh
tali add <path>
tali add <path> --json
tali list
tali run <id-or-name>
tali <id-or-name>
tali inspect <id-or-name>
tali run <id-or-name> --dry-run
tali run <id-or-name> --yes
tali run <id-or-name> --input key=value
tali run <id-or-name> --input-env secret_name=ENV_VAR
tali logs latest
tali logs latest --json
tali logs latest --for-ai
tali logs follow latest
tali logs <run-id>
tali cleanup --dry-run
tali cleanup --older-than 30d
tali cleanup --older-than 30d --yes
tali skill install <skill-dir>
tali skill install <skill-dir> --no-overwrite
tali update
tali doctor
tali self-test
tali completions zsh
```

## Cleanup

Tali keeps full run logs because they are valuable for repair. Over time, that
can take space. Use cleanup when old runs are no longer useful:

```sh
tali cleanup --dry-run
```

Preview output:

```text
Cleanup preview:
Runs older than 30d: 12
Cache entries older than 30d: 38
Estimated space to free: 84.0 MB

Nothing deleted. Run with:
tali cleanup --older-than 30d --yes
```

Cleanup is conservative:

- `tali cleanup` previews by default.
- `--yes` is required to delete.
- `runs/` and `cache/` are eligible.
- `manifests/` and `secrets/` are never deleted by cleanup.
- A currently running run is skipped.
- If the latest run is deleted, `logs/latest` is moved to the newest remaining run.

Supported ages: `60s`, `15m`, `12h`, `30d`.

## Project Manifests

Global manifests are temporary or semi-temporary handoffs created by agents.
Project manifests are shareable project assets:

```text
project/
└─ .tali/
   ├─ setup.toml
   ├─ build.toml
   └─ deploy.toml
```

Run them by name:

```sh
tali setup
tali build
tali deploy
```

## Storage

Tali uses platform-correct app data directories:

- macOS: `~/Library/Application Support/tali/`
- Linux: `$XDG_DATA_HOME/tali/` or `~/.local/share/tali/`
- Windows: `%APPDATA%\tali\`

Layout:

```text
tali/
├─ manifests/
├─ runs/
├─ logs/
├─ cache/
└─ secrets/
```

Each run stores:

```text
runs/<run-id>/
├─ run.json
├─ events.jsonl
├─ stdout.log
├─ stderr.log
├─ manifest.toml
└─ backups/
```

## Security Model

Tali is not a sandbox and does not try to prove shell commands are safe.

It does provide practical guardrails:

- The plan is shown before execution.
- Approval is required unless `--yes` is passed.
- Secret inputs use hidden prompts.
- Secret values are masked in commands, env values, stdout, stderr, live events, and JSON logs.
- File operations cannot escape the working directory by default.
- `allow_outside_cwd = true` is required for file writes/copies/replacements/mkdir outside the working directory.
- Persistent encrypted secrets are not implemented yet; `secrets/` exists for future use.

Tali does not guess where secrets should go. The manifest author must decide
where to interpolate them.

## Development

```sh
cargo test
cargo run -- doctor
cargo run -- self-test
```

Rust version is 1.85. Maintainer release tags match the Cargo version, for
example `v0.1.3`.

Release checks:

```sh
./scripts/release-check.sh
```

## Status

Tali is early, practical infrastructure for AI-assisted development workflows.
The core loop is already here: manifest, approval, execution, live logs, repair
logs, agent skill installation, update, and cleanup.

If you build with coding agents and are tired of turning chat instructions into
terminal chores, Tali is for you.
