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

    def _ensure_image(self) -> Optional[str]:
        """Check if the Docker image exists locally; pull it if missing.

        Returns:
            ``None`` on success, or an error message string on failure.
        """
        # Check if the image exists locally
        try:
            inspect_result = subprocess.run(
                ["docker", "image", "inspect", self.image],
                capture_output=True,
                text=True,
                timeout=30,
            )
            if inspect_result.returncode == 0:
                return None  # Image exists
        except FileNotFoundError:
            return (
                "Docker binary not found. "
                "Install Docker or use a different runner."
            )
        except subprocess.TimeoutExpired:
            return (
                "Docker daemon did not respond when checking for the image. "
                "Is Docker running?"
            )

        # Image not found locally — try to pull it
        try:
            pull_result = subprocess.run(
                ["docker", "pull", self.image],
                capture_output=True,
                text=True,
                timeout=120,
            )
        except FileNotFoundError:
            return (
                "Docker binary not found. "
                "Install Docker or use a different runner."
            )
        except subprocess.TimeoutExpired:
            return (
                f"Timed out pulling Docker image '{self.image}' (120 s). "
                f"Check your network connection or try manually: "
                f"docker pull {self.image}"
            )

        if pull_result.returncode != 0:
            stderr = (pull_result.stderr or "").strip()
            msg = (
                f"Docker image '{self.image}' not found and pull failed."
            )
            if stderr:
                msg += f"\n{stderr}"
            msg += f"\nTry manually: docker pull {self.image}"
            return msg

        return None  # Successfully pulled

    def validate(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        if not shutil.which("docker"):
            return (
                False,
                "Docker is not available. Install Docker or use a different runner "
                "(e.g. --runner jenkins).",
            )

        # Ensure the Docker image is available (auto-pull if needed)
        err = self._ensure_image()
        if err is not None:
            return False, err

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
                        "--workdir",
                        "/workspace",
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
