#!/usr/bin/env python3
"""Download the files that the corpus manifest lists.

Usage: python3 corpus/fetch.py [--manifest corpus/manifest.tsv] [--cache corpus/.cache]

Each file is fetched from raw.githubusercontent.com at the commit the
manifest names, checked against its Git blob hash, and saved as
<cache>/files/<blob>. Files already there are skipped, so a second run only
fetches what is missing. Failures are listed in <cache>/fetch-failures.tsv.

The files are other people's code: they stay in the cache, outside Git.
"""

import argparse
import hashlib
import sys
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from urllib.parse import quote

HERE = Path(__file__).resolve().parent


def read_manifest(path):
    """(blob, repository, commit, path, query) for each line."""
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line and not line.startswith("#"):
            rows.append(tuple(line.split("\t")))
    return rows


def blob_hash(data):
    """The hash Git gives these bytes as a blob."""
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def fetch(row, files):
    blob, repo, commit, path, _ = row
    dest = files / blob
    if dest.exists():
        return None
    url = f"https://raw.githubusercontent.com/{repo}/{commit}/{quote(path)}"
    try:
        with urllib.request.urlopen(url, timeout=60) as r:
            data = r.read()
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        return (blob, url, str(e))
    if blob_hash(data) != blob:
        return (blob, url, "content does not match the blob hash")
    tmp = dest.with_suffix(".part")
    tmp.write_bytes(data)
    tmp.replace(dest)
    return None


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--manifest", type=Path, default=HERE / "manifest.tsv")
    ap.add_argument("--cache", type=Path, default=HERE / ".cache")
    ap.add_argument("--jobs", type=int, default=8)
    args = ap.parse_args()

    files = args.cache / "files"
    files.mkdir(parents=True, exist_ok=True)
    rows = read_manifest(args.manifest)
    with ThreadPoolExecutor(args.jobs) as pool:
        failures = [f for f in pool.map(lambda r: fetch(r, files), rows) if f]
    with (args.cache / "fetch-failures.tsv").open("w", encoding="utf-8") as f:
        for failure in failures:
            f.write("\t".join(failure) + "\n")
    print(
        f"{len(rows) - len(failures)} of {len(rows)} files in {files}; {len(failures)} failed",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
