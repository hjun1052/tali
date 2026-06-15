# Changelog

## 0.1.2

- Added `tali cleanup` for conservative run log and cache cleanup with preview-by-default behavior.
- Added `replace_in_file` steps for filling existing placeholders without logging rendered secret content.
- Documented cleanup, replacement steps, and a clearer first-run project story in the README.
- Updated the bundled `tali-agent` skill guidance so agents prefer native file steps for placeholder replacement.

## 0.1.1

- Added `tali update` for installer-based self updates.
- Added `tali skill install <dir>` for explicit `$tali-agent` skill installation.
- Added `tali add --json` so agents can register manifests and show users only `tali <id>`.
- Refreshed the README around the user problem, quick demo, agent workflow, and release install path.
- Bundled the `$tali-agent` skill into the binary for direct skill installation.

## 0.1.0

Initial MVP release.

- Added global and project-local manifest execution.
- Added short numeric manifest IDs.
- Added `shell`, `write_file`, `mkdir`, and `copy` steps.
- Added `when` conditions for step-level conditional execution.
- Added prompt-time inputs, secret masking, `--input`, and `--input-env`.
- Added run logs with `run.json`, stdout/stderr logs, latest pointer, and AI repair summaries.
- Added live `events.jsonl`, `logs/latest-running`, and `tali logs follow` for real-time run observation.
- Added path safety for file operations, including symlink escape protection.
- Added lightweight backups before overwriting files.
- Added doctor/environment snapshot capture.
- Added JSON doctor output and post-install self-test.
- Added shell completion generation.
- Added cross-platform CI and release packaging workflows.
- Added one-line macOS/Linux and Windows installer scripts for GitHub Releases.
- Added bundled `$tali-agent` skill installation through release archives and installers.
- Added MSRV, license policy, and security audit automation.
