#!/usr/bin/env python3
"""Package the jenkins-lint binary of each wheel as a release archive.

The wheels are the files that release.yml tests on every platform, so the
binaries of the GitHub release are those same files, not a second build.

The input directory has one directory per target, as the wheel-<target>
artifacts download: wheels/wheel-x86_64-unknown-linux-gnu/*.whl. For each
target it writes to the output directory:

- jenkins-lint-<target>.tar.gz, with jenkins-lint-<target>/jenkins-lint,
  LICENSE and README.md, or for Windows jenkins-lint-<target>.zip, with
  jenkins-lint.exe, LICENSE and README.md at its top;
- <archive>.sha256, in the format of sha256sum.

Usage:
    python3 crates/jenkins-lint/scripts/package_binaries.py WHEELS OUT
"""

import argparse
import hashlib
import io
import sys
import tarfile
import zipfile
from pathlib import Path
from typing import List, Optional

FILES = ["LICENSE", "README.md"]


def binary(wheel: Path) -> bytes:
    """The jenkins-lint binary in the wheel."""
    with zipfile.ZipFile(wheel) as archive:
        names = [
            name
            for name in archive.namelist()
            if name.split("/")[-2:-1] == ["scripts"]
            and name.split("/")[-1] in ("jenkins-lint", "jenkins-lint.exe")
        ]
        if len(names) != 1:
            raise ValueError(
                f"{wheel}: expected one jenkins-lint binary, found {names}"
            )
        return archive.read(names[0])


def add(archive: tarfile.TarFile, name: str, data: bytes, mode: int) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    info.mtime = 0
    archive.addfile(info, io.BytesIO(data))


def tar_gz(path: Path, target: str, exe: bytes, files: dict) -> None:
    top = f"jenkins-lint-{target}"
    with tarfile.open(path, "w:gz") as archive:
        add(archive, f"{top}/jenkins-lint", exe, 0o755)
        for name, data in files.items():
            add(archive, f"{top}/{name}", data, 0o644)


def zip_file(path: Path, exe: bytes, files: dict) -> None:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("jenkins-lint.exe", exe)
        for name, data in files.items():
            archive.writestr(name, data)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("wheels", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args(argv)

    files = {name: Path(name).read_bytes() for name in FILES}
    args.out.mkdir(parents=True, exist_ok=True)
    count = 0
    for directory in sorted(args.wheels.glob("wheel-*")):
        target = directory.name[len("wheel-") :]
        wheels = list(directory.glob("*.whl"))
        if len(wheels) != 1:
            raise ValueError(f"{directory}: expected one wheel, found {wheels}")
        exe = binary(wheels[0])
        if "windows" in target:
            path = args.out / f"jenkins-lint-{target}.zip"
            zip_file(path, exe, files)
        else:
            path = args.out / f"jenkins-lint-{target}.tar.gz"
            tar_gz(path, target, exe, files)
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        Path(f"{path}.sha256").write_text(f"{digest}  {path.name}\n")
        print(f"{path.name}  {digest}")
        count += 1
    if count == 0:
        raise ValueError(f"no wheel-* directories in {args.wheels}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
