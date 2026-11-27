//! Security audits: ways a pipeline can be attacked or can leak secrets.
//!
//! Each rule reports findings with a severity (how bad it is if real), a
//! confidence (how likely it is real) and the persona that sees it:
//! `regular` shows what is worth fixing, `pedantic` adds lower-confidence
//! findings and hardening advice, `auditor` shows everything. A finding is
//! turned off with `// jenkins-lint: ignore[rule-id]` on its line or on
//! the line above the statement it is in.

mod controller_execution;
pub mod fix;
pub mod sarif;
mod script_injection;
mod secret_interpolation;
mod taint;
mod unpinned_image;
mod unpinned_library;
mod unrestricted_input;

use std::fmt;

use crate::ast::Stmt;
use crate::diag::{Pos, Span};
use crate::lexer::{Comment, TokKind, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Persona {
    Regular,
    Pedantic,
    Auditor,
}

macro_rules! names {
    ($ty:ident { $($variant:ident => $name:literal),* }) => {
        impl $ty {
            pub fn parse(s: &str) -> Option<Self> {
                match s {
                    $($name => Some($ty::$variant),)*
                    _ => None,
                }
            }
            pub fn name(self) -> &'static str {
                match self {
                    $($ty::$variant => $name,)*
                }
            }
        }
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.name())
            }
        }
    };
}

names!(Severity { Low => "low", Medium => "medium", High => "high" });
names!(Confidence { Low => "low", Medium => "medium", High => "high" });
names!(Persona { Regular => "regular", Pedantic => "pedantic", Auditor => "auditor" });

/// A rule, as its documentation and SARIF describe it.
pub struct Rule {
    pub id: &'static str,
    /// One line, for SARIF and the list of rules.
    pub summary: &'static str,
    /// The severity of its most serious findings.
    pub severity: Severity,
}

impl Rule {
    pub fn help_url(&self) -> String {
        format!("https://jenkinsci.github.io/jenkins-lint/audit/{}/", self.id)
    }
}

/// The id of a rule, as `--ignore` names it.
pub fn rule_id(name: &str) -> Option<&'static str> {
    RULES.iter().map(|r| r.id).find(|id| *id == name)
}

pub static RULES: &[&Rule] = &[
    &script_injection::RULE,
    &secret_interpolation::RULE,
    &controller_execution::RULE,
    &unpinned_library::RULE,
    &unpinned_image::RULE,
    &unrestricted_input::RULE,
];

#[derive(Debug, Clone)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    pub confidence: Confidence,
    /// The least thorough persona that shows it.
    pub persona: Persona,
    pub message: String,
    pub span: Span,
    /// Where the statement it is in starts: an ignore comment on the line
    /// above that turns it off, even when the finding is further down, as
    /// in a multi-line string.
    pub stmt_line: usize,
    /// The rewrite of the script it is in that `--fix` makes.
    pub fix: Option<fix::Fix>,
}

/// Which findings to show.
#[derive(Debug, Clone)]
pub struct Filter {
    pub persona: Persona,
    pub min_severity: Severity,
    pub min_confidence: Confidence,
    /// Rules turned off for every file, by id.
    pub ignore: Vec<&'static str>,
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            persona: Persona::Regular,
            min_severity: Severity::Low,
            min_confidence: Confidence::Low,
            ignore: Vec::new(),
        }
    }
}

impl Filter {
    fn shows(&self, f: &Finding) -> bool {
        f.persona <= self.persona
            && f.severity >= self.min_severity
            && f.confidence >= self.min_confidence
            && !self.ignore.contains(&f.rule)
    }
}

/// Run every rule on a parsed file and keep the findings the filter shows
/// and no comment turns off, in file order. `src` is the file's text, which
/// fixes rewrite.
pub fn audit(src: &str, stmts: &[Stmt], comments: &[Comment], annotations: &[Annotation], filter: &Filter) -> Vec<Finding> {
    let ignores: Vec<(usize, Vec<&str>)> = comments
        .iter()
        .filter_map(|c| ignored_rules(&c.text).map(|rules| (c.pos.line, rules)))
        .collect();
    let ignored = |f: &Finding| {
        ignores.iter().any(|(line, rules)| {
            let applies = *line == f.span.start.line || *line == f.stmt_line || *line + 1 == f.stmt_line;
            applies && rules.contains(&f.rule)
        })
    };
    let (flows, scripts) = taint::flows(stmts);
    let source = fix::Source::new(src);
    let fixes: Vec<Option<fix::Fix>> = scripts.iter().map(|s| fix::plan(&source, s)).collect();
    let mut findings: Vec<Finding> = script_injection::check(&flows, &fixes)
        .into_iter()
        .chain(secret_interpolation::check(&flows, &fixes))
        .chain(controller_execution::check(stmts))
        .chain(unpinned_library::check(stmts, annotations))
        .chain(unpinned_image::check(stmts))
        .chain(unrestricted_input::check(stmts))
        .filter(|f| filter.shows(f) && !ignored(f))
        .collect();
    findings.sort_by_key(|f| (f.span.start, f.rule));
    findings
}

