#!/usr/bin/env python3
"""Tests for offline checking and the jenkinsfilelint-engine binary it runs."""

import importlib.util
import json
import os
import re
import subprocess
import sys
import types
from importlib import metadata
from pathlib import Path
from unittest.mock import Mock, patch

import pytest

from jenkins_lint.cli import OFFLINE_NOTE, REMOVED_LOCAL, main
from jenkins_lint.offline import (
    ENGINE_ENV,
    OLD_ENGINE_ENV,
    ENGINE_MISSING_MESSAGE,
    OfflineEngineError,
    find_engine,
    lint_files,
)

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "crates" / "jenkinsfilelint"
CASES = CRATE / "tests" / "jenkins"
NOT_DECLARATIVE = "did not contain the 'pipeline' step"
VALID_PIPELINE = (
    "pipeline {\n  agent any\n  stages {\n"
    "    stage('A') {\n      steps {\n        echo 'x'\n      }\n    }\n  }\n}\n"
)


def run_cli(argv):
    """Run the CLI in-process and return its exit code."""
    with patch("sys.argv", ["jenkins-lint", *argv]):
        with pytest.raises(SystemExit) as exc_info:
            main()
    return exc_info.value.code


def engine_output(reports, returncode=1, stderr=""):
    """What subprocess.run returns for one engine run."""
    return subprocess.CompletedProcess(
        args=[], returncode=returncode, stdout=json.dumps(reports), stderr=stderr
    )


# The parsers of crates/jenkinsfilelint/scripts/check_engine_version.py,
# which the release workflow runs.
_spec = importlib.util.spec_from_file_location(
    "check_engine_version", CRATE / "scripts" / "check_engine_version.py"
)
check_engine_version = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(check_engine_version)


def cargo_version():
    text = (CRATE / "Cargo.toml").read_text(encoding="utf-8")
    return check_engine_version.cargo_version(text)


@pytest.fixture
def no_engine_override(monkeypatch):
    monkeypatch.delenv(ENGINE_ENV, raising=False)
    monkeypatch.delenv(OLD_ENGINE_ENV, raising=False)


@pytest.fixture(scope="session")
def engine():
    """The real engine; required in CI, skipped locally when not installed."""
    try:
        return find_engine()
    except OfflineEngineError:
        if os.environ.get("CI"):
            pytest.fail("jenkinsfilelint-engine must be installed to run these tests")
        pytest.skip("jenkinsfilelint-engine is not installed")


class TestEnginePin:
    """jenkins-lint requires exactly the engine version in Cargo.toml."""

    def test_pin_matches_cargo_version(self):
        lines = check_engine_version.check_pin(ROOT)
        assert not lines[0].startswith("error:"), lines

    def test_installed_engine_matches_cargo_version(self):
        try:
            installed = metadata.version("jenkinsfilelint-engine")
        except metadata.PackageNotFoundError:
            pytest.skip("jenkinsfilelint-engine is not installed")
        assert installed == cargo_version()


class TestFindEngine:
    def test_env_variable_overrides(self, monkeypatch):
        monkeypatch.setenv(ENGINE_ENV, "/opt/engine")
        assert find_engine() == "/opt/engine"

    def test_old_env_variable_still_works(self, no_engine_override, monkeypatch):
        monkeypatch.setenv(OLD_ENGINE_ENV, "/opt/old-engine")
        assert find_engine() == "/opt/old-engine"

    def test_package_not_installed(self, no_engine_override):
        with patch.dict(sys.modules, {"jenkinsfilelint_engine": None}):
            with pytest.raises(OfflineEngineError, match="not installed"):
                find_engine()

    def test_binary_missing_from_package(self, no_engine_override):
        package = types.ModuleType("jenkinsfilelint_engine")
        package.find_bin = Mock(side_effect=FileNotFoundError("gone"))
        with patch.dict(sys.modules, {"jenkinsfilelint_engine": package}):
            with pytest.raises(OfflineEngineError, match="not installed"):
                find_engine()

    def test_installed_package(self, no_engine_override):
        package = types.ModuleType("jenkinsfilelint_engine")
        package.find_bin = Mock(return_value="/venv/bin/jenkinsfilelint-engine")
        with patch.dict(sys.modules, {"jenkinsfilelint_engine": package}):
            assert find_engine() == "/venv/bin/jenkinsfilelint-engine"


