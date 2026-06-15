#!/usr/bin/env sh
set -eu

repo="${TALI_REPO:-hjun1052/tali}"
version="${TALI_VERSION:-latest}"
install_dir="${TALI_INSTALL_DIR:-}"
install_skill="${TALI_INSTALL_SKILL:-1}"

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "tali installer: missing required command: $1" >&2
    exit 1
  fi
}

download() {
  url="$1"
  dest="$2"
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget >/dev/null 2>&1; then
    wget -q "$url" -O "$dest"
  else
    echo "tali installer: curl or wget is required" >&2
    exit 1
  fi
}

add_skill_dir() {
  candidate="$1"
  if [ -z "$candidate" ]; then
    return
  fi
  case ":$skill_dirs:" in
    *":$candidate:"*) ;;
    *) skill_dirs="${skill_dirs}${candidate}:" ;;
  esac
}

detect_skill_dirs() {
  skill_dirs=""
  if [ -n "${TALI_SKILL_DIRS:-}" ]; then
    old_ifs="$IFS"
    IFS=":"
    for dir in $TALI_SKILL_DIRS; do
      add_skill_dir "$dir"
    done
    IFS="$old_ifs"
    return
  fi

  if [ -n "${CODEX_HOME:-}" ]; then
    add_skill_dir "$CODEX_HOME/skills"
  elif [ -d "$HOME/.codex" ] || command -v codex >/dev/null 2>&1; then
    add_skill_dir "$HOME/.codex/skills"
  fi

  if [ -d "$HOME/.agents/skills" ]; then
    add_skill_dir "$HOME/.agents/skills"
  fi

  if [ -n "${CLAUDE_CONFIG_DIR:-}" ]; then
    add_skill_dir "$CLAUDE_CONFIG_DIR/skills"
  elif [ -d "$HOME/.claude" ] || command -v claude >/dev/null 2>&1; then
    add_skill_dir "$HOME/.claude/skills"
  fi
}

install_agent_skill() {
  if [ "$install_skill" = "0" ] || [ "$install_skill" = "false" ]; then
    echo "Skipping Tali agent skill installation."
    return
  fi

  skill_src="$(find "$tmpdir/extract" -type d -path '*/skills/tali-agent' | head -n 1)"
  if [ -z "$skill_src" ]; then
    echo "Warning: release archive did not contain the tali-agent skill." >&2
    return
  fi

  detect_skill_dirs
  if [ -z "$skill_dirs" ]; then
    echo "No supported agent skill directory detected."
    echo "Set TALI_SKILL_DIRS=/path/to/skills to install the tali-agent skill manually."
    return
  fi

  installed_any=0
  old_ifs="$IFS"
  IFS=":"
  for skill_dir in $skill_dirs; do
    [ -n "$skill_dir" ] || continue
    mkdir -p "$skill_dir"
    dest="$skill_dir/tali-agent"
    if [ -d "$dest" ]; then
      if command -v diff >/dev/null 2>&1 && diff -qr "$skill_src" "$dest" >/dev/null 2>&1; then
        echo "Tali agent skill already up to date at $dest"
        installed_any=1
        continue
      fi
      backup="$dest.bak-$(date +%Y%m%d%H%M%S)"
      mv "$dest" "$backup"
      echo "Backed up existing tali-agent skill to $backup"
    fi
    cp -R "$skill_src" "$dest"
    echo "Installed tali-agent skill to $dest"
    installed_any=1
  done
  IFS="$old_ifs"

  if [ "$installed_any" -eq 0 ]; then
    echo "No agent skill directory was writable; set TALI_SKILL_DIRS to install the skill."
  fi
}

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *)
    echo "tali installer: unsupported OS: $(uname -s)" >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64)
    if [ "$os" = "macos" ]; then
      arch="arm64"
    else
      echo "tali installer: Linux arm64 release archive is not available yet" >&2
      exit 1
    fi
    ;;
  *)
    echo "tali installer: unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

archive="tali-${os}-${arch}.tar.gz"
if [ "$version" = "latest" ]; then
  base_url="https://github.com/${repo}/releases/latest/download"
else
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
  base_url="https://github.com/${repo}/releases/download/${tag}"
fi
if [ -n "${TALI_BASE_URL:-}" ]; then
  base_url="$TALI_BASE_URL"
fi

if [ -z "$install_dir" ]; then
  if [ -d /usr/local/bin ] && [ -w /usr/local/bin ]; then
    install_dir="/usr/local/bin"
  else
    install_dir="$HOME/.local/bin"
  fi
fi

need tar
need mktemp
need chmod

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT INT TERM

echo "Downloading ${archive} from ${repo}..."
download "${base_url}/${archive}" "$tmpdir/$archive"
download "${base_url}/${archive}.sha256" "$tmpdir/$archive.sha256"

if command -v shasum >/dev/null 2>&1; then
  (cd "$tmpdir" && shasum -a 256 -c "$archive.sha256")
elif command -v sha256sum >/dev/null 2>&1; then
  (cd "$tmpdir" && sha256sum -c "$archive.sha256")
else
  echo "tali installer: shasum or sha256sum is required for checksum verification" >&2
  exit 1
fi

mkdir -p "$tmpdir/extract"
tar -xzf "$tmpdir/$archive" -C "$tmpdir/extract"

binary="$(find "$tmpdir/extract" -type f -name tali -perm -u+x | head -n 1)"
if [ -z "$binary" ]; then
  binary="$(find "$tmpdir/extract" -type f -name tali | head -n 1)"
fi
if [ -z "$binary" ]; then
  echo "tali installer: archive did not contain a tali binary" >&2
  exit 1
fi

mkdir -p "$install_dir"
cp "$binary" "$install_dir/tali"
chmod 0755 "$install_dir/tali"

echo "Installed tali to $install_dir/tali"
"$install_dir/tali" --version
TALI_DATA_DIR="$tmpdir/tali-self-test" "$install_dir/tali" self-test >/dev/null
echo "tali self-test passed."
install_agent_skill

case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    echo "Warning: $install_dir is not on PATH." >&2
    echo "Add it to PATH or run $install_dir/tali directly." >&2
    ;;
esac
