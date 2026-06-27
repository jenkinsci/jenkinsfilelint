#!/usr/bin/env python3
"""Command-line interface for jenkinsfilelint."""

import sys
import argparse
import io
from pathlib import Path
from typing import List, Optional
from .linter import JenkinsfileLinter
from .runners import registry
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


def build_parser() -> argparse.ArgumentParser:
    """Build and return the argument parser."""
    parser = argparse.ArgumentParser(
        description="Validate Jenkinsfiles using Jenkins API or Jenkinsfile Runner"
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
        "--runner",
        choices=registry.names(),
        default="jenkins",
        help=f"Validation backend. Available runners:\n\n{registry.help_text()}",
    )
    return parser


def main():
    """Main entry point for the CLI."""
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

    parser = build_parser()
    args = parser.parse_args()

    # Create linter instance
    linter = JenkinsfileLinter(
        jenkins_url=args.jenkins_url,
        username=args.username,
        token=args.token,
        runner=args.runner,
    )

    # Validate all provided files
    all_valid = True
    printed_messages = set()  # Track messages already printed for deduplication

    for jenkinsfile in args.jenkinsfile:
        # Check if file should be included (whitelist)
        if not should_include_file(jenkinsfile, args.include):
            if args.verbose:
                print(f"⊘ {jenkinsfile}: Skipped (does not match include pattern)")
            continue

        # Check if file should be skipped (blacklist)
        if should_skip_file(jenkinsfile, args.skip):
            if args.verbose:
                print(f"⊘ {jenkinsfile}: Skipped (matches skip pattern)")
            continue

        if args.verbose:
            print(f"Validating {jenkinsfile}...")

        is_valid, message = linter.validate(jenkinsfile)

        if is_valid:
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

    # Exit with appropriate code
    sys.exit(0 if all_valid else 1)


if __name__ == "__main__":
    main()
