//! Offline Jenkinsfile linter: checks Groovy syntax and Declarative
//! Pipeline structure without a Jenkins controller.

mod ast;
mod audit;
mod diag;
mod lexer;
mod parser;
mod validate;

use std::ffi::OsString;
use std::process::ExitCode;

use audit::fix::Fix;
use audit::{Confidence, Filter, Finding, Persona};
use diag::{Diagnostic, Report, Severity};

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

/// Audit one file's source text: its findings, or the error that keeps it
/// from being read as Groovy.
pub fn audit_source(src: &str, filter: &Filter) -> Result<Vec<Finding>, Diagnostic> {
    let (tokens, comments) = lexer::tokenize_with_comments(src)?;
    let annotations = audit::annotations(&tokens);
    let stmts = parser::parse(tokens)?;
    Ok(audit::audit(src, &stmts, &comments, &annotations, filter))
}

/// A file's findings and the fixes for them. Each finding keeps the fix that
/// `--fix` makes for it, if any.
#[derive(Debug)]
pub struct Fixed {
    pub findings: Vec<Finding>,
    /// The file's text with every fix made.
    pub src: String,
    /// The findings left in that text.
    pub remaining: Vec<Finding>,
}

/// Audit one file and work out its fixes. A fix is kept only if the fixed
/// file still parses and has the same findings as before, less those in
/// the scripts it rewrites; otherwise it is dropped and its findings stay.
pub fn fix_source(src: &str, filter: &Filter) -> Result<Fixed, Diagnostic> {
    let mut findings = audit_source(src, filter)?;
    let mut fixes: Vec<Fix> = Vec::new();
    for fix in findings.iter().filter_map(|f| f.fix.clone()) {
        if !fixes.contains(&fix) {
            fixes.push(fix);
        }
    }
    if fixes.is_empty() {
        return Ok(Fixed {
            remaining: findings.clone(),
            findings,
            src: src.to_string(),
        });
    }
    // Every finding, hidden or not, so that a fix cannot make a new one.
    let everything = Filter {
        persona: Persona::Auditor,
        ..Filter::default()
    };
    let before = audit_source(src, &everything)?;
    let works = |chosen: &[&Fix]| {
        let Ok(after) = audit_source(&audit::fix::apply(src, chosen), &everything) else {
            return false;
        };
        let rewritten = |f: &Finding| chosen.iter().any(|x| x.span.start <= f.span.start && f.span.end <= x.span.end);
        // A fix changes columns, never lines.
        let key = |f: &Finding| (f.rule, f.span.start.line, f.message.clone());
        let mut expected: Vec<_> = before.iter().filter(|f| !rewritten(f)).map(key).collect();
        let mut got: Vec<_> = after.iter().map(key).collect();
        expected.sort();
        got.sort();
        expected == got
    };
    let mut chosen: Vec<&Fix> = fixes.iter().collect();
    if !works(&chosen) {
        chosen.retain(|f| works(&[*f]));
        if !works(&chosen) {
            chosen.clear();
        }
    }
    for f in &mut findings {
        if f.fix.as_ref().is_some_and(|fix| !chosen.contains(&fix)) {
            f.fix = None;
        }
    }
    let fixed = audit::fix::apply(src, &chosen);
    let mut remaining = audit_source(&fixed, filter)?;
    // What is left after --fix cannot be fixed by running it again.
    remaining.iter_mut().for_each(|f| f.fix = None);
    Ok(Fixed {
        findings,
        src: fixed,
        remaining,
    })
}

const USAGE: &str = "Usage: jenkinsfilelint-engine [--format text|json] [--] <Jenkinsfile>...
       jenkinsfilelint-engine --audit [--format text|json|sarif] [--persona P]
                              [--min-severity S] [--min-confidence C]
                              [--ignore RULE[,RULE...]] [--fix] [--] <file>...

Checks Groovy syntax and Declarative Pipeline structure without a Jenkins
controller. Exits with 1 when any file has an error.

--format json prints a JSON array with one object per file: \"file\",
\"declarative\" (false when there is no pipeline block, as in a scripted
Pipeline), and \"diagnostics\" with \"line\", \"column\", \"severity\" and
\"message\"; or \"file\" and \"error\" when the file cannot be read.

