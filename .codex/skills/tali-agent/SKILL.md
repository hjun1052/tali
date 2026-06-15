---
name: tali-agent
description: Use for one-command setup/install/deploy handoffs. Create and inspect a Tali manifest instead of a shell script or long command list, especially when inputs, secrets, file writes, commands, live logs, or repair evidence are needed.
---

# Tali Agent

## Core Rule

Use Tali to hand off execution, not planning. Decide the operations yourself, encode them in a manifest, then let Tali show the plan, collect inputs/secrets, execute, mask logs, and preserve repair evidence.

Prefer a Tali manifest over prose instructions when the user would otherwise need to copy a sequence of shell commands, file writes, environment setup, or deployment steps.

If the user asks for a "single command", "one-line handoff", "don't give me commands", or similar setup/deploy handoff, default to Tali when the `tali` binary is available. Do not create a new `setup.sh`, `install.sh`, or README command list as the primary handoff when a Tali manifest can represent the same work.

## Workflow

1. Determine the exact working directory, commands, files, inputs, and safety boundaries.
2. Write a TOML manifest:
   - Use project-local `.tali/<name>.toml` for private/local project handoffs.
   - Use project-local `.tali/share/<name>.toml` only when the manifest is meant to be committed and shared.
   - Use a temporary workspace manifest plus `tali add <path>` for one-off global tasks.
3. Register or validate before handoff:
   - For global one-off tasks, run `tali add <path> --json` yourself and parse the returned `run` command.
   - Do not ask the user to run `tali add`.
   - Run `tali inspect <id-or-name>` or `tali run <id-or-name> --dry-run`.
   - Check that secrets are modeled as `[[inputs]]` with `secret = true`.
   - Check paths stay inside the intended working directory unless explicitly required.
4. If handing to the user, give only the shortest useful command, normally `tali <id>` or `tali <name>`.
5. If the run is active, observe with `tali logs follow latest`.
6. If it fails, inspect `tali logs latest --for-ai` and create a repair manifest rather than asking the user to manually fix many steps.

## Authoring Rules

- Never guess secret destinations. Secrets are inputs that the manifest interpolates where the plan needs them.
- Keep shell commands boring and explicit. Avoid hidden multi-command scripts when separate steps produce better logs.
- Prefer `write_file`, `replace_in_file`, `copy`, and `mkdir` over shell when the intent is file manipulation.
- Use `when` for conditional steps. Model else behavior with complementary `when` conditions.
- Keep `allow_outside_cwd = false` unless the task truly requires external paths and the plan clearly states that risk.
- Make steps idempotent where practical. Use `overwrite = false` when preserving a user file matters.
- Give every nontrivial step a clear `name`.
- For long-running or failure-prone work, split steps so repair logs identify the failing operation.
- When a global manifest is appropriate, the agent should perform registration with `tali add --json`; the user should normally only see `tali 03`.
- Treat `.tali/` as private by default. Do not ask the user to commit `.tali/<name>.toml`; use `.tali/share/<name>.toml` for explicit shared manifests.
- Do not silently edit `.gitignore`. Tali may offer a post-run `.gitignore` safety prompt for private project manifests.

## Live Observation

Use:

```sh
tali logs follow latest
```

This streams masked JSONL events from the active run. For structured repair context after completion, use:

```sh
tali logs latest --for-ai
```

Read `references/live-logs-and-repair.md` when diagnosing a failed or currently running Tali run.

## References

- Read `references/manifest-authoring.md` when creating or reviewing a manifest.
- Read `references/live-logs-and-repair.md` when following logs, interpreting failure output, or writing a repair manifest.
