#!/usr/bin/env python3
"""Offline mode: check Jenkinsfiles with the jenkinsfilelint-engine binary.

The engine is a Rust program shipped as prebuilt wheels in the
``jenkinsfilelint-engine`` package, which this package depends on. It checks
Groovy syntax and Declarative Pipeline structure without a Jenkins controller.
"""

import json
import os
import subprocess
from dataclasses import dataclass
from typing import Dict, List, Optional

# Path of an engine binary to use instead of the installed one.
ENGINE_ENV = "JENKINSFILELINT_ENGINE"

ENGINE_MISSING_MESSAGE = (
    "The offline engine (jenkinsfilelint-engine) is not installed. "
    "No prebuilt wheel exists for this platform; install it from source with "
    "'pip install jenkinsfilelint-engine' (needs a Rust toolchain), "
    "or validate with a Jenkins server (--jenkins-url) or --local."
)


class OfflineEngineError(RuntimeError):
    """The offline engine is missing or did not answer as expected."""


@dataclass
class OfflineResult:
    """The engine's verdict on one file."""

    is_valid: bool
    # A pipeline block was found and checked.
    declarative: bool
    # Diagnostics as ``file:line:col: severity: message``.
    lines: List[str]


def find_engine() -> str:
    """Return the path of the engine binary.

    Raises:
        OfflineEngineError: if the engine is not installed.
    """
    path = os.environ.get(ENGINE_ENV)
    if path:
        return path
    try:
        from jenkinsfilelint_engine import find_bin

        return find_bin()
    except (ImportError, FileNotFoundError):
        raise OfflineEngineError(ENGINE_MISSING_MESSAGE) from None


def lint_files(
    paths: List[str], engine: Optional[str] = None
) -> Dict[str, OfflineResult]:
    """Check files with one run of the engine.

    Args:
        paths: Files to check
        engine: Engine binary; found with :func:`find_engine` when omitted

    Returns:
        A result for every path

    Raises:
        OfflineEngineError: if the engine is missing or fails.
    """
    results: Dict[str, OfflineResult] = {}
    existing = []
    for path in paths:
        if os.path.isfile(path):
            existing.append(path)
        else:
            results[path] = OfflineResult(False, False, [f"File not found: {path}"])
    if not existing:
        return results

    engine = engine or find_engine()
    try:
        proc = subprocess.run(
            [engine, "--format", "json", "--", *existing],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except OSError as exc:
        raise OfflineEngineError(f"Could not run {engine}: {exc}") from exc

    try:
        reports = json.loads(proc.stdout)
    except ValueError:
        detail = (proc.stderr or proc.stdout).strip() or f"exit code {proc.returncode}"
        raise OfflineEngineError(
            f"Unexpected output from {engine}: {detail}"
        ) from None

    for report in reports:
        path = report["file"]
        if "error" in report:
            results[path] = OfflineResult(
                False, False, [f"Error reading file {path}: {report['error']}"]
            )
            continue
        diagnostics = report["diagnostics"]
        results[path] = OfflineResult(
            is_valid=not any(d["severity"] == "error" for d in diagnostics),
            declarative=report["declarative"],
            lines=[
                f"{path}:{d['line']}:{d['column']}: {d['severity']}: {d['message']}"
                for d in diagnostics
            ],
        )

    missing = [path for path in existing if path not in results]
    if missing:
        raise OfflineEngineError(
            f"{engine} returned no result for {', '.join(missing)}"
        )
    return results
