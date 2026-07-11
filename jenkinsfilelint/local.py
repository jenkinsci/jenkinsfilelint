"""Local Docker container lifecycle management for jenkinsfilelint.

This module manages a minimal Jenkins container that runs on localhost,
allowing ``jenkinsfilelint --local`` to validate Jenkinsfiles without
requiring a remote Jenkins server.
"""

import os
import time
import logging
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from typing import Optional

log = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------
DEFAULT_IMAGE = "ghcr.io/jenkinsci/jenkinsfilelint-server:latest"
DEFAULT_PORT = 18080
CONTAINER_LABEL = "jenkinsfilelint-server"
CONTAINER_NAME = "jenkinsfilelint-server"
PULL_TIMEOUT = 60  # max seconds to wait for the initial image pull
POLL_INTERVAL = 2  # seconds between readiness checks
READY_TIMEOUT = 120  # max seconds to wait for Jenkins to start
START_TIMEOUT = 30  # max seconds to wait for container create


def _find_runtime() -> Optional[str]:
    """Return the path to 'docker' or 'podman', or *None* if neither is found."""
    for binary in ("docker", "podman"):
        path = shutil.which(binary)
        if path is not None:
            return path
    return None


def _run(
    cmd: list[str],
    timeout: int = 30,
    check: bool = True,
    **kwargs,
) -> subprocess.CompletedProcess:
    """Run *cmd* and return the result.

    Wraps :func:`subprocess.run` with sensible defaults (text mode, merged
    stderr so error messages are visible).
    """
    return subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=check,
        **kwargs,
    )


# ---------------------------------------------------------------------------
# Container helpers
# ---------------------------------------------------------------------------


def _container_id(runtime: str, label: str = CONTAINER_LABEL) -> Optional[str]:
    """Return the container ID of the running container with *label*, or *None*."""
    try:
        result = _run(
            [runtime, "ps", "--filter", f"label={label}", "--format", "{{.ID}}"],
            check=False,
        )
        cid = result.stdout.strip()
        return cid if cid else None
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return None


def _container_is_running(runtime: str, container_id: str) -> bool:
    """Check if the container with *container_id* is actually running."""
    try:
        result = _run(
            [runtime, "inspect", "--format", "{{.State.Status}}", container_id],
            check=False,
        )
        return result.stdout.strip() == "running"
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return False


def _start_container(
    runtime: str,
    image: str,
    port: int = DEFAULT_PORT,
    name: str = CONTAINER_NAME,
    label: str = CONTAINER_LABEL,
) -> str:
    """Start a new container and return its ID.

    Pulls the image first (with a generous timeout) so that a slow first-time
    pull does not trigger the shorter ``docker run`` timeout.

    Raises :exc:`RuntimeError` if the container cannot be started.
    """
    log.info("Starting Jenkins container (%s)…", image)

    # Explicitly pull the image so a slow first-time download doesn't
    # trip the much shorter docker-run timeout (START_TIMEOUT=30s).
    log.info("Pulling image %s …", image)
    try:
        _run([runtime, "pull", image], timeout=PULL_TIMEOUT)
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(f"Failed to pull image {image}:\n{exc.stderr}") from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError(
            f"Timed out pulling image {image} after {PULL_TIMEOUT}s. "
            f"Check your network connection or try pulling manually."
        ) from exc

    cmd = [
        runtime,
        "run",
        "--detach",
        "--name",
        name,
        "--label",
        label,
        "--publish",
        f"127.0.0.1:{port}:8080",
        "--restart",
        "no",
        image,
    ]
    try:
        result = _run(cmd, timeout=START_TIMEOUT)
    except subprocess.CalledProcessError as exc:
        raise RuntimeError(f"Failed to start container:\n{exc.stderr}") from exc
    except subprocess.TimeoutExpired as exc:
        raise RuntimeError("Timed out waiting for container to start") from exc

    cid = result.stdout.strip()
    if not cid:
        raise RuntimeError("Container started but no ID was returned")
    log.info("Container %s started", cid[:12])
    return cid


def _stop_container(runtime: str, container_id: str, timeout: int = 15) -> None:
    """Stop and remove the container."""
    log.info("Stopping container %s …", container_id[:12])
    try:
        _run([runtime, "stop", "--time", str(timeout), container_id], check=False)
    except subprocess.TimeoutExpired:
        pass
    try:
        _run([runtime, "rm", "--force", container_id], check=False)
    except subprocess.TimeoutExpired:
        pass


def _container_logs(runtime: str, container_id: str, tail: int = 20) -> str:
    """Return the last *tail* lines of container logs."""
    try:
        result = _run(
            [runtime, "logs", "--tail", str(tail), container_id],
            check=False,
        )
        return result.stdout + result.stderr
    except (subprocess.TimeoutExpired, FileNotFoundError):
        return "<unable to read logs>"


# ---------------------------------------------------------------------------
# Readiness probe
# ---------------------------------------------------------------------------


