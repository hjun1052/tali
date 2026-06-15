#!/usr/bin/env python3
import json
import sys
import tomllib
import urllib.error
import urllib.request
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CRATES_API = "https://crates.io/api/v1/crates/{name}"


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    package = cargo["package"]
    name = package["name"]
    version = package["version"]

    request = urllib.request.Request(
        CRATES_API.format(name=name),
        headers={
            "Accept": "application/json",
            "User-Agent": f"{name}-release-check/{version} (https://crates.io/crates/{name})",
        },
    )

    try:
        with urllib.request.urlopen(request, timeout=15) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        if error.code == 404:
            print(f"Crate '{name}' does not exist on crates.io yet; first publish is available.")
            return
        fail(f"failed to query crates.io for '{name}': HTTP {error.code}")
    except OSError as error:
        fail(f"failed to query crates.io for '{name}': {error}")

    versions = {entry.get("num") for entry in payload.get("versions", [])}
    if version in versions:
        fail(f"crate '{name}' version {version} already exists on crates.io")

    print(f"Crate '{name}' version {version} is not published on crates.io.")


if __name__ == "__main__":
    main()
