#!/usr/bin/env python3
"""Tests for crates/jenkinsfilelint/scripts/check_engine_version.py."""

import importlib.util
import shutil
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "crates" / "jenkinsfilelint" / "scripts" / "check_engine_version.py"
CRATE = "crates/jenkinsfilelint"

spec = importlib.util.spec_from_file_location("check_engine_version", SCRIPT)
check_engine_version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check_engine_version)
check = check_engine_version.check

pytestmark = pytest.mark.skipif(shutil.which("git") is None, reason="needs git")


class Repo:
    """A throwaway repository shaped like this one."""

    def __init__(self, path):
        self.path = path
        self.git("init", "--quiet")
        self.write("README.md", "jenkinsfilelint\n")
        self.commit("first commit")

    def git(self, *args):
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "tag.gpgsign=false",
                *args,
            ],
            cwd=self.path,
            check=True,
            capture_output=True,
        )

    def write(self, name, text):
        target = self.path / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    def commit(self, message):
        self.git("add", "--all")
        self.git("commit", "--quiet", "--message", message)

    def engine(self, version, source="fn main() {}\n"):
        self.write(
            f"{CRATE}/Cargo.toml",
            f'[package]\nname = "jenkinsfilelint-engine"\nversion = "{version}"\n',
        )
        self.write(f"{CRATE}/src/main.rs", source)

    def release(self, tag):
        self.git("tag", tag)


@pytest.fixture
def repo(tmp_path):
    return Repo(tmp_path)


def failed(lines):
    return lines[0].startswith("error:")


def test_no_release_yet(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    assert check(repo.path) == [
        "No release tag found: nothing to compare the engine with."
    ]


def test_engine_added_after_the_last_release(repo):
    repo.release("v1.5.3")
    repo.engine("0.1.0")
    repo.commit("add the engine")
    assert check(repo.path) == ["v1.5.3 has no engine: nothing to compare with."]


def test_engine_unchanged(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.write("jenkinsfilelint/cli.py", "print('hello')\n")
    repo.commit("change the Python package only")
    assert check(repo.path) == ["The engine is the same as in v1.6.0 (0.1.0)."]


def test_engine_changed_without_a_bump(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.engine("0.1.0", source="fn main() { fix(); }\n")
    repo.commit("fix the engine")

    lines = check(repo.path)

    assert failed(lines)
    assert (
        lines[0]
        == "error: the engine changed since v1.6.0, but its version is still 0.1.0."
    )
    assert lines[1] == f"Changed: {CRATE}/src/main.rs"


def test_engine_changed_with_a_bump(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.engine("0.2.0", source="fn main() { fix(); }\n")
    repo.commit("fix the engine")
    assert check(repo.path) == ["The engine changed since v1.6.0: 0.1.0 -> 0.2.0."]


def test_second_change_after_a_bump_needs_no_other_bump(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.engine("0.2.0", source="fn main() { fix(); }\n")
    repo.commit("fix the engine")
    repo.engine("0.2.0", source="fn main() { fix(); more(); }\n")
    repo.commit("fix the engine again")
    assert not failed(check(repo.path))


def test_tests_and_documents_are_not_the_engine(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.write(f"{CRATE}/tests/jenkins/v99_new_case.groovy", "pipeline { }\n")
    repo.write(f"{CRATE}/README.md", "more words\n")
    repo.write(f"{CRATE}/scripts/tool.py", "print('x')\n")
    repo.commit("add a test case")
    assert check(repo.path) == ["The engine is the same as in v1.6.0 (0.1.0)."]


def test_uncommitted_and_new_files_count(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.write(f"{CRATE}/src/lexer.rs", "// new file, not added yet\n")

    lines = check(repo.path)

    assert failed(lines)
    assert lines[1] == f"Changed: {CRATE}/src/lexer.rs"


def test_release_commit_is_compared_with_the_release_before(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.engine("0.1.0", source="fn main() { fix(); }\n")
    repo.commit("fix the engine")
    repo.release("v1.6.1")  # HEAD is the commit being released

    lines = check(repo.path)

    assert failed(lines)
    assert lines[0].startswith("error: the engine changed since v1.6.0,")


def test_base_can_be_given(repo):
    repo.engine("0.1.0")
    repo.commit("add the engine")
    repo.release("v1.6.0")
    repo.engine("0.2.0", source="fn main() { fix(); }\n")
    repo.commit("fix the engine")
    repo.release("v1.7.0")
    repo.write("README.md", "more\n")
    repo.commit("docs")
    assert check(repo.path) == ["The engine is the same as in v1.7.0 (0.2.0)."]
    assert check(repo.path, "v1.6.0") == [
        "The engine changed since v1.6.0: 0.1.0 -> 0.2.0."
    ]


def test_main_exit_code_on_this_repository(capsys):
    """The script runs here; whether it passes depends on the checkout."""
    code = check_engine_version.main([])
    captured = capsys.readouterr()
    assert code in (0, 1)
    assert (code == 1) == captured.err.startswith("error:")
