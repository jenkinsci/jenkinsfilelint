//! What jenkins-lint prints for a file: text lines, or a JSON report.

use serde::Serialize;

use crate::diag::{Diagnostic, Report};
use crate::rules::fix::{Fix, Mode};
use crate::rules::{Finding, RULES};

/// The errors and warnings of checking the file, one line each.
pub fn diagnostics_text(path: &str, report: &Report) -> String {
    report
        .diagnostics
        .iter()
        .map(|d| format!("{path}:{}:{}: {}: {}\n", d.pos.line, d.pos.col, d.severity, d.message))
        .collect()
}

/// The findings, one line each and a link to the rule. Those that `mode`
/// fixes start with `[*]`.
pub fn findings_text(path: &str, findings: &[Finding], mode: Mode) -> String {
    findings
        .iter()
        .map(|f| {
            let fixable = if f.fix.as_ref().is_some_and(|x| mode.applies(x.disposition)) {
                "[*] "
            } else {
                ""
            };
            format!(
                "{path}:{}:{}: {}: {}: {fixable}{}\n  help: {}\n",
                f.span.start.line,
                f.span.start.col,
                f.severity,
                f.rule,
                f.message,
                help_url(f.rule)
            )
        })
        .collect()
}

/// The findings that --fix fixed, one line each, where they were.
pub fn fixed_text(path: &str, fixed: &[Finding]) -> String {
    fixed
        .iter()
        .map(|f| {
            format!(
                "{path}:{}:{}: fixed: {}: {}\n",
                f.span.start.line, f.span.start.col, f.rule, f.message
            )
        })
        .collect()
}

fn help_url(rule: &str) -> String {
    RULES.iter().find(|r| r.id == rule).map(|r| r.help_url()).unwrap_or_default()
}

#[derive(Serialize)]
struct DiagnosticJson<'a> {
    line: usize,
    column: usize,
    severity: String,
    message: &'a str,
}

#[derive(Serialize)]
struct EditJson<'a> {
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    text: &'a str,
}

#[derive(Serialize)]
struct FixJson<'a> {
    disposition: &'static str,
    edits: Vec<EditJson<'a>>,
}

#[derive(Serialize)]
struct FindingJson<'a> {
    rule: &'a str,
    severity: &'static str,
    confidence: &'static str,
    persona: &'static str,
    message: &'a str,
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
    help: String,
    fix: Option<FixJson<'a>>,
}

/// One file in the JSON report.
#[derive(Serialize)]
pub struct FileJson<'a> {
    file: &'a str,
    declarative: bool,
    diagnostics: Vec<DiagnosticJson<'a>>,
    findings: Vec<FindingJson<'a>>,
    fixed: Vec<FindingJson<'a>>,
}

/// A file that could not be read, in the JSON report.
#[derive(Serialize)]
pub struct UnreadableJson<'a> {
    pub file: &'a str,
    pub error: &'a str,
}

fn diagnostic_json(d: &Diagnostic) -> DiagnosticJson<'_> {
    DiagnosticJson {
        line: d.pos.line,
        column: d.pos.col,
        severity: d.severity.to_string(),
        message: &d.message,
    }
}

fn fix_json(fix: &Fix) -> FixJson<'_> {
    FixJson {
        disposition: fix.disposition.name(),
        edits: vec![EditJson {
            line: fix.span.start.line,
            column: fix.span.start.col,
            end_line: fix.span.end.line,
            end_column: fix.span.end.col,
            text: &fix.text,
        }],
    }
}

fn finding_json(f: &Finding) -> FindingJson<'_> {
    FindingJson {
        rule: f.rule,
        severity: f.severity.name(),
        confidence: f.confidence.name(),
        persona: f.persona.name(),
        message: &f.message,
        line: f.span.start.line,
        column: f.span.start.col,
        end_line: f.span.end.line,
        end_column: f.span.end.col,
        help: help_url(f.rule),
        fix: f.fix.as_ref().map(fix_json),
    }
}

