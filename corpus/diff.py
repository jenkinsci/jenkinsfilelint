#!/usr/bin/env python3
"""Show what a change to the engine changes on the corpus.

Usage: python3 corpus/diff.py [--base REF] [--engine PATH] [--sample N] [--copy DIR]

Runs the engine of a base commit (origin/main by default) and the engine of
this checkout on every file that fetch.py downloaded, with --audit and every
persona, and lists each difference: files that now fail or pass validation,
and audit findings added, removed or changed.

The base engine is built from the base commit's source with cargo and kept
in <cache>/engines/<commit>. The head engine is this checkout's, built
first, unless --engine names one. With --base none, the base is a run that
found nothing: every finding counts as added.

When files change verdict, Jenkins decides which side was right, through
the cache and the validator of compare.py: "fixed" files now agree with
Jenkins, "broken" files no longer do. The exit status is 1 when a file
broke. --no-jenkins skips this.

The summary names no repository, to paste into a pull request. The details
go to <cache>/diff/: changes.tsv lists every difference, and with --sample N,
sample-<rule>.md shows N added findings of each rule, picked at random, with
the code around them, to check by hand how many are real.
"""

import argparse
import io
import json
import random
import shutil
import subprocess
import sys
import tarfile
import tempfile
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import NamedTuple

HERE = Path(__file__).resolve().parent
REPO = HERE.parent
CRATE = "crates/jenkins-lint"
ENGINE = "jenkins-lint"
# Where the crate and its binary were before 2.0, for older base commits.
OLD_CRATE = "crates/jenkinsfilelint"
OLD_ENGINE = "jenkinsfilelint-engine"
sys.path[:0] = [str(HERE), str(REPO)]

from compare import ask_jenkins, category, normalize  # noqa: E402
from fetch import read_manifest  # noqa: E402

# The categories of compare.py in which the engine agrees with Jenkins, as
# far as it can without the plugins.
RIGHT = {"agree", "syntax", "needs-plugins", "known"}
CONTEXT = 6  # lines of code shown around a sampled finding


VERDICTS = ("now rejected", "now accepted", "error changed")


class Change(NamedTuple):
    """One difference: a file's verdict, or the findings at one place."""

    kind: str  # one of VERDICTS, or added, removed, changed
    blob: str
    rule: str = ""
    persona: str = ""
    severity: str = ""
    confidence: str = ""
    line: str = ""
    before: str = ""
    after: str = ""


def git(*args):
    return subprocess.run(
        ["git", "-C", str(REPO), *args], capture_output=True, text=True, check=True
    ).stdout.strip()


def cargo_build(crate_dir):
    cmd = ["cargo", "build", "--release", "--locked", "--quiet"]
    r = subprocess.run(cmd, cwd=crate_dir, capture_output=True, text=True, check=False)
    if r.returncode != 0:
        sys.exit(f"cargo build failed in {crate_dir}:\n{r.stderr}")
    built = crate_dir / "target" / "release" / ENGINE
    return built if built.exists() else crate_dir / "target" / "release" / OLD_ENGINE


def base_engine(ref, cache):
    """The engine of a commit, built once and kept by commit hash."""
    try:
        commit = git("rev-parse", "--verify", f"{ref}^{{commit}}")
    except subprocess.CalledProcessError:
        sys.exit(f"{ref} is not a commit")
    built = cache / "engines" / commit / ENGINE
    if built.exists():
        return commit, built
    print(f"building the engine of {commit[:12]}", file=sys.stderr)
    crate = CRATE
    if subprocess.run(
        ["git", "-C", str(REPO), "cat-file", "-e", f"{commit}:{CRATE}"],
        capture_output=True,
        check=False,
    ).returncode:
        crate = OLD_CRATE
    archive = subprocess.run(
        ["git", "-C", str(REPO), "archive", "--format=tar", commit, crate],
        capture_output=True,
        check=True,
    ).stdout
    # A fresh build directory each time: the files of the archive carry the
    # commit's time, so cargo would take an older commit's build in a
    # shared one as up to date.
    with tempfile.TemporaryDirectory() as tmp:
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(tmp, filter="data")
        engine = cargo_build(Path(tmp) / crate)
        built.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(engine, built)
    return commit, built


def head_engine():
    """This checkout's engine, built, and how to name it."""
    engine = cargo_build(REPO / CRATE)
    name = git("rev-parse", "--short=12", "HEAD")
    if git("status", "--porcelain", "--", CRATE):
        name += " with uncommitted changes"
    return name, engine


