#!/usr/bin/env python3
"""Check the version of the engine: its pin, and that it changes with the engine.

1. pyproject.toml must pin jenkinsfilelint-engine to the version in
   crates/jenkinsfilelint/Cargo.toml.
2. The engine must not have changed since the last release while its
   version stayed the same.

A release uploads jenkinsfilelint-engine only if its version is not on PyPI
yet. So when the engine's code changes and its version stays the same, the
release is green, nothing new is uploaded, and `pip install jenkinsfilelint`
keeps installing the old engine.

For the second check, this script compares the files that go into the engine
with the last release tag. If they differ, the version in Cargo.toml must
differ too.

Usage:
    python3 crates/jenkinsfilelint/scripts/check_engine_version.py [--base TAG]

Without --base, the base is the last `v*` tag before HEAD. It needs the
repository's history and tags (`fetch-depth: 0` in GitHub Actions); without
them it fails rather than pass without comparing anything.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path
from typing import List, Optional

CRATE = "crates/jenkinsfilelint"
CARGO_TOML = f"{CRATE}/Cargo.toml"
PYPROJECT_TOML = "pyproject.toml"

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
    """The version in the text of crates/jenkinsfilelint/Cargo.toml."""
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    if not match:
        raise ValueError(f"no version in {CARGO_TOML}")
    return match.group(1)


def pinned_version(text: str) -> str:
    """The jenkinsfilelint-engine version that the text of pyproject.toml pins."""
    match = re.search(r"jenkinsfilelint-engine==([^;\s\"]+)", text)
    if not match:
        raise ValueError(f"no jenkinsfilelint-engine== pin in {PYPROJECT_TOML}")
    return match.group(1)


def check_pin(repo: Path) -> List[str]:
    """Like check(): the pin in pyproject.toml against Cargo.toml."""
    version = cargo_version((repo / CARGO_TOML).read_text(encoding="utf-8"))
    pin = pinned_version((repo / PYPROJECT_TOML).read_text(encoding="utf-8"))
    if pin != version:
        return [
            f"error: {PYPROJECT_TOML} pins jenkinsfilelint-engine=={pin}, "
            f"but {CARGO_TOML} says {version}."
        ]
    return [f"{PYPROJECT_TOML} pins jenkinsfilelint-engine=={pin}, as in {CARGO_TOML}."]


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
        shallow = git(repo, "rev-parse", "--is-shallow-repository").stdout.strip()
        why = (
            "this is a shallow clone" if shallow == "true" else "no v* tag is reachable"
        )
        return [
            f"error: no release tag to compare the engine with ({why}).",
            "Fetch the history and tags (fetch-depth: 0 in GitHub Actions), "
            "or name the release to compare with: --base TAG.",
        ]

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
    failed = False
    for lines in (check_pin(repo), check(repo, args.base)):
        error = lines[0].startswith("error:")
        failed |= error
        print("\n".join(lines), file=sys.stderr if error else sys.stdout)
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
