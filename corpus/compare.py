#!/usr/bin/env python3
"""Compare the offline engine with Jenkins's own validator on the corpus.

Usage: python3 corpus/compare.py [--jenkins-url URL] [--engine PATH] [--copy DIR]

Needs the files that fetch.py downloaded, and a Jenkins with the
Declarative validator, such as the one `jenkins-lint --local` starts
(the default URL). Jenkins's answers are kept in <cache>/jenkins.jsonl by
blob, so a later run only asks about new files: delete it after changing
the server or its plugins.

Prints how many files fall in each category, and writes every file's
category and messages to <cache>/report.tsv:

  agree          the same verdict, and the same first error if any
  syntax         both find a Groovy syntax error, worded differently
  false-error    the engine reports an error that Jenkins does not, or a
                 syntax error in a file Jenkins compiles: a bug
  missed         Jenkins reports an error that the engine does not, or a
                 syntax error in a file the engine reads
  needs-plugins  Jenkins reports an error that the engine cannot know
                 without the plugins: step parameters, agent types, tools
  other-message  both report errors, but different ones
  known          the engine is stricter on purpose: a second pipeline
                 block, a byte order mark (see tests/jenkins/expected.txt)
  unreadable     the engine cannot read the file (not UTF-8)
  no-answer      Jenkins did not answer

With --copy, the files of the categories to look at (false-error, missed,
other-message) are copied to DIR, to read or to reduce to test cases.
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import threading
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
sys.path.insert(0, str(REPO))

from jenkins_lint.linter import JenkinsfileLinter  # noqa: E402

from fetch import read_manifest  # noqa: E402

# Errors only a controller with the plugins finds (the validator's
# ModelValidatorImpl messages about step signatures, agents and tools).
NEEDS_PLUGINS = re.compile(
    r'^(Invalid parameter ".*", did you mean'
    r"|Missing required parameter"
    r'|Arguments to ".*" must be explicitly named'
    r'|".*" should have \d+ arguments'
    r"|Step does not take a single required parameter"
    r"|Cannot mix named and unnamed arguments"
    r"|Invalid type for parameter"
    r'|Expecting ".*" (for parameter ".*" )?but got'
    r"|Invalid agent type"
    r'|Invalid config option ".*" for agent type'
    r"|Multiple parameters are required for agent type"
    r'|Invalid [\w ]+ type ".*"\. Valid'
    r'|Tool type ".*" does not have an install'
    r"|Scripts not permitted)"
)

# Groovy's compiler errors: its parser's start in lower case, or with
# "Unexpected input"; the validator's own start with a capital.
COMPILER = re.compile(
    r"^([a-z]|Unexpected input|Ambiguous expression|Groovy compilation error"
    r"|Constructor call must be the first)"
)

# Errors the engine reports on purpose where the validator does not, as
# tests/jenkins/expected.txt explains: Jenkins refuses to run these files.
KNOWN = re.compile(
    r"^(Multiple occurrences of the pipeline section|Unexpected byte order mark)"
)

# The engine's syntax errors, from its lexer and parser.
OFFLINE_SYNTAX = re.compile(
    r"^(Unexpected |Unterminated |Unclosed |illegal string body|Invalid unicode escape"
    r"|Ambiguous expression|Constructor call must be the first)"
)

LOOK_AT = ("false-error", "missed", "other-message")


def default_engine():
    env = os.environ.get("JENKINS_LINT_ENGINE") or os.environ.get(
        "JENKINSFILELINT_ENGINE"
    )
    if env:
        return env
    built = REPO / "crates/jenkinsfilelint/target/release/jenkinsfilelint-engine"
    return str(built) if built.exists() else "jenkinsfilelint-engine"


def normalize(message):
    return " ".join(message.split()).rstrip(".")


def jenkins_messages(text):
    """The error messages in a validator response, without positions. A
    message that ends with `;` goes on with a `solution:` line."""
    out = []
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = re.match(r"WorkflowScript: \d+: (.*)$", line)
        if not m:
            continue
        message = m.group(1)
        k = i + 1
        while message.endswith(";") and k < len(lines) and lines[k].startswith(" "):
            message += " " + lines[k].strip()
            k += 1
        message = re.sub(r" @ line \d+, column \d+\.$", "", message)
        out.append(normalize(message))
    return out


def offline(engine, paths):
    """blob -> list of error messages, or None when the file is unreadable."""
    out = {}
    for i in range(0, len(paths), 300):
        batch = paths[i : i + 300]
        r = subprocess.run(
            [engine, "--format", "json", "--", *map(str, batch)],
            capture_output=True,
            text=True,
            encoding="utf-8",
        )
        for path, report in zip(batch, json.loads(r.stdout)):
            if "error" in report:
                out[path.name] = None
            else:
                out[path.name] = [
                    normalize(d["message"])
                    for d in report["diagnostics"]
                    if d["severity"] == "error"
                ]
    return out


def ask_jenkins(url, paths, cache, jobs):
    """blob -> (ok, messages, first line), from the cache or from Jenkins."""
    answers = {}
    if cache.exists():
        for line in cache.read_text(encoding="utf-8").splitlines():
            a = json.loads(line)
            answers[a["blob"]] = (a["ok"], a["messages"], a["first"])
    todo = [p for p in paths if p.name not in answers]
    local = threading.local()
    lock = threading.Lock()

    def one(path):
        if not hasattr(local, "linter"):
            local.linter = JenkinsfileLinter(url)
        ok, message = local.linter.validate(str(path))
        first = message.strip().splitlines()[0][:200] if message.strip() else ""
        return path.name, ok, jenkins_messages(message), first

    with cache.open("a", encoding="utf-8") as f, ThreadPoolExecutor(jobs) as pool:
        futures = [pool.submit(one, p) for p in todo]
        for n, fut in enumerate(as_completed(futures), 1):
            blob, ok, messages, first = fut.result()
            answers[blob] = (ok, messages, first)
            # Keep only real answers: ask again after a connection error.
            if ok or messages:
                with lock:
                    f.write(
                        json.dumps(
                            {
                                "blob": blob,
                                "ok": ok,
                                "messages": messages,
                                "first": first,
                            }
                        )
                        + "\n"
                    )
            if n % 500 == 0:
                print(
                    f"  asked Jenkins about {n} of {len(todo)} files", file=sys.stderr
                )
    return answers


def category(off, jenkins):
    """The category of one file, from both verdicts."""
    if off is None:
        return "unreadable"
    ok, messages, _ = jenkins
    if not ok and not messages:
        return "no-answer"
    if not off and ok:
        return "agree"
    if off and KNOWN.match(off[0]):
        return "known"
    if off and ok:
        return "false-error"
    if not off:
        return (
            "needs-plugins"
            if all(NEEDS_PLUGINS.match(m) for m in messages)
            else "missed"
        )
    if off[0] in messages:
        return "agree"
    ours = bool(OFFLINE_SYNTAX.match(off[0]))
    theirs = any(COMPILER.match(m) for m in messages)
    if ours and theirs:
        return "syntax"
    # Jenkins compiled the file and found only other errors.
    if ours:
        return "false-error"
    if theirs:
        return "missed"
    return "other-message"


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--manifest", type=Path, default=HERE / "manifest.tsv")
    ap.add_argument("--cache", type=Path, default=HERE / ".cache")
    ap.add_argument("--jenkins-url", default="http://127.0.0.1:18080")
    ap.add_argument("--engine", default=default_engine())
    ap.add_argument("--jobs", type=int, default=6)
    ap.add_argument(
        "--copy", type=Path, help="copy the files to look at to this directory"
    )
    args = ap.parse_args()

    rows = {r[0]: r for r in read_manifest(args.manifest)}
    files = args.cache / "files"
    paths = sorted(files / b for b in rows if (files / b).exists())
    print(f"{len(paths)} of {len(rows)} files downloaded", file=sys.stderr)

    off = offline(args.engine, paths)
    jenkins = ask_jenkins(
        args.jenkins_url, paths, args.cache / "jenkins.jsonl", args.jobs
    )

    counts = Counter()
    if args.copy:
        args.copy.mkdir(parents=True, exist_ok=True)
    with (args.cache / "report.tsv").open("w", encoding="utf-8") as f:
        f.write("category\tblob\trepository\tpath\toffline\tjenkins\n")
        for path in paths:
            blob = path.name
            cat = category(off[blob], jenkins[blob])
            counts[cat] += 1
            _, repo, _, file_path, _ = rows[blob]
            ok, messages, first = jenkins[blob]
            shown = " | ".join(messages) if messages else ("" if ok else first)
            f.write(
                f"{cat}\t{blob}\t{repo}\t{file_path}\t{' | '.join(off[blob] or [])}\t{shown}\n"
            )
            if args.copy and cat in LOOK_AT:
                shutil.copyfile(path, args.copy / f"{cat}-{blob[:12]}.groovy")

    total = sum(counts.values())
    for cat in (
        "agree",
        "syntax",
        "needs-plugins",
        "known",
        *LOOK_AT,
        "unreadable",
        "no-answer",
    ):
        print(f"  {cat:14} {counts[cat]:6}  {100 * counts[cat] / max(total, 1):5.1f}%")
    print(f"  {'total':14} {total:6}")


if __name__ == "__main__":
    main()
