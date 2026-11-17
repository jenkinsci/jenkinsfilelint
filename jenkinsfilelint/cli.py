#!/usr/bin/env python3
"""Command-line interface for jenkinsfilelint."""

import os
import sys
import argparse
import io
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import List, Optional, Set, Tuple
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


DEFAULT_JOBS = 8


def _positive_int(value: str) -> int:
    """argparse type for ``--jobs``: an integer >= 1."""
    try:
        number = int(value)
    except ValueError:
        raise argparse.ArgumentTypeError(f"invalid int value: '{value}'")
    if number < 1:
        raise argparse.ArgumentTypeError("must be at least 1")
    return number


def _skip_reason(jenkinsfile: str, args: argparse.Namespace) -> Optional[str]:
    """Return why *jenkinsfile* is filtered out, or None to validate it."""
    # Include patterns act as a whitelist, skip patterns as a blacklist.
    if not should_include_file(jenkinsfile, args.include):
        return "does not match include pattern"
    if should_skip_file(jenkinsfile, args.skip):
        return "matches skip pattern"
    return None


def _validate_files(linter: JenkinsfileLinter, args: argparse.Namespace) -> None:
    """Shared validation loop used by both remote and local modes.

    Up to ``args.jobs`` files are validated at once, but results are printed
    in the order the files were given, so the output is the same as a serial
    run.
    """
    all_valid = True
    printed_messages: Set[str] = set()

    executor = ThreadPoolExecutor(max_workers=args.jobs)
    try:
        pending: List[Tuple[str, Optional[str], Optional[Future]]] = []
        for jenkinsfile in args.jenkinsfile:
            reason = _skip_reason(jenkinsfile, args)
            future = None if reason else executor.submit(linter.validate, jenkinsfile)
            pending.append((jenkinsfile, reason, future))

        for jenkinsfile, reason, future in pending:
            if future is None:
                if args.verbose:
                    print(f"⊘ {jenkinsfile}: Skipped ({reason})")
                continue

            if args.verbose:
                print(f"Validating {jenkinsfile}...")

            is_valid, message = future.result()
            all_valid = (
                _report(jenkinsfile, is_valid, message, args, printed_messages)
                and all_valid
            )
    except KeyboardInterrupt:
        # Drop the queued files and exit now. Python would otherwise wait for
        # the requests already sent, up to their 30s timeout if Jenkins hangs.
        executor.shutdown(wait=False, cancel_futures=True)
        sys.stdout.flush()
        print("✗ Interrupted", file=sys.stderr)
        sys.stderr.flush()
        os._exit(130)
    except BaseException:
        # On a crash, don't send the files still waiting in the queue.
        executor.shutdown(wait=False, cancel_futures=True)
        raise
    executor.shutdown()

    sys.exit(0 if all_valid else 1)


def _report(
    jenkinsfile: str,
    is_valid: bool,
    message: str,
    args: argparse.Namespace,
    printed_messages: Set[str],
) -> bool:
    """Print the result for one file and return whether it passed."""
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
    return is_valid


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


# Settings that mean "check with this Jenkins server".
JENKINS_ENV_VARS = ("JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN")

NO_JENKINS_NOTE = (
    "ℹ No Jenkins server configured, so checking offline: step names, step "
    "parameters and other plugin-dependent checks are skipped. Set JENKINS_URL "
    "to check with Jenkins, or pass --offline to hide this note."
)

NO_JENKINS_NO_ENGINE = (
    "✗ No Jenkins server configured (set JENKINS_URL or pass --jenkins-url), "
    "and offline checking is not available here."
)


def _jenkins_configured(args: argparse.Namespace) -> bool:
    """Whether a Jenkins server was set up, on the command line or in the
    environment. Credentials alone count: they mean a server was intended,
    and without its URL that is an error to report, not a reason to check
    offline instead."""
    return any((args.jenkins_url, args.username, args.token)) or any(
        os.environ.get(name) for name in JENKINS_ENV_VARS
    )


def _run_offline_validation(args: argparse.Namespace, automatic: bool = False) -> None:
    """Validate files with the offline engine, without a Jenkins server.

    *automatic* is true when no Jenkins server is configured and offline
    checking was chosen for the user rather than asked for with --offline.
    """
    selected = [f for f in args.jenkinsfile if _skip_reason(f, args) is None]
    try:
        engine = find_engine() if selected else None
        if engine and args.verbose:
            print(f"✓ Offline engine: {engine}")
        results = lint_files(selected, engine) if selected else {}
    except OfflineEngineError as exc:
        if automatic:
            print(NO_JENKINS_NO_ENGINE, file=sys.stderr)
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)

    if automatic and selected:
        sys.stdout.flush()
        print(NO_JENKINS_NOTE, file=sys.stderr)

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
    linting CLI. Files are checked with the configured Jenkins server, with a
    local Docker one (``--local``), or offline (``--offline``, and whenever no
    Jenkins server is configured).
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
    # A file name that is not valid UTF-8 reaches Python as lone surrogates,
    # which a strict stream cannot print. Show them escaped instead.
    for stream in (sys.stdout, sys.stderr):
        if getattr(stream, "errors", None) == "strict" and hasattr(
            stream, "reconfigure"
        ):
            stream.reconfigure(errors="backslashreplace")

    parser = argparse.ArgumentParser(
        description="Validate Jenkinsfiles with a Jenkins server, or offline when "
        "none is configured."
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
        help="Jenkins server URL (can also be set via JENKINS_URL env var). "
        "Without a Jenkins server, files are checked offline.",
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
        help="Check offline, even when a Jenkins server is configured: Groovy "
        "syntax and Declarative Pipeline structure, without Jenkins, Docker or a "
        "JVM. Cannot check what depends on plugins, such as step names and "
        "parameters; use a Jenkins server or --local for that. This is also what "
        "happens when no Jenkins server is configured.",
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
    parser.add_argument(
        "-j",
        "--jobs",
        type=_positive_int,
        default=DEFAULT_JOBS,
        metavar="N",
        help="Number of files to validate at the same time (default: "
        f"{DEFAULT_JOBS}). Lower it to go easier on a shared Jenkins; "
        "use 1 to validate one file at a time. Not used when checking offline, "
        "which checks all files in one run of the engine.",
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
    elif _jenkins_configured(args):
        _run_remote_validation(args)
    else:
        _run_offline_validation(args, automatic=True)


if __name__ == "__main__":
    main()
