"""End to end: `jenkins-lint --remote` against a real Jenkins in Docker.

Builds docker/ and starts it (tests/jenkins_server.py), then checks files
with the jenkins-lint binary. Not run by default: it needs Docker and a few
minutes for the first build and boot.

Run with::

    cargo build --manifest-path crates/jenkins-lint/Cargo.toml
    pytest tests/test_docker_linter.py -v -m docker

JENKINS_LINT_BIN names another binary.
"""

import json
import os
import shutil
import subprocess
import urllib.parse
import urllib.request
from pathlib import Path

import pytest

from . import jenkins_server

pytestmark = pytest.mark.docker

ROOT = Path(__file__).resolve().parent.parent
GOOD = ROOT / "tests" / "Jenkinsfile"
BAD = ROOT / "tests" / "Jenkinsfile.fail"


def binary() -> str:
    if os.environ.get("JENKINS_LINT_BIN"):
        return os.environ["JENKINS_LINT_BIN"]
    exe = "jenkins-lint.exe" if os.name == "nt" else "jenkins-lint"
    for profile in ("debug", "release"):
        built = ROOT / "crates" / "jenkins-lint" / "target" / profile / exe
        if built.exists():
            return str(built)
    return shutil.which("jenkins-lint") or "jenkins-lint"


@pytest.fixture(scope="module")
def jenkins_url():
    """Build docker/, start Jenkins once for the module, then remove it."""
    with jenkins_server.running() as url:
        yield url


def lint(url, *args):
    env = {k: v for k, v in os.environ.items() if not k.startswith("JENKINS_")}
    return subprocess.run(
        [binary(), "--remote", url, "--verbose", *map(str, args)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        check=False,
    )


def test_valid_file_is_validated_by_jenkins(jenkins_url):
    proc = lint(jenkins_url, GOOD)
    assert proc.returncode == 0, proc.stderr
    assert f"✓ {GOOD}: Valid" in proc.stdout
    assert "Jenkinsfile successfully validated" in proc.stdout


def test_invalid_file_is_reported_offline(jenkins_url):
    proc = lint(jenkins_url, BAD)
    assert proc.returncode == 1
    assert 'Not a valid section definition: "agent"' in proc.stderr
    assert "Validating" not in proc.stdout


@pytest.mark.parametrize(
    "source",
    [
        "node {\n    stage('Build') {\n        sh 'make'\n    }\n}\n",
        "node {\n    stage('Build') {\n        sh 'make || error \"failed\"'\n    }\n}\n",
    ],
    ids=["scripted", "scripted-with-error-word"],
)
def test_scripted_file_is_not_sent(jenkins_url, tmp_path, source):
    path = tmp_path / "Jenkinsfile"
    path.write_text(source)
    proc = lint(jenkins_url, path)
    assert proc.returncode == 0, proc.stderr
    assert "Skipped (not a Declarative Pipeline)" in proc.stdout


def test_jenkins_knows_what_the_engine_cannot(jenkins_url, tmp_path):
    """A step parameter only the plugin knows: offline passes, Jenkins fails."""
    path = tmp_path / "Jenkinsfile"
    path.write_text(
        "pipeline {\n  agent any\n  stages {\n    stage('A') {\n      steps {\n"
        "        timeout(tim: 5) { echo 'x' }\n      }\n    }\n  }\n}\n"
    )
    proc = lint(jenkins_url, path)
    assert proc.returncode == 1, proc.stdout
    assert f"✗ {path}: rejected by Jenkins" in proc.stderr


def test_validate_endpoint_works_without_crumb(jenkins_url):
    """The endpoint answers a POST without a crumb.

    docker/jenkins.yaml enables the crumb issuer, but the container runs
    unsecured. If this fails, CSRF protection blocks anonymous requests,
    which breaks the other tests here and corpus/compare.py.
    """
    data = urllib.parse.urlencode(
        {
            "jenkinsfile": "pipeline {\n agent any\n stages { stage('X') "
            "{ steps { echo 'hi' } } }\n}\n"
        }
    ).encode()
    request = urllib.request.Request(
        f"{jenkins_url}/pipeline-model-converter/validate", data=data
    )
    with urllib.request.urlopen(request, timeout=15) as response:
        assert response.status == 200
        body = response.read().decode()
    if body.strip().startswith("{"):
        assert json.loads(body).get("status") == "ok"
