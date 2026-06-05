#!/usr/bin/env python3
"""Tests for the JenkinsfileLinter class."""

import os
import tempfile
from unittest.mock import patch, Mock
from jenkinsfilelint.linter import JenkinsfileLinter


class TestJenkinsfileLinterInit:
    """Test JenkinsfileLinter initialization."""

    def test_init_with_parameters(self):
        """Test initialization with explicit parameters."""
        linter = JenkinsfileLinter(
            jenkins_url="https://jenkins.example.com",
            username="testuser",
            token="testtoken",
        )
        assert linter.jenkins_url == "https://jenkins.example.com"
        assert linter.username == "testuser"
        assert linter.token == "testtoken"

    def test_init_with_env_vars(self):
        """Test initialization with environment variables."""
        with patch.dict(
            os.environ,
            {
                "JENKINS_URL": "https://jenkins.env.com",
                "JENKINS_USER": "envuser",
                "JENKINS_TOKEN": "envtoken",
            },
        ):
            linter = JenkinsfileLinter()
            assert linter.jenkins_url == "https://jenkins.env.com"
            assert linter.username == "envuser"
            assert linter.token == "envtoken"

    def test_init_parameters_override_env_vars(self):
        """Test that explicit parameters override environment variables."""
        with patch.dict(
            os.environ,
            {
                "JENKINS_URL": "https://jenkins.env.com",
                "JENKINS_USER": "envuser",
                "JENKINS_TOKEN": "envtoken",
            },
        ):
            linter = JenkinsfileLinter(
                jenkins_url="https://jenkins.param.com",
                username="paramuser",
                token="paramtoken",
            )
            assert linter.jenkins_url == "https://jenkins.param.com"
            assert linter.username == "paramuser"
            assert linter.token == "paramtoken"

    def test_init_with_no_credentials(self):
        """Test initialization without any credentials."""
        with patch.dict(os.environ, {}, clear=True):
            linter = JenkinsfileLinter()
            assert linter.jenkins_url is None
            assert linter.username is None
            assert linter.token is None


