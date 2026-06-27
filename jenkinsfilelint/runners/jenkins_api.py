#!/usr/bin/env python3
"""Jenkins API validation runner.

Sends the Jenkinsfile to an existing Jenkins server's
``/pipeline-model-converter/validate`` endpoint.
"""

import os
from typing import Tuple, Optional
import requests
from .base import ValidationRunner


class JenkinsApiRunner(ValidationRunner):
    """Validate Jenkinsfiles by POSTing to a remote Jenkins server."""

    name = "jenkins"
    description = "Remote Jenkins API (requires a running Jenkins server)"

    def __init__(
        self,
        jenkins_url: Optional[str] = None,
        username: Optional[str] = None,
        token: Optional[str] = None,
    ):
        #: Jenkins server URL (from arg, env var, or ``None``).
        self.jenkins_url = jenkins_url or os.environ.get("JENKINS_URL")
        #: Jenkins username for basic auth.
        self.username = username or os.environ.get("JENKINS_USER")
        #: Jenkins API token for basic auth.
        self.token = token or os.environ.get("JENKINS_TOKEN")

    def validate(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        if not self.jenkins_url:
            return (
                False,
                "Jenkins URL not provided for the 'jenkins' runner. "
                "Set JENKINS_URL or pass --jenkins-url.",
            )

        # Read file
        try:
            with open(jenkinsfile_path, "r", encoding="utf-8") as f:
                jenkinsfile_content = f.read()
        except IOError as e:
            return False, f"Error reading file: {e}"

        url = f"{self.jenkins_url.rstrip('/')}/pipeline-model-converter/validate"
        auth = (self.username, self.token) if self.username and self.token else None

        try:
            resp = requests.post(
                url, data={"jenkinsfile": jenkinsfile_content}, auth=auth, timeout=30
            )
            resp.raise_for_status()
        except requests.exceptions.RequestException as e:
            return False, f"Error connecting to Jenkins: {e}"

        # Parse JSON response
        try:
            body = resp.json()
        except ValueError:
            body = None

        if isinstance(body, dict):
            if body.get("status") == "ok":
                return True, "Jenkinsfile successfully validated"

            errors = body.get("data", {}).get("errors", [])
            if errors:
                return False, "Validation errors:\n" + "\n".join(str(e) for e in errors)
            return False, str(body)

        # Fallback: text parsing
        text = resp.text.strip()
        error_hints = [
            "Errors",
            "error",
            "No Jenkinsfile specified",
            "WorkflowScript:",
            "Expected",
            "unexpected token",
            "unable to resolve class",
        ]
        if any(h in text for h in error_hints):
            return False, text
        return True, text