def run(engine, paths, jobs):
    """blob -> (errors, findings): the errors of validation, and the audit's
    findings, as a map from (rule, line, column) to the sorted details of
    the findings there. The audit does not validate, so each file is checked
    twice."""

    def json_of(options, chunk):
        r = subprocess.run(
            [engine, *options, "--format", "json", "--", *map(str, chunk)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        try:
            return json.loads(r.stdout)
        except json.JSONDecodeError:
            sys.exit(f"{engine} failed: {r.stderr.strip()[:500]}")

    def batch(chunk):
        audit = ["--audit", "--persona", "auditor"]
        return chunk, json_of([], chunk), json_of(audit, chunk)

    out = {}
    chunks = [paths[i : i + 300] for i in range(0, len(paths), 300)]
    with ThreadPoolExecutor(jobs) as pool:
        for chunk, checked, audited in pool.map(batch, chunks):
            for path, report, audit in zip(chunk, checked, audited):
                if "error" in report:
                    out[path.name] = (["(the engine cannot read this file)"], {})
                    continue
                errors = [
                    normalize(d["message"])
                    for d in report["diagnostics"]
                    if d["severity"] == "error"
                ]
                findings = defaultdict(list)
                for f in audit.get("findings", []):
                    findings[(f["rule"], f["line"], f["column"])].append(
                        (
                            f["persona"],
                            f["severity"],
                            f["confidence"],
                            f["end_line"],
                            f["end_column"],
                            f["message"],
                        )
                    )
                out[path.name] = (errors, {k: sorted(v) for k, v in findings.items()})
    return out


def verdict_change(before, after):
    if before == after:
        return None
    if not before:
        return "now rejected"
    if not after:
        return "now accepted"
    return "error changed"


def describe(details):
    """persona, severity, confidence of the first finding at a place."""
    persona, severity, confidence, *_ = details[0]
    return persona, severity, confidence


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--base", default="origin/main", help="commit to compare with")
    ap.add_argument("--engine", type=Path, help="head engine (default: build it)")
    ap.add_argument("--manifest", type=Path, default=HERE / "manifest.tsv")
    ap.add_argument("--cache", type=Path, default=HERE / ".cache")
    ap.add_argument("--jenkins-url", default="http://127.0.0.1:18080")
    ap.add_argument("--no-jenkins", action="store_true")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument(
        "--sample", type=int, default=0, help="added findings to show per rule"
    )
    ap.add_argument("--seed", type=int, default=0)
    ap.add_argument("--copy", type=Path, help="copy the changed files here")
    args = ap.parse_args()

    rows = {r[0]: r for r in read_manifest(args.manifest)}
    files = args.cache / "files"
    paths = sorted(files / b for b in rows if (files / b).exists())
    if not paths:
        sys.exit(f"no files in {files}: run fetch.py first")

    if args.engine:
        head_name, head = str(args.engine), args.engine
    else:
        head_name, head = head_engine()
    after = run(head, paths, args.jobs)
    if args.base == "none":
        base_name = "nothing"
        before = {blob: (errors, {}) for blob, (errors, _) in after.items()}
    else:
        commit, base = base_engine(args.base, args.cache)
        base_name = f"{commit[:12]} ({args.base})"
        before = run(base, paths, args.jobs)

    # Every difference, file by file.
    changes = []
    verdicts = {}
    for path in paths:
        blob = path.name
        (errors_b, found_b), (errors_a, found_a) = before[blob], after[blob]
        change = verdict_change(errors_b, errors_a)
        if change:
            verdicts[blob] = change
            changes.append(
                Change(
                    change,
                    blob,
                    before=" | ".join(errors_b),
                    after=" | ".join(errors_a),
                )
            )
        for key in sorted(found_b.keys() | found_a.keys()):
            rule, line, _ = key
            b, a = found_b.get(key), found_a.get(key)
            if b == a:
                continue
            kind = "added" if not b else "removed" if not a else "changed"
            changes.append(
                Change(
                    kind,
                    blob,
                    rule,
                    *describe(a or b),
                    str(line),
                    b[0][-1] if b else "",
                    a[0][-1] if a else "",
                )
            )

    # Jenkins decides the changed verdicts.
    judged = {}
    if verdicts and not args.no_jenkins and args.base != "none":
        changed = [files / blob for blob in verdicts]
        jenkins = ask_jenkins(
            args.jenkins_url, changed, args.cache / "jenkins.jsonl", args.jobs
        )
        for blob in verdicts:
            was = category(before[blob][0], jenkins[blob])
            now = category(after[blob][0], jenkins[blob])
            if "no-answer" in (was, now):
                judged[blob] = "no answer"
            elif was in RIGHT and now in RIGHT:
                judged[blob] = "still right"
            elif now in RIGHT:
                judged[blob] = "fixed"
            elif was in RIGHT:
                judged[blob] = "broken"
            else:
                judged[blob] = "still wrong"
            judged[blob] += f" ({was} -> {now})"

    out = args.cache / "diff"
    out.mkdir(parents=True, exist_ok=True)
    with (out / "changes.tsv").open("w", encoding="utf-8") as f:
        f.write(
            "change\trule\tpersona\tseverity\tconfidence\tblob\trepository\tpath"
            "\tline\tbefore\tafter\tjenkins\n"
        )
        for c in changes:
            _, repo, _, file_path, _ = rows[c.blob]
            jenkins = judged.get(c.blob, "") if c.kind in VERDICTS else ""
            fields = (c.kind, c.rule, c.persona, c.severity, c.confidence, c.blob)
            fields += (repo, file_path, c.line, c.before, c.after, jenkins)
            f.write("\t".join(fields) + "\n")

    # The summary, without repository names.
    print(f"{len(paths):,} files. Base: {base_name}. Head: {head_name}.")
    print()
    counts = Counter(verdicts.values())
    print("Verdicts")
    for kind in VERDICTS:
        print(f"  {kind:22} {counts[kind]:6,}")
    if judged:
        print()
        print("Checked with Jenkins")
        outcome = Counter(j.split(" (")[0] for j in judged.values())
        for kind in ("fixed", "broken", "still right", "still wrong", "no answer"):
            if outcome[kind]:
                print(f"  {kind:22} {outcome[kind]:6,}")
    print()
    table = defaultdict(Counter)
    in_changed = Counter()
    for c in changes:
        if c.rule:
            table[(c.rule, c.persona)][c.kind] += 1
            if c.blob in verdicts:
                in_changed[c.kind] += 1
    print(f"Findings {'':24} {'added':>7} {'removed':>7} {'changed':>7}")
    for (rule, persona), c in sorted(table.items()):
        label = f"{rule} ({persona})"
        print(f"  {label:30} {c['added']:7,} {c['removed']:7,} {c['changed']:7,}")
    if not table:
        print("  (none)")
    if in_changed:
        print(
            f"  of these, in files whose verdict changed: {in_changed['added']:,} added,"
            f" {in_changed['removed']:,} removed, {in_changed['changed']:,} changed"
        )
    print(f"\nDetails: {out / 'changes.tsv'}")

    if args.copy:
        args.copy.mkdir(parents=True, exist_ok=True)
        for blob in {c.blob for c in changes}:
            shutil.copyfile(files / blob, args.copy / f"{blob[:12]}.groovy")

    if args.sample:
        by_rule = defaultdict(list)
        for c in changes:
            if c.kind == "added":
                by_rule[c.rule].append(c)
        for rule, found in sorted(by_rule.items()):
            picked = random.Random(args.seed).sample(
                found, min(args.sample, len(found))
            )
            sample = out / f"sample-{rule}.md"
            sample.write_text(
                render_sample(rule, picked, len(found), rows, files), encoding="utf-8"
            )
            print(f"Sample:  {sample}")

    return 1 if any(j.startswith("broken") for j in judged.values()) else 0


def render_sample(rule, picked, total, rows, files):
    parts = [f"# {rule}: {len(picked)} of {total} added findings\n"]
    for n, c in enumerate(picked, 1):
        _, repo, commit, file_path, _ = rows[c.blob]
        text = (
            (files / c.blob).read_text(encoding="utf-8", errors="replace").splitlines()
        )
        at = int(c.line)
        lo, hi = max(1, at - CONTEXT), min(len(text), at + CONTEXT)
        code = "\n".join(
            f"{'>' if i == at else ' '} {i:4} | {text[i - 1]}"
            for i in range(lo, hi + 1)
        )
        parts.append(
            f"## {n}. {repo}/{file_path}:{c.line}\n\n"
            f"{c.persona}, {c.severity} severity, {c.confidence} confidence."
            f" Blob {c.blob}, commit {commit}.\n\n"
            f"{c.after}\n\n```\n{code}\n```\n"
        )
    return "\n".join(parts)


if __name__ == "__main__":
    sys.exit(main())
