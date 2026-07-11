#!/usr/bin/env python3
"""Tests for the local Docker lifecycle manager."""

import os
import json
import subprocess
import sys
from unittest.mock import patch, Mock, call, PropertyMock

import pytest

from jenkinsfilelint.local import (
    LocalJenkins,
    handle_server_command,
    _find_runtime,
    _container_id,
    _container_is_running,
    _start_container,
    _stop_container,
    _wait_for_jenkins,
    CONTAINER_LABEL,
    CONTAINER_NAME,
    DEFAULT_PORT,
    READY_TIMEOUT,
)


# ---------------------------------------------------------------------------
# _find_runtime
# ---------------------------------------------------------------------------


class TestFindRuntime:
    """Test container runtime discovery."""

    @patch("jenkinsfilelint.local.shutil.which")
    def test_finds_docker(self, mock_which):
        """Should return 'docker' when it's on PATH."""
        mock_which.side_effect = lambda x: f"/usr/bin/{x}" if x == "docker" else None
        assert _find_runtime() == "/usr/bin/docker"

    @patch("jenkinsfilelint.local.shutil.which")
    def test_falls_back_to_podman(self, mock_which):
        """Should return 'podman' when docker is not on PATH."""
        mock_which.side_effect = lambda x: "/usr/bin/podman" if x == "podman" else None
        assert _find_runtime() == "/usr/bin/podman"

    @patch("jenkinsfilelint.local.shutil.which")
    def test_returns_none_when_neither_found(self, mock_which):
        """Should return None when neither docker nor podman is found."""
        mock_which.return_value = None
        assert _find_runtime() is None


# ---------------------------------------------------------------------------
# _container_id
# ---------------------------------------------------------------------------


class TestContainerID:
    """Test container ID lookup."""

    @patch("jenkinsfilelint.local._run")
    def test_returns_id_when_running(self, mock_run):
        """Should return the container ID when a container is running."""
        mock_result = Mock()
        mock_result.stdout = "abc123\n"
        mock_run.return_value = mock_result

        result = _container_id("docker", label="test-label")
        assert result == "abc123"
        mock_run.assert_called_once_with(
            ["docker", "ps", "--filter", "label=test-label", "--format", "{{.ID}}"],
            check=False,
        )

    @patch("jenkinsfilelint.local._run")
    def test_returns_none_when_no_container(self, mock_run):
        """Should return None when no container is running."""
        mock_result = Mock()
        mock_result.stdout = "\n"
        mock_run.return_value = mock_result

        result = _container_id("docker")
        assert result is None

    @patch("jenkinsfilelint.local._run")
    def test_returns_none_on_error(self, mock_run):
        """Should return None when the docker command fails."""
        mock_run.side_effect = subprocess.TimeoutExpired(cmd="docker", timeout=5)

        result = _container_id("docker")
        assert result is None


# ---------------------------------------------------------------------------
# _container_is_running
# ---------------------------------------------------------------------------


class TestContainerIsRunning:
    """Test container running state check."""

    @patch("jenkinsfilelint.local._run")
    def test_returns_true_when_running(self, mock_run):
        """Should return True when container status is 'running'."""
        mock_result = Mock()
        mock_result.stdout = "running\n"
        mock_run.return_value = mock_result

        assert _container_is_running("docker", "abc123") is True

    @patch("jenkinsfilelint.local._run")
    def test_returns_false_when_not_running(self, mock_run):
        """Should return False when container status is not 'running'."""
        mock_result = Mock()
        mock_result.stdout = "exited\n"
        mock_run.return_value = mock_result

        assert _container_is_running("docker", "abc123") is False

    @patch("jenkinsfilelint.local._run")
    def test_returns_false_on_error(self, mock_run):
        """Should return False when the docker command fails."""
        mock_run.side_effect = subprocess.TimeoutExpired(cmd="docker", timeout=5)

        assert _container_is_running("docker", "abc123") is False


# ---------------------------------------------------------------------------
# _start_container
# ---------------------------------------------------------------------------


