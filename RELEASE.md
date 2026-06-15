# Releasing Tali

This checklist is for maintainers publishing official Tali builds.

## Prerequisites

- A GitHub repository with Actions enabled.
- A crates.io account that has publish rights for the `tali` crate.
- A GitHub Actions secret named `CARGO_REGISTRY_TOKEN` in the repository or in the `crates-io` environment.
- The release version set in `Cargo.toml` and `CHANGELOG.md`.

## Local Preflight

Run the full release gate:

```sh
scripts/release-check.sh
```

This checks formatting, MSRV compatibility when Rust 1.85.0 is installed, clippy, tests, license policy, crate packaging, publish dry-run, completions, optional advisory audit, and local archive packaging.

## GitHub Release

Commit the release changes, then create and push a version tag:

```sh
git tag v0.1.3
git push origin v0.1.3
```

The `Release` workflow builds and smoke-tests archives for Linux, macOS Intel, macOS Apple Silicon, and Windows. It uploads the archives, checksum files, installer scripts, bundled `$tali-agent` skill, and GitHub artifact attestations to the GitHub Release.
The pushed tag must match the version in `Cargo.toml`. For example, `Cargo.toml` version `0.1.3` must be released with tag `v0.1.3`.

After the workflow completes, verify at least one downloaded archive:

```sh
shasum -a 256 -c tali-linux-x86_64.tar.gz.sha256
gh attestation verify tali-linux-x86_64.tar.gz --repo OWNER/REPO
./tali self-test
```

Then verify the one-line installers against the published release:

```sh
curl -fsSL https://github.com/OWNER/REPO/releases/latest/download/install.sh | TALI_INSTALL_DIR="$(mktemp -d)" sh
```

On Windows PowerShell:

```powershell
$env:TALI_INSTALL_DIR = Join-Path $env:TEMP "tali-install-test"
irm https://github.com/OWNER/REPO/releases/latest/download/install.ps1 | iex
```

## crates.io

Publish the crate after the GitHub Release succeeds.

Use the `Publish crate` GitHub Actions workflow and enter the exact version from `Cargo.toml`, for example `0.1.3`.

As a fallback, publish locally:

```sh
cargo login
cargo publish --locked
```

Then verify installation from crates.io:

```sh
cargo install tali
tali --version
tali self-test
```

## Post-release

- Confirm GitHub Release assets, checksums, and attestations are present.
- Confirm `install.sh` and `install.ps1` are present in the GitHub Release.
- Confirm archives contain `skills/tali-agent/SKILL.md`.
- Confirm one-line installers install `$tali-agent` or cleanly report that no supported skill directory was detected.
- Confirm `cargo install tali` installs the published version.
- Create the next changelog section.
- If the release was faulty, yank the crates.io version rather than deleting the GitHub tag:

```sh
cargo yank tali --version 0.1.3
```