/// A file's entry in the JSON report.
pub fn file_json<'a>(path: &'a str, report: &'a Report, findings: &'a [Finding], fixed: &'a [Finding]) -> FileJson<'a> {
    FileJson {
        file: path,
        declarative: report.declarative,
        diagnostics: report.diagnostics.iter().map(diagnostic_json).collect(),
        findings: findings.iter().map(finding_json).collect(),
        fixed: fixed.iter().map(finding_json).collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{find, fix, lint};
    use crate::rules::fix::Mode;
    use crate::rules::Filter;
    use crate::sarif::{sarif, Checked};

    use super::*;

    fn json(path: &str, report: &Report, findings: &[Finding], fixed: &[Finding]) -> String {
        serde_json::to_string(&file_json(path, report, findings, fixed)).unwrap()
    }

    #[test]
    fn fixes_are_checked_before_they_are_kept() {
        // The second script cannot be fixed: `.trim()` is not an
        // environment variable.
        let src =
            "withCredentials([string(credentialsId: 'c', variable: 'T')]) {\n    sh \"deploy ${T}\"\n    sh \"echo ${T.trim()}\"\n}\n";
        let filter = Filter::default();
        let all = fix(src, &filter, Mode::All).unwrap();
        assert_eq!(all.findings.iter().filter(|f| f.fix.is_some()).count(), 1);
        assert!(all.src.contains("sh 'deploy \"$T\"'"), "{}", all.src);
        assert_eq!((all.fixed.len(), all.remaining.len()), (1, 1));
        assert!(all.remaining[0].fix.is_none());
        // Its fix is unsafe, so --fix alone makes none.
        let safe = fix(src, &filter, Mode::Safe).unwrap();
        assert_eq!((safe.src.as_str(), safe.fixed.len()), (src, 0));
        assert!(safe.remaining[0].fix.is_some());
        let marked = findings_text("J", &all.findings, Mode::All);
        assert!(
            marked.starts_with("J:2:16: high: secret-interpolation: [*] T is a secret"),
            "{marked}"
        );
        assert!(!findings_text("J", &all.findings, Mode::Safe).contains("[*]"));
        let out = json("J", &lint(&all.src), &all.remaining, &all.fixed);
        assert!(out.contains("\"fix\":null"), "{out}");
        assert!(
            out.contains("\"fixed\":[{\"rule\":\"secret-interpolation\"")
                && out.contains("\"fix\":{\"disposition\":\"unsafe\",\"edits\":[{\"line\":2,\"column\":8,\"end_line\":2,\"end_column\":21,\"text\":\"'deploy \\\"$T\\\"'\"}]}"),
            "{out}"
        );
        assert!(fixed_text("J", &all.fixed).starts_with("J:2:16: fixed: secret-interpolation: T is a secret"));
        let report = sarif(&[Checked {
            path: "J".to_string(),
            findings: all.findings.clone(),
            diagnostics: Vec::new(),
        }]);
        assert!(
            report.contains("\"deletedRegion\":{\"startLine\":2,\"startColumn\":8,\"endLine\":2,\"endColumn\":21},\"insertedContent\":{\"text\":\"'deploy \\\"$T\\\"'\"}}]}],\"properties\":{\"disposition\":\"unsafe\"}}]"),
            "{report}"
        );
        // Nothing to fix: the file and its findings as they are.
        let none = fix("sh 'make'\n", &filter, Mode::All).unwrap();
        assert_eq!((none.src.as_str(), none.findings.len()), ("sh 'make'\n", 0));
    }

    #[test]
    fn reports() {
        let src = "withCredentials([string(credentialsId: 'c', variable: 'TOKEN')]) {\n    sh \"deploy ${TOKEN}\"\n}\n";
        let findings = find(src, &Filter::default()).unwrap();
        let text = findings_text("J", &findings, Mode::Safe);
        assert!(text.starts_with("J:2:16: high: secret-interpolation: TOKEN is a secret"), "{text}");
        assert!(text.ends_with("\n  help: https://jenkinsci.github.io/jenkins-lint/rules/secret-interpolation/\n"));
        let out = json("J", &lint(src), &findings, &[]);
        assert!(out.contains("\"line\":2,\"column\":16,\"end_line\":2,\"end_column\":24"), "{out}");
        assert!(
            out.starts_with("{\"file\":\"J\",\"declarative\":false,\"diagnostics\":[],\"findings\":["),
            "{out}"
        );
        let checked = |path: &str, src: &str| Checked {
            path: path.to_string(),
            findings: find(src, &Filter::default()).unwrap_or_default(),
            diagnostics: lint(src).diagnostics,
        };
        let out = sarif(&[checked("dir\\J", src)]);
        assert!(out.contains("\"uri\":\"dir/J\""), "{out}");
        assert!(out.contains("\"region\":{\"startLine\":2,\"startColumn\":16,\"endLine\":2,\"endColumn\":24}"));
        // A syntax error: no findings, and the error as Jenkins words it.
        let broken = "sh \"${a +}\"";
        assert!(find(broken, &Filter::default()).is_err());
        let report = lint(broken);
        assert!(diagnostics_text("J", &report).starts_with("J:1:10: error: "));
        assert!(json("J", &report, &[], &[]).contains("\"diagnostics\":[{\"line\":1"));
        assert!(sarif(&[checked("J", broken)]).contains("\"ruleId\":\"invalid-pipeline\""));
        // A Declarative error in Groovy that parses: reported with the findings.
        let invalid = "pipeline {\n    agent\n    stages { stage('A') { steps { echo 'x' } } }\n}\n";
        let report = lint(invalid);
        assert!(diagnostics_text("J", &report).starts_with("J:2:5: error: Not a valid section definition"));
        assert!(sarif(&[checked("J", invalid)]).contains("\"ruleId\":\"invalid-pipeline\",\"ruleIndex\":6,\"level\":\"error\""));
    }
}