def _wait_for_jenkins(url: str, timeout: int = READY_TIMEOUT) -> bool:
    """Poll *url*/login until Jenkins responds (HTTP 200) or *timeout* expires.

    Returns ``True`` if Jenkins is ready, ``False`` on timeout.
    """
    login_url = f"{url.rstrip('/')}/login"
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            resp = urllib.request.urlopen(login_url, timeout=5)
            if resp.status == 200:
                log.info("Jenkins is ready at %s", url)
                return True
        except (urllib.error.URLError, ConnectionError, OSError):
            pass
        time.sleep(POLL_INTERVAL)
    return False


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------


class LocalJenkins:
    """Manages a local Jenkins container for Declarative Pipeline validation.

    Typical usage::

        lj = LocalJenkins()
        url = lj.ensure_running()
        # … validate using url …
        lj.stop()
    """

    def __init__(
        self,
        image: Optional[str] = None,
        port: Optional[int] = None,
    ):
        self.image = image or os.environ.get(
            "JENKINSFILELINT_SERVER_IMAGE", DEFAULT_IMAGE
        )
        self.port = port or DEFAULT_PORT
        self._runtime: Optional[str] = None
        self._container_id: Optional[str] = None

    @property
    def runtime(self) -> str:
        """The container runtime (docker/podman), discovered on first access."""
        if self._runtime is None:
            r = _find_runtime()
            if r is None:
                raise RuntimeError(
                    "Neither 'docker' nor 'podman' was found on your PATH. "
                    "Install Docker Desktop or Podman to use --local mode."
                )
            self._runtime = r
        return self._runtime

    @property
    def container_id(self) -> Optional[str]:
        """The running container's ID, lazily discovered."""
        if self._container_id is None:
            self._container_id = _container_id(self.runtime)
        return self._container_id

    def status(self) -> dict:
        """Return a dict describing the current container state.

        Keys: ``running`` (bool), ``container_id`` (str or None),
        ``url`` (str or None), ``port`` (int).
        """
        cid = self.container_id
        if cid is not None and _container_is_running(self.runtime, cid):
            return {
                "running": True,
                "container_id": cid,
                "port": self.port,
                "url": f"http://127.0.0.1:{self.port}",
            }
        return {
            "running": False,
            "container_id": None,
            "port": self.port,
            "url": None,
        }

    def ensure_running(self) -> str:
        """Ensure the local Jenkins container is running and ready.

        If no container with the expected label exists, a new one is started.
        Blocks until Jenkins responds on its HTTP port.

        Returns:
            The base URL of the local Jenkins (e.g. ``http://127.0.0.1:18080``).

        Raises:
            RuntimeError: If the runtime cannot be found, the container cannot
                be started, or Jenkins does not become ready within the timeout.
        """
        # 1. Check for an existing container.
        cid = self.container_id
        if cid is not None and _container_is_running(self.runtime, cid):
            log.info("Reusing existing container %s", cid[:12])
        else:
            # 2. Start a new container.
            cid = _start_container(
                self.runtime,
                self.image,
                port=self.port,
            )
            self._container_id = cid

        # 3. Wait for Jenkins to be ready.
        url = f"http://127.0.0.1:{self.port}"
        if not _wait_for_jenkins(url):
            logs = _container_logs(self.runtime, cid)
            # Stop the failed container so the user can retry cleanly
            _stop_container(self.runtime, cid, timeout=5)
            self._container_id = None
            raise RuntimeError(
                f"Jenkins did not become ready within {READY_TIMEOUT}s.\n"
                f"Container logs (last 20 lines):\n{logs}"
            )

        return url

    def stop(self) -> None:
        """Stop and remove the local Jenkins container (if running)."""
        cid = self.container_id
        if cid is not None:
            _stop_container(self.runtime, cid)
            self._container_id = None
            print("✓ Jenkins container stopped", file=sys.stderr)
        else:
            print("ℹ No Jenkins container is running", file=sys.stderr)

    def restart(self) -> str:
        """Restart the container and wait for readiness.

        Returns:
            The base URL of the local Jenkins.
        """
        self.stop()
        # Clear the cached container ID so a fresh one is discovered
        self._container_id = None
        return self.ensure_running()


def handle_server_command(argv: list[str]) -> None:
    """Handle the ``jenkinsfilelint server <action>`` subcommand."""
    import argparse

    parser = argparse.ArgumentParser(
        prog="jenkinsfilelint server",
        description="Manage the local Jenkins validation server container.",
    )
    parser.add_argument(
        "action",
        choices=["start", "stop", "status", "restart"],
        help="Action to perform on the local server",
    )
    args = parser.parse_args(argv)

    local = LocalJenkins()

    try:
        if args.action == "start":
            url = local.ensure_running()
            print(f"✓ Jenkins is ready at {url}", file=sys.stderr)

        elif args.action == "stop":
            local.stop()

        elif args.action == "status":
            state = local.status()
            if state["running"]:
                print(
                    f"✓ Jenkins is running\n"
                    f"  Container: {state['container_id'][:12]}\n"
                    f"  URL:       {state['url']}",
                    file=sys.stderr,
                )
            else:
                print("ℹ Jenkins is not running", file=sys.stderr)

        elif args.action == "restart":
            url = local.restart()
            print(f"✓ Jenkins restarted and ready at {url}", file=sys.stderr)

    except RuntimeError as exc:
        print(f"✗ {exc}", file=sys.stderr)
        sys.exit(1)
