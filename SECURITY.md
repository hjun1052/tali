# Security Policy

Tali executes commands and file operations described by manifests. Treat manifests as executable instructions.

## Reporting Vulnerabilities

If you find a vulnerability, report it privately through the repository security advisory flow once a public repository is available. Do not open a public issue with exploit details for secret leakage, path traversal, command execution bypasses, or log disclosure issues.

## Current Security Model

- Tali is a runtime, not a sandbox.
- Shell steps are shown before execution and require approval unless `--yes` is used.
- File steps are restricted to the project working directory by default.
- `allow_outside_cwd = true` disables the path restriction for a manifest.
- Secret inputs are masked in terminal output and run logs.
- Secret inputs may intentionally be written to files or passed to commands when the manifest author interpolates them.
- Persistent encrypted secrets are not implemented in the MVP.

## Safe Automation

Prefer `--input-env key=ENV_VAR` over `--input key=value` for secret automation. Command-line arguments may be visible in shell history or process inspection tools.

## Dependency Auditing

The repository includes a scheduled GitHub Actions workflow that installs `cargo-audit` and checks RustSec advisories. Local release checks run `cargo audit` when the command is available.
The same workflow also runs the repository license policy in `scripts/check-licenses.py`.

## Release Provenance

The release workflow generates GitHub artifact attestations for release archives and their SHA-256 checksum files. Consumers should verify both the attestation and checksum before trusting a downloaded binary.