--audit runs the security checks instead. --persona regular (the default),
pedantic or auditor chooses how much to show; --min-severity and
--min-confidence (low, medium or high) leave out the rest. --ignore turns
off rules in every file; it takes a comma-separated list and can be given
more than once. --fix rewrites the files to make the safe fixes, then
reports what is left; each fixed finding is printed as \"fixed\" in text
and listed in \"fixed\" in JSON. With no files, it
prints an empty report. It exits with 1
when it shows a finding or a file does not parse, except with --format
sarif, which exits with 0 so that the report can be uploaded. With --format
json, each file has \"findings\" with \"rule\", \"severity\", \"confidence\",
\"persona\", \"message\", \"line\", \"column\", \"end_line\", \"end_column\",
\"help\" and \"fixable\", and \"diagnostics\" with the syntax error, if any.";

pub(crate) fn json_escape(s: &str) -> String {
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

/// Stack for the linting thread. The parser stops at parser::MAX_DEPTH,
/// which needs far less than this even in a debug build; a fixed size keeps
/// that true on Windows, whose main thread gets only 1 MiB.
const LINT_STACK: usize = 64 * 1024 * 1024;

fn main() -> ExitCode {
    match std::thread::Builder::new().stack_size(LINT_STACK).spawn(run) {
        Ok(handle) => handle.join().unwrap_or(ExitCode::from(2)),
        Err(e) => {
            eprintln!("cannot start the linting thread: {e}");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Text,
    Json,
    Sarif,
}

#[derive(Debug)]
struct Options {
    files: Vec<OsString>,
    format: Format,
    /// The filter for --audit, or None to check syntax and structure.
    audit: Option<Filter>,
    /// --fix: rewrite the files.
    fix: bool,
}

/// What the command line asks for: a run, or an exit after --help,
/// --version or a usage error.
fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Options, ExitCode> {
    fn value<T>(args: &mut impl Iterator<Item = OsString>, flag: &str, choices: &str, parse: fn(&str) -> Option<T>) -> Result<T, ExitCode> {
        args.next()
            .and_then(|v| v.into_string().ok())
            .as_deref()
            .and_then(parse)
            .ok_or_else(|| {
                eprintln!("{flag} must be {choices}");
                ExitCode::from(2)
            })
    }
    let mut files = Vec::new();
    let mut format = Format::Text;
    let mut audit = false;
    let mut fix = false;
    let mut filter = Filter::default();
    let mut audit_option = None;
    // args_os: a file name that is not UTF-8 must not panic.
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        match a.to_str().unwrap_or("") {
            "--" => files.extend(args.by_ref()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return Err(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
                return Err(ExitCode::SUCCESS);
            }
            "--format" => {
                format = value(&mut args, "--format", "text, json or sarif", |f| match f {
                    "text" => Some(Format::Text),
                    "json" => Some(Format::Json),
                    "sarif" => Some(Format::Sarif),
                    _ => None,
                })?
            }
            "--audit" => audit = true,
            "--fix" => {
                fix = true;
                audit_option = Some("--fix");
            }
            "--persona" => {
                filter.persona = value(&mut args, "--persona", "regular, pedantic or auditor", Persona::parse)?;
                audit_option = Some("--persona");
            }
            "--min-severity" => {
                filter.min_severity = value(&mut args, "--min-severity", "low, medium or high", audit::Severity::parse)?;
                audit_option = Some("--min-severity");
            }
            "--min-confidence" => {
                filter.min_confidence = value(&mut args, "--min-confidence", "low, medium or high", Confidence::parse)?;
                audit_option = Some("--min-confidence");
            }
            "--ignore" => {
                let rules = args.next().and_then(|v| v.into_string().ok()).unwrap_or_default();
                for name in rules.split(',').map(str::trim) {
                    let Some(id) = audit::rule_id(name) else {
                        let ids: Vec<&str> = audit::RULES.iter().map(|r| r.id).collect();
                        eprintln!("--ignore: unknown rule {name:?}; the rules are {}", ids.join(", "));
                        return Err(ExitCode::from(2));
                    };
                    if !filter.ignore.contains(&id) {
                        filter.ignore.push(id);
                    }
                }
                audit_option = Some("--ignore");
            }
            _ => files.push(a),
        }
    }
    if !audit {
        let needs_audit = if format == Format::Sarif {
            Some("--format sarif")
        } else {
            audit_option
        };
        if let Some(option) = needs_audit {
            eprintln!("{option} needs --audit");
            return Err(ExitCode::from(2));
        }
    }
    // An audit of no files is an empty report, which SARIF output needs.
    if files.is_empty() && !audit {
        eprintln!("{USAGE}");
        return Err(ExitCode::from(2));
    }
    Ok(Options {
        files,
        format,
        audit: audit.then_some(filter),
        fix,
    })
}

fn run() -> ExitCode {
    let options = match parse_args(std::env::args_os().skip(1)) {
        Ok(o) => o,
        Err(code) => return code,
    };
    if let Some(filter) = &options.audit {
        return run_audit(&options.files, options.format, filter, options.fix);
    }
    let json = options.format == Format::Json;
    let mut failed = false;
    let mut json_items = Vec::new();
    for os_path in &options.files {
        let path = &*os_path.to_string_lossy();
        let src = match std::fs::read_to_string(os_path) {
            Ok(s) => s,
            Err(e) => {
                if json {
                    json_items.push(json_read_error(path, &e));
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

fn json_read_error(path: &str, e: &std::io::Error) -> String {
    format!(
        "{{\"file\":\"{}\",\"error\":\"{}\"}}",
        json_escape(path),
        json_escape(&e.to_string())
    )
}

/// --audit: findings in every file, as text, JSON or SARIF. With `fix`,
/// the files are rewritten and the findings left are reported.
fn run_audit(files: &[OsString], format: Format, filter: &Filter, fix: bool) -> ExitCode {
    let mut unreadable = false;
    let mut flagged = false;
    let mut items = Vec::new();
    let mut audited = Vec::new();
    for os_path in files {
        let path = os_path.to_string_lossy().into_owned();
        let src = match std::fs::read_to_string(os_path) {
            Ok(s) => s,
            Err(e) => {
                if format == Format::Json {
                    items.push(json_read_error(&path, &e));
                } else {
                    eprintln!("{path}: {e}");
                }
                unreadable = true;
                continue;
            }
        };
        let (fixed, outcome) = match fix_source(&src, filter) {
            Ok(f) if fix && f.src != src => match std::fs::write(os_path, &f.src) {
                Ok(()) => (f.findings.into_iter().filter(|f| f.fix.is_some()).collect(), Ok(f.remaining)),
                Err(e) => {
                    eprintln!("{path}: {e}");
                    unreadable = true;
                    (Vec::new(), Ok(f.findings))
                }
            },
            Ok(f) => (Vec::new(), Ok(f.findings)),
            Err(d) => (Vec::new(), Err(d)),
        };
        flagged |= !matches!(&outcome, Ok(f) if f.is_empty());
        match format {
            Format::Text => print!("{}{}", fixed_text(&path, &fixed), audit_text(&path, &outcome)),
            Format::Json => items.push(audit_json(&path, &outcome, &fixed)),
            Format::Sarif => audited.push((path, outcome)),
        }
    }
    match format {
        Format::Text => {}
        Format::Json => println!("[{}]", items.join(",")),
        Format::Sarif => println!("{}", audit::sarif::sarif(&audited)),
    }
    if unreadable || (flagged && format != Format::Sarif) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The findings that --fix fixed, one line each, where they were.
fn fixed_text(path: &str, fixed: &[Finding]) -> String {
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

fn audit_text(path: &str, outcome: &Result<Vec<Finding>, Diagnostic>) -> String {
    match outcome {
        Ok(findings) => findings
            .iter()
            .map(|f| {
                let rule = audit::RULES
                    .iter()
                    .find(|r| r.id == f.rule)
                    .map(|r| r.help_url())
                    .unwrap_or_default();
                let fixable = if f.fix.is_some() { " (fixable with --fix)" } else { "" };
                format!(
                    "{path}:{}:{}: {}: {}: {}{fixable}\n  help: {rule}\n",
                    f.span.start.line, f.span.start.col, f.severity, f.rule, f.message
                )
            })
            .collect(),
        Err(d) => format!("{path}:{}:{}: error: {}\n", d.pos.line, d.pos.col, d.message),
    }
}

fn audit_json(path: &str, outcome: &Result<Vec<Finding>, Diagnostic>, fixed: &[Finding]) -> String {
    let (findings, diagnostics) = match outcome {
        Ok(findings) => (findings.iter().map(finding_json).collect::<Vec<_>>(), String::new()),
        Err(d) => (
            Vec::new(),
            format!(
                "{{\"line\":{},\"column\":{},\"severity\":\"error\",\"message\":\"{}\"}}",
                d.pos.line,
                d.pos.col,
                json_escape(&d.message)
            ),
        ),
    };
    format!(
        "{{\"file\":\"{}\",\"findings\":[{}],\"diagnostics\":[{diagnostics}],\"fixed\":[{}]}}",
        json_escape(path),
        findings.join(","),
        fixed.iter().map(finding_json).collect::<Vec<_>>().join(",")
    )
}

fn finding_json(f: &Finding) -> String {
    let help = audit::RULES
        .iter()
        .find(|r| r.id == f.rule)
        .map(|r| r.help_url())
        .unwrap_or_default();
    format!(
        "{{\"rule\":\"{}\",\"severity\":\"{}\",\"confidence\":\"{}\",\"persona\":\"{}\",\"message\":\"{}\",\
         \"line\":{},\"column\":{},\"end_line\":{},\"end_column\":{},\"help\":\"{}\",\"fixable\":{}}}",
        f.rule,
        f.severity,
        f.confidence,
        f.persona,
        json_escape(&f.message),
        f.span.start.line,
        f.span.start.col,
        f.span.end.line,
        f.span.end.col,
        help,
        f.fix.is_some()
    )
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

    /// Absurd nesting gives an error instead of overflowing the stack.
    /// Run on a thread with the same stack as the binary's linting thread.
    #[test]
    fn deep_nesting_is_an_error() {
        let wrap = |body: String| {
            format!("pipeline {{\n agent any\n stages {{\n stage('A') {{\n steps {{\n script {{\n{body}\n }}\n }}\n }}\n }}\n}}\n")
        };
        let cases = [
            // Closures in closures: a bare `{` statement is an error of its own.
            format!("x = {}", "{ -> ".repeat(100_000)),
            wrap(format!("def x = {}1{}", "(".repeat(100_000), ")".repeat(100_000))),
            wrap(format!("{}echo 'x'{}", "[1].each { ".repeat(100_000), " }".repeat(100_000))),
            wrap(format!("def x = {}true", "!".repeat(100_000))),
            wrap(format!("def x = {}2", "a = ".repeat(100_000))),
            wrap(format!("if (a) {{ echo 'x' }}{}", " else if (a) { echo 'x' }".repeat(100_000))),
            wrap(format!("echo \"{}x{}\"", "${\"".repeat(100_000), "\"}".repeat(100_000))),
            // Within the lexer's limit, so the parser's limit stops it.
            wrap(format!("echo \"{}x{}\"", "${\"".repeat(990), "\"}".repeat(990))),
            format!(
                "pipeline {{\n agent any\n stages {{\n{}stage('l') {{ steps {{ echo 'x' }} }}\n{}}}\n}}\n",
                "stage('s') { stages {\n".repeat(10_000),
                "} }\n".repeat(10_000)
            ),
        ];
        let handle = std::thread::Builder::new()
            .stack_size(super::LINT_STACK)
            .spawn(move || cases.iter().map(|src| messages(src)).collect::<Vec<_>>())
            .unwrap();
        for m in handle.join().unwrap() {
            assert_eq!(m.len(), 1, "{m:?}");
            assert!(m[0].contains("Nesting is too deep to check"), "{m:?}");
        }
    }

    /// Modifiers before `class` are common in shared library src/ files.
    #[test]
    fn class_with_modifiers() {
        for src in [
            "public class A implements Serializable {}",
            "abstract class A { abstract def run() }",
            "final class A {}",
            "public static final class A {}",
            "public interface I {}",
            "private enum E { X }",
        ] {
            assert_eq!(messages(src), Vec::<String>::new(), "{src}");
        }
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

    /// Audit every case in tests/audit/<rule>/ and compare with its
    /// expected.txt, which lists each finding the auditor persona sees.
    #[test]
    fn audit_cases() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/audit");
        let filter = super::Filter {
            persona: super::Persona::Auditor,
            ..super::Filter::default()
        };
        let mut failures = Vec::new();
        let mut rules = 0;
        for rule in std::fs::read_dir(&root).unwrap() {
            let dir = rule.unwrap().path();
            rules += 1;
            let expected = std::fs::read_to_string(dir.join("expected.txt")).unwrap();
            let mut cases: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut current = None;
            for line in expected.lines().filter(|l| !l.is_empty() && !l.starts_with('#')) {
                if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                    current = Some(name.to_string());
                    cases.insert(name.to_string(), Vec::new());
                } else if line != "none" {
                    cases
                        .get_mut(current.as_ref().expect("a [case] first"))
                        .unwrap()
                        .push(line.to_string());
                }
            }
            let mut seen = 0;
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_none_or(|e| e != "groovy") {
                    continue;
                }
                seen += 1;
                let name = path.file_stem().unwrap().to_str().unwrap().to_string();
                let got: Vec<String> = match super::audit_source(&std::fs::read_to_string(&path).unwrap(), &filter) {
                    Ok(findings) => findings
                        .iter()
                        .filter(|f| dir.ends_with(f.rule))
                        .map(|f| {
                            let var = f.message.split(' ').next().unwrap_or("");
                            let at = f.span.start;
                            format!("{}:{} {} {} {} {var}", at.line, at.col, f.severity, f.confidence, f.persona)
                        })
                        .collect(),
                    Err(d) => vec![format!("does not parse: {}", d.message)],
                };
                if cases.get(&name) != Some(&got) {
                    failures.push(format!(
                        "{}/{name}:\n  expected {:?}\n  got {got:?}",
                        dir.display(),
                        cases.get(&name)
                    ));
                }
            }
            assert_eq!(seen, cases.len(), "{}: every [case] needs a .groovy file", dir.display());
        }
        assert_eq!(rules, super::audit::RULES.len(), "every rule needs tests/audit/<rule>/");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Fix every case in tests/audit/<rule>/fix/: `x.groovy` must become
    /// `x.fixed.groovy`, or stay as it is when there is none. The fixed file
    /// must parse, keep no finding of the rule, and not change when fixed
    /// again.
    #[test]
    fn audit_fixes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/audit");
        let filter = super::Filter {
            persona: super::Persona::Auditor,
            ..super::Filter::default()
        };
        let mut failures = Vec::new();
        let mut cases = 0;
        for rule in ["script-injection", "secret-interpolation"] {
            let dir = root.join(rule).join("fix");
            for entry in std::fs::read_dir(&dir).unwrap() {
                let path = entry.unwrap().path();
                let name = path.file_name().unwrap().to_str().unwrap().to_string();
                let Some(stem) = name.strip_suffix(".groovy").filter(|s| !s.ends_with(".fixed")) else {
                    continue;
                };
                cases += 1;
                let src = std::fs::read_to_string(&path).unwrap();
                let expected = std::fs::read_to_string(dir.join(format!("{stem}.fixed.groovy"))).unwrap_or_else(|_| src.clone());
                let fixed = super::fix_source(&src, &filter).unwrap();
                let shown = fixed.findings.iter().any(|f| f.rule == rule);
                if !shown && !stem.starts_with("s15") {
                    failures.push(format!("{rule}/{stem}: no {rule} finding to fix"));
                }
                if fixed.src != expected {
                    failures.push(format!("{rule}/{stem}:\n  expected {expected:?}\n  got {:?}", fixed.src));
                    continue;
                }
                if fixed.src != src && fixed.remaining.iter().any(|f| f.rule == rule) {
                    failures.push(format!("{rule}/{stem}: a {rule} finding is left"));
                }
                let again = super::fix_source(&fixed.src, &filter).unwrap();
                if again.src != fixed.src || again.findings.iter().any(|f| f.fix.is_some()) {
                    failures.push(format!("{rule}/{stem}: fixing it again changes it"));
                }
                if super::lint(&fixed.src).diagnostics.iter().any(|d| d.severity == Severity::Error) {
                    failures.push(format!("{rule}/{stem}: the fixed file has an error"));
                }
            }
        }
        assert!(cases > 20, "{cases} cases");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn fixes_are_checked_before_they_are_kept() {
        // A fix is reported only when the fixed file audits the same, less
        // what it fixes; the text output says which findings it fixes.
        let src =
            "withCredentials([string(credentialsId: 'c', variable: 'T')]) {\n    sh \"deploy ${T}\"\n    sh \"echo ${T.trim()}\"\n}\n";
        let fixed = super::fix_source(src, &super::Filter::default()).unwrap();
        assert_eq!(fixed.findings.iter().filter(|f| f.fix.is_some()).count(), 1);
        assert!(fixed.src.contains("sh 'deploy \"$T\"'"), "{}", fixed.src);
        assert_eq!(fixed.remaining.len(), 1);
        let outcome = Ok(fixed.findings.clone());
        let text = super::audit_text("J", &outcome);
        assert!(text.contains("(fixable with --fix)\n"), "{text}");
        let json = super::audit_json("J", &Ok(fixed.remaining), &fixed.findings[..1]);
        assert!(json.contains("\"fixable\":false"), "{json}");
        assert!(json.contains("\"fixed\":[{\"rule\":\"secret-interpolation\""), "{json}");
        assert!(super::fixed_text("J", &fixed.findings[..1]).starts_with("J:2:16: fixed: secret-interpolation: T is a secret"));
        let sarif = crate::audit::sarif::sarif(&[("J".to_string(), outcome)]);
        assert!(
            sarif.contains("\"deletedRegion\":{\"startLine\":2,\"startColumn\":8,\"endLine\":2,\"endColumn\":21},\"insertedContent\":{\"text\":\"'deploy \\\"$T\\\"'\"}"),
            "{sarif}"
        );
        // Nothing to fix: the file and its findings as they are.
        let none = super::fix_source("sh 'make'\n", &super::Filter::default()).unwrap();
        assert_eq!((none.src.as_str(), none.findings.len()), ("sh 'make'\n", 0));
    }

    fn parse(args: &[&str]) -> Result<super::Options, std::process::ExitCode> {
        super::parse_args(args.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn audit_options() {
        let o = parse(&[
            "--audit",
            "--persona",
            "pedantic",
            "--min-severity",
            "medium",
            "--min-confidence",
            "high",
            "--format",
            "sarif",
            "J",
        ])
        .unwrap();
        let filter = o.audit.unwrap();
        assert_eq!(
            (filter.persona, filter.min_severity, filter.min_confidence, o.format),
            (
                super::Persona::Pedantic,
                crate::audit::Severity::Medium,
                super::Confidence::High,
                super::Format::Sarif
            )
        );
        assert!(parse(&["J"]).unwrap().audit.is_none());
        assert!(parse(&["--audit", "--fix", "J"]).unwrap().fix);
        for bad in [
            &["--persona", "auditor", "J"][..],
            &["--min-severity", "high", "J"],
            &["--min-confidence", "low", "J"],
            &["--format", "sarif", "J"],
            &["--audit", "--persona", "everyone", "J"],
            &["--audit", "--min-severity"],
            &["--ignore", "script-injection", "J"],
            &["--audit", "--ignore", "script-injections", "J"],
            &["--audit", "--ignore", "script-injection,", "J"],
            &["--audit", "--ignore"],
            &["--fix", "J"],
            &["--format", "xml", "J"],
            &[],
        ] {
            assert_eq!(parse(bad).unwrap_err(), std::process::ExitCode::from(2), "{bad:?}");
        }
        assert_eq!(parse(&["--version"]).unwrap_err(), std::process::ExitCode::SUCCESS);
        assert_eq!(parse(&["--audit", "--", "--persona"]).unwrap().files, ["--persona"]);
        assert!(parse(&["--audit", "--format", "sarif"]).unwrap().files.is_empty());
        let ignore = |args: &[&str]| parse(args).unwrap().audit.unwrap().ignore;
        assert!(ignore(&["--audit", "J"]).is_empty());
        assert_eq!(
            ignore(&[
                "--audit",
                "--ignore",
                "unpinned-library, unpinned-image",
                "--ignore",
                "unpinned-library",
                "J"
            ]),
            ["unpinned-library", "unpinned-image"]
        );
    }

    #[test]
    fn audit_output() {
        let src = "withCredentials([string(credentialsId: 'c', variable: 'TOKEN')]) {\n    sh \"deploy ${TOKEN}\"\n}\n";
        let outcome = super::audit_source(src, &super::Filter::default());
        let text = super::audit_text("J", &outcome);
        assert!(text.starts_with("J:2:16: high: secret-interpolation: TOKEN is a secret"), "{text}");
        assert!(text.ends_with("\n  help: https://jenkinsci.github.io/jenkinsfilelint/audit/secret-interpolation/\n"));
        let json = super::audit_json("J", &outcome, &[]);
        assert!(json.contains("\"line\":2,\"column\":16,\"end_line\":2,\"end_column\":24"), "{json}");
        let sarif = crate::audit::sarif::sarif(&[("dir\\J".to_string(), outcome)]);
        assert!(sarif.contains("\"uri\":\"dir/J\""), "{sarif}");
        assert!(sarif.contains("\"region\":{\"startLine\":2,\"startColumn\":16,\"endLine\":2,\"endColumn\":24}"));
        let broken = super::audit_source("sh \"${a +}\"", &super::Filter::default());
        assert!(super::audit_text("J", &broken).starts_with("J:1:10: error: "));
        assert!(super::audit_json("J", &broken, &[]).contains("\"findings\":[],\"diagnostics\":[{\"line\":1"));
        let sarif = crate::audit::sarif::sarif(&[("J".to_string(), broken)]);
        assert!(sarif.contains("\"ruleId\":\"syntax-error\""), "{sarif}");
    }
}
