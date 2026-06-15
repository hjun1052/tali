#!/usr/bin/env bash
set -euo pipefail

strict="${TALI_RELEASE_STRICT:-0}"

cargo fmt --check
if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  if [ -n "$(git status --porcelain)" ]; then
    echo "working tree is not clean" >&2
    git status --short >&2
    exit 1
  fi
fi
if command -v rustup >/dev/null 2>&1; then
  if rustup toolchain list | grep -q '^1.85.0'; then
    rustup run 1.85.0 cargo check --locked
  else
    echo "Rust 1.85.0 not installed; skipping local MSRV check." >&2
  fi
else
  echo "rustup not installed; skipping local MSRV check." >&2
fi
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
sh -n scripts/install.sh
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -Command '$errors = $null; [System.Management.Automation.PSParser]::Tokenize((Get-Content -Raw "scripts/install.ps1"), [ref]$errors) | Out-Null; if ($errors) { $errors | ForEach-Object { Write-Error $_ }; exit 1 }'
else
  echo "pwsh not installed; skipping local PowerShell installer syntax check." >&2
fi
python3 scripts/check-release-metadata.py
python3 scripts/check-licenses.py
python3 scripts/check-crates-version.py
if command -v actionlint >/dev/null 2>&1; then
  actionlint .github/workflows/*.yml
elif [ "$strict" = "1" ]; then
  echo "actionlint is required when TALI_RELEASE_STRICT=1" >&2
  exit 1
else
  echo "actionlint not installed; skipping local workflow lint." >&2
fi
cargo package --locked --allow-dirty
if cargo package --locked --allow-dirty --list | grep -E '^(target|dist)/'; then
  echo "cargo package includes local build artifacts" >&2
  exit 1
fi
cargo build --release --locked
cargo publish --dry-run --locked --allow-dirty

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

mkdir -p "$tmpdir/completions"
target/release/tali completions bash > "$tmpdir/completions/tali.bash"
target/release/tali completions zsh > "$tmpdir/completions/_tali"
target/release/tali completions fish > "$tmpdir/completions/tali.fish"
target/release/tali completions powershell > "$tmpdir/completions/tali.ps1"
target/release/tali completions elvish > "$tmpdir/completions/tali.elv"

test -s "$tmpdir/completions/tali.bash"
test -s "$tmpdir/completions/_tali"
test -s "$tmpdir/completions/tali.fish"
test -s "$tmpdir/completions/tali.ps1"
test -s "$tmpdir/completions/tali.elv"

if command -v cargo-audit >/dev/null 2>&1; then
  cargo audit
elif [ "$strict" = "1" ]; then
  echo "cargo-audit is required when TALI_RELEASE_STRICT=1" >&2
  exit 1
else
  echo "cargo-audit not installed; skipping local advisory audit." >&2
fi

cargo install --path . --locked --root "$tmpdir/cargo-install" >/dev/null
TALI_DATA_DIR="$tmpdir/tali-install-data" "$tmpdir/cargo-install/bin/tali" self-test >/dev/null

scripts/package-local.sh >/dev/null
rm -rf dist

echo "Release check passed."
