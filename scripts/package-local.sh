#!/usr/bin/env bash
set -euo pipefail

version="$(awk -F\" '/^version = / { print $2; exit }' Cargo.toml)"
case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *) os="$(uname -s | tr '[:upper:]' '[:lower:]')" ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="arm64" ;;
  *) arch="$(uname -m)" ;;
esac
target_name="$os-$arch"
dist_dir="dist"
package_dir="$dist_dir/tali-$version-$target_name"

rm -rf "$package_dir"
mkdir -p "$package_dir/completions" "$package_dir/skills"

cargo build --release --locked
cp target/release/tali "$package_dir/tali"
cp README.md INSTALL.md RELEASE.md LICENSE SECURITY.md CHANGELOG.md "$package_dir/"
cp scripts/install.sh scripts/install.ps1 "$package_dir/"
cp scripts/install.sh scripts/install.ps1 "$dist_dir/"
cp -R .codex/skills/tali-agent "$package_dir/skills/tali-agent"

target/release/tali completions bash > "$package_dir/completions/tali.bash"
target/release/tali completions zsh > "$package_dir/completions/_tali"
target/release/tali completions fish > "$package_dir/completions/tali.fish"
target/release/tali completions powershell > "$package_dir/completions/tali.ps1"
target/release/tali completions elvish > "$package_dir/completions/tali.elv"

archive="$dist_dir/tali-$target_name.tar.gz"
tar -czf "$archive" -C "$dist_dir" "tali-$version-$target_name"

if command -v shasum >/dev/null 2>&1; then
  (cd "$dist_dir" && shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256")
  (cd "$dist_dir" && shasum -a 256 -c "$(basename "$archive").sha256")
else
  (cd "$dist_dir" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
  (cd "$dist_dir" && sha256sum -c "$(basename "$archive").sha256")
fi

smoke_dir="$(mktemp -d)"
trap 'rm -rf "$smoke_dir"' EXIT
tar -xzf "$archive" -C "$smoke_dir"
"$smoke_dir/tali-$version-$target_name/tali" --version >/dev/null
TALI_DATA_DIR="$smoke_dir/tali-data" "$smoke_dir/tali-$version-$target_name/tali" self-test >/dev/null
test -s "$smoke_dir/tali-$version-$target_name/README.md"
test -s "$smoke_dir/tali-$version-$target_name/INSTALL.md"
test -s "$smoke_dir/tali-$version-$target_name/RELEASE.md"
test -s "$smoke_dir/tali-$version-$target_name/LICENSE"
test -s "$smoke_dir/tali-$version-$target_name/SECURITY.md"
test -s "$smoke_dir/tali-$version-$target_name/CHANGELOG.md"
test -s "$smoke_dir/tali-$version-$target_name/install.sh"
test -s "$smoke_dir/tali-$version-$target_name/install.ps1"
test -s "$smoke_dir/tali-$version-$target_name/skills/tali-agent/SKILL.md"
test -s "$smoke_dir/tali-$version-$target_name/completions/tali.bash"
test -s "$smoke_dir/tali-$version-$target_name/completions/_tali"
test -s "$smoke_dir/tali-$version-$target_name/completions/tali.fish"
test -s "$smoke_dir/tali-$version-$target_name/completions/tali.ps1"
test -s "$smoke_dir/tali-$version-$target_name/completions/tali.elv"

echo "$archive"
