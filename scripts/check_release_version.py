"""Validate a release tag against the committed workspace and lockfile."""

import argparse
from pathlib import Path
import re
import tomllib


def release_version(tag: str) -> str:
    match = re.fullmatch(r"(?:v|bindings/go/v|sdk/go/v)?((?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*))", tag)
    if match is None:
        raise ValueError(f"not a stable release tag: {tag!r}")
    return match[1]


def check_version(root: Path, tag: str) -> str:
    version = release_version(tag)
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    if manifest["workspace"]["package"]["version"] != version:
        raise ValueError("release tag does not match the committed workspace version")
    packages = tomllib.loads((root / "Cargo.lock").read_text())["package"]
    local = [package for package in packages if "source" not in package]
    if not local or any(package["version"] != version for package in local):
        raise ValueError("release tag does not match every local lockfile package")
    return version


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    try:
        print(check_version(args.root, args.tag))
    except (ValueError, KeyError) as error:
        parser.exit(1, f"{error}\n")


if __name__ == "__main__":
    main()
