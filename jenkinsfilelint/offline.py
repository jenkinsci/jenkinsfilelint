#!/usr/bin/env python3
"""Offline mode: check Jenkinsfiles with the jenkinsfilelint-engine binary.

The engine is a Rust program shipped as prebuilt wheels in the
``jenkinsfilelint-engine`` package, which this package depends on. It checks
Groovy syntax and Declarative Pipeline structure without a Jenkins controller.
"""

import json
import os
import subprocess
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

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


@dataclass
class Finding:
    """One finding of ``--audit``."""

    rule: str
    severity: str
    confidence: str
    message: str
    line: int
    column: int
    help: str
    # The disposition of its fix, ``safe`` or ``unsafe``, or None.
    fix: Optional[str] = None

    def render(self, path: str, marked: bool = False) -> str:
        """``file:line:col: severity: rule: message``, and where to read more.
        A *marked* finding, which the fix mode fixes, starts with ``[*]``."""
        mark = "[*] " if marked else ""
        return (
            f"{path}:{self.line}:{self.column}: {self.severity}: {self.rule}: "
            f"{mark}{self.message}\n  help: {self.help}"
        )

    def render_fixed(self, path: str) -> str:
        """``file:line:col: fixed: rule: message``, for what ``--fix`` fixed."""
        return f"{path}:{self.line}:{self.column}: fixed: {self.rule}: {self.message}"


@dataclass
class AuditResult:
    """The findings in one file, or why it could not be audited."""

    findings: List[Finding]
    # Syntax errors and read errors, as ``file:line:col: error: message``.
    errors: List[str]
    # What ``--fix`` fixed, where it was before.
    fixed: List[Finding] = field(default_factory=list)


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
    reports = _reports(engine, ["--format", "json", "--", *existing], len(existing))
    # The engine reports files in the order given. Match by position, not by
    # the "file" field, which is lossy for file names that are not UTF-8.
    for path, report in zip(existing, reports):
        results[path] = _parse(engine, report, lambda r, path=path: _result(path, r))

    return results


def _run(engine: str, args: List[str]) -> "subprocess.CompletedProcess[str]":
    try:
        return subprocess.run(
            [engine, *args],
            capture_output=True,
            text=True,
            encoding="utf-8",
            errors="replace",
            check=False,
        )
    except OSError as exc:
        raise OfflineEngineError(f"Could not run {engine}: {exc}") from exc


def _reports(engine: str, args: List[str], count: int) -> list:
    """Run the engine and return its JSON array of per-file reports."""
    proc = _run(engine, args)
    try:
        reports = json.loads(proc.stdout)
    except ValueError:
        detail = (proc.stderr or proc.stdout).strip() or f"exit code {proc.returncode}"
        raise OfflineEngineError(f"Unexpected output from {engine}: {detail}") from None

    if not isinstance(reports, list) or len(reports) != count:
        raise OfflineEngineError(
            f"{engine} returned {len(reports) if isinstance(reports, list) else 'no'} "
            f"result(s) for {count} file(s)"
        )
    return reports


def _parse(engine: str, report: dict, read):
    """Read one report with *read*, or fail on a report of another shape."""
    try:
        return read(report)
    except (KeyError, TypeError, AttributeError):
        raise OfflineEngineError(
            f"Unexpected output from {engine}: {json.dumps(report)[:200]}"
        ) from None


def _audit_args(
    persona: str,
    min_severity: str,
    min_confidence: str,
    ignore: Optional[List[str]] = None,
    fix: Optional[str] = None,
) -> List[str]:
    args = [
        "--audit",
        "--persona",
        persona,
        "--min-severity",
        min_severity,
        "--min-confidence",
        min_confidence,
    ]
    for rules in ignore or []:
        args += ["--ignore", rules]
    if fix:
        args.append(f"--fix={fix}")
    return args


def audit_files(
    paths: List[str],
    engine: Optional[str] = None,
    persona: str = "regular",
    min_severity: str = "low",
    min_confidence: str = "low",
    ignore: Optional[List[str]] = None,
    fix: Optional[str] = None,
) -> Dict[str, AuditResult]:
    """Run the security checks on files with one run of the engine. With
    *fix*, ``safe``, ``unsafe-only`` or ``all``, the engine first rewrites
    the files with the fixes of that disposition.

    Raises:
        OfflineEngineError: if the engine is missing or fails.
    """
    engine = engine or find_engine()
    options = _audit_args(persona, min_severity, min_confidence, ignore, fix)
    reports = _reports(engine, [*options, "--format", "json", "--", *paths], len(paths))
    return {
        path: _parse(engine, report, lambda r, path=path: _audit_result(path, r))
        for path, report in zip(paths, reports)
    }


def audit_report(
    paths: List[str],
    fmt: str,
    engine: Optional[str] = None,
    persona: str = "regular",
    min_severity: str = "low",
    min_confidence: str = "low",
    ignore: Optional[List[str]] = None,
    fix: Optional[str] = None,
    diff: bool = False,
) -> Tuple[str, str, int]:
    """Run the security checks and return the engine's report as is, in
    ``json`` or ``sarif``: its output, its error output and its exit code.
    With *diff*, the report is the diff of what the fixes would change,
    which is empty when there is none.

    Raises:
        OfflineEngineError: if the engine is missing or fails.
    """
    engine = engine or find_engine()
    options = _audit_args(persona, min_severity, min_confidence, ignore, fix)
    extra = ["--diff"] if diff else ["--format", fmt]
    proc = _run(engine, [*options, *extra, "--", *paths])
    if proc.returncode not in (0, 1) or not (diff or proc.stdout.strip()):
        detail = proc.stderr.strip() or f"exit code {proc.returncode}"
        raise OfflineEngineError(f"Unexpected output from {engine}: {detail}")
    return proc.stdout, proc.stderr, proc.returncode


def _result(path: str, report: dict) -> OfflineResult:
    """Turn the engine's report on one file into a result.

    Raises:
        KeyError, TypeError, AttributeError: if the report is not shaped as
        the engine documents it.
    """
    if "error" in report:
        return OfflineResult(
            False, False, [f"Error reading file {path}: {report['error']}"]
        )
    diagnostics = report["diagnostics"]
    declarative = report["declarative"]
    if not isinstance(diagnostics, list) or not isinstance(declarative, bool):
        raise TypeError("diagnostics must be a list and declarative a boolean")
    return OfflineResult(
        is_valid=not any(d["severity"] == "error" for d in diagnostics),
        declarative=declarative,
        lines=[
            f"{path}:{d['line']}:{d['column']}: {d['severity']}: {d['message']}"
            for d in diagnostics
        ],
    )


def _audit_result(path: str, report: dict) -> AuditResult:
    """Turn the engine's audit of one file into a result.

    Raises:
        KeyError, TypeError, AttributeError: if the report is not shaped as
        the engine documents it.
    """
    if "error" in report:
        return AuditResult([], [f"Error reading file {path}: {report['error']}"])

    def finding(f: dict) -> Finding:
        return Finding(
            rule=f["rule"],
            severity=f["severity"],
            confidence=f["confidence"],
            message=f["message"],
            line=f["line"],
            column=f["column"],
            help=f["help"],
            fix=(f.get("fix") or {}).get("disposition"),
        )

    errors = [
        f"{path}:{d['line']}:{d['column']}: error: {d['message']}"
        for d in report["diagnostics"]
    ]
    return AuditResult(
        [finding(f) for f in report["findings"]],
        errors,
        [finding(f) for f in report.get("fixed", [])],
    )