class TestStartContainer:
    """Test container startup."""

    @patch("jenkinsfilelint.local._run")
    def test_starts_container_and_returns_id(self, mock_run):
        """Should start a container and return its ID."""
        mock_result = Mock()
        mock_result.stdout = "container-id-123\n"
        mock_run.return_value = mock_result

        cid = _start_container("docker", "my-image:latest", port=18080)
        assert cid == "container-id-123"

        mock_run.assert_called_once()
        cmd = mock_run.call_args[0][0]
        assert cmd[:3] == ["docker", "run", "--detach"]
        assert "--name" in cmd
        assert CONTAINER_NAME in cmd
        assert "127.0.0.1:18080:8080" in " ".join(cmd)
        assert "my-image:latest" in cmd

    @patch("jenkinsfilelint.local._run")
    def test_raises_runtime_error_on_failure(self, mock_run):
        """Should raise RuntimeError when container fails to start."""
        mock_run.side_effect = subprocess.CalledProcessError(
            1, cmd="docker", stderr="port already in use"
        )

        with pytest.raises(RuntimeError, match="Failed to start container"):
            _start_container("docker", "my-image")

    @patch("jenkinsfilelint.local._run")
    def test_raises_runtime_error_when_no_id_returned(self, mock_run):
        """Should raise RuntimeError when no container ID is returned."""
        mock_result = Mock()
        mock_result.stdout = "\n"
        mock_run.return_value = mock_result

        with pytest.raises(RuntimeError, match="no ID was returned"):
            _start_container("docker", "my-image")


# ---------------------------------------------------------------------------
# _stop_container
# ---------------------------------------------------------------------------


class TestStopContainer:
    """Test container shutdown."""

    @patch("jenkinsfilelint.local._run")
    def test_stops_and_removes_container(self, mock_run):
        """Should stop and remove the container."""
        _stop_container("docker", "abc123", timeout=10)
        assert mock_run.call_count == 2
        # First call: stop
        stop_args = mock_run.call_args_list[0][0][0]
        assert stop_args[:3] == ["docker", "stop", "--time"]
        assert "10" in stop_args
        # Second call: rm
        rm_args = mock_run.call_args_list[1][0][0]
        assert rm_args[:3] == ["docker", "rm", "--force"]

    @patch("jenkinsfilelint.local._run")
    def test_does_not_raise_on_timeout(self, mock_run):
        """Should not raise when stop times out."""
        mock_run.side_effect = subprocess.TimeoutExpired(cmd="docker", timeout=5)

        # Should not raise
        _stop_container("docker", "abc123")


# ---------------------------------------------------------------------------
# _wait_for_jenkins
# ---------------------------------------------------------------------------


class TestWaitForJenkins:
    """Test Jenkins readiness probe."""

    @patch("jenkinsfilelint.local.urllib.request.urlopen")
    def test_returns_true_when_ready(self, mock_urlopen):
        """Should return True when Jenkins responds with HTTP 200."""
        mock_response = Mock()
        mock_response.status = 200
        mock_urlopen.return_value = mock_response

        assert _wait_for_jenkins("http://127.0.0.1:18080", timeout=10) is True
        mock_urlopen.assert_called_with("http://127.0.0.1:18080/login", timeout=5)

    @patch("jenkinsfilelint.local.urllib.request.urlopen")
    def test_returns_false_on_timeout(self, mock_urlopen):
        """Should return False when Jenkins does not become ready."""
        mock_urlopen.side_effect = ConnectionError("Connection refused")

        assert _wait_for_jenkins("http://127.0.0.1:18080", timeout=0.1) is False

    @patch("jenkinsfilelint.local.urllib.request.urlopen")
    def test_retries_on_connection_errors(self, mock_urlopen):
        """Should retry when connection is refused."""
        # First two calls fail, third succeeds
        mock_urlopen.side_effect = [
            ConnectionError("Connection refused"),
            OSError("Connection reset"),
            Mock(status=200),
        ]

        result = _wait_for_jenkins("http://127.0.0.1:18080", timeout=10)
        assert result is True
        assert mock_urlopen.call_count == 3


