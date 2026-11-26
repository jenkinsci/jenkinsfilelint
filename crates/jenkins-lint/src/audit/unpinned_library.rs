//! `unpinned-library`: a shared library loaded from a branch.
//!
//! `@Library('lib@main') _` runs whatever the `main` branch of the library
//! holds when the build starts. Whoever can push to that branch changes
//! what this pipeline runs, and a trusted global library runs outside the
//! sandbox, with the controller's permissions. A tag or a commit SHA,
//! `@Library('lib@v1.4.0') _`, runs the code that was reviewed.

use super::{Annotation, Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{visit, Arg, Expr, Node, Section, Stmt};
use crate::diag::{Pos, Span};

pub static RULE: Rule = Rule {
    id: "unpinned-library",
    summary: "A shared library is loaded from a branch",
    severity: Severity::Medium,
};

/// A library a file asks for, by `name@version`.
struct Request<'a> {
    value: &'a str,
    interp: bool,
    pos: Pos,
    /// How the file asks for it, for messages: `@Library`, `library`.
    how: &'static str,
    stmt_line: usize,
}

pub fn check(stmts: &[Stmt], annotations: &[Annotation]) -> Vec<Finding> {
    let mut requests = Vec::new();
    for a in annotations.iter().filter(|a| a.name == "Library") {
        for s in &a.strings {
            requests.push(Request {
                value: &s.value,
                interp: s.interp,
                pos: s.pos,
                how: "@Library",
                stmt_line: a.pos.line,
            });
        }
    }
    visit(stmts, &mut |node| {
        let Node::Expr(e) = node else { return };
        let Some(section) = e.section() else { return };
        match section.name {
            // The step: `library 'lib@main'`, `library identifier: 'lib@main', ...`.
            "library" => request(&section, "library", &mut requests),
            // Declarative: `libraries { lib('lib@main') }`. Elsewhere `lib`
            // is just a name.
            "libraries" => {
                for s in section.body.unwrap_or_default() {
                    if let Some(inner) = s.section().filter(|s| s.name == "lib") {
                        request(&inner, "lib", &mut requests);
                    }
                }
            }
            _ => {}
        }
    });
    requests.iter().filter_map(finding).collect()
}

/// The library a `library` step or a `lib` directive asks for: its first
/// argument, or its `identifier`.
fn request<'a>(section: &Section<'a>, how: &'static str, requests: &mut Vec<Request<'a>>) {
    let value = section.args.iter().find_map(|a| match a {
        Arg::Pos(v) => Some(v),
        Arg::Named { key, value, .. } if key == "identifier" => Some(value),
        Arg::Named { .. } => None,
    });
    // `retriever: legacySCM(scm)` loads the library from the repository and
    // branch of the Jenkinsfile itself, as a library's own tests do: the
    // version names nothing else.
    let own_scm = section.args.iter().any(|a| {
        matches!(a, Arg::Named { key, value: Expr::Call { args, .. }, .. }
            if key == "retriever" && args.iter().any(|a| matches!(a, Arg::Pos(Expr::Ident { name, .. }) if name == "scm")))
    });
    if own_scm {
        return;
    }
    if let Some(Expr::Str { value, interp, pos, .. }) = value {
        requests.push(Request {
            value,
            interp: *interp,
            pos: *pos,
            how,
            stmt_line: section.pos.line,
        });
    }
}

fn finding(r: &Request) -> Option<Finding> {
    let (name, version) = match r.value.split_once('@') {
        Some((name, version)) => (name.trim(), Some(version.trim())),
        None => (r.value.trim(), None),
    };
    let example = match r.how {
        "@Library" => format!("@Library('{name}@v1.4.0') _"),
        how => format!("{how}('{name}@v1.4.0')"),
    };
    let (severity, confidence, persona, message) = match version {
        // A version Groovy computes, often from a build parameter.
        _ if r.interp => (
            Severity::Medium,
            Confidence::Low,
            Persona::Auditor,
            format!(
                "{name} is loaded from a version that Groovy computes when the build runs, which may name a \
                 branch. Pin a tag or a commit SHA, as in {example}."
            ),
        ),
        None => (
            Severity::Medium,
            Confidence::Low,
            Persona::Pedantic,
            format!(
                "{name} has no version, so it loads the default version that Jenkins configures for it, \
                 which may be a branch. Pin a tag or a commit SHA, as in {example}."
            ),
        ),
        Some(v) if pinned(v) => return None,
        // A placeholder that a template fills in, `lib@@REF@`, not a version.
        Some(v) if v.contains('@') => return None,
        Some(v) => (
            Severity::Medium,
            Confidence::Medium,
            Persona::Regular,
            format!(
                "{name} is loaded from {v}, which looks like a branch: whoever can push to it changes what \
                 this pipeline runs, without sandbox limits if the library is trusted. Pin a tag or a commit SHA \
                 instead, as in {example}."
            ),
        ),
    };
    Some(Finding {
        rule: RULE.id,
        severity,
        confidence,
        persona,
        message,
        span: Span {
            start: r.pos,
            // The quotes and the text between them, for a version on one line.
            end: Pos {
                col: r.pos.col + r.value.chars().count() + 2,
                ..r.pos
            },
        },
        stmt_line: r.stmt_line,
        fix: None,
    })
}

/// A version that names one commit: a tag that looks like a release
/// (`1.2`, `v1.2.3`, `2.0.0-rc.1`, `refs/tags/x`) or a commit SHA. Branches
/// can be named like this too, but rarely are.
fn pinned(version: &str) -> bool {
    let v = version.strip_prefix(['v', 'V']).unwrap_or(version);
    let release =
        v.starts_with(|c: char| c.is_ascii_digit()) && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    let sha = (7..=40).contains(&version.len()) && version.chars().all(|c| c.is_ascii_hexdigit());
    release || sha || version.starts_with("refs/tags/")
}

#[cfg(test)]
mod tests {
    use super::pinned;

    #[test]
    fn versions() {
        for v in [
            "1",
            "1.2",
            "v1.2.3",
            "V2",
            "2.0.0-rc.1",
            "1.0+build.5",
            "a1b2c3d",
            "0123456789abcdef0123456789abcdef01234567",
            "refs/tags/stable",
        ] {
            assert!(pinned(v), "{v}");
        }
        for v in [
            "main",
            "master",
            "develop",
            "feature/x",
            "release-1.2",
            "v",
            "vnext",
            "abc",
            "1.2/fix",
            "",
            "refs/heads/main",
        ] {
            assert!(!pinned(v), "{v}");
        }
    }
}
