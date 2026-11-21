#!/usr/bin/env python3
"""Tests for searching directories for the files to check."""

import os
from pathlib import Path
from unittest.mock import patch

import pytest

from jenkinsfilelint.cli import expand_paths, find_files, main

VALID = "pipeline {\n  agent any\n  stages { stage('a') { steps { echo 'a' } } }\n}\n"
INVALID = "pipeline {\n  agent any\n}\n"
LEAK = (
    "withCredentials([string(credentialsId: 'c', variable: 'TOKEN')]) {\n"
    '    sh "deploy --token ${TOKEN}"\n'
    "}\n"
)


def write(root, *names, text=VALID):
    for name in names:
        path = Path(root) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)


def found(root, audit=False):
    paths = find_files(str(root), audit)
    return [Path(p).relative_to(root).as_posix() for p in paths]


def run(*argv):
    with patch("sys.argv", ["jenkinsfilelint", *argv]):
        with pytest.raises(SystemExit) as exc_info:
            main()
    return exc_info.value.code


@pytest.fixture(autouse=True)
def no_jenkins(monkeypatch):
    for name in ("JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN"):
        monkeypatch.delenv(name, raising=False)


class TestFindFiles:
    def test_names_matched(self, tmp_path):
        write(
            tmp_path,
            "Jenkinsfile",
            "Jenkinsfile.prod",
            "ci/deploy.jenkinsfile",
            "ci/nested/Jenkinsfile",
            "README.md",
            "Jenkinsfile2",
            "jenkinsfile",
            "script.groovy",
        )
        assert found(tmp_path) == [
            "Jenkinsfile",
            "Jenkinsfile.prod",
            "ci/deploy.jenkinsfile",
            "ci/nested/Jenkinsfile",
        ]

    def test_library_files_only_with_audit(self, tmp_path):
        write(
            tmp_path,
            "Jenkinsfile",
            "vars/deploy.groovy",
            "src/com/acme/Util.groovy",
            "scripts/other.groovy",
            "vars/notes.txt",
        )
        assert found(tmp_path) == ["Jenkinsfile"]
        assert found(tmp_path, audit=True) == [
            "Jenkinsfile",
            "src/com/acme/Util.groovy",
            "vars/deploy.groovy",
        ]

    def test_searching_inside_a_library_directory(self, tmp_path, monkeypatch):
        write(tmp_path, "vars/deploy.groovy")
        assert found(tmp_path / "vars", audit=True) == ["deploy.groovy"]
        monkeypatch.chdir(tmp_path / "vars")
        assert find_files(".", audit=True) == ["deploy.groovy"]

    def test_parent_of_the_search_does_not_count(self, tmp_path):
        write(tmp_path, "src/project/tools/helper.groovy")
        assert found(tmp_path / "src" / "project", audit=True) == []

    @pytest.mark.parametrize(
        "directory",
        [".git", ".venv", "venv", "node_modules", "target", "build", "site"],
    )
    def test_skipped_directories(self, tmp_path, directory):
        write(tmp_path, "Jenkinsfile", f"{directory}/Jenkinsfile")
        write(tmp_path, f"a/{directory}/Jenkinsfile")
        assert found(tmp_path) == ["Jenkinsfile"]

    def test_a_skipped_name_can_be_searched_by_asking(self, tmp_path):
        write(tmp_path, "build/Jenkinsfile")
        assert found(tmp_path / "build") == ["Jenkinsfile"]

    @pytest.mark.skipif(os.name == "nt", reason="symlinks need privileges on Windows")
    def test_symlinks(self, tmp_path):
        write(tmp_path, "real/Jenkinsfile")
        (tmp_path / "link").symlink_to(tmp_path / "real", target_is_directory=True)
        (tmp_path / "Jenkinsfile.broken").symlink_to(tmp_path / "missing")
        assert found(tmp_path) == ["real/Jenkinsfile"]


class TestExpandPaths:
    def test_files_stay_as_given(self):
        paths = ["./Jenkinsfile", "missing", "./Jenkinsfile"]
        assert expand_paths(paths) == (paths, [])

    def test_a_file_also_found_is_checked_once(self, tmp_path, monkeypatch):
        write(tmp_path, "Jenkinsfile", "ci/Jenkinsfile")
        monkeypatch.chdir(tmp_path)
        ci = os.path.join("ci", "Jenkinsfile")
        assert expand_paths([".", "./Jenkinsfile", "ci"]) == (
            [ci, "./Jenkinsfile"],
            [],
        )

    def test_empty_directory(self, tmp_path):
        assert expand_paths([str(tmp_path)]) == ([], [str(tmp_path)])