# ---------------------------------------------------------------------------
# LocalJenkins public API
# ---------------------------------------------------------------------------


class TestLocalJenkins:
    """Test the LocalJenkins class."""

    def test_default_image_from_constant(self):
        """Should use the default image when none is specified."""
        lj = LocalJenkins()
        assert "ghcr.io/jenkinsci/jenkinsfilelint-server" in lj.image
        assert lj.port == DEFAULT_PORT

    def test_custom_image_and_port(self):
        """Should accept custom image and port."""
        lj = LocalJenkins(image="my-jenkins:latest", port=9999)
        assert lj.image == "my-jenkins:latest"
        assert lj.port == 9999

    @patch.dict(os.environ, {"JENKINSFILELINT_SERVER_IMAGE": "custom-image:v1"})
    def test_image_from_env_var(self):
        """Should read image from environment variable."""
        lj = LocalJenkins()
        assert lj.image == "custom-image:v1"

    @patch("jenkinsfilelint.local._find_runtime")
    def test_runtime_discovery(self, mock_find):
        """Should discover the runtime."""
        mock_find.return_value = "/usr/bin/docker"
        lj = LocalJenkins()
        assert lj.runtime == "/usr/bin/docker"

    @patch("jenkinsfilelint.local._find_runtime")
    def test_runtime_not_found_raises(self, mock_find):
        """Should raise RuntimeError when no runtime is found."""
        mock_find.return_value = None
        lj = LocalJenkins()
        with pytest.raises(RuntimeError, match="Neither 'docker' nor 'podman'"):
            _ = lj.runtime

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_status_running(self, mock_find, mock_cid):
        """Status should indicate running when container is up."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = "abc123"

        with patch("jenkinsfilelint.local._container_is_running") as mock_ir:
            mock_ir.return_value = True
            lj = LocalJenkins()
            state = lj.status()

        assert state["running"] is True
        assert state["container_id"] == "abc123"
        assert state["port"] == DEFAULT_PORT
        assert state["url"] == f"http://127.0.0.1:{DEFAULT_PORT}"

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_status_not_running(self, mock_find, mock_cid):
        """Status should indicate not running when container is absent."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = None

        lj = LocalJenkins()
        state = lj.status()

        assert state["running"] is False
        assert state["container_id"] is None
        assert state["url"] is None

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_ensure_running_reuses_existing(self, mock_find, mock_cid):
        """Should reuse existing running container."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = "abc123"

        with (
            patch("jenkinsfilelint.local._container_is_running") as mock_ir,
            patch("jenkinsfilelint.local._wait_for_jenkins") as mock_wait,
        ):
            mock_ir.return_value = True
            mock_wait.return_value = True

            lj = LocalJenkins()
            url = lj.ensure_running()

        assert url == f"http://127.0.0.1:{DEFAULT_PORT}"
        # Should NOT try to start a new container
        # (no _start_container call)

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_ensure_running_starts_new(self, mock_find, mock_cid):
        """Should start a new container when none is running."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = None  # No existing container

        with (
            patch("jenkinsfilelint.local._start_container") as mock_start,
            patch("jenkinsfilelint.local._wait_for_jenkins") as mock_wait,
        ):
            mock_start.return_value = "new-container-id"
            mock_wait.return_value = True

            lj = LocalJenkins()
            url = lj.ensure_running()

        assert url == f"http://127.0.0.1:{DEFAULT_PORT}"
        mock_start.assert_called_once()

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_ensure_running_raises_on_timeout(self, mock_find, mock_cid):
        """Should raise RuntimeError when Jenkins does not become ready."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = None

        with (
            patch("jenkinsfilelint.local._start_container") as mock_start,
            patch("jenkinsfilelint.local._wait_for_jenkins") as mock_wait,
            patch("jenkinsfilelint.local._stop_container") as mock_stop,
            patch("jenkinsfilelint.local._container_logs") as mock_logs,
        ):
            mock_start.return_value = "new-container-id"
            mock_wait.return_value = False
            mock_logs.return_value = "some logs"

            lj = LocalJenkins()
            with pytest.raises(RuntimeError, match="did not become ready"):
                lj.ensure_running()

            # Should clean up the failed container
            mock_stop.assert_called_once()

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_stop_removes_container(self, mock_find, mock_cid):
        """Should stop and remove the container."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = "abc123"

        with patch("jenkinsfilelint.local._stop_container") as mock_stop:
            lj = LocalJenkins()
            lj.stop()

        mock_stop.assert_called_once()

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_stop_when_not_running(self, mock_find, mock_cid):
        """Should not error when stopping a non-running container."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = None

        lj = LocalJenkins()
        # Should not raise
        lj.stop()

    @patch("jenkinsfilelint.local._container_id")
    @patch("jenkinsfilelint.local._find_runtime")
    def test_restart_stops_then_starts(self, mock_find, mock_cid):
        """Restart should stop and re-start the container."""
        mock_find.return_value = "/usr/bin/docker"
        mock_cid.return_value = "abc123"

        with (
            patch.object(LocalJenkins, "stop") as mock_stop,
            patch.object(LocalJenkins, "ensure_running") as mock_ensure,
        ):
            mock_ensure.return_value = "http://127.0.0.1:18080"

            lj = LocalJenkins()
            url = lj.restart()

        mock_stop.assert_called_once()
        mock_ensure.assert_called_once()
        assert url == "http://127.0.0.1:18080"


# ---------------------------------------------------------------------------
# handle_server_command
# ---------------------------------------------------------------------------


class TestHandleServerCommand:
    """Test the server subcommand handler."""

    @patch("jenkinsfilelint.local.LocalJenkins")
    def test_server_start(self, mock_local_cls):
        """'server start' should call ensure_running."""
        mock_instance = Mock()
        mock_instance.ensure_running.return_value = "http://127.0.0.1:18080"
        mock_local_cls.return_value = mock_instance

        with patch.object(sys, "stderr"):
            handle_server_command(["start"])

        mock_instance.ensure_running.assert_called_once()

    @patch("jenkinsfilelint.local.LocalJenkins")
    def test_server_stop(self, mock_local_cls):
        """'server stop' should call stop."""
        mock_instance = Mock()
        mock_local_cls.return_value = mock_instance

        with patch.object(sys, "stderr"):
            handle_server_command(["stop"])

        mock_instance.stop.assert_called_once()

    @patch("jenkinsfilelint.local.LocalJenkins")
    def test_server_status(self, mock_local_cls):
        """'server status' should call status."""
        mock_instance = Mock()
        mock_instance.status.return_value = {
            "running": True,
            "container_id": "abc123",
            "port": 18080,
            "url": "http://127.0.0.1:18080",
        }
        mock_local_cls.return_value = mock_instance

        with patch.object(sys, "stderr"):
            handle_server_command(["status"])

        mock_instance.status.assert_called_once()

    @patch("jenkinsfilelint.local.LocalJenkins")
    def test_server_restart(self, mock_local_cls):
        """'server restart' should call restart."""
        mock_instance = Mock()
        mock_instance.restart.return_value = "http://127.0.0.1:18080"
        mock_local_cls.return_value = mock_instance

        with patch.object(sys, "stderr"):
            handle_server_command(["restart"])

        mock_instance.restart.assert_called_once()

    @patch("jenkinsfilelint.local.LocalJenkins")
    def test_server_start_raises_on_runtime_error(self, mock_local_cls):
        """Should exit with code 1 on RuntimeError."""
        mock_instance = Mock()
        mock_instance.ensure_running.side_effect = RuntimeError("something broke")
        mock_local_cls.return_value = mock_instance

        with pytest.raises(SystemExit) as exc:
            handle_server_command(["start"])

        assert exc.value.code == 1

    def test_server_invalid_action(self):
        """Should exit with code 2 on invalid action."""
        with pytest.raises(SystemExit) as exc:
            handle_server_command(["fly"])

        # argparse error → exit code 2
        assert exc.value.code == 2
