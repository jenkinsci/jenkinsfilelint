"""End-to-end integration tests against a real Jenkins container.

Starts a real Jenkins container (via Docker/Podman), validates both
a correct and an incorrect Jenkinsfile against the
``/pipeline-model-converter/validate`` endpoint, then cleans up.

These tests are **not** run by default — they require Docker and a few
minutes of wall-clock time (image pull + Jenkins container boot).

Run with::

    pytest tests/test_docker_linter.py -v -m docker

Mark all tests in this file with ``@pytest.mark.docker`` so they are
skipped in a plain ``pytest`` invocation unless ``-m docker`` is given.
"""

import os
import subprocess

import pytest

from jenkinsfilelint.linter import JenkinsfileLinter
from jenkinsfilelint.local import LocalJenkins

pytestmark = pytest.mark.docker

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_HERE = os.path.dirname(__file__)
_DOCKER_DIR = os.path.join(os.path.dirname(_HERE), "docker")

GOOD_JENKINSFILE = os.path.join(_HERE, "Jenkinsfile")
BAD_JENKINSFILE = os.path.join(_HERE, "Jenkinsfile.fail")


def _ensure_image(tag: str) -> str:
    """Make sure the Docker image *tag* is available locally.

    - If ``JENKINSFILELINT_SERVER_IMAGE`` is explicitly set in the
      environment, trust that and do nothing.
    - If *tag* is the default published image, let
      :meth:`LocalJenkins.ensure_running` pull it.
    - Otherwise build from ``docker/`` so it can be used locally.
    """
    if "JENKINSFILELINT_SERVER_IMAGE" in os.environ:
        return tag  # caller is responsible

    if tag == "ghcr.io/jenkinsci/jenkinsfilelint-server:latest":
        return tag  # will be pulled by LocalJenkins

    # Local-only tag → build from source
    subprocess.run(
        ["docker", "build", "-t", tag, _DOCKER_DIR],
        check=True,
        capture_output=True,
    )
    return tag


# ---------------------------------------------------------------------------
# Module-scoped fixtures — one container start/stop per test module
# ---------------------------------------------------------------------------


@pytest.fixture(scope="module")
def server_image() -> str:
    """Resolve and prepare the server image tag."""
    tag = os.environ.get(
        "JENKINSFILELINT_SERVER_IMAGE",
        "ghcr.io/jenkinsci/jenkinsfilelint-server:latest",
    )
    return _ensure_image(tag)


@pytest.fixture(scope="module")
def local_jenkins_url(server_image: str) -> str:
    """Start a local Jenkins container and return its base URL.

    The container is started once before any test in the module and
    torn down after all tests finish.
    """
    lj = LocalJenkins(image=server_image)
    try:
        url = lj.ensure_running()
        yield url
    finally:
        lj.stop()


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestDockerEndToEnd:
    """Real end-to-end validation against a live Jenkins container.

    These tests exercise the exact same code path that a user hitting
    ``jenkinsfilelint --local Jenkinsfile`` would take.
    """

    def test_good_jenkinsfile_passes(self, local_jenkins_url: str) -> None:
        """A syntactically valid Jenkinsfile should be accepted."""
        linter = JenkinsfileLinter(jenkins_url=local_jenkins_url)
        is_valid, message = linter.validate(GOOD_JENKINSFILE)
        assert is_valid, f"Expected valid, got: {message}"
        assert "successfully validated" in message

    def test_bad_jenkinsfile_fails(self, local_jenkins_url: str) -> None:
        """A Jenkinsfile with a syntax error should be rejected.

        ``Jenkinsfile.fail`` has ``agent`` without a required value
        (e.g. ``any`` / ``none`` / ``label``), which Jenkins should
        report as a compilation error.
        """
        linter = JenkinsfileLinter(jenkins_url=local_jenkins_url)
        is_valid, message = linter.validate(BAD_JENKINSFILE)
        assert not is_valid, f"Expected invalid, got: {message}"
        # Jenkins should flag the syntax problem
        assert any(
            indicator in message
            for indicator in ("error", "Error", "WorkflowScript", "Expected")
        ), f"Message doesn't look like a Jenkins error: {message}"

    def test_validate_endpoint_works_without_crumb(
        self,
        local_jenkins_url: str,
    ) -> None:
        """Verify the ``/pipeline-model-converter/validate`` endpoint responds
        successfully **without** a crumb header.

        The JCasC config in ``docker/jenkins.yaml`` enables the
        ``crumbIssuer``, but the container runs in unsecured (anonymous)
        mode.  This test confirms the critical path that the linter uses
        every time — POST without crumb — actually works against a real
        container.

        If this test starts failing, it likely means Jenkins's CSRF
        protection is blocking anonymous requests, which would break the
        linter's ``--local`` mode.
        """
        import requests

        resp = requests.post(
            f"{local_jenkins_url.rstrip('/')}/pipeline-model-converter/validate",
            data={
                "jenkinsfile": (
                    "pipeline {\n"
                    "    agent any\n"
                    "    stages {\n"
                    "        stage('X') {\n"
                    "            steps {\n"
                    "                echo 'hi'\n"
                    "            }\n"
                    "        }\n"
                    "    }\n"
                    "}\n"
                ),
            },
            timeout=15,
        )
        assert resp.status_code == 200, (
            f"Expected HTTP 200, got {resp.status_code}: {resp.text}"
        )
        # The response may be JSON {"status": "ok"} or empty text on success.
        # Either way, the linter uses it unchanged — just confirm no error.
        if resp.text.strip():
            try:
                assert resp.json().get("status") == "ok"
            except ValueError:
                # Non-JSON text is also fine (e.g. "Jenkinsfile successfully validated")
                pass
