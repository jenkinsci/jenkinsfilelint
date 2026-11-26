#!/usr/bin/env python3
"""Command-line interface for jenkinsfilelint."""

import os
import re
import sys
import argparse
import io
from concurrent.futures import Future, ThreadPoolExecutor
from pathlib import Path
from typing import List, Optional, Set, Tuple
from .linter import JenkinsfileLinter, NOT_DECLARATIVE_MESSAGE
from .local import LocalJenkins, handle_server_command
from .offline import (
    OfflineEngineError,
    audit_files,
    audit_report,
    find_engine,
    lint_files,
)
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


# Copies and notes named after a Jenkinsfile. They can hold a whole pipeline,
# so the content check would pick them.
NOT_PIPELINE_SUFFIXES = (
    ".adoc",
    ".bak",
    ".md",
    ".old",
    ".orig",
    ".rej",
    ".rst",
    ".swp",
    ".tmp",
    ".txt",
)
# Shared library files are not pipelines, so only --audit reads all of them.
LIBRARY_DIRS = {"src", "vars"}
# Directories of tools. Other hidden directories, like .jenkins, are searched.
TOOL_DIRS = {
    ".cache",
    ".git",
    ".gradle",
    ".hg",
    ".idea",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".svn",
    ".terraform",
    ".tox",
    ".venv",
    ".vscode",
    "__pycache__",
    "node_modules",
    "site",
    "target",
    "venv",
}
# A build/ directory next to one of these is build output.
BUILD_FILES = {
    "CMakeLists.txt",
    "build.gradle",
    "build.gradle.kts",
    "package.json",
    "pyproject.toml",
    "setup.py",
}
# Comments and strings, which can hold text that looks like a pipeline.
GROOVY_NON_CODE = re.compile(
    r"""
    //[^\n]*
    | /\*.*?(?:\*/|\Z)
    | \'\'\'(?:\\.|[^\\])*?(?:\'\'\'|\Z)
    | \"\"\"(?:\\.|[^\\])*?(?:\"\"\"|\Z)
    | '(?:\\.|[^'\\\n])*'?
    | "(?:\\.|[^"\\\n])*"?
    | \$/.*?(?:/\$|\Z)
    | (?<=[=(,~:\[!&|?{};\s])/(?![/*\s])(?:\\.|[^/\\\n])*/
    """,
    re.DOTALL | re.VERBOSE,
)
PIPELINE_START = re.compile(r"(?:^|;)\s*(?:pipeline\s*\{|node\s*[({])", re.MULTILINE)


def looks_like_pipeline(path: str) -> bool:
    """Whether a file has a top-level ``pipeline {``, ``node {`` or ``node(``.

    Comments and strings are ignored, and so is anything inside a block, such
    as a helper method that runs a ``node``.
    """
    try:
        with open(path, encoding="utf-8-sig", errors="replace") as source:
            text = source.read()
    except OSError:
        return False

    code = GROOVY_NON_CODE.sub(lambda m: " " + "\n" * m.group().count("\n"), text)
    top_level = []
    depth = 0
    for char in code:
        if char in "({[":
            if depth == 0:
                top_level.append(char)
            depth += 1
        elif char in ")}]":
            depth = max(depth - 1, 0)
            if depth == 0:
                top_level.append(char)
        elif depth == 0:
            top_level.append(char)
    return PIPELINE_START.search("".join(top_level)) is not None


def _is_pipeline_file(path: str, in_library: bool) -> bool:
    """Whether a directory search picks this file: the name finds candidates,
    and the content confirms those that could be something else."""
    name = os.path.basename(path).lower()
    if name.endswith(NOT_PIPELINE_SUFFIXES) or not os.path.isfile(path):
        return False
    if name == "jenkinsfile" or name.endswith(".jenkinsfile"):
        return True
    if in_library and name.endswith(".groovy"):
        return True
    if name.startswith("jenkinsfile") or name.endswith(".groovy"):
        return looks_like_pipeline(path)
    return False


def find_files(directory: str, audit: bool = False) -> List[str]:
    """Return the files to check under *directory*, sorted.

    Symlinks to directories are not followed.

    Args:
        directory: Directory to search
        audit: Also pick every ``*.groovy`` under a ``vars`` or ``src`` directory

    Returns:
        Paths of the files found
    """
    root_name = os.path.basename(os.path.abspath(directory))
    found = []
    for dirpath, dirnames, filenames in os.walk(directory):
        is_output = not BUILD_FILES.isdisjoint(filenames)
        dirnames[:] = [
            d
            for d in dirnames
            if d not in TOOL_DIRS and not (d == "build" and is_output)
        ]
        parents = {root_name, *Path(os.path.relpath(dirpath, directory)).parts}
        in_library = audit and not LIBRARY_DIRS.isdisjoint(parents)
        for name in filenames:
            path = os.path.normpath(os.path.join(dirpath, name))
            if _is_pipeline_file(path, in_library):
                found.append(path)
    return sorted(found)


