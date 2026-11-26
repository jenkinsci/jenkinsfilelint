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

import pytest

from jenkins_lint.linter import JenkinsfileLinter, NOT_DECLARATIVE_MESSAGE

from . import jenkins_server

pytestmark = pytest.mark.docker

_HERE = os.path.dirname(__file__)
GOOD_JENKINSFILE = os.path.join(_HERE, "Jenkinsfile")
BAD_JENKINSFILE = os.path.join(_HERE, "Jenkinsfile.fail")


@pytest.fixture(scope="module")
def local_jenkins_url():
    """Build docker/, start Jenkins once for the module, then remove it."""
    with jenkins_server.running() as url:
        yield url


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


class TestDockerEndToEnd:
    """Real end-to-end validation against a live Jenkins container.

    These tests exercise the code path of ``jenkins-lint --remote URL``.
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

    @pytest.mark.parametrize(
        "source",
        [
            "node {\n    stage('Build') {\n        sh 'make'\n    }\n}\n",
            "node {\n    stage('Build') {\n        sh 'make || error \"failed\"'\n    }\n}\n",
            "",
        ],
        ids=["scripted", "scripted-with-error-word", "empty"],
    )
    def test_non_declarative_file_is_skipped(
        self, local_jenkins_url: str, tmp_path, source: str
    ) -> None:
        """Files without a pipeline block are skipped whatever they contain."""
        jenkinsfile = tmp_path / "Jenkinsfile"
        jenkinsfile.write_text(source)

        linter = JenkinsfileLinter(jenkins_url=local_jenkins_url)
        assert linter.validate(str(jenkinsfile)) == (True, NOT_DECLARATIVE_MESSAGE)

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
        other tests here and corpus/compare.py.
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
