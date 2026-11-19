//! `unrestricted-input`: an `input` that any user may approve.
//!
//! `input 'Deploy to production?'` pauses the build until someone answers.
//! Without `submitter`, Jenkins lets any user answer, so an approval gate in
//! front of a deployment keeps out nobody who can reach the build.

use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{visit, Arg, Expr, Node, Stmt};
use crate::diag::{Pos, Span};

pub static RULE: Rule = Rule {
    id: "unrestricted-input",
    summary: "An input step or directive has no submitter, so any user can approve it",
    severity: Severity::Medium,
};

pub fn check(stmts: &[Stmt]) -> Vec<Finding> {
    let mut findings = Vec::new();
    visit(stmts, &mut |node| {
        // A call, not a variable named `input`.
        let Node::Expr(e @ Expr::Call { .. }) = node else { return };
        let Some(section) = e.section() else { return };
        if section.name != "input" {
            return;
        }
        let (restricted, fix) = match section.body {
            // The Declarative stage directive: `input { message '...'; submitter '...' }`.
            Some(body) if section.args.is_empty() => (
                body.iter()
                    .filter_map(Stmt::section)
                    .any(|s| s.name == "submitter" && !matches!(s.args.first(), Some(Arg::Pos(v)) if is_blank(v))),
                "input { message '...'; submitter 'release-managers' }",
            ),
            // The step: `input 'Proceed?'`, `input message: '...', submitter: '...'`.
            _ => (
                section
                    .args
                    .iter()
                    .any(|a| matches!(a, Arg::Named { key, value, .. } if key == "submitter" && !is_blank(value))),
                "input message: '...', submitter: 'release-managers'",
            ),
        };
        if restricted {
            return;
        }
        findings.push(Finding {
            rule: RULE.id,
            severity: Severity::Medium,
            confidence: Confidence::High,
            persona: Persona::Regular,
            message: format!(
                "input has no submitter, so any user who can reach this build can approve it, and the steps \
                 it guards then run. Name the users or groups who may approve, as in {fix}."
            ),
            span: Span {
                start: section.pos,
                end: Pos {
                    col: section.pos.col + "input".len(),
                    ..section.pos
                },
            },
            stmt_line: section.pos.line,
        });
    });
    findings
}

/// An empty submitter, `''`, restricts nothing.
fn is_blank(e: &Expr) -> bool {
    matches!(e, Expr::Str { value, interp: false, .. } if value.trim().is_empty())
}
