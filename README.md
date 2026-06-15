# Tali

Stop pasting long setup instructions into chat.

Tali is a small command runner for AI-generated task manifests. Instead of an agent telling a user to run:

```sh
cd app
npm install
cp .env.example .env
npm run db:migrate
npm run dev
```

the agent writes a Tali manifest, registers it, and tells the user:

```sh
tali 03
```

Tali then shows the plan, asks for approval, collects inputs and secrets, runs the steps, masks sensitive values, and saves logs that an AI agent can inspect later if something fails.

## Install

macOS/Linux:

```sh
curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/hjun1052/tali/releases/latest/download/install.ps1 | iex
```

The installer verifies checksums, runs `tali self-test`, and installs the bundled `$tali-agent` skill when a supported agent skill directory is detected.

Useful installer options:

- `TALI_VERSION=0.1.1` installs a specific release.
- `TALI_INSTALL_DIR=/path/to/bin` changes the binary install directory.
- `TALI_INSTALL_SKILL=0` skips agent skill installation.
- `TALI_SKILL_DIRS=/path/a:/path/b` installs `$tali-agent` into explicit skill directories.

## Why

AI agents are good at deciding what should happen, but users should not have to manually copy a fragile sequence of commands from a chat transcript. Tali makes the handoff explicit:

- The agent plans and writes a manifest.
- Tali executes the manifest predictably.
- The user approves before anything runs.
- Secrets are collected at runtime and masked in output.
- Every run leaves structured repair logs.
- A failed run can become a new repair manifest.

Tali is a runtime, not an intelligent planner. It does not guess where secrets go and it does not pretend shell commands are safe. It makes the plan visible, asks for consent, and records what happened.

## Quick Demo

An agent writes `setup.toml`:

```toml
version = 1
name = "nextjs-blog"
description = "Install dependencies, write .env, and start dev."

[[inputs]]
name = "database_url"
prompt = "Database URL"
secret = true
required = true

[[steps]]
name = "Install dependencies"
type = "shell"
cmd = "npm install"

[[steps]]
name = "Write env file"
type = "write_file"
path = ".env"
content = "DATABASE_URL={{database_url}}\n"

[[steps]]
name = "Start dev server"
type = "shell"
cmd = "npm run dev"
```

The agent registers it:

```sh
tali add setup.toml --json
```

The user only sees:

```sh
tali 03
```

Tali shows:

```text
Manifest: nextjs-blog
Plan:
1. Shell: npm install
2. Write file: .env
3. Shell: npm run dev
Inputs required:
- database_url [secret]
Okay to proceed? [y/N]
```

## Agent Workflow

Use the bundled `$tali-agent` skill with Codex-compatible agents. The installer tries to place it into detected skill directories automatically. You can also install it manually:

```sh
tali skill install ~/.codex/skills
```

Recommended agent behavior:

1. Write a manifest.
2. Run `tali add <manifest> --json` itself.
3. Inspect or dry-run the manifest.
4. Tell the user only the short command, such as `tali 03`.
5. While the run is active, observe with `tali logs follow latest`.
6. On failure, inspect `tali logs latest --for-ai` and write a repair manifest.

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
tali skill install <skill-dir>
tali skill install <skill-dir> --no-overwrite
tali update
tali doctor
tali self-test
tali completions zsh
```

## Manifests

Tali supports global manifests, which get short IDs like `01`, `02`, `03`, and project-local manifests stored in `.tali/`:

```text
.tali/setup.toml
.tali/build.toml
.tali/deploy.toml
```

Run project manifests by name:

```sh
tali setup
```

Supported step types:

- `shell`
- `write_file`
- `mkdir`
- `copy`

Every step can use `when` conditions:

```toml
[[steps]]
name = "Install macOS tools"
type = "shell"
cmd = "brew bundle"
when = "os_is('macos') && file_exists('Brewfile')"
```

Supported condition functions:

- `os_is("macos" | "linux" | "windows")`
- `file_exists("path")`
- `dir_exists("path")`
- `env_exists("NAME")`
- `input_exists("name")`
- `input_equals("name", "value")`

Use `not`, `&&`, `||`, and parentheses for boolean logic.

## Logs

Each run stores:

```text
runs/<run-id>/
├─ run.json
├─ events.jsonl
├─ stdout.log
├─ stderr.log
└─ manifest.toml
```

Follow a live run:

```sh
tali logs follow latest
```

Give an agent repair context:

```sh
tali logs latest --for-ai
```

`events.jsonl`, stdout/stderr logs, and JSON summaries all use the same secret masking.

## Storage

Tali uses platform app data directories:

- macOS: `~/Library/Application Support/tali/`
- Linux: `$XDG_DATA_HOME/tali/` or `~/.local/share/tali/`
- Windows: `%APPDATA%\tali\`

Inside that directory:

```text
manifests/
runs/
logs/
cache/
secrets/
```

The `secrets/` directory exists for future encrypted persistent secrets. Current releases only support prompt-time secret input and masking.

## Security Notes

Tali always shows the plan before execution and requires approval unless `--yes` is passed. Shell commands are not sandboxed and are not deeply analyzed for danger.

By default, file operations cannot use absolute paths, `..` traversal, or symlink paths that resolve outside the working directory. A manifest can opt out with:

```toml
allow_outside_cwd = true
```

Secret inputs use hidden prompts and are masked in commands, environment values, stdout, stderr, live events, and JSON logs.

Before overwriting files, Tali stores lightweight backups under the run directory so future rollback support can build on the log records.

## Release Builds

GitHub Releases include Linux, macOS Intel, macOS Apple Silicon, and Windows archives, checksums, provenance attestations, installer scripts, completions, and the bundled `$tali-agent` skill.

Maintainer release tags match the Cargo version, for example `v0.1.1`.

From source:

```sh
cargo install --path .
```

The declared minimum supported Rust version is 1.85.
