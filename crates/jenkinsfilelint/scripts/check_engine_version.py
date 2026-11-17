#!/usr/bin/env python3
"""Fail when the engine changed since the last release but its version did not.

A release uploads jenkinsfilelint-engine only if its version is not on PyPI
yet. So when the engine's code changes and its version stays the same, the
release is green, nothing new is uploaded, and `pip install jenkinsfilelint`
keeps installing the old engine.

This script compares the files that go into the engine with the last release
tag. If they differ, the version in Cargo.toml must differ too.

Usage:
    python3 crates/jenkinsfilelint/scripts/check_engine_version.py [--base TAG]

Without --base, the base is the last `v*` tag before HEAD. It needs the
repository's tags (`git fetch --tags`; `fetch-depth: 0` in GitHub Actions).
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import List, Optional

CRATE = "crates/jenkinsfilelint"
CARGO_TOML = f"{CRATE}/Cargo.toml"

# What is built into the wheels. Tests, scripts and documents are not.
ENGINE_PATHS = [
    f"{CRATE}/src",
    f"{CRATE}/python",
    f"{CRATE}/Cargo.toml",
    f"{CRATE}/Cargo.lock",
    f"{CRATE}/pyproject.toml",
]


def git(repo: Path, *args: str) -> "subprocess.CompletedProcess[str]":
    return subprocess.run(
        ["git", *args],
        cwd=repo,
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=False,
    )


def cargo_version(text: str) -> str:
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    if not match:
        raise ValueError(f"no version in {CARGO_TOML}")
    return match.group(1)


def last_release_tag(repo: Path) -> Optional[str]:
    """The last `v*` tag before HEAD, or None when there is none.

    Before HEAD, not at HEAD: when HEAD is the commit being released, the
    engine has to be compared with the release before it.
    """
    for commit in ("HEAD^", "HEAD"):
        described = git(
            repo, "describe", "--tags", "--abbrev=0", "--match", "v*", commit
        )
        if described.returncode == 0:
            return described.stdout.strip()
    return None


def check(repo: Path, base: Optional[str] = None) -> List[str]:
    """Return what to tell the user: one line when all is well, or the
    error, which starts with "error:"."""
    base = base or last_release_tag(repo)
    if base is None:
        return ["No release tag found: nothing to compare the engine with."]

    released = git(repo, "show", f"{base}:{CARGO_TOML}")
    if released.returncode != 0:
        return [f"{base} has no engine: nothing to compare with."]
    released_version = cargo_version(released.stdout)
    version = cargo_version((repo / CARGO_TOML).read_text(encoding="utf-8"))

    # Compares the working tree, so that it also works before committing.
    diff = git(repo, "diff", "--name-only", base, "--", *ENGINE_PATHS)
    if diff.returncode != 0:
        raise RuntimeError(diff.stderr.strip())
    changed = diff.stdout.split()
    untracked = git(
        repo, "ls-files", "--others", "--exclude-standard", "--", *ENGINE_PATHS
    )
    changed += untracked.stdout.split()

    if not changed:
        return [f"The engine is the same as in {base} ({version})."]
    if version != released_version:
        return [f"The engine changed since {base}: {released_version} -> {version}."]
    return [
        f"error: the engine changed since {base}, but its version is still {version}.",
        "Changed: " + ", ".join(changed),
        f"A release would keep jenkinsfilelint-engine {version} from PyPI, without these changes.",
        f'Bump "version" in {CARGO_TOML} and the jenkinsfilelint-engine pin in pyproject.toml.',
    ]


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--base",
        help="tag or commit to compare with (default: the last v* tag before HEAD)",
    )
    args = parser.parse_args(argv)

    repo = Path(__file__).resolve().parents[3]
    lines = check(repo, args.base)
    failed = lines[0].startswith("error:")
    print("\n".join(lines), file=sys.stderr if failed else sys.stdout)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
