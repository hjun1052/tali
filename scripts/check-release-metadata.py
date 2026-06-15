#!/usr/bin/env python3
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    cargo = tomllib.loads(read("Cargo.toml"))
    package = cargo["package"]
    version = package["version"]
    rust_version = package["rust-version"]

    changelog = read("CHANGELOG.md")
    readme = read("README.md")
    release = read("RELEASE.md")

    if f"## {version}" not in changelog:
        fail(f"CHANGELOG.md is missing a '## {version}' section")

    tag = f"v{version}"
    for path, content in [("README.md", readme), ("RELEASE.md", release)]:
        if tag not in content:
            fail(f"{path} is missing release tag example {tag}")
        if f"Rust version is {rust_version}" not in content and path == "README.md":
            fail(f"{path} is missing Rust version {rust_version}")

    if package.get("license") != "MIT":
        fail("Cargo.toml package.license must remain MIT for the current release policy")
    if package.get("readme") != "README.md":
        fail("Cargo.toml package.readme must be README.md")

    print("Release metadata passed.")


if __name__ == "__main__":
    main()
