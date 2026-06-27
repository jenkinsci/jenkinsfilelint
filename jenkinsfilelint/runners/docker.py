#!/usr/bin/env python3
"""Docker-based validation runner.

Runs the Jenkinsfile Runner ``lint`` command inside a Docker container,
using the same Pipeline Model Definition parser that Jenkins uses.
"""

import os
import shutil
import subprocess
import tempfile
from typing import Tuple, Optional
from .base import ValidationRunner


class DockerRunner(ValidationRunner):
    """Validate Jenkinsfiles via Jenkinsfile Runner in a Docker container."""

    name = "docker"
    description = "Local Docker container with Jenkinsfile Runner (no server needed)"

    #: Default Docker image.
    DEFAULT_IMAGE = "jenkins/jenkinsfile-runner"

    def __init__(self, image: Optional[str] = None):
        #: Docker image to use.
        self.image = image or os.environ.get("JFR_DOCKER_IMAGE") or self.DEFAULT_IMAGE

    def validate(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        if not shutil.which("docker"):
            return (
                False,
                "Docker is not available. Install Docker or use a different runner "
                "(e.g. --runner jenkins).",
            )

        temp_dir = tempfile.mkdtemp(prefix="jenkinsfilelint-")
        try:
            try:
                shutil.copy2(jenkinsfile_path, os.path.join(temp_dir, "Jenkinsfile"))
            except IOError as e:
                return False, f"Error reading file: {e}"

            try:
                result = subprocess.run(
                    [
                        "docker",
                        "run",
                        "--rm",
                        "-v",
                        f"{temp_dir}:/workspace",
                        self.image,
                        "lint",
                    ],
                    capture_output=True,
                    text=True,
                    timeout=300,
                )
            except FileNotFoundError:
                return (
                    False,
                    "Docker binary not found. "
                    "Install Docker or use a different runner.",
                )
            except subprocess.TimeoutExpired:
                return (
                    False,
                    "Jenkinsfile Runner timed out (300 s). "
                    "The image may need pulling first: "
                    f"docker pull {self.image}",
                )

            stderr = (result.stderr or "").strip()
            if "Cannot connect to the Docker daemon" in stderr:
                return False, "Cannot connect to Docker daemon. Is Docker running?"
            if "Unable to find image" in stderr and "locally" in stderr:
                return (
                    False,
                    f"Docker image '{self.image}' not found. "
                    f"Run 'docker pull {self.image}' first.",
                )

            if result.returncode == 0:
                return True, "Jenkinsfile successfully validated"

            # Extract meaningful errors from lint output
            lines = (result.stdout or "").splitlines()
            errors = [
                line
                for line in lines
                if line.strip() and "Linting" not in line and "Done" not in line
            ]
            if errors:
                return False, "Validation errors:\n" + "\n".join(errors)
            if stderr:
                return False, stderr
            return False, "Jenkinsfile validation failed"

        finally:
            shutil.rmtree(temp_dir, ignore_errors=True)
