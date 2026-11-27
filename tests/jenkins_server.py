#!/usr/bin/env python3
"""A real Jenkins in Docker, for development only.

The Docker integration tests and corpus/compare.py check against Jenkins's
own validator. This builds the image in tests/jenkins-server/ and runs it on localhost.
jenkins-lint itself does not use Docker.

Usage:
    python3 tests/jenkins_server.py start   # prints the URL
    python3 tests/jenkins_server.py stop
"""

import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator

DOCKER_DIR = Path(__file__).resolve().parent / "jenkins-server"
IMAGE = "jenkins-lint-test-jenkins"
NAME = "jenkins-lint-test-jenkins"
PORT = int(os.environ.get("JENKINS_LINT_TEST_PORT", "18080"))
URL = f"http://127.0.0.1:{PORT}"
READY_TIMEOUT = 180  # seconds; a cold boot on a slow CI runner


def runtime() -> str:
    for name in ("docker", "podman"):
        if shutil.which(name):
            return name
    raise RuntimeError("Neither docker nor podman is installed")


def _ready() -> bool:
    try:
        with urllib.request.urlopen(f"{URL}/login", timeout=5) as resp:
            return resp.status == 200
    except (urllib.error.URLError, ConnectionError, OSError):
        return False


def start() -> str:
    """Build the image, start the container and wait until Jenkins answers."""
    if _ready():
        return URL
    rt = runtime()
    subprocess.run([rt, "build", "-t", IMAGE, str(DOCKER_DIR)], check=True)
    subprocess.run([rt, "rm", "--force", NAME], capture_output=True)
    subprocess.run(
        [rt, "run", "--detach", "--name", NAME]
        + ["--publish", f"127.0.0.1:{PORT}:8080", IMAGE],
        check=True,
        capture_output=True,
    )
    deadline = time.monotonic() + READY_TIMEOUT
    while time.monotonic() < deadline:
        if _ready():
            return URL
        time.sleep(2)
    logs = subprocess.run([rt, "logs", "--tail", "50", NAME], capture_output=True)
    raise RuntimeError(
        f"Jenkins did not start in {READY_TIMEOUT}s:\n{logs.stdout.decode()}"
    )


def stop() -> None:
    subprocess.run([runtime(), "rm", "--force", NAME], capture_output=True)


@contextmanager
def running() -> Iterator[str]:
    """Start Jenkins for the duration of the block, then remove it."""
    try:
        yield start()
    finally:
        stop()


if __name__ == "__main__":
    action = sys.argv[1] if len(sys.argv) > 1 else ""
    if action == "start":
        print(start())
    elif action == "stop":
        stop()
    else:
        sys.exit(__doc__)
