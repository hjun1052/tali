#!/usr/bin/env python3
import json
import re
import subprocess
import sys

ALLOWED = {
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "LGPL-2.1-or-later",
    "LLVM-exception",
    "MIT",
    "MPL-2.0",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
}

OPERATORS = {"AND", "OR", "WITH"}


def main() -> int:
    metadata = subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        text=True,
    )
    data = json.loads(metadata)
    failures = []

    for package in data["packages"]:
        license_expr = package.get("license")
        license_file = package.get("license_file")
        if license_file and not license_expr:
            continue
        if not license_expr:
            failures.append(f"{package['name']}: missing license expression")
            continue
        tokens = set(re.findall(r"[A-Za-z0-9][A-Za-z0-9.+-]*", license_expr))
        unknown = sorted(token for token in tokens if token not in ALLOWED and token not in OPERATORS)
        if unknown:
            failures.append(
                f"{package['name']}: unapproved license token(s) {', '.join(unknown)} in {license_expr}"
            )

    if failures:
        print("License policy failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("License policy passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
