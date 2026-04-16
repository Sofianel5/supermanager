#!/usr/bin/env python3

from __future__ import annotations

import pathlib
import subprocess
import sys
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
CLI_MANIFEST = ROOT / "crates" / "supermanager-cli" / "Cargo.toml"
LOCKFILE = ROOT / "Cargo.lock"


def read_cli_version() -> str:
    with CLI_MANIFEST.open("rb") as handle:
        manifest = tomllib.load(handle)
    return manifest["package"]["version"]


def read_lock_version() -> str:
    with LOCKFILE.open("rb") as handle:
        lockfile = tomllib.load(handle)

    for package in lockfile.get("package", []):
        if package.get("name") == "supermanager":
            return package["version"]

    raise RuntimeError("Cargo.lock does not contain a package entry for supermanager")


def main() -> int:
    cli_version = read_cli_version()
    lock_version = read_lock_version()

    if cli_version != lock_version:
        print(
            "::error::crates/supermanager-cli/Cargo.toml is "
            f"{cli_version}, but Cargo.lock still records supermanager {lock_version}. "
            "Refresh Cargo.lock before merging or pushing another CLI release.",
            file=sys.stderr,
        )
        return 1

    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        cwd=ROOT,
        check=False,
        stdout=subprocess.DEVNULL,
    )
    if result.returncode != 0:
        print(
            "::error::Cargo.lock is not in sync with the workspace. "
            "Refresh it locally and commit the updated lockfile before releasing the CLI.",
            file=sys.stderr,
        )
        return result.returncode

    print(f"CLI release inputs are locked and consistent at version {cli_version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
