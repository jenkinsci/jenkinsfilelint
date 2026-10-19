#!/usr/bin/env python3
"""Command-line interface for jenkinsfilelint."""

import sys
import argparse
import io
from pathlib import Path
from typing import List, Optional, Set
from .linter import JenkinsfileLinter, NOT_DECLARATIVE_MESSAGE
from .local import LocalJenkins, handle_server_command
from .offline import OfflineEngineError, find_engine, lint_files
from . import __version__


def should_skip_file(filepath: str, skip_patterns: Optional[List[str]]) -> bool:
    """Check if a file should be skipped based on the provided patterns.

    Args:
        filepath: Path to the file to check
        skip_patterns: List of glob patterns to match against, or None

    Returns:
        True if the file should be skipped, False otherwise
    """
    if not skip_patterns:
        return False

    path = Path(filepath)
    for pattern in skip_patterns:
        if path.match(pattern):
            return True
    return False


def should_include_file(filepath: str, include_patterns: Optional[List[str]]) -> bool:
    """Check if a file should be included based on the provided patterns.

    When include patterns are specified, only files matching at least one pattern
    will be validated. Files that do not match any pattern are skipped.

    Args:
        filepath: Path to the file to check
        include_patterns: List of glob patterns to match against, or None/empty

    Returns:
        True if the file should be included, False otherwise
    """
    if not include_patterns:
        return True

    path = Path(filepath)
    for pattern in include_patterns:
        if path.match(pattern):
            return True
    return False


def _skip_reason(jenkinsfile: str, args: argparse.Namespace) -> Optional[str]:
    """Why --include/--skip leave a file out, or None to check it."""
    # Check if file should be included (whitelist)
    if not should_include_file(jenkinsfile, args.include):
        return "does not match include pattern"
    # Check if file should be skipped (blacklist)
    if should_skip_file(jenkinsfile, args.skip):
        return "matches skip pattern"
    return None


def _validate_files(linter: JenkinsfileLinter, args: argparse.Namespace) -> None:
    """Shared validation loop used by both remote and local modes."""
    all_valid = True
    printed_messages: Set[str] = set()

    for jenkinsfile in args.jenkinsfile:
        reason = _skip_reason(jenkinsfile, args)
        if reason:
            if args.verbose:
                print(f"⊘ {jenkinsfile}: Skipped ({reason})")
            continue

        if args.verbose:
            print(f"Validating {jenkinsfile}...")

        is_valid, message = linter.validate(jenkinsfile)

        if is_valid and message == NOT_DECLARATIVE_MESSAGE:
            # The Groovy parsed, but there is no Declarative syntax to check.
            if args.verbose or len(args.jenkinsfile) > 1:
                print(f"⊘ {jenkinsfile}: Skipped (not a Declarative Pipeline)")
        elif is_valid:
            # Show valid status for multiple files or when verbose
            if args.verbose or len(args.jenkinsfile) > 1:
                print(f"✓ {jenkinsfile}: Valid")
            if args.verbose and message:
                print(f"  {message}")
        else:
            # Deduplicate error messages (e.g., credentials errors)
            if message not in printed_messages:
                print(f"  {message}", file=sys.stderr)
                printed_messages.add(message)
            all_valid = False

    sys.exit(0 if all_valid else 1)


def _run_remote_validation(args: argparse.Namespace) -> None:
    """Validate files against a remote Jenkins server."""
    linter = JenkinsfileLinter(
        jenkins_url=args.jenkins_url,
        username=args.username,
        token=args.token,
    )
    _validate_files(linter, args)


def _run_local_validation(args: argparse.Namespace) -> None:
    """Validate files against a local Docker-based Jenkins server."""
    try:
        local = LocalJenkins()
        url = local.ensure_running()
        if args.verbose:
            print(f"✓ Local Jenkins ready at {url}")
        linter = JenkinsfileLinter(jenkins_url=url)
    except RuntimeError as exc:
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)

    _validate_files(linter, args)

    # In --local mode, the container stays running so subsequent
    # invocations are fast.  Use ``jenkinsfilelint server stop`` to
    # shut it down explicitly.


