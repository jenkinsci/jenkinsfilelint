#!/usr/bin/env python3
"""Tests for searching directories for the files to check."""

import os
from pathlib import Path
from unittest.mock import patch

import pytest

from jenkins_lint.cli import expand_paths, find_files, looks_like_pipeline, main

VALID = "pipeline {\n  agent any\n  stages { stage('a') { steps { echo 'a' } } }\n}\n"
INVALID = "pipeline {\n  agent any\n}\n"
SCRIPTED = "node {\n    sh 'make'\n}\n"
HELPER = "def greet(name) {\n    echo name\n}\n"
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
    with patch("sys.argv", ["jenkins-lint", *argv]):
        with pytest.raises(SystemExit) as exc_info:
            main()
    return exc_info.value.code


@pytest.fixture(autouse=True)
def no_jenkins(monkeypatch):
    for name in ("JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN"):
        monkeypatch.delenv(name, raising=False)


class TestFindFiles:
    @pytest.mark.parametrize(
        "name, text, picked",
        [
            ("Jenkinsfile", INVALID, True),
            ("Jenkinsfile.prod", VALID, True),
            ("deploy.jenkinsfile", INVALID, True),
            ("jenkinsfile", VALID, True),
            ("JenkinsFile", VALID, True),
            ("JENKINSFILE", VALID, True),
            ("Jenkinsfile-prod", VALID, True),
            ("Jenkinsfile_prod", VALID, True),
            ("ci/deploy.groovy", VALID, True),
            ("pipelines/release.groovy", SCRIPTED, True),
            (".jenkins/Jenkinsfile", VALID, True),
            (".ci/Jenkinsfile", VALID, True),
            ("build/Jenkinsfile", VALID, True),
            ("jenkins/build.pipeline", VALID, False),
            ("ci/deploy.jenkins", VALID, False),
            ("Jenkinsfile.md", VALID, False),
            ("Jenkinsfile.bak", VALID, False),
            ("Jenkinsfile.dockerfile", "FROM alpine\n", False),
        ],
    )
    def test_names(self, tmp_path, name, text, picked):
        write(tmp_path, name, text=text)
        assert found(tmp_path) == ([name] if picked else [])

    def test_a_plain_helper_is_not_picked(self, tmp_path):
        write(tmp_path, "scripts/helper.groovy", "Jenkinsfile-notes", text=HELPER)
        assert found(tmp_path) == []

    def test_library_files_only_with_audit(self, tmp_path):
        write(tmp_path, "Jenkinsfile")
        write(
            tmp_path,
            "vars/deploy.groovy",
            "src/com/acme/Util.groovy",
            "scripts/other.groovy",
            "vars/notes.txt",
            text=HELPER,
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
        write(tmp_path, "src/project/tools/helper.groovy", text=HELPER)
        assert found(tmp_path / "src" / "project", audit=True) == []

    @pytest.mark.parametrize(
        "directory",
        [".git", ".venv", ".tox", "venv", "node_modules", "target", "site"],
    )
    def test_skipped_directories(self, tmp_path, directory):
        write(tmp_path, "Jenkinsfile", f"{directory}/Jenkinsfile")
        write(tmp_path, f"a/{directory}/Jenkinsfile")
        assert found(tmp_path) == ["Jenkinsfile"]

    def test_a_skipped_name_can_be_searched_by_asking(self, tmp_path):
        write(tmp_path, "target/Jenkinsfile")
        assert found(tmp_path / "target") == ["Jenkinsfile"]

    @pytest.mark.parametrize("build_file", ["build.gradle", "pyproject.toml"])
    def test_build_output_is_left_out(self, tmp_path, build_file):
        write(tmp_path, "Jenkinsfile", "build/Jenkinsfile")
        write(tmp_path, build_file, text="")
        assert found(tmp_path) == ["Jenkinsfile"]

    @pytest.mark.skipif(os.name == "nt", reason="symlinks need privileges on Windows")
    def test_symlinks(self, tmp_path):
        write(tmp_path, "real/Jenkinsfile")
        (tmp_path / "link").symlink_to(tmp_path / "real", target_is_directory=True)
        (tmp_path / "Jenkinsfile.broken").symlink_to(tmp_path / "missing")
        assert found(tmp_path) == ["real/Jenkinsfile"]


class TestLooksLikePipeline:
    @pytest.mark.parametrize(
        "text",
        [
            VALID,
            SCRIPTED,
            "node('linux') {\n    sh 'make'\n}\n",
            "node ('linux') {\n}\n",
            "@Library('lib@v1') _\n\npipeline {\n}\n",
            "#!/usr/bin/env groovy\n// a comment\nnode {\n}\n",
            "/* a comment */ pipeline {\n}\n",
            "/* a\n comment */\npipeline {\n}\n",
            "\ufeffpipeline {\n}\n",
            "import groovy.json.JsonSlurper\ndef v = 'a'\nnode {\n}\n",
            "def v = 1; node {\n}\n",
            "def helper() {\n    echo 'a'\n}\nnode {\n}\n",
            "def p = ~/a{2}/\nnode {\n}\n",
            "}\nnode {\n}\n",
        ],
    )
    def test_pipelines(self, tmp_path, text):
        path = tmp_path / "a.groovy"
        path.write_text(text, encoding="utf-8")
        assert looks_like_pipeline(str(path))

    @pytest.mark.parametrize(
        "text",
        [
            HELPER,
            "",
            "// pipeline {\n",
            "/*\npipeline {\n}\n*/\n",
            "def a = 'pipeline {'\n",
            'def a = "node {"\n',
            "def a = '''\npipeline {\n}\n'''\n",
            'def a = """\nnode {\n}\n"""\n',
            "def a = $/\nnode {\n}\n/$\n",
            "def a = /pipeline {/\n",
            "def call() {\n    pipeline {\n    }\n}\n",
            "def call() {\n    node {\n    }\n}\n",
            "def node(String label) {\n    echo label\n}\n",
            "String node = 'a'\n",
            "helper.node {\n}\n",
            "pipelines {\n}\n",
        ],
    )
    def test_not_pipelines(self, tmp_path, text):
        path = tmp_path / "a.groovy"
        path.write_text(text, encoding="utf-8")
        assert not looks_like_pipeline(str(path))

    def test_unreadable_file(self, tmp_path):
        path = tmp_path / "a.groovy"
        path.write_text(VALID)
        with patch("builtins.open", side_effect=OSError):
            assert not looks_like_pipeline(str(path))


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

    @pytest.mark.parametrize(
        "mode", ["--remote=https://j.example", "--jenkins-url=https://j.example"]
    )
    def test_nothing_found_calls_no_jenkins(self, tmp_path, mode):
        with patch("jenkins_lint.cli.JenkinsfileLinter") as linter:
            assert run(str(tmp_path), mode) == 0
        linter.assert_not_called()

    def test_remote_mode_checks_found_files(self, tmp_path, monkeypatch):
        write(tmp_path, "Jenkinsfile", "ci/Jenkinsfile")
        monkeypatch.chdir(tmp_path)
        with patch("jenkins_lint.cli.JenkinsfileLinter") as linter:
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

    def test_check_reads_the_shared_library(self, tmp_path, monkeypatch, capsys):
        """Jenkins compiles vars/ and src/ too, so their syntax errors count."""
        write(tmp_path, "Jenkinsfile")
        write(tmp_path, "vars/broken.groovy", text="def x = {\n")
        monkeypatch.chdir(tmp_path)
        assert run(".") == 1
        assert "broken.groovy:" in capsys.readouterr().err

    def test_empty_audit_report_stays_valid(self, tmp_path, capsys):
        assert run("--audit", "--format", "sarif", "--verbose", str(tmp_path)) == 0
        captured = capsys.readouterr()
        assert captured.out.startswith('{"$schema"')
        assert "No files to check found" in captured.err
