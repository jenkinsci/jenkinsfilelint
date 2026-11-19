#!/usr/bin/env python3
"""Find public Jenkinsfiles and shared library steps with GitHub code search.

Usage: python3 corpus/collect.py [--out corpus/manifest.tsv] [--pages 2]

Writes a manifest: one line per distinct file (by Git blob hash), with the
repository, the commit and the path, so that fetch.py downloads exactly the
same files later. The files themselves are not stored here.

Code search returns at most 1,000 results per query and allows about ten
queries a minute, so the search is split by file size, and each size range
is a query of its own. Needs the GitHub CLI (`gh`), logged in.
"""

import argparse
import json
import subprocess
import sys
import time
from datetime import date
from pathlib import Path
from urllib.parse import unquote, urlsplit

HERE = Path(__file__).resolve().parent
PAUSE = 6.5  # seconds between searches: code search allows 10 a minute


def size_ranges(edges):
    return [f"{lo}..{hi - 1}" for lo, hi in zip(edges, edges[1:])]


def queries():
    """The searches, from small files to large ones."""
    edges = (
        [0, 200]
        + list(range(300, 5000, 100))
        + list(range(5000, 10000, 250))
        + list(range(10000, 30000, 1000))
        + [30000, 40000, 60000, 100000, 400000]
    )
    for r in size_ranges(edges):
        yield f"filename:Jenkinsfile size:{r}"
    # Steps of shared libraries.
    edges = [0, 500, 1000, 1500, 2000, 3000, 4000, 6000, 10000, 20000, 400000]
    for r in size_ranges(edges):
        yield f"path:vars extension:groovy size:{r}"


def search(query, page):
    """One page of results, retrying when the rate limit is hit."""
    for attempt in range(6):
        r = subprocess.run(
            [
                "gh",
                "api",
                "-X",
                "GET",
                "search/code",
                "-f",
                f"q={query}",
                "-f",
                "per_page=100",
                "-f",
                f"page={page}",
            ],
            capture_output=True,
            text=True,
        )
        if r.returncode == 0:
            return json.loads(r.stdout)
        if "rate limit" in r.stderr.lower() or "abuse" in r.stderr.lower():
            time.sleep(60 * (attempt + 1))
            continue
        # Past the last page, or a query GitHub refuses.
        print(f"  {query} page {page}: {r.stderr.strip()[:200]}", file=sys.stderr)
        return None
    return None


def entry(item, query):
    """A manifest line for a search result: blob, repo, commit, path, query."""
    parts = urlsplit(item["html_url"]).path.split("/")
    # /owner/repo/blob/<commit>/path...
    commit = unquote(parts[4])
    return (item["sha"], item["repository"]["full_name"], commit, item["path"], query)


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--out", type=Path, default=HERE / "manifest.tsv")
    ap.add_argument(
        "--pages",
        type=int,
        default=2,
        help="pages of 100 results per query (at most 10)",
    )
    args = ap.parse_args()

    found = {}
    plan = list(queries())
    for n, query in enumerate(plan, 1):
        before = len(found)
        for page in range(1, args.pages + 1):
            result = search(query, page)
            time.sleep(PAUSE)
            if not result or not result.get("items"):
                break
            for item in result["items"]:
                e = entry(item, query)
                found.setdefault(e[0], e)
            if len(result["items"]) < 100:
                break
        print(
            f"[{n}/{len(plan)}] {query}: +{len(found) - before} ({len(found)} files)",
            file=sys.stderr,
        )

    rows = sorted(found.values(), key=lambda e: (e[1].lower(), e[3], e[0]))
    with args.out.open("w", encoding="utf-8", newline="\n") as f:
        f.write(
            f"# Public files found by corpus/collect.py on {date.today().isoformat()}.\n"
        )
        f.write("# blob\trepository\tcommit\tpath\tquery\n")
        for row in rows:
            f.write("\t".join(row) + "\n")
    print(f"{len(rows)} files in {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
