"""Which files pre-commit passes to the jenkins-lint hook."""

import re
from pathlib import Path

import pytest
import yaml
from identify.identify import tags_from_path

ROOT = Path(__file__).resolve().parent.parent
HOOK = yaml.safe_load((ROOT / ".pre-commit-hooks.yaml").read_text())[0]


def passed(path):
    """pre-commit's file selection: files, exclude and types."""
    return (
        re.search(HOOK["files"], path) is not None
        and re.search(HOOK["exclude"], path) is None
        and set(HOOK["types"]) <= tags_from_path(path)
    )


@pytest.mark.parametrize(
    "name, checked",
    [
        ("Jenkinsfile", True),
        ("ci/Jenkinsfile.prod", True),
        ("Jenkinsfile-prod", True),
        ("jenkinsfile", True),
        ("deploy.jenkinsfile", True),
        ("ci/Deploy.Jenkinsfile", True),
        ("vars/notify.groovy", True),
        ("Jenkinsfile.md", False),
        ("docs/Jenkinsfile.bak", False),
        ("docs/Jenkinsfile-guide.png", False),
        ("README.md", False),
        ("notes/jenkinsfile.txt", False),
    ],
)
def test_files_the_hook_checks(tmp_path, monkeypatch, name, checked):
    monkeypatch.chdir(tmp_path)
    path = Path(name)
    path.parent.mkdir(parents=True, exist_ok=True)
    data = b"\x89PNG\r\n\x1a\n\x00\x00" if name.endswith(".png") else b"pipeline {}\n"
    path.write_bytes(data)
    assert passed(name) is checked
