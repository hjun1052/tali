# Installing Tali

## One-line Installer

macOS/Linux:

```sh
curl -fsSL https://github.com/hjun1052/tali/releases/latest/download/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/hjun1052/tali/releases/latest/download/install.ps1 | iex
```

The installers download the matching GitHub Release archive, verify the `.sha256` checksum, install the `tali` binary, run `tali self-test`, and install the bundled `$tali-agent` skill into detected agent skill directories.

Auto-detected skill locations include Codex (`$CODEX_HOME/skills` or `~/.codex/skills`), existing `~/.agents/skills`, and Claude-style skill directories when present. Existing `tali-agent` skills are backed up before replacement.

Installer environment variables:

- `TALI_VERSION=0.1.3` installs a specific release tag instead of the latest release.
- `TALI_INSTALL_DIR=/path/to/bin` changes where the binary is installed.
- `TALI_REPO=owner/repo` changes the GitHub repository.
- `TALI_BASE_URL=https://host/path` overrides the release asset base URL for mirrors or installer testing.
- `TALI_INSTALL_SKILL=0` skips automatic agent skill installation.
- `TALI_SKILL_DIRS=/path/a:/path/b` on macOS/Linux, or `TALI_SKILL_DIRS=C:\path\a;C:\path\b` on Windows, installs the bundled `$tali-agent` skill into explicit skill directories.

## From a Release Archive

1. Download the archive for your platform from GitHub Releases.
2. Download the matching `.sha256` file.
3. Verify the checksum.

macOS/Linux:

```sh
shasum -a 256 -c tali-linux-x86_64.tar.gz.sha256
tar -xzf tali-linux-x86_64.tar.gz
install -m 0755 tali /usr/local/bin/tali
tali --version
tali self-test
```

Windows PowerShell:

```powershell
$expected = (Get-Content .\tali-windows-x86_64.zip.sha256).Split(" ")[0]
$actual = (Get-FileHash .\tali-windows-x86_64.zip -Algorithm SHA256).Hash.ToLower()
if ($expected -ne $actual) { throw "checksum mismatch" }
Expand-Archive .\tali-windows-x86_64.zip
.\tali-windows-x86_64\tali.exe --version
.\tali-windows-x86_64\tali.exe self-test
```

## Verify GitHub Attestations

If the release was built by GitHub Actions, verify provenance with GitHub CLI:

```sh
gh attestation verify tali-linux-x86_64.tar.gz --repo OWNER/REPO
```

Replace `OWNER/REPO` with the published repository.

## From crates.io

This works after the crate has been published by a maintainer.

```sh
cargo install tali
tali --version
tali self-test
```

## Shell Completions

Release archives include pre-generated completion files under `completions/`.
They also include the `$tali-agent` skill under `skills/tali-agent/`.

You can also generate them directly:

```sh
tali completions bash
tali completions zsh
tali completions fish
tali completions powershell
tali completions elvish
```

Install completion files according to your shell's standard completion directory.

## Agent Skill

The installer attempts to install the bundled `$tali-agent` skill automatically. To install or refresh it manually:

```sh
tali skill install ~/.codex/skills
```

To update Tali in place:

```sh
tali update
```

## Maintainers

Maintainer release steps live in `RELEASE.md`, which is also included in release archives.
