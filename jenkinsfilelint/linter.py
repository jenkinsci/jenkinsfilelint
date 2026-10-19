#!/usr/bin/env python3
"""Core linter module for validating Jenkinsfiles."""

import os
import requests
from typing import Tuple, Optional

# Jenkins's /pipeline-model-converter/validate endpoint answers in plain text
# with one of these fixed shapes (ModelConverterAction#doValidate in the
# pipeline-model-definition plugin).
_VALIDATED_PREFIX = "Jenkinsfile successfully validated"
_ERRORS_PREFIX = "Errors encountered validating Jenkinsfile:"
_NO_JENKINSFILE = "No Jenkinsfile specified"
_COMPILE_ERROR_PREFIX = "WorkflowScript:"
# Sent when the Groovy parses but has no top-level ``pipeline`` block. Jenkins
# echoes the whole file between the quotes, so only the ends are matched.
_NOT_DECLARATIVE_PREFIX = "Jenkinsfile content '"
_NOT_DECLARATIVE_SUFFIX = "' did not contain the 'pipeline' step"

NOT_DECLARATIVE_MESSAGE = "Skipped: not a Declarative Pipeline (no 'pipeline' block)"


def _parse_text_response(text: str) -> Tuple[bool, str]:
    """Map a plain-text validate response to (is_valid, message).

    The file content echoed back by Jenkins is never searched or returned.
    """
    result = text.strip()

    if result.startswith(_NOT_DECLARATIVE_PREFIX) and result.endswith(
        _NOT_DECLARATIVE_SUFFIX
    ):
        return True, NOT_DECLARATIVE_MESSAGE

    if result.startswith(_VALIDATED_PREFIX):
        return True, result

    if result.startswith((_ERRORS_PREFIX, _NO_JENKINSFILE, _COMPILE_ERROR_PREFIX)):
        return False, result

    # Anything else (an HTML login page from a proxy, an empty body, ...) means
    # the file was not validated, so don't report it as valid.
    first_line = result.splitlines()[0][:200] if result else "<empty response>"
    return False, f"Unexpected response from Jenkins validate endpoint: {first_line}"


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
                return _parse_text_response(response.text)

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

        # Require Jenkins URL
        if not self.jenkins_url:
            return (
                False,
                "Jenkins URL not provided. Set JENKINS_URL environment variable or pass --jenkins-url.",
            )

        return self._validate_with_jenkins(jenkinsfile_path)