def expand_paths(paths: List[str], audit: bool = False) -> Tuple[List[str], List[str]]:
    """Replace each directory in *paths* with the files found in it.

    Other paths stay as they are. A file that was also given by name, or found
    through another directory, is checked once.

    Returns:
        The files to check, and the directories in which none were found
    """
    seen = {os.path.normpath(p) for p in paths if not os.path.isdir(p)}
    files: List[str] = []
    empty: List[str] = []
    for path in paths:
        if not os.path.isdir(path):
            files.append(path)
            continue
        found = find_files(path, audit)
        if not found:
            empty.append(path)
        new = [f for f in found if f not in seen]
        seen.update(new)
        files.extend(new)
    return files, empty


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
    # After a directory search, even one file is named: it says what was found.
    show = args.verbose or args.searched or len(args.jenkinsfile) > 1
    if is_valid and message == NOT_DECLARATIVE_MESSAGE:
        # The Groovy parsed, but there is no Declarative syntax to check.
        if show:
            print(f"⊘ {jenkinsfile}: Skipped (not a Declarative Pipeline)")
    elif is_valid:
        # Show valid status for multiple files or when verbose
        if show:
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

# Printed with --verbose when offline checking was chosen because no Jenkins
# server is configured.
NO_JENKINS_NOTE = (
    "ℹ No Jenkins server configured, so checking offline: step names, step "
    "parameters and other plugin-dependent checks are skipped. Set JENKINS_URL "
    "to check with Jenkins."
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
            if automatic:
                print(NO_JENKINS_NOTE)
            print(f"✓ Offline engine: {engine}")
        results = lint_files(selected, engine) if selected else {}
    except OfflineEngineError as exc:
        if automatic:
            print(NO_JENKINS_NO_ENGINE, file=sys.stderr)
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)

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

        result = results[jenkinsfile]
        if result.lines:
            sys.stdout.flush()  # keep the output in file order when piped
        for line in result.lines:
            print(line, file=sys.stderr)

        if not result.is_valid:
            all_valid = False
        else:
            # Valid, or skipped when there is no Declarative syntax to check:
            # reported the same way as in the other modes.
            message = "" if result.declarative else NOT_DECLARATIVE_MESSAGE
            _report(jenkinsfile, True, message, args, printed_messages)

    sys.exit(0 if all_valid else 1)


SEVERITIES = ("high", "medium", "low")

# The dispositions of the fixes that each --fix mode makes.
FIX_MODES = {"safe": ("safe",), "unsafe-only": ("unsafe",), "all": ("safe", "unsafe")}


def _audit_options(args: argparse.Namespace) -> dict:
    return {
        "persona": args.persona or "regular",
        "min_severity": args.min_severity or "low",
        "min_confidence": args.min_confidence or "low",
        "ignore": args.ignore,
        "fix": args.fix,
    }