class TestMain:
    def test_checks_a_directory(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile", "ci/Jenkinsfile.prod")
        monkeypatch.chdir(tmp_path)
        assert run(".") == 0
        out = capsys.readouterr().out
        assert "✓ Jenkinsfile: Valid" in out
        assert f"✓ {os.path.join('ci', 'Jenkinsfile.prod')}: Valid" in out

    def test_one_file_found_is_named(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile")
        monkeypatch.chdir(tmp_path)
        assert run(".") == 0
        assert "✓ Jenkinsfile: Valid" in capsys.readouterr().out

    def test_one_file_by_name_stays_quiet(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile")
        monkeypatch.chdir(tmp_path)
        assert run("Jenkinsfile") == 0
        assert capsys.readouterr().out == ""

    def test_an_invalid_file_fails_the_run(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile")
        write(tmp_path, "ci/Jenkinsfile", text=INVALID)
        monkeypatch.chdir(tmp_path)
        assert run(".") == 1
        assert os.path.join("ci", "Jenkinsfile") in capsys.readouterr().err

    def test_mixing_files_and_directories(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "ci/Jenkinsfile", "other.groovy")
        monkeypatch.chdir(tmp_path)
        assert run("other.groovy", "ci") == 0
        out = capsys.readouterr().out
        assert "✓ other.groovy: Valid" in out
        assert f"✓ {os.path.join('ci', 'Jenkinsfile')}: Valid" in out

    def test_include_and_skip(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile", "Jenkinsfile.prod", "ci/Jenkinsfile")
        write(tmp_path, "old/Jenkinsfile", text=INVALID)
        monkeypatch.chdir(tmp_path)
        assert run(".", "--skip", "old/*", "--include", "Jenkinsfile") == 0
        captured = capsys.readouterr()
        assert "✓ Jenkinsfile: Valid" in captured.out
        assert f"✓ {os.path.join('ci', 'Jenkinsfile')}: Valid" in captured.out
        assert "Jenkinsfile.prod" not in captured.out
        assert captured.err == ""

    def test_empty_directory(self, tmp_path, capsys):
        assert run(str(tmp_path)) == 0
        assert capsys.readouterr().out == ""

    def test_empty_directory_verbose(self, tmp_path, capsys):
        assert run(str(tmp_path), "--verbose") == 0
        assert f"No files to check found in {tmp_path}" in capsys.readouterr().out

    @pytest.mark.parametrize("mode", ["--local", "--jenkins-url=https://j.example"])
    def test_nothing_found_starts_no_jenkins(self, tmp_path, mode):
        with (
            patch("jenkinsfilelint.cli.LocalJenkins") as local,
            patch("jenkinsfilelint.cli.JenkinsfileLinter") as linter,
        ):
            assert run(str(tmp_path), mode) == 0
        local.assert_not_called()
        linter.assert_not_called()

    def test_remote_mode_checks_found_files(self, tmp_path, monkeypatch):
        write(tmp_path, "Jenkinsfile", "ci/Jenkinsfile")
        monkeypatch.chdir(tmp_path)
        with patch("jenkinsfilelint.cli.JenkinsfileLinter") as linter:
            linter.return_value.validate.return_value = (True, "")
            assert run(".", "--jenkins-url=https://j.example") == 0
        checked = {c.args[0] for c in linter.return_value.validate.call_args_list}
        assert checked == {"Jenkinsfile", os.path.join("ci", "Jenkinsfile")}

    def test_audit_reads_the_shared_library(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "vars/deploy.groovy", "scripts/other.groovy", text=LEAK)
        monkeypatch.chdir(tmp_path)
        assert run("--audit", ".") == 1
        out = capsys.readouterr().out
        assert os.path.join("vars", "deploy.groovy") in out
        assert "other.groovy" not in out

    def test_syntax_check_leaves_the_shared_library(
        self, tmp_path, monkeypatch, capsys
    ):
        write(tmp_path, "Jenkinsfile")
        write(tmp_path, "vars/broken.groovy", text="def x = {\n")
        monkeypatch.chdir(tmp_path)
        assert run(".") == 0
        assert "broken.groovy" not in capsys.readouterr().out

    def test_empty_audit_report_stays_valid(self, tmp_path, capsys):
        assert run("--audit", "--format", "sarif", "--verbose", str(tmp_path)) == 0
        captured = capsys.readouterr()
        assert captured.out.startswith('{"$schema"')
        assert "No files to check found" in captured.err