class TestJenkinsfileLinterValidateWithJenkins:
    """Test validation using Jenkins API."""

    def test_validate_without_jenkins_url(self):
        """Test validation when Jenkins URL is not set."""
        linter = JenkinsfileLinter()
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            temp_path = f.name

        try:
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Jenkins URL not provided" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_successful_with_text_response(self, mock_post):
        """Test successful validation with text response."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Jenkinsfile successfully validated"
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is True
            assert "successfully validated" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_successful_with_json_response(self, mock_post):
        """Test successful validation with JSON response."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"status": "ok"}
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is True
            assert "successfully validated" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_with_errors_in_json(self, mock_post):
        """Test validation with errors in JSON response."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "status": "error",
            "data": {"errors": ["Error 1", "Error 2"]},
        }
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("invalid jenkinsfile")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error 1" in message
            assert "Error 2" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_with_json_error_no_error_list(self, mock_post):
        """Test validation with JSON error response but no errors list."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "status": "error",
            "message": "Something went wrong",
        }
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("invalid jenkinsfile")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "error" in message.lower()
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_with_errors_in_text(self, mock_post):
        """Test validation with errors in text response."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = "Errors encountered in validation"
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("invalid jenkinsfile")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Errors" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_with_authentication(self, mock_post):
        """Test validation with authentication."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"status": "ok"}
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(
                jenkins_url="https://jenkins.example.com",
                username="user",
                token="token",
            )
            linter._validate_with_jenkins(temp_path)

            # Check that auth was passed to requests.post and data (not files) was used
            mock_post.assert_called_once()
            call_kwargs = mock_post.call_args[1]
            assert call_kwargs["auth"] == ("user", "token")
            assert "data" in call_kwargs
            assert "jenkinsfile" in call_kwargs["data"]
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_connection_error(self, mock_post):
        """Test validation when connection to Jenkins fails."""
        import requests

        mock_post.side_effect = requests.exceptions.RequestException(
            "Connection refused"
        )

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
        finally:
            os.unlink(temp_path)

    def test_validate_with_jenkins_file_read_error(self):
        """Test validation when file cannot be read."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        is_valid, message = linter._validate_with_jenkins("/nonexistent/file.groovy")
        assert is_valid is False
        assert "Error reading file" in message


class TestJenkinsfileLinterHTTPErrors:
    """Test HTTP error responses from Jenkins."""

    @patch("requests.Session.post")
    def test_validate_http_401(self, mock_post):
        """Test validation when Jenkins returns HTTP 401 Unauthorized."""
        from requests.exceptions import HTTPError

        mock_response = Mock()
        mock_response.status_code = 401
        mock_response.raise_for_status.side_effect = HTTPError(
            "401 Client Error: Unauthorized", response=mock_response
        )
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
            assert "401" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_http_403(self, mock_post):
        """Test validation when Jenkins returns HTTP 403 Forbidden."""
        from requests.exceptions import HTTPError

        mock_response = Mock()
        mock_response.status_code = 403
        mock_response.raise_for_status.side_effect = HTTPError(
            "403 Client Error: Forbidden", response=mock_response
        )
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
            assert "403" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_http_500(self, mock_post):
        """Test validation when Jenkins returns HTTP 500 Internal Server Error."""
        from requests.exceptions import HTTPError

        mock_response = Mock()
        mock_response.status_code = 500
        mock_response.raise_for_status.side_effect = HTTPError(
            "500 Server Error: Internal Server Error", response=mock_response
        )
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterTimeout:
    """Test timeout scenarios."""

    @patch("requests.Session.post")
    def test_validate_timeout(self, mock_post):
        """Test validation when Jenkins request times out."""
        import requests

        mock_post.side_effect = requests.exceptions.Timeout("Connection timed out")

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
            assert "timed out" in message.lower()
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_connection_timeout(self, mock_post):
        """Test validation when connection times out during connect phase."""
        from requests.exceptions import ConnectTimeout

        mock_post.side_effect = ConnectTimeout("Connection timed out")

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Error connecting to Jenkins" in message
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterGroovyErrors:
    """Test Groovy compilation error responses from Jenkins."""

    @patch("requests.Session.post")
    def test_validate_workflowscript_error(self, mock_post):
        """Test validation with Groovy WorkflowScript compilation error."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = (
            "WorkflowScript: 12: Expected a stage @ line 12, column 1.\n"
            "   stages {\n"
            "   ^\n"
            "1 error"
        )
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "WorkflowScript" in message
            assert "12" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_unexpected_token_error(self, mock_post):
        """Test validation with Groovy unexpected token error."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = (
            "WorkflowScript: 5: unexpected token: } @ line 5, column 1.\n"
        )
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "unexpected token" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_unable_to_resolve_class_error(self, mock_post):
        """Test validation with unresolved class error."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = (
            "WorkflowScript: 3: unable to resolve class MyCustomClass\n"
        )
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("def x = new MyCustomClass()")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "unable to resolve class" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_expected_error(self, mock_post):
        """Test validation with syntax 'Expected' error pattern."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.text = (
            "WorkflowScript: 8: Expected a symbol @ line 8, column 5.\n"
        )
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any stages {} }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "Expected" in message
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterJSONEdgeCases:
    """Test edge cases in JSON response parsing."""

    @patch("requests.Session.post")
    def test_validate_json_non_dict_response(self, mock_post):
        """Test validation when JSON response is not a dict."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = ["item1", "item2"]  # List, not dict
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            # Non-dict JSON falls through to text parsing; no error indicators
            # mean it's treated as valid
            assert is_valid is True
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_json_status_ok_no_errors_list(self, mock_post):
        """Test JSON with status=ok and extra fields."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "status": "ok",
            "data": {"result": "success"},
        }
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is True
            assert "successfully validated" in message
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_json_status_error_empty_data(self, mock_post):
        """Test JSON error with empty data dict."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "status": "error",
            "data": {},
        }
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            # Falls back to str(result_json) when no errors in data
            assert "error" in message.lower()
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterFileScenarios:
    """Test various file-related validation scenarios."""

    @patch("requests.Session.post")
    def test_validate_with_unicode_content(self, mock_post):
        """Test validation with Jenkinsfile containing Unicode characters."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"status": "ok"}
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False, encoding="utf-8") as f:
            f.write(
                'pipeline { agent any stages { stage("Déploiement") { steps { sh "echo 你好" } } } }'
            )
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is True
            # Verify the content was sent correctly
            call_kwargs = mock_post.call_args[1]
            assert "Déploiement" in call_kwargs["data"]["jenkinsfile"]
            assert "你好" in call_kwargs["data"]["jenkinsfile"]
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_with_empty_file(self, mock_post):
        """Test validation of an empty file via Jenkins."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.text = ""
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            # Empty file with no error indicators → valid with empty result
            assert is_valid is True
            assert message == ""
        finally:
            os.unlink(temp_path)

    @patch("requests.Session.post")
    def test_validate_empty_file_with_jenkins_error(self, mock_post):
        """Test validation of an empty file when Jenkins returns error."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.side_effect = ValueError("Not JSON")
        mock_response.text = "No Jenkinsfile specified"
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter._validate_with_jenkins(temp_path)
            assert is_valid is False
            assert "No Jenkinsfile specified" in message
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterValidate:
    """Test the main validate method."""

    def test_validate_file_not_found(self):
        """Test validation when file does not exist."""
        linter = JenkinsfileLinter()
        is_valid, message = linter.validate("/nonexistent/file.groovy")
        assert is_valid is False
        assert "File not found" in message

    @patch("requests.Session.post")
    def test_validate_with_jenkins_url_uses_jenkins_validation(self, mock_post):
        """Test that Jenkins validation is used when URL is set."""
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"status": "ok"}
        mock_response.raise_for_status = Mock()
        mock_post.return_value = mock_response

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
            is_valid, message = linter.validate(temp_path)
            assert is_valid is True
            # Verify Jenkins API was called
            mock_post.assert_called_once()
        finally:
            os.unlink(temp_path)

    def test_validate_without_jenkins_url_requires_credentials(self):
        """Test that validation fails when Jenkins URL is not set."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

        try:
            linter = JenkinsfileLinter()
            is_valid, message = linter.validate(temp_path)
            assert is_valid is False
            assert "jenkins url not provided" in message.lower()
        finally:
            os.unlink(temp_path)


