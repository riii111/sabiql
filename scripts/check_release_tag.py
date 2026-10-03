#!/usr/bin/env python3
"""Require a stable tag matching the workspace before any release publication."""

import re
import sys
import tomllib
from pathlib import Path


def validate_release_tag(tag: str, version: str) -> None:
    stable_version = r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)"
    if re.fullmatch(stable_version, version) is None:
        raise ValueError("Workspace version must be stable for publication")
    if tag != f"v{version}":
        raise ValueError(
            f"Release tag must be v{version}; use Release Build Validation for pre-release checks"
        )


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: check_release_tag.py TAG", file=sys.stderr)
        return 1
    manifest = Path(__file__).resolve().parent.parent / "Cargo.toml"
    with manifest.open("rb") as source:
        version = tomllib.load(source)["workspace"]["package"]["version"]
    try:
        validate_release_tag(sys.argv[1], version)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