/// An annotation and the string literals in its arguments: `@Library('a')`,
/// `@Library(['a', 'b'])`, `@Library(value = 'a', changelog = false)`. The
/// parser skips annotations, so the audit reads them from the tokens.
#[derive(Debug)]
pub struct Annotation {
    pub name: String,
    pub strings: Vec<AnnotationString>,
    /// Where the `@` is.
    pub pos: Pos,
}

#[derive(Debug)]
pub struct AnnotationString {
    pub value: String,
    pub interp: bool,
    pub pos: Pos,
}

/// The annotations in a file's tokens, before they are parsed.
pub fn annotations(tokens: &[Token]) -> Vec<Annotation> {
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let (true, Some(TokKind::Ident(name))) = (t.is_punct("@"), tokens.get(i + 1).map(|t| &t.kind)) else {
            continue;
        };
        let mut strings = Vec::new();
        if tokens.get(i + 2).is_some_and(|t| t.is_punct("(") && !t.nl_before) {
            let mut depth = 0;
            for t in &tokens[i + 2..] {
                match &t.kind {
                    TokKind::Punct("(" | "[") => depth += 1,
                    TokKind::Punct(")" | "]") => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    // `value = 'a'`: a string after `=` is a value; `changelog = false` has none.
                    TokKind::Str { value, interp, .. } => strings.push(AnnotationString {
                        value: value.clone(),
                        interp: *interp,
                        pos: t.pos,
                    }),
                    TokKind::Eof => break,
                    _ => {}
                }
            }
        }
        out.push(Annotation {
            name: name.clone(),
            strings,
            pos: t.pos,
        });
    }
    out
}

/// The rules that `jenkins-lint: ignore[a, b] why` turns off. The
/// directive may follow other text in the comment.
fn ignored_rules(comment: &str) -> Option<Vec<&str>> {
    const DIRECTIVE: &str = "jenkins-lint:";
    let rest = &comment[comment.find(DIRECTIVE)? + DIRECTIVE.len()..];
    let list = rest.trim_start().strip_prefix("ignore[")?;
    let list = &list[..list.find(']')?];
    Some(list.split(',').map(str::trim).filter(|r| !r.is_empty()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_directives() {
        assert_eq!(
            ignored_rules(" jenkins-lint: ignore[secret-interpolation]"),
            Some(vec!["secret-interpolation"])
        );
        assert_eq!(
            ignored_rules(" TODO; jenkins-lint: ignore[a, b] the token is fake"),
            Some(vec!["a", "b"])
        );
        assert_eq!(ignored_rules("jenkins-lint:ignore[]"), Some(vec![]));
        assert_eq!(ignored_rules(" jenkins-lint: ignore secret-interpolation"), None);
        assert_eq!(ignored_rules(" jenkins-lint: ignore[unclosed"), None);
        assert_eq!(ignored_rules(" just a comment"), None);
        // The name from before 2.0 is not a directive.
        assert_eq!(ignored_rules(" jenkinsfilelint: ignore[secret-interpolation]"), None);
    }

    #[test]
    fn names_round_trip() {
        for s in [Severity::Low, Severity::Medium, Severity::High] {
            assert_eq!(Severity::parse(s.name()), Some(s));
        }
        for c in [Confidence::Low, Confidence::Medium, Confidence::High] {
            assert_eq!(Confidence::parse(c.name()), Some(c));
        }
        for p in [Persona::Regular, Persona::Pedantic, Persona::Auditor] {
            assert_eq!(Persona::parse(&p.to_string()), Some(p));
        }
        assert_eq!(Persona::parse("everyone"), None);
    }
}