def _run_offline_validation(args: argparse.Namespace) -> None:
    """Validate files with the offline engine, without a Jenkins server."""
    selected = [f for f in args.jenkinsfile if _skip_reason(f, args) is None]
    try:
        engine = find_engine() if selected else None
        if engine and args.verbose:
            print(f"✓ Offline engine: {engine}")
        results = lint_files(selected, engine) if selected else {}
    except OfflineEngineError as exc:
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)

    all_valid = True
    for jenkinsfile in args.jenkinsfile:
        reason = _skip_reason(jenkinsfile, args)
        if reason:
            if args.verbose:
                print(f"⊘ {jenkinsfile}: Skipped ({reason})")
            continue

        if args.verbose:
            print(f"Validating {jenkinsfile}...")

        result = results[jenkinsfile]
        if result.lines:
            sys.stdout.flush()  # keep the output in file order when piped
        for line in result.lines:
            print(line, file=sys.stderr)

        if not result.is_valid:
            all_valid = False
        elif not result.declarative:
            # The Groovy parsed, but there is no Declarative syntax to check.
            if args.verbose or len(args.jenkinsfile) > 1:
                print(f"⊘ {jenkinsfile}: Skipped (not a Declarative Pipeline)")
        elif args.verbose or len(args.jenkinsfile) > 1:
            print(f"✓ {jenkinsfile}: Valid")

    sys.exit(0 if all_valid else 1)


def main():
    """Main entry point for jenkinsfilelint.

    Dispatches ``server`` subcommands (start|stop|status|restart) or runs the
    linting CLI (with optional ``--local`` or ``--offline`` mode).
    """
    if len(sys.argv) > 1 and sys.argv[1] == "server":
        handle_server_command(sys.argv[2:])
        return

    # Ensure stdout and stderr use UTF-8 encoding on Windows
    # Only wrap if not already wrapped to avoid issues in tests
    if sys.platform == "win32":
        if (
            not isinstance(sys.stdout, io.TextIOWrapper)
            or sys.stdout.encoding.lower() != "utf-8"
        ):
            sys.stdout = io.TextIOWrapper(
                sys.stdout.buffer, encoding="utf-8", errors="replace"
            )
        if (
            not isinstance(sys.stderr, io.TextIOWrapper)
            or sys.stderr.encoding.lower() != "utf-8"
        ):
            sys.stderr = io.TextIOWrapper(
                sys.stderr.buffer, encoding="utf-8", errors="replace"
            )

    parser = argparse.ArgumentParser(
        description="Validate Jenkinsfiles using Jenkins API, or offline with --offline"
    )
    parser.add_argument(
        "-V",
        "--version",
        action="version",
        version=f"%(prog)s {__version__}",
    )
    parser.add_argument(
        "jenkinsfile",
        nargs="+",
        help="Path to Jenkinsfile(s) to validate",
    )
    parser.add_argument(
        "--jenkins-url",
        help="Jenkins server URL (can also be set via JENKINS_URL env var)",
    )
    parser.add_argument(
        "--username",
        help="Jenkins username (can also be set via JENKINS_USER env var)",
    )
    parser.add_argument(
        "--token",
        help="Jenkins API token (can also be set via JENKINS_TOKEN env var)",
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--local",
        action="store_true",
        help="Validate using a local Docker-based Jenkins server (no remote Jenkins required). "
        "The container is started automatically on first use and stays running for fast "
        "subsequent invocations. Use 'jenkinsfilelint server stop' to shut it down.",
    )
    mode.add_argument(
        "--offline",
        action="store_true",
        help="Check Groovy syntax and Declarative Pipeline structure without Jenkins, "
        "Docker or a JVM. Cannot check what depends on plugins, such as step names "
        "and parameters; use a Jenkins server or --local for that.",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Verbose output",
    )
    parser.add_argument(
        "--skip",
        action="append",
        default=[],
        metavar="PATTERN",
        help="Glob pattern(s) for files to skip. Can be used multiple times. "
        "Example: --skip '*/src/*.groovy' --skip 'vars/*.groovy'",
    )
    parser.add_argument(
        "--include",
        action="append",
        default=[],
        metavar="PATTERN",
        help="Glob pattern(s) for files to include. When specified, only files "
        "matching at least one pattern are validated. Can be used multiple times. "
        "Example: --include 'Jenkinsfile*' --include 'pipelines/*.groovy'",
    )

    args = parser.parse_args()

    if args.offline:
        for option in ("jenkins_url", "username", "token"):
            if getattr(args, option):
                flag = "--" + option.replace("_", "-")
                parser.error(f"argument --offline: not allowed with argument {flag}")
        _run_offline_validation(args)
    elif args.local:
        _run_local_validation(args)
    else:
        _run_remote_validation(args)


if __name__ == "__main__":
    main()