class TestLintFiles:
    """Parsing the engine's JSON, with the engine mocked."""

    def test_reports_become_results(self, tmp_path):
        good = tmp_path / "Jenkinsfile"
        bad = tmp_path / "bad.groovy"
        scripted = tmp_path / "scripted.groovy"
        for f in (good, bad, scripted):
            f.write_text("")
        reports = [
            {"file": str(good), "declarative": True, "diagnostics": []},
            {
                "file": str(bad),
                "declarative": True,
                "diagnostics": [
                    {"line": 2, "column": 5, "severity": "error", "message": "E"},
                    {"line": 3, "column": 1, "severity": "warning", "message": "W"},
                ],
            },
            {"file": str(scripted), "declarative": False, "diagnostics": []},
        ]
        with patch(
            "jenkins_lint.offline.subprocess.run",
            return_value=engine_output(reports),
        ) as run:
            results = lint_files([str(good), str(bad), str(scripted)], "engine")

        assert run.call_args.args[0] == [
            "engine",
            "--format",
            "json",
            "--",
            str(good),
            str(bad),
            str(scripted),
        ]
        assert results[str(good)].is_valid and results[str(good)].declarative
        assert not results[str(bad)].is_valid
        assert results[str(bad)].lines == [
            f"{bad}:2:5: error: E",
            f"{bad}:3:1: warning: W",
        ]
        assert results[str(scripted)].is_valid
        assert not results[str(scripted)].declarative

    def test_warnings_alone_do_not_fail(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        report = {
            "file": str(f),
            "declarative": True,
            "diagnostics": [
                {"line": 1, "column": 1, "severity": "warning", "message": "W"}
            ],
        }
        with patch(
            "jenkins_lint.offline.subprocess.run",
            return_value=engine_output([report], returncode=0),
        ):
            assert lint_files([str(f)], "engine")[str(f)].is_valid

    def test_missing_file_is_not_sent_to_engine(self, tmp_path):
        missing = str(tmp_path / "nope")
        with patch("jenkins_lint.offline.subprocess.run") as run:
            results = lint_files([missing], "engine")
        run.assert_not_called()
        assert not results[missing].is_valid
        assert results[missing].lines == [f"File not found: {missing}"]

    def test_read_error(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.offline.subprocess.run",
            return_value=engine_output([{"file": str(f), "error": "denied"}]),
        ):
            result = lint_files([str(f)], "engine")[str(f)]
        assert not result.is_valid
        assert result.lines == [f"Error reading file {f}: denied"]

    def test_engine_cannot_start(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.offline.subprocess.run", side_effect=OSError("exec format")
        ):
            with pytest.raises(OfflineEngineError, match="Could not run engine"):
                lint_files([str(f)], "engine")

    def test_unexpected_output(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        crashed = subprocess.CompletedProcess(
            args=[], returncode=101, stdout="", stderr="thread 'main' panicked"
        )
        with patch("jenkins_lint.offline.subprocess.run", return_value=crashed):
            with pytest.raises(OfflineEngineError, match="panicked"):
                lint_files([str(f)], "engine")

    @pytest.mark.parametrize(
        "report",
        [
            {"file": "x"},
            1,
            "text",
            {"file": "x", "declarative": True, "diagnostics": [{"line": 1}]},
            {"file": "x", "declarative": "yes", "diagnostics": []},
            {"file": "x", "declarative": True, "diagnostics": "none"},
        ],
        ids=[
            "no-diagnostics",
            "number",
            "string",
            "diagnostic-fields",
            "declarative-type",
            "diagnostics-type",
        ],
    )
    def test_malformed_report(self, report, tmp_path):
        """An engine that answers in another shape gives a clean error."""
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.offline.subprocess.run",
            return_value=engine_output([report]),
        ):
            with pytest.raises(
                OfflineEngineError, match="Unexpected output from engine"
            ):
                lint_files([str(f)], "engine")

    def test_file_left_out_by_engine(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.offline.subprocess.run", return_value=engine_output([])
        ):
            with pytest.raises(
                OfflineEngineError, match=r"returned 0 result\(s\) for 1 file"
            ):
                lint_files([str(f)], "engine")


class TestOfflineCLI:
    """Offline options, with the engine mocked."""

    @pytest.mark.parametrize(
        "other", [["--remote", "https://j"], ["--jenkins-url", "https://j"]]
    )
    def test_offline_not_allowed_with_remote(self, other, capsys):
        assert run_cli(["--offline", *other, "Jenkinsfile"]) == 2
        assert "not allowed with argument --remote" in capsys.readouterr().err

    @pytest.mark.parametrize("option", ["--username", "--token"])
    def test_credentials_require_remote(self, option, capsys):
        assert run_cli([option, "x", "Jenkinsfile"]) == 2
        assert f"argument {option}: requires --remote" in capsys.readouterr().err

    def test_local_was_removed(self, capsys):
        assert run_cli(["--local", "Jenkinsfile"]) == 2
        assert REMOVED_LOCAL in capsys.readouterr().err

    def test_server_command_was_removed(self, capsys):
        assert run_cli(["server", "start"]) == 2
        assert REMOVED_LOCAL in capsys.readouterr().err

    def test_jenkins_url_env_variable_is_ignored(self, tmp_path, monkeypatch):
        monkeypatch.setenv("JENKINS_URL", "https://jenkins.example.com")
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        report = {"file": str(f), "declarative": True, "diagnostics": []}
        with (
            patch("jenkins_lint.cli.find_engine", return_value="engine"),
            patch(
                "jenkins_lint.offline.subprocess.run",
                return_value=engine_output([report], returncode=0),
            ),
            patch("jenkins_lint.linter.requests.Session.post") as post,
        ):
            assert run_cli(["--offline", str(f)]) == 0
        post.assert_not_called()

    @pytest.mark.parametrize("declarative", [True, False])
    def test_passing_file_is_quiet(self, declarative, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        report = {"file": str(f), "declarative": declarative, "diagnostics": []}
        with (
            patch("jenkins_lint.cli.find_engine", return_value="engine"),
            patch(
                "jenkins_lint.offline.subprocess.run",
                return_value=engine_output([report], returncode=0),
            ),
        ):
            assert run_cli(["--offline", str(f)]) == 0
        assert capsys.readouterr() == ("", "")

    def test_engine_missing(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.cli.find_engine",
            side_effect=OfflineEngineError(ENGINE_MISSING_MESSAGE),
        ):
            assert run_cli(["--offline", str(f)]) == 1
        assert f"✗ {ENGINE_MISSING_MESSAGE}" in capsys.readouterr().err

    def test_skip_and_include(self, tmp_path, capsys):
        keep = tmp_path / "Jenkinsfile"
        skipped = tmp_path / "vars" / "step.groovy"
        other = tmp_path / "README.md"
        for f in (keep, other):
            f.write_text("")
        skipped.parent.mkdir()
        skipped.write_text("")
        report = {"file": str(keep), "declarative": True, "diagnostics": []}
        with (
            patch("jenkins_lint.cli.find_engine", return_value="engine"),
            patch(
                "jenkins_lint.offline.subprocess.run",
                return_value=engine_output([report], returncode=0),
            ) as run,
        ):
            code = run_cli(
                [
                    "--offline",
                    "--verbose",
                    "--include",
                    "Jenkinsfile",
                    "--include",
                    "*.groovy",
                    "--skip",
                    "vars/*.groovy",
                    str(keep),
                    str(skipped),
                    str(other),
                ]
            )
        assert code == 0
        assert run.call_args.args[0][4:] == [str(keep)]
        out = capsys.readouterr().out
        assert f"✓ {keep}: Valid" in out
        assert f"⊘ {skipped}: Skipped (matches skip pattern)" in out
        assert f"⊘ {other}: Skipped (does not match include pattern)" in out

    def test_everything_skipped_needs_no_engine(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        with patch("jenkins_lint.cli.find_engine", side_effect=OfflineEngineError("x")):
            assert run_cli(["--offline", "--skip", "Jenkinsfile", str(f)]) == 0


class TestDefaultMode:
    """Files are checked offline unless --remote asks for Jenkins too.

    conftest.py clears JENKINS_URL, JENKINS_USER and JENKINS_TOKEN.
    """

    def offline_run(self, tmp_path, argv, reports=None, returncode=0):
        """Run the CLI with the engine mocked; return (code, engine run)."""
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        if reports is None:
            reports = [{"file": str(f), "declarative": True, "diagnostics": []}]
        with (
            patch("jenkins_lint.cli.find_engine", return_value="engine"),
            patch(
                "jenkins_lint.offline.subprocess.run",
                return_value=engine_output(reports, returncode=returncode),
            ) as run,
            patch("jenkins_lint.linter.requests.Session.post") as post,
        ):
            code = run_cli([*argv, str(f)])
        post.assert_not_called()
        return code, run

    def test_checks_offline_quietly(self, tmp_path, capsys):
        code, run = self.offline_run(tmp_path, [])
        assert code == 0
        run.assert_called_once()
        assert capsys.readouterr() == ("", "")

    def test_errors_come_without_a_note(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        report = {
            "file": str(f),
            "declarative": True,
            "diagnostics": [
                {"line": 1, "column": 1, "severity": "error", "message": "E"}
            ],
        }
        code, _ = self.offline_run(tmp_path, [], reports=[report], returncode=1)
        assert code == 1
        assert capsys.readouterr() == ("", f"{f}:1:1: error: E\n")

    def test_verbose_says_what_offline_skips(self, tmp_path, capsys):
        code, _ = self.offline_run(tmp_path, ["--verbose"])
        assert code == 0
        captured = capsys.readouterr()
        assert captured.out.splitlines()[:2] == [
            OFFLINE_NOTE,
            "✓ Offline engine: engine",
        ]
        assert captured.err == ""

    def test_offline_flag_changes_nothing(self, tmp_path, capsys):
        """--offline is what happens anyway; scripts from before 2.0 keep working."""
        code, run = self.offline_run(tmp_path, ["--offline"])
        assert code == 0
        run.assert_called_once()
        assert capsys.readouterr() == ("", "")

    def test_no_note_when_every_file_is_skipped(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        with patch("jenkins_lint.cli.find_engine", side_effect=OfflineEngineError("x")):
            assert run_cli(["--verbose", "--skip", "Jenkinsfile", str(f)]) == 0
        captured = capsys.readouterr()
        assert OFFLINE_NOTE not in captured.out + captured.err

    def test_no_engine(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkins_lint.cli.find_engine",
            side_effect=OfflineEngineError(ENGINE_MISSING_MESSAGE),
        ):
            assert run_cli([str(f)]) == 1
        assert capsys.readouterr().err.splitlines() == [f"✗ {ENGINE_MISSING_MESSAGE}"]

    @pytest.mark.parametrize("name", ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN"])
    def test_environment_alone_stays_offline(self, name, tmp_path, monkeypatch):
        """Jenkins sets JENKINS_URL in every build: it must not switch modes."""
        monkeypatch.setenv(name, "https://jenkins.example.com")
        code, run = self.offline_run(tmp_path, [])
        assert code == 0
        run.assert_called_once()

    @pytest.mark.parametrize(
        "argv",
        [
            ["--remote", "https://jenkins.example.com"],
            ["--jenkins-url=https://jenkins.example.com"],
        ],
        ids=["--remote", "--jenkins-url"],
    )
    def test_remote_selects_jenkins(self, argv, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch("jenkins_lint.cli._run_remote_validation") as remote:
            with patch("sys.argv", ["jenkins-lint", *argv, str(f)]):
                main()
        remote.assert_called_once()
        assert remote.call_args.args[1] == "https://jenkins.example.com"

    def test_bare_remote_uses_jenkins_url(self, tmp_path, monkeypatch):
        monkeypatch.setenv("JENKINS_URL", "https://from-env.example.com")
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch("jenkins_lint.cli._run_remote_validation") as remote:
            with patch("sys.argv", ["jenkins-lint", "--remote", str(f)]):
                main()
        assert remote.call_args.args[1] == "https://from-env.example.com"
        assert remote.call_args.args[0].jenkinsfile == [str(f)]

    def test_bare_remote_without_jenkins_url(self, capsys):
        assert run_cli(["--remote", "Jenkinsfile"]) == 2
        assert "no URL given and JENKINS_URL is not set" in capsys.readouterr().err


class TestRemote:
    """--remote: offline first, then Jenkins for the files that pass."""

    def remote_run(self, tmp_path, reports, engine=True):
        good = tmp_path / "Jenkinsfile"
        bad = tmp_path / "Jenkinsfile.bad"
        for f in (good, bad):
            f.write_text("pipeline {}")
        response = Mock(status_code=200, text="Jenkinsfile successfully validated.")
        find = (
            {"return_value": "engine"}
            if engine
            else {"side_effect": OfflineEngineError(ENGINE_MISSING_MESSAGE)}
        )
        with (
            patch("jenkins_lint.cli.find_engine", **find),
            patch(
                "jenkins_lint.offline.subprocess.run",
                return_value=engine_output(reports(good, bad), returncode=1),
            ),
            patch(
                "jenkins_lint.linter.requests.Session.post", return_value=response
            ) as post,
        ):
            code = run_cli(["--remote", "https://j.example", str(good), str(bad)])
        return code, post, good, bad

    def test_offline_errors_are_not_sent(self, tmp_path, capsys):
        def reports(good, bad):
            error = {"line": 1, "column": 1, "severity": "error", "message": "E"}
            return [
                {"file": str(good), "declarative": True, "diagnostics": []},
                {"file": str(bad), "declarative": True, "diagnostics": [error]},
            ]

        code, post, good, bad = self.remote_run(tmp_path, reports)
        assert code == 1
        post.assert_called_once()
        assert post.call_args.kwargs["data"]["jenkinsfile"] == "pipeline {}"
        captured = capsys.readouterr()
        assert f"{bad}:1:1: error: E" in captured.err
        assert f"✓ {good}: Valid" in captured.out

    def test_without_engine_every_file_is_sent(self, tmp_path):
        code, post, _, _ = self.remote_run(tmp_path, lambda g, b: [], engine=False)
        assert code == 0
        assert post.call_count == 2


def load_cases():
    """Read tests/jenkins/expected.txt: case name -> its lines, comments included."""
    cases = {}
    current = None
    for line in (CASES / "expected.txt").read_text(encoding="utf-8").splitlines():
        header = re.fullmatch(r"\[(.+)\]", line)
        if header:
            current = header.group(1)
            cases[current] = []
        elif current and line:
            cases[current].append(line)
    return cases


EXPECTED = load_cases()


@pytest.mark.usefixtures("engine")
class TestAgainstJenkinsVerdicts:
    """End to end: the CLI and the real engine on the cases Jenkins judged."""

    def test_every_case_has_a_file(self):
        files = {p.stem for p in CASES.glob("*.groovy")}
        assert files == set(EXPECTED)

    @pytest.mark.parametrize("name", sorted(EXPECTED))
    def test_case(self, name, capsys):
        want = EXPECTED[name]
        path = str(CASES / f"{name}.groovy")

        code = run_cli(["--verbose", path])
        captured = capsys.readouterr()

        diagnostic = re.compile(rf"{re.escape(path)}:\d+:\d+: (error|warning): (.*)")
        found = {"error": set(), "warning": set()}
        for line in captured.err.splitlines():
            match = diagnostic.fullmatch(line)
            assert match, f"not a file:line:col diagnostic: {line!r}"
            found[match.group(1)].add(match.group(2))

        want_errors = {w[len("error: ") :] for w in want if w.startswith("error: ")}
        want_warnings = {
            w[len("warning: ") :] for w in want if w.startswith("warning: ")
        }
        assert found["warning"] == want_warnings

        if "valid" in want:
            assert code == 0
            assert not found["error"]
            if any(NOT_DECLARATIVE in w for w in want):
                assert f"⊘ {path}: Skipped (not a Declarative Pipeline)" in captured.out
            else:
                assert f"✓ {path}: Valid" in captured.out
        else:
            assert code == 1
            if want_errors:
                assert found["error"] == want_errors
            else:
                assert found["error"]

    def test_all_cases_in_one_run(self, capsys):
        paths = [str(CASES / f"{name}.groovy") for name in sorted(EXPECTED)]
        assert run_cli(["--offline", *paths]) == 1
        captured = capsys.readouterr()
        for name, want in EXPECTED.items():
            path = str(CASES / f"{name}.groovy")
            if "valid" in want:
                assert f"{path}: " in captured.out
            else:
                assert f"{path}:" in captured.err

    def test_unreadable_file(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_bytes(b"pipeline { agent \xff }")
        assert run_cli(["--offline", str(f)]) == 1
        assert f"Error reading file {f}: " in capsys.readouterr().err

    @pytest.mark.skipif(
        sys.platform != "linux", reason="needs a file system that allows any bytes"
    )
    def test_file_name_not_utf8(self, tmp_path, capsys):
        good = tmp_path / "Jenkinsfile"
        good.write_text(VALID_PIPELINE)
        odd = os.fsdecode(bytes(tmp_path) + b"/caf\xe9.groovy")
        Path(odd).write_text(VALID_PIPELINE)
        assert run_cli(["--offline", str(good), odd]) == 0
        out = capsys.readouterr().out
        assert f"✓ {good}: Valid" in out
        # Printed escaped, since a strict UTF-8 stream cannot show it.
        assert "caf\\udce9.groovy: Valid" in out

    def test_console_script(self):
        """A real process, so output encoding is exercised too."""
        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "jenkins_lint.cli",
                "--offline",
                str(ROOT / "tests" / "Jenkinsfile"),
                str(ROOT / "tests" / "Jenkinsfile.fail"),
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        assert proc.returncode == 1, proc.stderr
        assert f"✓ {ROOT / 'tests' / 'Jenkinsfile'}: Valid" in proc.stdout
        assert (
            f"{ROOT / 'tests' / 'Jenkinsfile.fail'}:2:5: error: "
            'Not a valid section definition: "agent".'
        ) in proc.stderr

    def test_console_script_without_jenkins(self):
        """No flag and no Jenkins configured: the real engine checks the files."""
        env = {k: v for k, v in os.environ.items() if not k.startswith("JENKINS_")}
        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "jenkins_lint.cli",
                str(ROOT / "tests" / "Jenkinsfile"),
                str(ROOT / "tests" / "Jenkinsfile.fail"),
            ],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=env,
            check=False,
        )
        assert proc.returncode == 1, proc.stderr
        assert f"✓ {ROOT / 'tests' / 'Jenkinsfile'}: Valid" in proc.stdout
        assert OFFLINE_NOTE not in proc.stdout + proc.stderr
        lines = proc.stderr.splitlines()
        assert lines[0].startswith(
            f"{ROOT / 'tests' / 'Jenkinsfile.fail'}:2:5: error: "
            'Not a valid section definition: "agent".'
        )
