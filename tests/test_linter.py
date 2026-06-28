#!/usr/bin/env python3
"""Tests for the JenkinsfileLinter class and its runners."""

import os
import subprocess
import tempfile
from unittest.mock import patch, Mock

import pytest

from jenkinsfilelint.linter import JenkinsfileLinter
from jenkinsfilelint.runners import registry
from jenkinsfilelint.runners.jenkins_api import JenkinsApiRunner
from jenkinsfilelint.runners.docker import DockerRunner


# ===========================================================================
# Facade
# ===========================================================================


class TestJenkinsfileLinterInit:
    """Test facade initialization and runner dispatch."""

    def test_default_runner_is_jenkins(self):
        linter = JenkinsfileLinter()
        assert linter.runner_name == "jenkins"
        assert isinstance(linter._runner, JenkinsApiRunner)

    def test_runner_selection(self):
        linter = JenkinsfileLinter(runner="docker")
        assert linter.runner_name == "docker"
        assert isinstance(linter._runner, DockerRunner)

    def test_unknown_runner(self):
        with pytest.raises(ValueError, match="Unknown runner"):
            JenkinsfileLinter(runner="nonexistent")

    def test_runner_kwargs_forwarded(self):
        """Runner-specific kwargs like docker image are forwarded."""
        linter = JenkinsfileLinter(runner="docker", image="my-img:tag")
        assert linter._runner.image == "my-img:tag"

    @patch.dict(os.environ, {"JENKINS_URL": "https://env.example.com"})
    def test_jenkins_url_from_env(self):
        linter = JenkinsfileLinter(runner="jenkins")
        assert linter._runner.jenkins_url == "https://env.example.com"

    @patch.dict(os.environ, {"JFR_DOCKER_IMAGE": "env/jfr:latest"})
    def test_docker_image_from_env(self):
        linter = JenkinsfileLinter(runner="docker")
        assert linter._runner.image == "env/jfr:latest"


class TestJenkinsfileLinterValidate:
    """Test the facade's validate method."""

    def test_file_not_found(self):
        linter = JenkinsfileLinter()
        ok, msg = linter.validate("/nonexistent/file.groovy")
        assert ok is False
        assert "File not found" in msg

    def test_dispatches_to_runner(self):
        """validate() delegates to the runner's validate()."""
        linter = JenkinsfileLinter(runner="docker")

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            with patch.object(linter._runner, "validate") as mock_v:
                mock_v.return_value = (True, "ok")
                ok, msg = linter.validate(p)
                assert ok is True
                mock_v.assert_called_once_with(p)
        finally:
            os.unlink(p)


# ===========================================================================
# JenkinsApiRunner
# ===========================================================================


class TestJenkinsApiRunner:
    """Test validation using Jenkins API."""

    def test_no_url(self):
        r = JenkinsApiRunner()
        ok, msg = r.validate("/some/file")
        assert ok is False
        assert "Jenkins URL not provided" in msg

    def test_file_read_error(self):
        r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
        ok, msg = r.validate("/nonexistent/file.groovy")
        assert ok is False
        assert "Error reading file" in msg

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_success_text(self, mock_post):
        mock_resp = Mock(status_code=200, text="ok")
        mock_resp.json.side_effect = ValueError("Not JSON")
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is True
            # Text response says "ok", no error indicators → passes
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_success_json(self, mock_post):
        mock_resp = Mock(status_code=200)
        mock_resp.json.return_value = {"status": "ok"}
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is True
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_validation_error(self, mock_post):
        mock_resp = Mock(status_code=200)
        mock_resp.json.return_value = {
            "status": "error",
            "data": {"errors": ["line 5: Expected stage"]},
        }
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("invalid")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is False
            assert "Expected stage" in msg
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_http_error(self, mock_post):
        from requests.exceptions import HTTPError

        mock_resp = Mock(status_code=401)
        mock_resp.raise_for_status.side_effect = HTTPError("401")
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is False
            assert "401" in msg
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_connection_error(self, mock_post):
        from requests.exceptions import ConnectionError

        mock_post.side_effect = ConnectionError("refused")

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is False
            assert "refused" in msg
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_auth_passed(self, mock_post):
        mock_resp = Mock(status_code=200)
        mock_resp.json.return_value = {"status": "ok"}
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(
                jenkins_url="https://jenkins.example.com",
                username="u",
                token="t",
            )
            r.validate(p)
            _, kwargs = mock_post.call_args
            assert kwargs["auth"] == ("u", "t")
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_workflowscript_error(self, mock_post):
        text = (
            "WorkflowScript: 3: Expected a stage @ line 3, column 1.\n"
            "    stages {\n    ^\n"
        )
        mock_resp = Mock(status_code=200, text=text)
        mock_resp.json.side_effect = ValueError("Not JSON")
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is False
            assert "WorkflowScript" in msg
        finally:
            os.unlink(p)

    @patch("jenkinsfilelint.runners.jenkins_api.requests.post")
    def test_json_error_no_errors_list(self, mock_post):
        """JSON response where status is not 'ok' and 'errors' key is missing."""
        mock_resp = Mock(status_code=200)
        mock_resp.json.return_value = {"status": "error", "message": "Something broke"}
        mock_resp.raise_for_status = Mock()
        mock_post.return_value = mock_resp

        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            f.flush()
            p = f.name
        try:
            r = JenkinsApiRunner(jenkins_url="https://jenkins.example.com")
            ok, msg = r.validate(p)
            assert ok is False
            assert "Something broke" in msg
        finally:
            os.unlink(p)


# ===========================================================================
# DockerRunner
# ===========================================================================


