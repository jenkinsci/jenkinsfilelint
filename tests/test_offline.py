#!/usr/bin/env python3
"""Tests for --offline mode and the jenkinsfilelint-engine binary it runs."""

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

from jenkinsfilelint.cli import main
from jenkinsfilelint.offline import (
    ENGINE_ENV,
    ENGINE_MISSING_MESSAGE,
    OfflineEngineError,
    find_engine,
    lint_files,
)

ROOT = Path(__file__).resolve().parent.parent
CRATE = ROOT / "crates" / "jenkinsfilelint"
CASES = CRATE / "tests" / "jenkins"
NOT_DECLARATIVE = "did not contain the 'pipeline' step"


def run_cli(argv):
    """Run the CLI in-process and return its exit code."""
    with patch("sys.argv", ["jenkinsfilelint", *argv]):
        with pytest.raises(SystemExit) as exc_info:
            main()
    return exc_info.value.code


def engine_output(reports, returncode=1, stderr=""):
    """What subprocess.run returns for one engine run."""
    return subprocess.CompletedProcess(
        args=[], returncode=returncode, stdout=json.dumps(reports), stderr=stderr
    )


def cargo_version():
    text = (CRATE / "Cargo.toml").read_text(encoding="utf-8")
    return re.search(r'^version = "([^"]+)"', text, re.MULTILINE).group(1)


@pytest.fixture
def no_engine_override(monkeypatch):
    monkeypatch.delenv(ENGINE_ENV, raising=False)


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
    """jenkinsfilelint requires exactly the engine version in Cargo.toml."""

    def test_pin_matches_cargo_version(self):
        pyproject = (ROOT / "pyproject.toml").read_text(encoding="utf-8")
        pin = re.search(r"jenkinsfilelint-engine==([^;\s\"]+)", pyproject).group(1)
        assert pin == cargo_version()

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
            "jenkinsfilelint.offline.subprocess.run",
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
            "diagnostics": [{"line": 1, "column": 1, "severity": "warning", "message": "W"}],
        }
        with patch(
            "jenkinsfilelint.offline.subprocess.run",
            return_value=engine_output([report], returncode=0),
        ):
            assert lint_files([str(f)], "engine")[str(f)].is_valid

    def test_missing_file_is_not_sent_to_engine(self, tmp_path):
        missing = str(tmp_path / "nope")
        with patch("jenkinsfilelint.offline.subprocess.run") as run:
            results = lint_files([missing], "engine")
        run.assert_not_called()
        assert not results[missing].is_valid
        assert results[missing].lines == [f"File not found: {missing}"]

    def test_read_error(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkinsfilelint.offline.subprocess.run",
            return_value=engine_output([{"file": str(f), "error": "denied"}]),
        ):
            result = lint_files([str(f)], "engine")[str(f)]
        assert not result.is_valid
        assert result.lines == [f"Error reading file {f}: denied"]

    def test_engine_cannot_start(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkinsfilelint.offline.subprocess.run", side_effect=OSError("exec format")
        ):
            with pytest.raises(OfflineEngineError, match="Could not run engine"):
                lint_files([str(f)], "engine")

    def test_unexpected_output(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        crashed = subprocess.CompletedProcess(
            args=[], returncode=101, stdout="", stderr="thread 'main' panicked"
        )
        with patch("jenkinsfilelint.offline.subprocess.run", return_value=crashed):
            with pytest.raises(OfflineEngineError, match="panicked"):
                lint_files([str(f)], "engine")

    def test_file_left_out_by_engine(self, tmp_path):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkinsfilelint.offline.subprocess.run", return_value=engine_output([])
        ):
            with pytest.raises(OfflineEngineError, match="no result"):
                lint_files([str(f)], "engine")


class TestOfflineCLI:
    """--offline options, with the engine mocked."""

    @pytest.mark.parametrize(
        "other", [["--local"], ["--jenkins-url", "https://j"], ["--username", "u"], ["--token", "t"]]
    )
    def test_not_allowed_with_other_modes(self, other, capsys):
        assert run_cli(["--offline", *other, "Jenkinsfile"]) == 2
        assert "not allowed with argument" in capsys.readouterr().err

    def test_jenkins_url_env_variable_is_ignored(self, tmp_path, monkeypatch):
        monkeypatch.setenv("JENKINS_URL", "https://jenkins.example.com")
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        report = {"file": str(f), "declarative": True, "diagnostics": []}
        with (
            patch("jenkinsfilelint.cli.find_engine", return_value="engine"),
            patch(
                "jenkinsfilelint.offline.subprocess.run",
                return_value=engine_output([report], returncode=0),
            ),
            patch("jenkinsfilelint.linter.requests.post") as post,
        ):
            assert run_cli(["--offline", str(f)]) == 0
        post.assert_not_called()

    @pytest.mark.parametrize("declarative", [True, False])
    def test_passing_file_is_quiet(self, declarative, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        report = {"file": str(f), "declarative": declarative, "diagnostics": []}
        with (
            patch("jenkinsfilelint.cli.find_engine", return_value="engine"),
            patch(
                "jenkinsfilelint.offline.subprocess.run",
                return_value=engine_output([report], returncode=0),
            ),
        ):
            assert run_cli(["--offline", str(f)]) == 0
        assert capsys.readouterr() == ("", "")

    def test_engine_missing(self, tmp_path, capsys):
        f = tmp_path / "Jenkinsfile"
        f.write_text("")
        with patch(
            "jenkinsfilelint.cli.find_engine",
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
            patch("jenkinsfilelint.cli.find_engine", return_value="engine"),
            patch(
                "jenkinsfilelint.offline.subprocess.run",
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
        with patch(
            "jenkinsfilelint.cli.find_engine", side_effect=OfflineEngineError("x")
        ):
            assert run_cli(["--offline", "--skip", "Jenkinsfile", str(f)]) == 0


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

        code = run_cli(["--offline", "--verbose", path])
        captured = capsys.readouterr()

        diagnostic = re.compile(
            rf"{re.escape(path)}:\d+:\d+: (error|warning): (.*)"
        )
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

    def test_console_script(self):
        """A real process, so output encoding is exercised too."""
        proc = subprocess.run(
            [
                sys.executable,
                "-m",
                "jenkinsfilelint.cli",
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
