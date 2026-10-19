//! Offline Jenkinsfile linter: checks Groovy syntax and Declarative
//! Pipeline structure without a Jenkins controller.

mod ast;
mod diag;
mod lexer;
mod parser;
mod validate;

use std::process::ExitCode;

use diag::{Diagnostic, Severity};

/// Lint one file's source text.
pub fn lint(src: &str) -> Vec<Diagnostic> {
    let tokens = match lexer::tokenize(src) {
        Ok(t) => t,
        Err(d) => return vec![d],
    };
    match parser::parse(tokens) {
        Ok(stmts) => validate::validate(&stmts),
        Err(d) => vec![d],
    }
}

const USAGE: &str = "Usage: jenkinsfilelint [--format text|json] <Jenkinsfile>...

Checks Groovy syntax and Declarative Pipeline structure without a Jenkins
controller. Exits with 1 when any file has an error.";

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

fn main() -> ExitCode {
    let mut files = Vec::new();
    let mut json = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            "-V" | "--version" => {
                println!("jenkinsfilelint {}", env!("CARGO_PKG_VERSION"));
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
                eprintln!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        let diags = lint(&src);
        failed |= diags.iter().any(|d| d.severity == Severity::Error);
        for d in &diags {
            if json {
                json_items.push(format!(
                    "{{\"file\":\"{}\",\"line\":{},\"column\":{},\"severity\":\"{}\",\"message\":\"{}\"}}",
                    json_escape(path),
                    d.pos.line,
                    d.pos.col,
                    d.severity,
                    json_escape(&d.message)
                ));
            } else {
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
    use super::lint;

    fn messages(src: &str) -> Vec<String> {
        lint(src)
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
}
