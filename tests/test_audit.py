#!/usr/bin/env python3
"""Tests for --audit, the security checks run by the jenkinsfilelint-engine."""

import json
import os
import re
import subprocess
from pathlib import Path
from unittest.mock import patch

import pytest

from jenkinsfilelint.cli import main
from jenkinsfilelint.offline import (
    AuditResult,
    Finding,
    OfflineEngineError,
    audit_files,
    audit_report,
    find_engine,
)

ROOT = Path(__file__).resolve().parent.parent
CASES = ROOT / "crates" / "jenkinsfilelint" / "tests" / "audit"
HELP = "https://github.com/jenkinsci/jenkinsfilelint/blob/main/docs/audit/secret-interpolation.md"
LEAKS = (
    "withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', "
    "passwordVariable: 'PASS')]) {\n"
    '    sh "curl -u ${USER}:${PASS} https://example.com"\n'
    "}\n"
)
SAFE = (
    "withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'USER', "
    "passwordVariable: 'PASS')]) {\n"
    "    sh 'curl -u \"$USER:$PASS\" https://example.com'\n"
    "}\n"
)


@pytest.fixture(scope="module", autouse=True)
def engine():
    """The real engine; required in CI, skipped locally when not installed."""
    try:
        return find_engine()
    except OfflineEngineError:
        if os.environ.get("CI"):
            pytest.fail("jenkinsfilelint-engine must be installed to run these tests")
        pytest.skip("jenkinsfilelint-engine is not installed")


def run_cli(argv):
    """Run the CLI in-process and return its exit code."""
    with patch("sys.argv", ["jenkinsfilelint", *argv]):
        with pytest.raises(SystemExit) as exc_info:
            main()
    return exc_info.value.code


def write(tmp_path, name, text):
    path = tmp_path / name
    path.write_text(text, encoding="utf-8")
    return str(path)


