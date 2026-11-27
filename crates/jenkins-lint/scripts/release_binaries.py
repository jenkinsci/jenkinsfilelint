#!/usr/bin/env python3
"""Take the jenkins-lint binaries out of the wheels, for GitHub Releases
and the Docker image.

Each wheel holds the binary that maturin built for one target, in
jenkins_lint-<version>.data/scripts/. This writes, for each target:

    <out>/jenkins-lint-<target>.tar.gz          (.zip for Windows)
    <out>/jenkins-lint-<target>.tar.gz.sha256

with the binary and LICENSE at the top of the archive, and with
--docker-context, the Linux musl binaries at <dir>/<platform>/jenkins-lint
for the Dockerfile, which copies dist/$TARGETPLATFORM/jenkins-lint.

The names carry no version, so that
https://github.com/jenkinsci/jenkins-lint/releases/latest/download/jenkins-lint-<target>.tar.gz
always gives the latest release.

Usage:
    python3 crates/jenkins-lint/scripts/release_binaries.py \
        --wheels DIR --out DIR [--docker-context DIR]
"""

import argparse
import hashlib
import io
import re
import sys
import tarfile
import zipfile
from pathlib import Path
from typing import Dict, Optional

REPO = Path(__file__).resolve().parents[3]

# The target each wheel's platform tag was built for, as in release.yml.
TARGETS = {
    r"manylinux_\d+_\d+_x86_64|manylinux2014_x86_64": "x86_64-unknown-linux-gnu",
    r"manylinux_\d+_\d+_aarch64|manylinux2014_aarch64": "aarch64-unknown-linux-gnu",
    r"manylinux_\d+_\d+_i686|manylinux2014_i686": "i686-unknown-linux-gnu",
    r"manylinux_\d+_\d+_armv7l|manylinux2014_armv7l": "armv7-unknown-linux-gnueabihf",
    r"musllinux_\d+_\d+_x86_64": "x86_64-unknown-linux-musl",
    r"musllinux_\d+_\d+_aarch64": "aarch64-unknown-linux-musl",
    r"musllinux_\d+_\d+_i686": "i686-unknown-linux-musl",
    r"musllinux_\d+_\d+_armv7l": "armv7-unknown-linux-musleabihf",
    r"macosx_\d+_\d+_x86_64": "x86_64-apple-darwin",
    r"macosx_\d+_\d+_arm64": "aarch64-apple-darwin",
    r"win_amd64": "x86_64-pc-windows-msvc",
    r"win32": "i686-pc-windows-msvc",
}

# The Docker platforms of the image, and the static binary each one gets.
DOCKER = {
    "linux/amd64": "x86_64-unknown-linux-musl",
    "linux/arm64": "aarch64-unknown-linux-musl",
}


def target_of(wheel: str) -> Optional[str]:
    """The target of a wheel file name, from its platform tag."""
    platform = wheel[: -len(".whl")].split("-")[-1]
    for tag in platform.split("."):
        for pattern, target in TARGETS.items():
            if re.fullmatch(pattern, tag):
                return target
    return None


def binary_of(wheel: Path) -> "tuple[str, bytes]":
    """The name and bytes of the jenkins-lint binary in a wheel."""
    with zipfile.ZipFile(wheel) as z:
        for name in z.namelist():
            base = name.rsplit("/", 1)[-1]
            if ".data/scripts/" in name and base in (
                "jenkins-lint",
                "jenkins-lint.exe",
            ):
                return base, z.read(name)
    raise ValueError(f"no jenkins-lint binary in {wheel.name}")


def archive(
    target: str, name: str, data: bytes, license_text: bytes
) -> "tuple[str, bytes]":
    """A .zip for Windows, a .tar.gz with an executable binary elsewhere."""
    buf = io.BytesIO()
    if target.endswith("windows-msvc"):
        with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
            z.writestr(name, data)
            z.writestr("LICENSE", license_text)
        return f"jenkins-lint-{target}.zip", buf.getvalue()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        for member, content, mode in (
            (name, data, 0o755),
            ("LICENSE", license_text, 0o644),
        ):
            info = tarfile.TarInfo(member)
            info.size = len(content)
            info.mode = mode
            tar.addfile(info, io.BytesIO(content))
    return f"jenkins-lint-{target}.tar.gz", buf.getvalue()


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--wheels", type=Path, required=True)
    ap.add_argument("--out", type=Path, required=True)
    ap.add_argument("--docker-context", type=Path)
    args = ap.parse_args()

    license_text = (REPO / "LICENSE").read_bytes()
    binaries: Dict[str, "tuple[str, bytes]"] = {}
    for wheel in sorted(args.wheels.rglob("*.whl")):
        target = target_of(wheel.name)
        if target is None:
            print(f"error: no target for {wheel.name}", file=sys.stderr)
            return 1
        if target in binaries:
            print(f"error: two wheels for {target}", file=sys.stderr)
            return 1
        binaries[target] = binary_of(wheel)

    missing = sorted(set(TARGETS.values()) - binaries.keys())
    if missing:
        print(f"error: no wheel for {', '.join(missing)}", file=sys.stderr)
        return 1

    args.out.mkdir(parents=True, exist_ok=True)
    for target, (name, data) in sorted(binaries.items()):
        file_name, content = archive(target, name, data, license_text)
        (args.out / file_name).write_bytes(content)
        digest = hashlib.sha256(content).hexdigest()
        (args.out / f"{file_name}.sha256").write_text(
            f"{digest}  {file_name}\n", encoding="utf-8"
        )
        print(f"{file_name}  {digest}")

    if args.docker_context:
        for platform, target in DOCKER.items():
            dest = args.docker_context / platform / "jenkins-lint"
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(binaries[target][1])
            dest.chmod(0o755)
    return 0


if __name__ == "__main__":
    sys.exit(main())
