#!/usr/bin/env python3
"""Command-line interface for jenkinsfilelint."""

import sys
import argparse
import io
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import List, Optional, Set, Tuple
from .linter import JenkinsfileLinter, NOT_DECLARATIVE_MESSAGE
from .local import LocalJenkins, handle_server_command
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

    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        try:
            pending: List[Tuple[str, Optional[str], Optional[Future]]] = []
            for jenkinsfile in args.jenkinsfile:
                reason = _skip_reason(jenkinsfile, args)
                future = (
                    None if reason else executor.submit(linter.validate, jenkinsfile)
                )
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
        except BaseException:
            # On Ctrl-C (or a crash) don't send the files still waiting in the queue.
            executor.shutdown(wait=False, cancel_futures=True)
            raise

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


def main():
    """Main entry point for jenkinsfilelint.

    Dispatches ``server`` subcommands (start|stop|status|restart) or runs the
    linting CLI (with optional ``--local`` mode).
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
        description="Validate Jenkinsfiles using Jenkins API"
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
    parser.add_argument(
        "--local",
        action="store_true",
        help="Validate using a local Docker-based Jenkins server (no remote Jenkins required). "
        "The container is started automatically on first use and stays running for fast "
        "subsequent invocations. Use 'jenkinsfilelint server stop' to shut it down.",
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
        "use 1 to validate one file at a time.",
    )

    args = parser.parse_args()

    if args.local:
        _run_local_validation(args)
    else:
        _run_remote_validation(args)


if __name__ == "__main__":
    main()
