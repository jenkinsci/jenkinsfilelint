//! SARIF 2.1.0 output, for GitHub code scanning and other tools.

use serde_json::{json, Value};

use crate::diag::{Diagnostic, Pos, Severity as DiagSeverity};
use crate::rules::{Finding, Severity, RULES};

/// The rule of the errors that make Jenkins reject a file: Groovy syntax
/// and Declarative structure.
const INVALID: &str = "invalid-pipeline";

/// A file's findings, and the errors and warnings of checking it. A file
/// that does not parse has no findings, and its syntax error is among them.
pub struct Checked {
    pub path: String,
    pub findings: Vec<Finding>,
    pub diagnostics: Vec<Diagnostic>,
}

fn level(s: Severity) -> &'static str {
    match s {
        Severity::High => "error",
        Severity::Medium => "warning",
        Severity::Low => "note",
    }
}

/// GitHub ranks security alerts by this score: 7 and up is high.
fn security_severity(s: Severity) -> &'static str {
    match s {
        Severity::High => "8.0",
        Severity::Medium => "5.0",
        Severity::Low => "2.0",
    }
}

/// A URI for a file name, with `/` separators: relative, or `file:` for
/// an absolute path.
fn uri(path: &str) -> String {
    let mut out = String::new();
    let drive = path.as_bytes().get(1) == Some(&b':') && path.as_bytes()[0].is_ascii_alphabetic();
    if path.starts_with('/') {
        out.push_str("file://");
    } else if drive {
        out.push_str("file:///");
    }
    for c in path.chars() {
        match c {
            '\\' => out.push('/'),
            ' ' => out.push_str("%20"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            c => out.push(c),
        }
    }
    out
}

fn region(start: Pos, end: Pos) -> Value {
    json!({"startLine": start.line, "startColumn": start.col, "endLine": end.line, "endColumn": end.col})
}

fn result(rule: &str, index: usize, level: &str, message: &str, path: &str, region: Value) -> Value {
    json!({
        "ruleId": rule,
        "ruleIndex": index,
        "level": level,
        "message": {"text": message},
        "locations": [{"physicalLocation": {"artifactLocation": {"uri": uri(path)}, "region": region}}],
    })
}

/// One run with every rule, and the results of every file.
pub fn sarif(files: &[Checked]) -> String {
    let mut rules: Vec<Value> = RULES
        .iter()
        .map(|r| {
            let url = r.help_url();
            json!({
                "id": r.id,
                "name": r.id,
                "shortDescription": {"text": r.summary},
                "fullDescription": {"text": r.summary},
                "helpUri": url,
                "help": {"text": format!("See {url}"), "markdown": format!("See [{}]({url}).", r.id)},
                "defaultConfiguration": {"level": level(r.severity)},
                "properties": {"tags": ["security"], "security-severity": security_severity(r.severity)},
            })
        })
        .collect();
    rules.push(json!({
        "id": INVALID,
        "name": INVALID,
        "shortDescription": {"text": "Jenkins would reject the file: Groovy syntax or Declarative Pipeline structure"},
        "defaultConfiguration": {"level": "error"},
    }));
    let mut results = Vec::new();
    for file in files {
        let path = &file.path;
        for d in &file.diagnostics {
            let end = Pos {
                col: d.pos.col + 1,
                ..d.pos
            };
            let level = match d.severity {
                DiagSeverity::Error => "error",
                DiagSeverity::Warning => "warning",
            };
            results.push(result(INVALID, RULES.len(), level, &d.message, path, region(d.pos, end)));
        }
        for f in &file.findings {
            let index = RULES.iter().position(|r| r.id == f.rule).unwrap_or(0);
            let mut r = result(f.rule, index, level(f.severity), &f.message, path, region(f.span.start, f.span.end));
            r["properties"] = json!({"severity": f.severity.name(), "confidence": f.confidence.name()});
            if !f.related.is_empty() {
                let related: Vec<Value> = f
                    .related
                    .iter()
                    .enumerate()
                    .map(|(id, rel)| {
                        json!({
                            "id": id,
                            "message": {"text": rel.label},
                            "physicalLocation": {
                                "artifactLocation": {"uri": uri(path)},
                                "region": region(rel.span.start, rel.span.end),
                            },
                        })
                    })
                    .collect();
                r["relatedLocations"] = Value::Array(related);
            }
            if let Some(fix) = &f.fix {
                r["fixes"] = json!([{
                    "description": {"text": "Let the shell read the value from the environment"},
                    "artifactChanges": [{
                        "artifactLocation": {"uri": uri(path)},
                        "replacements": [{
                            "deletedRegion": region(fix.span.start, fix.span.end),
                            "insertedContent": {"text": fix.text},
                        }],
                    }],
                    "properties": {"disposition": fix.disposition.name()},
                }]);
            }
            results.push(r);
        }
    }
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {"driver": {
                "name": "jenkins-lint",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/jenkinsci/jenkins-lint",
                "rules": rules,
            }},
            "columnKind": "unicodeCodePoints",
            "results": results,
        }],
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris() {
        assert_eq!(uri("ci\\Jenkinsfile"), "ci/Jenkinsfile");
        assert_eq!(uri("a b/c#d?e%f"), "a%20b/c%23d%3Fe%25f");
        assert_eq!(uri("/repo/Jenkinsfile"), "file:///repo/Jenkinsfile");
        assert_eq!(uri("C:\\repo\\Jenkinsfile"), "file:///C:/repo/Jenkinsfile");
    }
}
