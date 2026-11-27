//! Checking one file: what Jenkins would reject, the findings of the
//! security rules, and their fixes.

use crate::diag::{Diagnostic, Report};
use crate::rules::fix::{Fix, Mode};
use crate::rules::{self, Filter, Finding, Persona};
use crate::{lexer, parser, validate};

/// Stack for the thread that checks files. The parser stops at
/// parser::MAX_DEPTH, which needs far less than this even in a debug build;
/// a fixed size keeps that true on Windows, whose main thread gets 1 MiB.
pub const STACK_SIZE: usize = 64 * 1024 * 1024;

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
pub fn find(src: &str, filter: &Filter) -> Result<Vec<Finding>, Diagnostic> {
    let (tokens, comments) = lexer::tokenize_with_comments(src)?;
    let annotations = rules::annotations(&tokens);
    let stmts = parser::parse(tokens)?;
    Ok(rules::find(src, &stmts, &comments, &annotations, filter))
}

/// A file's findings and its fixes.
#[derive(Debug)]
pub struct Fixed {
    /// The findings, each with its fix if it has one that works.
    pub findings: Vec<Finding>,
    /// The file's text with the fixes of the mode made.
    pub src: String,
    /// The findings those fixes fix, where they were.
    pub fixed: Vec<Finding>,
    /// The findings left in the fixed text, with their fixes.
    pub remaining: Vec<Finding>,
}

/// Audit one file and make the fixes that `mode` allows, in memory.
pub fn fix(src: &str, filter: &Filter, mode: Mode) -> Result<Fixed, Diagnostic> {
    let mut findings = find(src, filter)?;
    let working = working_fixes(src, &findings)?;
    keep_fixes(&mut findings, &working);
    let mut chosen: Vec<Fix> = working.iter().filter(|f| mode.applies(f.disposition)).cloned().collect();
    if chosen.len() < working.len() {
        chosen = working_fixes_of(src, chosen)?;
    }
    if chosen.is_empty() {
        return Ok(Fixed {
            remaining: findings.clone(),
            findings,
            src: src.to_string(),
            fixed: Vec::new(),
        });
    }
    let refs: Vec<&Fix> = chosen.iter().collect();
    let fixed_src = rules::fix::apply(src, &refs);
    let mut remaining = find(&fixed_src, filter)?;
    let left = working_fixes(&fixed_src, &remaining)?;
    keep_fixes(&mut remaining, &left);
    let fixed = findings
        .iter()
        .filter(|f| f.fix.as_ref().is_some_and(|x| chosen.iter().any(|c| c.span == x.span)))
        .cloned()
        .collect();
    Ok(Fixed {
        findings,
        src: fixed_src,
        fixed,
        remaining,
    })
}

/// The fixes of these findings that work, one per script. A script that
/// two findings share gets the more cautious of their dispositions.
fn working_fixes(src: &str, findings: &[Finding]) -> Result<Vec<Fix>, Diagnostic> {
    let mut fixes: Vec<Fix> = Vec::new();
    for fix in findings.iter().filter_map(|f| f.fix.as_ref()) {
        match fixes.iter_mut().find(|f| f.span == fix.span) {
            Some(f) => f.disposition = f.disposition.max(fix.disposition),
            None => fixes.push(fix.clone()),
        }
    }
    working_fixes_of(src, fixes)
}

/// The fixes that work: the fixed file still parses and has the same
/// findings as before, less those in the scripts they rewrite. When they do
/// not all work together, those that work alone, or none.
fn working_fixes_of(src: &str, fixes: Vec<Fix>) -> Result<Vec<Fix>, Diagnostic> {
    if fixes.is_empty() {
        return Ok(fixes);
    }
    // Every finding, hidden or not, so that a fix cannot make a new one.
    let everything = Filter {
        persona: Persona::Auditor,
        ..Filter::default()
    };
    let before = find(src, &everything)?;
    let works = |chosen: &[&Fix]| {
        let Ok(after) = find(&rules::fix::apply(src, chosen), &everything) else {
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
    let all: Vec<&Fix> = fixes.iter().collect();
    if works(&all) {
        return Ok(fixes);
    }
    let alone: Vec<&Fix> = all.into_iter().filter(|f| works(&[*f])).collect();
    Ok(if works(&alone) {
        alone.into_iter().cloned().collect()
    } else {
        Vec::new()
    })
}

/// Keep on each finding only the fix that works, as `working` has it.
fn keep_fixes(findings: &mut [Finding], working: &[Fix]) {
    for f in findings {
        f.fix = f.fix.as_ref().and_then(|x| working.iter().find(|w| w.span == x.span).cloned());
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
            .stack_size(super::STACK_SIZE)
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

    /// Check every case in tests/rules/<rule>/ and compare with its
    /// expected.txt, which lists each finding the auditor persona sees.
    #[test]
    fn rule_cases() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rules");
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
                let got: Vec<String> = match super::find(&std::fs::read_to_string(&path).unwrap(), &filter) {
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
        assert_eq!(rules, crate::rules::RULES.len(), "every rule needs tests/rules/<rule>/");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Fix every case in tests/rules/<rule>/fix/: `x.groovy` must become
    /// `x.fixed.groovy`, or stay as it is when there is none. The fixed file
    /// must parse, keep no finding of the rule, and not change when fixed
    /// again.
    #[test]
    fn rule_fixes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rules");
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
                let fixed = super::fix(&src, &filter, super::Mode::All).unwrap();
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
                let again = super::fix(&fixed.src, &filter, super::Mode::All).unwrap();
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
}
