# Tali Manifest Authoring

## Minimal Manifest Shape

```toml
version = 1
name = "setup-project"
description = "Install dependencies, write config, and run checks."

[[inputs]]
name = "api_key"
prompt = "API key"
secret = true
required = true

[[steps]]
name = "Install dependencies"
type = "shell"
cmd = "npm install"

[[steps]]
name = "Write env"
type = "write_file"
path = ".env"
content = "API_KEY={{api_key}}\n"

[[steps]]
name = "Run checks"
type = "shell"
cmd = "npm test"
```

## Step Types

Use `shell` for commands:

```toml
[[steps]]
name = "Build"
type = "shell"
cmd = "cargo build --release --locked"
cwd = "."

[steps.env]
RUST_LOG = "info"
TOKEN = "{{token}}"
```

Use `write_file` for generated files:

```toml
[[steps]]
name = "Write config"
type = "write_file"
path = "config/local.toml"
content = """
endpoint = "{{endpoint}}"
"""
overwrite = true
create_dirs = true
```

Use `mkdir` for directories:

```toml
[[steps]]
name = "Create cache"
type = "mkdir"
path = "cache"
```

Use `copy` for templates:

```toml
[[steps]]
name = "Copy env template"
type = "copy"
from = ".env.example"
to = ".env"
overwrite = false
```

## Conditional Steps

Every step can include `when`. False conditions are logged as skipped.

```toml
[[steps]]
name = "macOS dependencies"
type = "shell"
cmd = "brew bundle"
when = "os_is('macos') && file_exists('Brewfile')"

[[steps]]
name = "Preview deploy"
type = "shell"
cmd = "npm run deploy:preview"
when = "input_equals('target', 'preview')"
```

Supported operators:

- `not`
- `&&`
- `||`
- parentheses

Supported functions:

- `os_is("macos" | "linux" | "windows")`
- `file_exists("path")`
- `dir_exists("path")`
- `env_exists("NAME")`
- `input_exists("name")`
- `input_equals("name", "value")`

Do not invent `else`. Use a second step with the complementary condition.

## Safety Checklist

Before adding or running a manifest:

- Confirm the manifest name is stable and descriptive.
- Confirm every secret is an input with `secret = true`.
- Confirm secret values are not hardcoded in `cmd`, `content`, paths, or env values.
- Confirm write/copy/mkdir paths are project-relative unless `allow_outside_cwd = true` is deliberately needed.
- Confirm dangerous shell commands are visible as separate reviewed steps.
- Confirm outputs needed by future repair are captured by normal stdout/stderr.
- Prefer project-local `.tali/<name>.toml` for repeatable repository workflows.
- Prefer `tali add <path>` for temporary or one-off global handoffs.

## Handoff Patterns

For a global handoff:

```sh
tali add /path/to/setup.toml
tali inspect 01
```

Then tell the user:

```sh
tali 01
```

For a project-local handoff:

```sh
mkdir -p .tali
# write .tali/setup.toml
tali inspect setup
```

Then tell the user:

```sh
tali setup
```
