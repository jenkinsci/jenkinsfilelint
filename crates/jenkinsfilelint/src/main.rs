//! Offline Jenkinsfile linter: checks Groovy syntax and Declarative
//! Pipeline structure without a Jenkins controller.

mod ast;
mod diag;
mod lexer;
mod parser;
mod validate;

use std::process::ExitCode;

use diag::{Report, Severity};

/// Lint one file's source text.
pub fn lint(src: &str) -> Report {
    let tokens = match lexer::tokenize(src) {
        Ok(t) => t,
        Err(d) => {
            return Report {
                diagnostics: vec![d],
                declarative: false,
            }
        }
    };
    match parser::parse(tokens) {
        Ok(stmts) => validate::validate(&stmts),
        Err(d) => Report {
            diagnostics: vec![d],
            declarative: false,
        },
    }
}

const USAGE: &str = "Usage: jenkinsfilelint-engine [--format text|json] [--] <Jenkinsfile>...

Checks Groovy syntax and Declarative Pipeline structure without a Jenkins
controller. Exits with 1 when any file has an error.

--format json prints a JSON array with one object per file: \"file\",
\"declarative\" (false when there is no pipeline block, as in a scripted
Pipeline), and \"diagnostics\" with \"line\", \"column\", \"severity\" and
\"message\"; or \"file\" and \"error\" when the file cannot be read.";

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_report(path: &str, report: &Report) -> String {
    let diags: Vec<String> = report
        .diagnostics
        .iter()
        .map(|d| {
            format!(
                "{{\"line\":{},\"column\":{},\"severity\":\"{}\",\"message\":\"{}\"}}",
                d.pos.line,
                d.pos.col,
                d.severity,
                json_escape(&d.message)
            )
        })
        .collect();
    format!(
        "{{\"file\":\"{}\",\"declarative\":{},\"diagnostics\":[{}]}}",
        json_escape(path),
        report.declarative,
        diags.join(",")
    )
}

fn main() -> ExitCode {
    let mut files = Vec::new();
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--" => {
                files.extend(args.by_ref());
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            "--format" => match args.next().as_deref() {
                Some("json") => json = true,
                Some("text") => json = false,
                _ => {
                    eprintln!("--format must be text or json");
                    return ExitCode::from(2);
                }
            },
            _ => files.push(a),
        }
    }
    if files.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let mut failed = false;
    let mut json_items = Vec::new();
    for path in &files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                if json {
                    json_items.push(format!(
                        "{{\"file\":\"{}\",\"error\":\"{}\"}}",
                        json_escape(path),
                        json_escape(&e.to_string())
                    ));
                } else {
                    eprintln!("{path}: {e}");
                }
                failed = true;
                continue;
            }
        };
        let report = lint(&src);
        failed |= report.diagnostics.iter().any(|d| d.severity == Severity::Error);
        if json {
            json_items.push(json_report(path, &report));
        } else {
            for d in &report.diagnostics {
                println!("{path}:{}:{}: {}: {}", d.pos.line, d.pos.col, d.severity, d.message);
            }
        }
    }
    if json {
        println!("[{}]", json_items.join(","));
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use super::lint;
    use crate::diag::Severity;

    fn messages(src: &str) -> Vec<String> {
        lint(src)
            .diagnostics
            .into_iter()
            .map(|d| format!("{}:{} {}", d.pos.line, d.pos.col, d.message))
            .collect()
    }

    #[test]
    fn valid_pipeline() {
        let src = include_str!("../tests/fixtures/valid.groovy");
        assert_eq!(messages(src), Vec::<String>::new());
    }

    #[test]
    fn missing_agent_value() {
        let src = "pipeline {\n    agent\n    stages {\n        stage('Build') {\n            steps {\n                sh 'make'\n            }\n        }\n    }\n}\n";
        let m = messages(src);
        assert_eq!(
            m,
            vec!["2:5 Not a valid section definition: \"agent\". Some extra configuration is required.".to_string()]
        );
    }

    #[test]
    fn stage_errors() {
        let src = "pipeline {\n  agent any\n  stages {\n    stage('A') { echo 'x' }\n    stage('A') { steps { sh 'y' } }\n  }\n}\n";
        let m = messages(src);
        assert!(m.iter().any(|x| x.contains("Unknown stage section \"echo\"")), "{m:?}");
        assert!(m.iter().any(|x| x.contains("Duplicate stage name: \"A\"")), "{m:?}");
    }

    #[test]
    fn syntax_error_position() {
        let m = messages("pipeline {\n  agent any\n  stages {\n    stage('A') {\n      steps { sh 'x' 'y' }\n    }\n  }\n}\n");
        assert_eq!(m.len(), 1);
        assert!(m[0].starts_with("5:22"), "{m:?}");
    }

    /// Lint every case in tests/jenkins/ and compare with expected.txt,
    /// which records what Jenkins's own validator said about each file.
    #[test]
    fn agrees_with_jenkins_validator() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jenkins");
        let expected = std::fs::read_to_string(dir.join("expected.txt")).unwrap();
        let mut cases: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut current = None;
        for line in expected.lines() {
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                current = Some(name);
                cases.insert(name, Vec::new());
            } else if let Some(name) = current.filter(|_| !line.is_empty()) {
                // Comments stay: "# Jenkins: ..." lines are checked below.
                cases.get_mut(name).unwrap().push(line);
            } else {
                assert!(line.is_empty() || line.starts_with('#'), "expectation before the first [case]");
            }
        }

        let mut seen = 0;
        let mut failures = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "groovy") {
                continue;
            }
            seen += 1;
            let name = path.file_stem().unwrap().to_str().unwrap();
            let Some(want) = cases.get(name) else {
                failures.push(format!("{name}: no [{name}] in expected.txt"));
                continue;
            };
            let report = lint(&std::fs::read_to_string(&path).unwrap());
            let diags = &report.diagnostics;
            let errors: BTreeSet<&str> = diags
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .map(|d| d.message.as_str())
                .collect();
            let warnings: BTreeSet<&str> = diags
                .iter()
                .filter(|d| d.severity != Severity::Error)
                .map(|d| d.message.as_str())
                .collect();
            let want_errors: BTreeSet<&str> = want.iter().filter_map(|l| l.strip_prefix("error: ")).collect();
            let want_warnings: BTreeSet<&str> = want.iter().filter_map(|l| l.strip_prefix("warning: ")).collect();
            let errors_ok = if want.contains(&"valid") {
                errors.is_empty()
            } else if want.contains(&"error") {
                !errors.is_empty()
            } else {
                errors == want_errors
            };
            // Jenkins answers "did not contain the 'pipeline' step" for
            // files with no pipeline block; those must not count as checked.
            let no_pipeline = want.iter().any(|l| l.contains("did not contain the 'pipeline' step"));
            let declarative_ok = !errors.is_empty() || report.declarative != no_pipeline;
            if !errors_ok || !declarative_ok || warnings != want_warnings {
                failures.push(format!(
                    "{name}:\n  expected {want:?}\n  got errors {errors:?}, warnings {warnings:?}, declarative {}",
                    report.declarative
                ));
            }
        }
        assert_eq!(seen, cases.len(), "every [case] in expected.txt needs a .groovy file");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