def _run_audit(args: argparse.Namespace) -> None:
    """Run the security checks with the offline engine: findings on stdout,
    or the engine's JSON or SARIF report as is."""
    selected = [f for f in args.jenkinsfile if _skip_reason(f, args) is None]
    text = args.format == "text"
    try:
        engine = find_engine() if selected or not text else None
        if engine and args.verbose:
            print(
                f"✓ Offline engine: {engine}", file=sys.stdout if text else sys.stderr
            )
        if not text or args.diff:
            output, errors, code = audit_report(
                selected, args.format, engine, diff=args.diff, **_audit_options(args)
            )
            sys.stderr.write(errors)
            print(output, end="")
            sys.exit(code)
        results = (
            audit_files(selected, engine, **_audit_options(args)) if selected else {}
        )
    except OfflineEngineError as exc:
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)

    counts = {severity: 0 for severity in SEVERITIES}
    fixed = 0
    fixable = {"safe": 0, "unsafe": 0}
    # What --fix made, and what a finding's [*] says it would make.
    applied = FIX_MODES[args.fix] if args.fix else ()
    marked = FIX_MODES[args.fix or "safe"]
    failed = False
    for jenkinsfile in args.jenkinsfile:
        reason = _skip_reason(jenkinsfile, args)
        if reason:
            if args.verbose:
                print(f"⊘ {jenkinsfile}: Skipped ({reason})")
            continue
        result = results[jenkinsfile]
        for finding in result.fixed:
            print(finding.render_fixed(jenkinsfile))
            fixed += 1
        for line in result.errors:
            sys.stdout.flush()  # keep the output in file order when piped
            print(line, file=sys.stderr)
            failed = True
        for finding in result.findings:
            print(finding.render(jenkinsfile, finding.fix in marked))
            counts[finding.severity] = counts.get(finding.severity, 0) + 1
            if finding.fix:
                fixable[finding.fix] += 1
        if args.verbose and not result.findings and not result.errors:
            print(f"✓ {jenkinsfile}: No findings")

    total = sum(counts.values())
    if args.fix:
        found = fixed + total
        print(
            f"Found {found} finding{'' if found == 1 else 's'} "
            f"({fixed} fixed, {total} remaining)."
        )
    elif total:
        parts = ", ".join(f"{counts[s]} {s}" for s in SEVERITIES if counts.get(s))
        print(f"{total} finding{'' if total == 1 else 's'} ({parts})")
    # Like ruff: say what the fixes not made would fix.
    safe = fixable["safe"] if "safe" not in applied else 0
    unsafe = fixable["unsafe"] if "unsafe" not in applied else 0
    if safe:
        print(f"[*] {safe} fixable with --fix.")
    if unsafe:
        more = " more" if safe else ""
        print(f"{unsafe}{more} fixable with --fix=all (unsafe, review the changes).")
    sys.exit(1 if total or failed else 0)


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
        help="Jenkinsfile(s) to validate, or directories to search for them. "
        "In a directory, picks Jenkinsfile and *.jenkinsfile, and Jenkinsfile* "
        "and *.groovy files that contain a pipeline; --audit also picks all "
        "*.groovy under vars/ and src/.",
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
    mode.add_argument(
        "--audit",
        action="store_true",
        help="Run security checks instead of validating, offline: find secrets "
        "interpolated into shell steps and other ways a pipeline can be attacked. "
        "Each finding links to its documentation.",
    )
    audit = parser.add_argument_group("audit options (with --audit)")
    audit.add_argument(
        "--persona",
        choices=("regular", "pedantic", "auditor"),
        help="How much to show: regular (default) shows what is worth fixing, "
        "pedantic adds lower-confidence findings and hardening advice, auditor "
        "shows everything.",
    )
    audit.add_argument(
        "--min-severity",
        choices=("low", "medium", "high"),
        help="Leave out findings below this severity.",
    )
    audit.add_argument(
        "--min-confidence",
        choices=("low", "medium", "high"),
        help="Leave out findings below this confidence.",
    )
    audit.add_argument(
        "--ignore",
        action="append",
        default=[],
        metavar="RULE",
        help="Turn off a rule in every file, such as unpinned-library. Takes a "
        "comma-separated list and can be used multiple times.",
    )
    audit.add_argument(
        "--fix",
        action="store_const",
        const="safe",
        metavar="MODE",
        help="Rewrite the files with the fixes that --fix=MODE allows, then report "
        "the findings that remain: safe (the default), unsafe-only or all. An "
        "unsafe fix can change what a pipeline does, so review it. Today's fixes, "
        "for script-injection and secret-interpolation, are unsafe.",
    )
    audit.add_argument(
        "--diff",
        action="store_true",
        help="Print what --fix would change as a diff, and change nothing. Exits "
        "with 1 when there is a change.",
    )
    audit.add_argument(
        "--format",
        choices=("text", "json", "sarif"),
        default="text",
        help="Output format (default: text). sarif is for GitHub code scanning "
        "and other tools, and exits with 0 when there are findings, so that the "
        "report can be uploaded.",
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

    # --fix takes its mode only as --fix=MODE, like zizmor's, so that
    # `--fix Jenkinsfile` does not read the file name as a mode.
    argv, modes = [], []
    for i, arg in enumerate(sys.argv[1:], 1):
        if arg == "--":
            argv += sys.argv[i:]
            break
        if arg.startswith("--fix="):
            modes.append(arg[len("--fix=") :])
            arg = "--fix"
        argv.append(arg)
    args = parser.parse_args(argv)
    if modes:
        if modes[-1] not in FIX_MODES:
            choices = ", ".join(FIX_MODES)
            parser.error(
                f"argument --fix: invalid choice: '{modes[-1]}' (choose from {choices})"
            )
        args.fix = modes[-1]

    if not args.audit:
        for option in (
            "persona",
            "min_severity",
            "min_confidence",
            "ignore",
            "fix",
            "diff",
        ):
            if getattr(args, option):
                flag = "--" + option.replace("_", "-")
                parser.error(f"argument {flag}: requires --audit")
        if args.format != "text":
            parser.error("argument --format: requires --audit")
    if args.diff and args.format != "text":
        parser.error("argument --diff: not allowed with argument --format")
    if args.offline or args.audit:
        offline_flag = "--audit" if args.audit else "--offline"
        for option in ("jenkins_url", "username", "token"):
            if getattr(args, option):
                flag = "--" + option.replace("_", "-")
                parser.error(
                    f"argument {offline_flag}: not allowed with argument {flag}"
                )

    args.searched = any(os.path.isdir(path) for path in args.jenkinsfile)
    args.jenkinsfile, empty = expand_paths(args.jenkinsfile, args.audit)
    report = args.audit and args.format != "text"
    if args.verbose:
        for directory in empty:
            print(
                f"ℹ No files to check found in {directory}",
                file=sys.stderr if report else sys.stdout,
            )
    # Nothing to check: don't start a container or call Jenkins. A report is
    # still written, so that a tool reading it gets an empty one.
    if not args.jenkinsfile and not report:
        sys.exit(0)

    if args.audit:
        _run_audit(args)
    elif args.offline:
        _run_offline_validation(args)
    elif args.local:
        _run_local_validation(args)
    elif _jenkins_configured(args):
        _run_remote_validation(args)
    else:
        _run_offline_validation(args, automatic=True)


if __name__ == "__main__":
    main()
