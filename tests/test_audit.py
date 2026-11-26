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
HELP = "https://jenkinsci.github.io/jenkinsfilelint/audit/secret-interpolation/"
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

    def test_ignore(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert audit_files([f], ignore=["secret-interpolation"])[f].findings == []
        kept = audit_files([f], ignore=["script-injection,unpinned-image"])[f]
        assert [x.rule for x in kept.findings] == ["secret-interpolation"]
        with pytest.raises(OfflineEngineError, match="unknown rule"):
            audit_files([f], ignore=["no-such-rule"])

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
                    if x.rule == rule.name
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
        rules = {r["id"]: r for r in run["tool"]["driver"]["rules"]}
        assert set(rules) >= {"script-injection", "secret-interpolation"}
        rule = rules["secret-interpolation"]
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
        assert out[-2:] == [
            "2 findings (1 high, 1 low)",
            "2 fixable with --fix=all (unsafe, review the changes).",
        ]

    def test_script_injection(self, tmp_path, capsys):
        f = write(
            tmp_path, "Jenkinsfile", 'node {\n    sh "echo ${env.CHANGE_TITLE}"\n}\n'
        )
        assert run_cli(["--audit", f]) == 1
        out = capsys.readouterr().out.splitlines()
        assert out[0].startswith(
            f"{f}:2:14: high: script-injection: CHANGE_TITLE is the pull request title"
        )
        assert (
            out[1]
            == f"  help: {HELP.replace('secret-interpolation', 'script-injection')}"
        )
        assert out[-2] == "1 finding (1 high)"

    def test_ignore(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--ignore", "secret-interpolation", f]) == 0
        assert capsys.readouterr().out == ""

    def test_one_finding(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", f]) == 1
        assert capsys.readouterr().out.splitlines()[-2] == "1 finding (1 high)"

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
            (["--ignore", "unpinned-image"], "argument --ignore: requires --audit"),
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


UNFIXABLE = (
    "withCredentials([string(credentialsId: 'c', variable: 'TOKEN')]) {\n"
    '    sh "deploy ${TOKEN.trim()}"\n'
    "}\n"
)


class TestFix:
    def test_audit_files_fixes_and_reports_what_is_left(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        result = audit_files([f], fix="all")[f]
        assert result.findings == []
        assert [(x.rule, x.line, x.column, x.fix) for x in result.fixed] == [
            ("secret-interpolation", 2, 25, "unsafe")
        ]
        assert 'sh \'curl -u "$USER":"$PASS" https://example.com\'' in (
            tmp_path / "Jenkinsfile"
        ).read_text(encoding="utf-8")

    def test_without_fix_nothing_changes(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", f]) == 1
        out = capsys.readouterr().out
        assert "[*]" not in out
        assert out.endswith(
            "1 finding (1 high)\n"
            "1 fixable with --fix=all (unsafe, review the changes).\n"
        )
        assert (tmp_path / "Jenkinsfile").read_text(encoding="utf-8") == LEAKS

    def test_safe_fixes_leave_unsafe_ones(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--fix", f]) == 1
        out = capsys.readouterr().out
        assert out.endswith(
            "Found 1 finding (0 fixed, 1 remaining).\n"
            "1 fixable with --fix=all (unsafe, review the changes).\n"
        )
        assert (tmp_path / "Jenkinsfile").read_text(encoding="utf-8") == LEAKS

    @pytest.mark.parametrize("mode", ["all", "unsafe-only"])
    def test_fix(self, tmp_path, capsys, mode):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", f"--fix={mode}", f]) == 0
        out = capsys.readouterr().out
        assert out.startswith(
            f"{f}:2:25: fixed: secret-interpolation: PASS is a secret"
        )
        assert out.endswith("\nFound 1 finding (1 fixed, 0 remaining).\n")
        assert "help:" not in out
        assert run_cli(["--audit", f"--fix={mode}", f]) == 0
        assert capsys.readouterr().out == "Found 0 findings (0 fixed, 0 remaining).\n"

    def test_what_cannot_be_fixed_fails_the_run(self, tmp_path, capsys):
        fixable = write(tmp_path, "Jenkinsfile", LEAKS)
        unfixable = write(tmp_path, "Jenkinsfile.prod", UNFIXABLE)
        assert run_cli(["--audit", "--fix=all", fixable, unfixable]) == 1
        out = capsys.readouterr().out
        assert f"{fixable}:2:25: fixed: secret-interpolation" in out
        assert f"{unfixable}:2:16: high: secret-interpolation: TOKEN" in out
        assert out.endswith("Found 2 findings (1 fixed, 1 remaining).\n")
        assert (tmp_path / "Jenkinsfile.prod").read_text(encoding="utf-8") == UNFIXABLE

    def test_marks_what_the_mode_fixes(self, tmp_path, capsys):
        # A safe fix is marked [*] and counted for --fix: today's fixes are
        # unsafe, so the engine's report is made up here.
        result = AuditResult(
            [
                Finding("script-injection", "high", "high", "m", 1, 1, "h", "safe"),
                Finding("script-injection", "high", "high", "m", 2, 1, "h", "unsafe"),
            ],
            [],
        )
        with patch("jenkinsfilelint.cli.audit_files", return_value={"J": result}):
            with patch("jenkinsfilelint.cli.find_engine", return_value="e"):
                assert run_cli(["--audit", "J"]) == 1
                out = capsys.readouterr().out
                assert out.startswith("J:1:1: high: script-injection: [*] m\n")
                assert "J:2:1: high: script-injection: m\n" in out
                assert out.endswith(
                    "[*] 1 fixable with --fix.\n"
                    "1 more fixable with --fix=all (unsafe, review the changes).\n"
                )
                assert run_cli(["--audit", "--fix=unsafe-only", "J"]) == 1
                assert capsys.readouterr().out.endswith(
                    "Found 2 findings (0 fixed, 2 remaining).\n"
                    "[*] 1 fixable with --fix.\n"
                )

    def test_diff(self, tmp_path, capsys):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli(["--audit", "--fix=all", "--diff", f]) == 1
        out = capsys.readouterr().out
        assert out.startswith(f"--- {f}\n+++ {f}\n@@ -1,3 +1,3 @@\n")
        assert '-    sh "curl -u ${USER}:${PASS} https://example.com"\n' in out
        assert '+    sh \'curl -u "$USER":"$PASS" https://example.com\'\n' in out
        assert (tmp_path / "Jenkinsfile").read_text(encoding="utf-8") == LEAKS
        # The fixes --fix alone makes, none here: no diff.
        assert run_cli(["--audit", "--diff", f]) == 0
        assert capsys.readouterr().out == ""

    def test_directory(self, tmp_path, monkeypatch, capsys):
        write(tmp_path, "Jenkinsfile", LEAKS)
        (tmp_path / "ci").mkdir()
        write(tmp_path, os.path.join("ci", "deploy.jenkinsfile"), LEAKS)
        monkeypatch.chdir(tmp_path)
        assert run_cli(["--audit", "--fix=all", "."]) == 0
        assert capsys.readouterr().out.endswith(
            "Found 2 findings (2 fixed, 0 remaining).\n"
        )
        assert '"$PASS"' in (tmp_path / "ci" / "deploy.jenkinsfile").read_text()

    def test_report_formats(self, tmp_path):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        output, _, code = audit_report([f], "sarif")
        assert code == 0
        assert '"properties":{"disposition":"unsafe"}' in output
        output, _, code = audit_report([f], "json")
        assert json.loads(output)[0]["findings"][0]["fix"]["disposition"] == "unsafe"
        output, _, code = audit_report([f], "json", fix="all")
        assert code == 0
        report = json.loads(output)[0]
        assert (report["findings"], [x["rule"] for x in report["fixed"]]) == (
            [],
            ["secret-interpolation"],
        )

    @pytest.mark.parametrize(
        "argv, error",
        [
            (["--offline", "--fix"], "argument --fix: requires --audit"),
            (["--offline", "--diff"], "argument --diff: requires --audit"),
            (["--audit", "--fix=some"], "argument --fix: invalid choice: 'some'"),
            (
                ["--audit", "--diff", "--format", "json"],
                "argument --diff: not allowed with argument --format",
            ),
        ],
    )
    def test_options(self, tmp_path, capsys, argv, error):
        f = write(tmp_path, "Jenkinsfile", LEAKS)
        assert run_cli([*argv, f]) == 2
        assert error in capsys.readouterr().err

    def test_files_after_double_dash(self, tmp_path, monkeypatch, capsys):
        # After --, an argument that looks like --fix=MODE is a file.
        monkeypatch.chdir(tmp_path)
        write(tmp_path, "--fix=all", LEAKS)
        assert run_cli(["--audit", "--fix=all", "--", "--fix=all"]) == 0
        assert capsys.readouterr().out.endswith("(1 fixed, 0 remaining).\n")
