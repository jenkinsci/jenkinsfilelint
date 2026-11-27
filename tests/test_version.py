"""Tests for crates/jenkins-lint/scripts/check_version.py."""

import importlib.util
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "crates" / "jenkins-lint" / "scripts" / "check_version.py"

spec = importlib.util.spec_from_file_location("check_version", SCRIPT)
check_version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_version)


def repo(tmp_path, cargo="2.0.0", shim="2.0.0", pin="2.0.0"):
    crate = tmp_path / "crates" / "jenkins-lint"
    crate.mkdir(parents=True)
    (crate / "Cargo.toml").write_text(
        f'[package]\nname = "jenkins-lint"\nversion = "{cargo}"\n'
    )
    (tmp_path / "pyproject.toml").write_text(
        f'[project]\nname = "jenkins-lint-pre-commit"\nversion = "{shim}"\n'
        f'dependencies = ["jenkins-lint=={pin}"]\n'
    )
    return tmp_path


def test_this_repository_agrees():
    assert check_version.check(ROOT) == []


def test_versions_agree(tmp_path):
    assert check_version.check(repo(tmp_path), "v2.0.0") == []


def test_shim_version_differs(tmp_path):
    errors = check_version.check(repo(tmp_path, shim="1.9.0"))
    assert errors == [
        "pyproject.toml has version 1.9.0, but crates/jenkins-lint/Cargo.toml has 2.0.0."
    ]


def test_pin_differs(tmp_path):
    errors = check_version.check(repo(tmp_path, pin="1.9.0"))
    assert errors == [
        "pyproject.toml pins jenkins-lint==1.9.0, but crates/jenkins-lint/Cargo.toml has 2.0.0."
    ]


def test_tag_differs(tmp_path):
    errors = check_version.check(repo(tmp_path), "v2.0.1")
    assert errors == [
        "The tag is v2.0.1, but crates/jenkins-lint/Cargo.toml has 2.0.0: tag v2.0.0."
    ]
