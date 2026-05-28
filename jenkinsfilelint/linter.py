#!/usr/bin/env python3
"""Core linter module for validating Jenkinsfiles."""

import os
import requests
from typing import Tuple, Optional


class JenkinsfileLinter:
    """Linter for validating Jenkinsfiles using Jenkins API."""

    def __init__(
        self,
        jenkins_url: Optional[str] = None,
        username: Optional[str] = None,
        token: Optional[str] = None,
    ):
        """Initialize the linter.

        Args:
            jenkins_url: Jenkins server URL (optional, can be set via JENKINS_URL env var)
            username: Jenkins username (optional, can be set via JENKINS_USER env var)
            token: Jenkins API token (optional, can be set via JENKINS_TOKEN env var)
        """
        self.jenkins_url = jenkins_url or os.environ.get("JENKINS_URL")
        self.username = username or os.environ.get("JENKINS_USER")
        self.token = token or os.environ.get("JENKINS_TOKEN")

    def _validate_with_jenkins(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        """Validate Jenkinsfile using Jenkins API.

        Args:
            jenkinsfile_path: Path to the Jenkinsfile

        Returns:
            Tuple of (is_valid, message)
        """
        if not self.jenkins_url:
            return (
                False,
                "Jenkins URL not provided. Set JENKINS_URL environment variable or pass --jenkins-url.",
            )

        # Read the Jenkinsfile content
        try:
            with open(jenkinsfile_path, "r", encoding="utf-8") as f:
                jenkinsfile_content = f.read()
        except IOError as e:
            return False, f"Error reading file: {e}"

        # Prepare the validation endpoint
        validation_url = (
            f"{self.jenkins_url.rstrip('/')}/pipeline-model-converter/validate"
        )

        # Prepare authentication
        auth = None
        if self.username and self.token:
            auth = (self.username, self.token)

        try:
            # Send validation request
            # Jenkins expects 'jenkinsfile' as form data, not a file upload
            data = {"jenkinsfile": jenkinsfile_content}
            response = requests.post(validation_url, data=data, auth=auth, timeout=30)

            # Check response
            response.raise_for_status()

            # Try to parse as JSON first for structured error handling
            try:
                result_json = response.json()
                # If JSON response, check for errors in structured format
                if isinstance(result_json, dict):
                    if result_json.get("status") == "ok":
                        return True, "Jenkinsfile successfully validated"
                    else:
                        # Extract error messages if available
                        errors = result_json.get("data", {}).get("errors", [])
                        if errors:
                            error_msg = "\n".join([str(err) for err in errors])
                            return False, f"Validation errors:\n{error_msg}"
                        # If no errors list but status is not ok, return the whole response
                        return False, str(result_json)
                else:
                    # JSON response is not a dict (e.g., a list), treat as valid if no errors
                    return True, str(result_json)
            except ValueError:
                # Not JSON, fall back to text parsing
                result = response.text.strip()

                # Check if there are errors in the response
                # Common error patterns from Jenkins
                error_indicators = [
                    "Errors",
                    "error",
                    "No Jenkinsfile specified",
                    "WorkflowScript:",  # Groovy compilation errors
                    "Expected",  # Syntax error patterns
                    "unexpected token",
                    "unable to resolve class",
                ]

                if any(indicator in result for indicator in error_indicators):
                    return False, result
                else:
                    return True, result

        except requests.exceptions.RequestException as e:
            return False, f"Error connecting to Jenkins: {e}"

    def validate(self, jenkinsfile_path: str) -> Tuple[bool, str]:
        """Validate a Jenkinsfile.

        Args:
            jenkinsfile_path: Path to the Jenkinsfile

        Returns:
            Tuple of (is_valid, message)
        """
        # Check if file exists
        if not os.path.isfile(jenkinsfile_path):
            return False, f"File not found: {jenkinsfile_path}"

        # Require Jenkins credentials
        if not self.jenkins_url:
            return (
                False,
                "Jenkins credentials required. Set JENKINS_URL, JENKINS_USER, and JENKINS_TOKEN environment variables or use --jenkins-url, --username, and --token arguments.",
            )

        return self._validate_with_jenkins(jenkinsfile_path)
