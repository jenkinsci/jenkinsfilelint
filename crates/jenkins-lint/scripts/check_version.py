#!/usr/bin/env python3
"""Check that the versions of jenkins-lint agree.

1. crates/jenkins-lint/Cargo.toml has the version of the jenkins-lint
   package on PyPI.
2. The root pyproject.toml, which pre-commit installs, has the same version
   and pins jenkins-lint to it. Otherwise a hook would install another
   jenkins-lint than the one of its tag.
3. With --tag, the release tag is v and that version.

Usage:
    python3 crates/jenkins-lint/scripts/check_version.py [--tag v2.0.0]
"""

import argparse
import re
import sys
from pathlib import Path
from typing import List, Optional

CARGO_TOML = "crates/jenkins-lint/Cargo.toml"
PYPROJECT_TOML = "pyproject.toml"


def cargo_version(text: str) -> str:
    """The version in the text of Cargo.toml."""
    match = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    if not match:
        raise ValueError(f"no version in {CARGO_TOML}")
    return match.group(1)


def shim_versions(text: str) -> "tuple[str, str]":
    """The version of the root pyproject.toml and its jenkins-lint pin."""
    version = re.search(r'^version = "([^"]+)"', text, re.MULTILINE)
    pin = re.search(r'"jenkins-lint==([^"]+)"', text)
    if not version or not pin:
        raise ValueError(f"no version or no jenkins-lint== pin in {PYPROJECT_TOML}")
    return version.group(1), pin.group(1)


def check(repo: Path, tag: Optional[str] = None) -> List[str]:
    """The errors, each a line, or an empty list when all agree."""
    version = cargo_version((repo / CARGO_TOML).read_text(encoding="utf-8"))
    shim, pin = shim_versions((repo / PYPROJECT_TOML).read_text(encoding="utf-8"))
    errors = []
    if shim != version:
        errors.append(
            f"{PYPROJECT_TOML} has version {shim}, but {CARGO_TOML} has {version}."
        )
    if pin != version:
        errors.append(
            f"{PYPROJECT_TOML} pins jenkins-lint=={pin}, but {CARGO_TOML} has {version}."
        )
    if tag is not None and tag != f"v{version}":
        errors.append(
            f"The tag is {tag}, but {CARGO_TOML} has {version}: tag v{version}."
        )
    return errors


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tag", help="the release tag, which must be v<version>")
    args = parser.parse_args(argv)
    repo = Path(__file__).resolve().parents[3]
    errors = check(repo, args.tag)
    for line in errors:
        print(f"error: {line}", file=sys.stderr)
    if not errors:
        version = cargo_version((repo / CARGO_TOML).read_text(encoding="utf-8"))
        print(f"jenkins-lint {version}: {CARGO_TOML} and {PYPROJECT_TOML} agree.")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