class TestDockerRunner:
    """Test Docker-based validation."""

    def test_default_image(self):
        with patch.dict(os.environ, {}, clear=True):
            r = DockerRunner()
            assert r.image == DockerRunner.DEFAULT_IMAGE

    def test_image_from_env(self):
        with patch.dict(os.environ, {"JFR_DOCKER_IMAGE": "my/img:tag"}):
            r = DockerRunner()
            assert r.image == "my/img:tag"

    def test_image_explicit(self):
        r = DockerRunner(image="explicit:tag")
        assert r.image == "explicit:tag"

    @patch("shutil.which", return_value="/usr/bin/docker")
    def test_file_not_found(self, mock_which):
        r = DockerRunner()
        ok, msg = r.validate("/nonexistent")
        assert ok is False
        assert "Error reading file" in msg

    @patch("shutil.which", return_value="/usr/bin/docker")
    def test_file_read_error(self, mock_which):
        ok, msg = DockerRunner().validate("/nonexistent/file.groovy")
        assert ok is False
        assert "Error reading file" in msg

    def test_docker_not_available(self):
        with patch("shutil.which", return_value=None):
            r = DockerRunner()
            with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
                f.write("pipeline { }")
                p = f.name
            try:
                ok, msg = r.validate(p)
                assert ok is False
                assert "Docker is not available" in msg
            finally:
                os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_success(self, mock_run, _):
        # First call: docker image inspect (image exists)
        # Second call: docker run (valid)
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(
                returncode=0, stdout="Linting...\nDone\n", stderr=""
            ),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is True
            assert "successfully validated" in msg
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_syntax_errors(self, mock_run, _):
        # First call: inspect (image exists), second call: docker run (errors)
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(
                returncode=1,
                stdout="Linting...\nWorkflowScript: 3: Expected a stage @ line 3, column 1.\n",
                stderr="",
            ),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is False
            assert "Expected a stage" in msg
            assert "Linting" not in msg  # filtered out
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_stderr_error(self, mock_run, _):
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(
                returncode=1, stdout="Linting...\nDone\n", stderr="ERROR: something broke"
            ),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("broken")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is False
            assert "ERROR:" in msg
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_fallback_message(self, mock_run, _):
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(returncode=1, stdout="", stderr=""),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("?")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is False
            assert "failed" in msg
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_custom_image(self, mock_run, _):
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(
                returncode=0, stdout="Linting...\nDone\n", stderr=""
            ),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            p = f.name
        try:
            DockerRunner(image="custom/img:v1").validate(p)
            # Second call should be the docker run command
            args = mock_run.call_args_list[1][0][0]
            assert "custom/img:v1" in args
            assert "--workdir" in args
            assert "/workspace" in args
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    def test_daemon_not_running(self, mock_which):
        with patch("subprocess.run") as mock_run:
            mock_run.side_effect = FileNotFoundError()
            with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
                f.write("pipeline { }")
                p = f.name
            try:
                ok, msg = DockerRunner().validate(p)
                assert ok is False
                assert "Docker binary" in msg
            finally:
                os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    def test_timeout(self, mock_which):
        with patch("subprocess.run") as mock_run:
            # inspect times out → handled with a "daemon not responding" message
            mock_run.side_effect = subprocess.TimeoutExpired("docker image inspect", 30)
            with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
                f.write("pipeline { }")
                p = f.name
            try:
                ok, msg = DockerRunner().validate(p)
                assert ok is False
                assert "daemon did not respond" in msg.lower()
            finally:
                os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_daemon_connection_error(self, mock_run, mock_which):
        """Docker installed but daemon not running."""
        # First call: inspect succeeds (image exists)
        # Second call: docker run fails with daemon error
        mock_run.side_effect = [
            Mock(returncode=0, stdout="", stderr=""),  # inspect
            Mock(
                returncode=1,
                stdout="",
                stderr="Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?",
            ),  # run
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is False
            assert "Cannot connect to Docker daemon" in msg
        finally:
            os.unlink(p)

    @patch("shutil.which", return_value="/usr/bin/docker")
    @patch("subprocess.run")
    def test_image_not_found_and_pull_fails(self, mock_run, mock_which):
        """Docker image not available locally and pull fails."""
        # First call: inspect fails (image not found)
        # Second call: pull fails
        mock_run.side_effect = [
            Mock(returncode=1, stdout="", stderr=""),  # inspect: not found
            Mock(
                returncode=1,
                stdout="",
                stderr="Error response from daemon: pull access denied",
            ),  # pull: failed
        ]
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("pipeline { }")
            p = f.name
        try:
            ok, msg = DockerRunner().validate(p)
            assert ok is False
            assert "not found" in msg.lower()
            assert "pull failed" in msg.lower()
            assert "pull access denied" in msg
        finally:
            os.unlink(p)


# ===========================================================================
# Registry
# ===========================================================================


class TestRunnerRegistry:
    """Test auto-discovery in the runner registry."""

    def test_names_include_builtin_runners(self):
        names = registry.names()
        assert "jenkins" in names
        assert "docker" in names

    def test_get_jenkins_runner(self):
        r = registry.get("jenkins")
        assert isinstance(r, JenkinsApiRunner)

    def test_get_docker_runner(self):
        r = registry.get("docker")
        assert isinstance(r, DockerRunner)

    def test_get_unknown_raises(self):
        with pytest.raises(KeyError):
            registry.get("nonexistent")

    def test_help_text(self):
        text = registry.help_text()
        assert "jenkins" in text
        assert "docker" in text
        assert "remote" in text.lower() or "requires" in text.lower()