class TestJenkinsfileLinterGetCrumb:
    """Test the _get_crumb method for CSRF protection."""

    def test_get_crumb_success_standard_field(self):
        """Test successful crumb fetch with standard field name."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "crumb": "abc123crumb",
            "crumbRequestField": "Jenkins-Crumb",
        }
        mock_response.raise_for_status = Mock()
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        assert result == {"Jenkins-Crumb": "abc123crumb"}
        mock_session.get.assert_called_once()

    def test_get_crumb_success_custom_field(self):
        """Test crumb fetch with a custom request field name."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "crumb": "xyz789crumb",
            "crumbRequestField": ".crumb",
        }
        mock_response.raise_for_status = Mock()
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        assert result == {".crumb": "xyz789crumb"}

    def test_get_crumb_empty_value(self):
        """Test crumb fetch when crumb value is empty."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "crumb": "",
            "crumbRequestField": "Jenkins-Crumb",
        }
        mock_response.raise_for_status = Mock()
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        # Empty crumb should return empty dict
        assert result == {}

    def test_get_crumb_missing_crumb_field(self):
        """Test crumb fetch when JSON has no crumb field."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {"crumbRequestField": "Jenkins-Crumb"}
        mock_response.raise_for_status = Mock()
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        assert result == {}

    def test_get_crumb_http_404(self):
        """Test crumb fetch when crumb issuer returns 404 (not available)."""
        import requests as real_requests

        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_response = Mock()
        mock_response.raise_for_status.side_effect = real_requests.exceptions.HTTPError(
            "404 Not Found", response=Mock(status_code=404)
        )
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        # Should gracefully return empty dict
        assert result == {}

    def test_get_crumb_connection_error(self):
        """Test crumb fetch when connection fails."""
        import requests as real_requests

        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_session.get.side_effect = real_requests.exceptions.ConnectionError(
            "Connection refused"
        )

        result = linter._get_crumb(mock_session)

        # Should gracefully return empty dict
        assert result == {}

    def test_get_crumb_timeout(self):
        """Test crumb fetch when request times out."""
        import requests as real_requests

        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")
        mock_session = Mock()
        mock_session.get.side_effect = real_requests.exceptions.Timeout(
            "Connection timed out"
        )

        result = linter._get_crumb(mock_session)

        # Should gracefully return empty dict
        assert result == {}

    def test_get_crumb_with_auth(self):
        """Test crumb fetch passes auth credentials."""
        linter = JenkinsfileLinter(
            jenkins_url="https://jenkins.example.com",
            username="testuser",
            token="testtoken",
        )
        mock_session = Mock()
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "crumb": "auth-crumb",
            "crumbRequestField": "Jenkins-Crumb",
        }
        mock_response.raise_for_status = Mock()
        mock_session.get.return_value = mock_response

        result = linter._get_crumb(mock_session)

        assert result == {"Jenkins-Crumb": "auth-crumb"}
        # Verify auth was passed
        call_kwargs = mock_session.get.call_args[1]
        assert call_kwargs["auth"] == ("testuser", "testtoken")

    def test_validate_includes_crumb_in_post(self):
        """Test that crumb headers are included in the validation POST request."""
        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")

        with (
            tempfile.NamedTemporaryFile(mode="w", delete=False) as f,
            patch("requests.Session.post") as mock_post,
        ):
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

            mock_response = Mock()
            mock_response.status_code = 200
            mock_response.json.return_value = {"status": "ok"}
            mock_response.raise_for_status = Mock()
            mock_post.return_value = mock_response

            is_valid, message = linter._validate_with_jenkins(temp_path)

            # Verify crumb header was passed to POST
            mock_post.assert_called_once()
            call_kwargs = mock_post.call_args[1]
            assert "headers" in call_kwargs
            assert call_kwargs["headers"] == {"Jenkins-Crumb": "mock-crumb"}
            assert is_valid is True

    def test_validate_works_when_crumb_fails(self):
        """Test that validation still works when crumb fetch fails."""
        import requests as real_requests

        linter = JenkinsfileLinter(jenkins_url="https://jenkins.example.com")

        with (
            tempfile.NamedTemporaryFile(mode="w", delete=False) as f,
            patch("requests.Session.get") as mock_get,
            patch("requests.Session.post") as mock_post,
        ):
            f.write("pipeline { agent any }")
            f.flush()
            temp_path = f.name

            # Crumb fetch fails
            mock_get.side_effect = real_requests.exceptions.ConnectionError(
                "Connection refused"
            )

            mock_response = Mock()
            mock_response.status_code = 200
            mock_response.json.return_value = {"status": "ok"}
            mock_response.raise_for_status = Mock()
            mock_post.return_value = mock_response

            is_valid, message = linter._validate_with_jenkins(temp_path)

            # Should still validate successfully (with empty headers)
            mock_post.assert_called_once()
            call_kwargs = mock_post.call_args[1]
            assert call_kwargs["headers"] == {}
            assert is_valid is True