class TestAuditFiles:
    def test_findings(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        result = audit_files([f])[f]
        assert result.errors == []
        assert [
            (x.line, x.column, x.severity, x.confidence) for x in result.findings
        ] == [(2, 25, "high", "high")]
        finding = result.findings[0]
        assert finding.rule == "secret-interpolation"
        assert finding.message.startswith("PASS is a secret")
        assert finding.help == HELP

    def test_persona_and_filters(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        pedantic = audit_files([f], persona="pedantic")[f].findings
        assert [x.severity for x in pedantic] == ["low", "high"]
        high = audit_files([f], persona="pedantic", min_severity="high")[f].findings
        assert [x.severity for x in high] == ["high"]
        assert audit_files([f], min_confidence="high")[f].findings == high

    def test_safe_file(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", SAFE)
        assert audit_files([f])[f] == AuditResult([], [])

    def test_syntax_error_and_unreadable_file(self, tmp_path):
        broken = write(tmp_path, "Jenkinsfile", 'sh "${a +}"\n')
        missing = str(tmp_path / "missing")
        results = audit_files([broken, missing])
        assert results[broken].findings == []
        assert results[broken].errors == [
            f"{broken}:1:10: error: Unexpected '}}', expected an expression"
        ]
        assert results[missing].errors[0].startswith(f"Error reading file {missing}: ")

    @pytest.mark.parametrize(
        "report",
        [
            {"file": "x"},
            {"file": "x", "findings": [{"rule": "r"}], "diagnostics": []},
            {"file": "x", "findings": [], "diagnostics": [{"line": 1}]},
            1,
        ],
        ids=["no-findings", "finding-fields", "diagnostic-fields", "number"],
    )
    def test_malformed_report(self, report):
        out = subprocess.CompletedProcess([], 1, json.dumps([report]), "")
        with patch("jenkinsfilelint.offline.subprocess.run", return_value=out):
            with pytest.raises(OfflineEngineError, match="Unexpected output from e"):
                audit_files(["Jenkinsfile"], "e")

    def test_every_case(self):
        """The rule's cases through the Python side, in one engine run."""
        for rule in CASES.iterdir():
            expected, name = {}, None
            for line in (
                (rule / "expected.txt").read_text(encoding="utf-8").splitlines()
            ):
                if line.startswith("["):
                    name = line.strip("[]")
                    expected[name] = []
                elif line and not line.startswith("#") and line != "none":
                    expected[name].append(" ".join(line.split()[:3]))
            paths = {str(rule / f"{n}.groovy"): n for n in expected}
            results = audit_files(list(paths), persona="auditor")
            for path, n in paths.items():
                got = [
                    f"{x.line}:{x.column} {x.severity} {x.confidence}"
                    for x in results[path].findings
                ]
                assert got == expected[n], n


class TestAuditReport:
    def test_json(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        output, errors, code = audit_report([f], "json")
        assert (errors, code) == ("", 1)
        assert json.loads(output)[0]["findings"][0]["rule"] == "secret-interpolation"

    def test_sarif(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        output, _, code = audit_report([f], "sarif", persona="pedantic")
        assert code == 0  # a report with findings still uploads
        sarif = json.loads(output)
        assert sarif["version"] == "2.1.0"
        run = sarif["runs"][0]
        assert run["tool"]["driver"]["name"] == "jenkinsfilelint"
        rule = run["tool"]["driver"]["rules"][0]
        assert rule["id"] == "secret-interpolation"
        assert rule["properties"]["tags"] == ["security"]
        assert rule["helpUri"] == HELP
        levels = [
            (r["level"], r["locations"][0]["physicalLocation"]["region"])
            for r in run["results"]
        ]
        assert levels == [
            (
                "note",
                {"startLine": 2, "startColumn": 17, "endLine": 2, "endColumn": 24},
            ),
            (
                "error",
                {"startLine": 2, "startColumn": 25, "endLine": 2, "endColumn": 32},
            ),
        ]

    def test_engine_fails(self):
        crashed = subprocess.CompletedProcess([], 101, "", "thread 'main' panicked")
        with patch("jenkinsfilelint.offline.subprocess.run", return_value=crashed):
            with pytest.raises(OfflineEngineError, match="panicked"):
                audit_report(["Jenkinsfile"], "sarif", "e")
        silent = subprocess.CompletedProcess([], 2, "", "")
        with patch("jenkinsfilelint.offline.subprocess.run", return_value=silent):
            with pytest.raises(OfflineEngineError, match="exit code 2"):
                audit_report(["Jenkinsfile"], "json", "e")


class TestAuditCLI:
    def test_findings(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--persona", "pedantic", f]) == 1
        out = capsys.readouterr().out.splitlines()
        assert re.match(
            rf"{re.escape(f)}:2:17: low: secret-interpolation: USER ", out[0]
        )
        assert out[1] == f"  help: {HELP}"
        assert out[2].startswith(
            f"{f}:2:25: high: secret-interpolation: PASS is a secret"
        )
        assert out[-1] == "2 findings (1 high, 1 low)"

    def test_one_finding(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", f]) == 1
        assert capsys.readouterr().out.splitlines()[-1] == "1 finding (1 high)"

    def test_no_findings_is_quiet(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", SAFE)
        assert run_cli(["--audit", f]) == 0
        assert capsys.readouterr().out == ""

    def test_verbose(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", SAFE)
        skipped = write(tmp_path, "skipped.groovy", LEAKS)
        assert run_cli(["--audit", "-v", "--skip", "*skipped*", f, skipped]) == 0
        out = capsys.readouterr().out
        assert "✓ Offline engine: " in out
        assert f"✓ {f}: No findings" in out
        assert f"⊘ {skipped}: Skipped (matches skip pattern)" in out

    def test_syntax_error(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", 'sh "${a +}"\n')
        assert run_cli(["--audit", f]) == 1
        captured = capsys.readouterr()
        assert captured.err.startswith(f"{f}:1:10: error: ")
        assert captured.out == ""

    def test_everything_skipped_needs_no_engine(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        with patch("jenkinsfilelint.cli.find_engine") as find:
            assert run_cli(["--audit", "--skip", "*", f]) == 0
        find.assert_not_called()

    def test_json_and_sarif_output(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--format", "json", f]) == 1
        assert json.loads(capsys.readouterr().out)[0]["file"] == f
        assert run_cli(["--audit", "--format", "sarif", "-v", f]) == 0
        captured = capsys.readouterr()
        assert len(json.loads(captured.out)["runs"][0]["results"]) == 1
        assert "Offline engine" in captured.err  # stdout stays a SARIF document

    def test_sarif_of_nothing(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--format", "sarif", "--skip", "*", f]) == 0
        assert json.loads(capsys.readouterr().out)["runs"][0]["results"] == []

    def test_sarif_with_unreadable_file(self, tmp_path, capsys):
        missing = str(tmp_path / "missing")
        assert run_cli(["--audit", "--format", "sarif", missing]) == 1
        captured = capsys.readouterr()
        assert missing in captured.err
        assert json.loads(captured.out)["runs"][0]["results"] == []

    def test_engine_missing(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        with patch(
            "jenkinsfilelint.cli.find_engine",
            side_effect=OfflineEngineError("no engine"),
        ):
            assert run_cli(["--audit", f]) == 1
        assert capsys.readouterr().err == "✗ no engine\n"

    @pytest.mark.parametrize(
        "argv, message",
        [
            (["--persona", "auditor"], "argument --persona: requires --audit"),
            (["--min-severity", "high"], "argument --min-severity: requires --audit"),
            (
                ["--min-confidence", "low"],
                "argument --min-confidence: requires --audit",
            ),
            (["--format", "sarif"], "argument --format: requires --audit"),
            (
                ["--audit", "--jenkins-url", "http://j"],
                "argument --audit: not allowed with argument --jenkins-url",
            ),
            (["--audit", "--local"], "not allowed with argument"),
            (["--audit", "--persona", "everyone"], "invalid choice: 'everyone'"),
        ],
    )
    def test_options(self, argv, message, capsys):
        assert run_cli([*argv, "Jenkinsfile"]) == 2
        assert message in capsys.readouterr().err


def test_finding_render():
    f = Finding("r", "high", "medium", "it leaks", 3, 7, "https://h")
    assert f.render("J") == "J:3:7: high: r: it leaks\n  help: https://h"
