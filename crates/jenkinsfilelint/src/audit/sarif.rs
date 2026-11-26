//! SARIF 2.1.0 output, for GitHub code scanning and other tools.

use super::{Finding, Severity, RULES};
use crate::diag::{Diagnostic, Pos};
use crate::json_escape;

/// For a file that does not parse, which cannot be audited.
const SYNTAX_ERROR: &str = "syntax-error";

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

fn region(start: Pos, end: Pos) -> String {
    format!(
        "{{\"startLine\":{},\"startColumn\":{},\"endLine\":{},\"endColumn\":{}}}",
        start.line, start.col, end.line, end.col
    )
}

fn result(rule: &str, index: usize, level: &str, message: &str, path: &str, region: &str, extra: &str) -> String {
    format!(
        "{{\"ruleId\":\"{rule}\",\"ruleIndex\":{index},\"level\":\"{level}\",\"message\":{{\"text\":\"{}\"}},\
         \"locations\":[{{\"physicalLocation\":{{\"artifactLocation\":{{\"uri\":\"{}\"}},\"region\":{region}}}}}]{extra}}}",
        json_escape(message),
        json_escape(&uri(path)),
    )
}

/// One run with every rule, and the findings or the syntax error of each
/// file.
pub fn sarif(files: &[(String, Result<Vec<Finding>, Diagnostic>)]) -> String {
    let mut rules: Vec<String> = RULES
        .iter()
        .map(|r| {
            format!(
                "{{\"id\":\"{id}\",\"name\":\"{id}\",\"shortDescription\":{{\"text\":\"{s}\"}},\
                 \"fullDescription\":{{\"text\":\"{s}\"}},\"helpUri\":\"{url}\",\
                 \"help\":{{\"text\":\"See {url}\",\"markdown\":\"See [{id}]({url}).\"}},\
                 \"defaultConfiguration\":{{\"level\":\"{level}\"}},\
                 \"properties\":{{\"tags\":[\"security\"],\"security-severity\":\"{score}\"}}}}",
                id = r.id,
                s = json_escape(r.summary),
                url = r.help_url(),
                level = level(r.severity),
                score = security_severity(r.severity),
            )
        })
        .collect();
    rules.push(format!(
        "{{\"id\":\"{SYNTAX_ERROR}\",\"name\":\"{SYNTAX_ERROR}\",\"shortDescription\":{{\"text\":\"The file is not \
         valid Groovy, so it cannot be audited\"}},\"defaultConfiguration\":{{\"level\":\"error\"}}}}"
    ));
    let mut results = Vec::new();
    for (path, outcome) in files {
        match outcome {
            Ok(findings) => {
                for f in findings {
                    let index = RULES.iter().position(|r| r.id == f.rule).unwrap_or(0);
                    let mut extra = format!(
                        ",\"properties\":{{\"severity\":\"{}\",\"confidence\":\"{}\"}}",
                        f.severity, f.confidence
                    );
                    if let Some(fix) = &f.fix {
                        extra.push_str(&format!(
                            ",\"fixes\":[{{\"description\":{{\"text\":\"Let the shell read the value from the environment\"}},\
                             \"artifactChanges\":[{{\"artifactLocation\":{{\"uri\":\"{}\"}},\"replacements\":[{{\
                             \"deletedRegion\":{},\"insertedContent\":{{\"text\":\"{}\"}}}}]}}],\
                             \"properties\":{{\"disposition\":\"{}\"}}}}]",
                            json_escape(&uri(path)),
                            region(fix.span.start, fix.span.end),
                            json_escape(&fix.text),
                            fix.disposition.name()
                        ));
                    }
                    results.push(result(
                        f.rule,
                        index,
                        level(f.severity),
                        &f.message,
                        path,
                        &region(f.span.start, f.span.end),
                        &extra,
                    ));
                }
            }
            Err(d) => {
                let end = Pos {
                    col: d.pos.col + 1,
                    ..d.pos
                };
                results.push(result(
                    SYNTAX_ERROR,
                    RULES.len(),
                    "error",
                    &d.message,
                    path,
                    &region(d.pos, end),
                    "",
                ));
            }
        }
    }
    format!(
        "{{\"$schema\":\"https://json.schemastore.org/sarif-2.1.0.json\",\"version\":\"2.1.0\",\"runs\":[{{\
         \"tool\":{{\"driver\":{{\"name\":\"jenkins-lint\",\
         \"informationUri\":\"https://github.com/jenkinsci/jenkins-lint\",\"rules\":[{}]}}}},\
         \"columnKind\":\"unicodeCodePoints\",\"results\":[{}]}}]}}",
        rules.join(","),
        results.join(",")
    )
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
