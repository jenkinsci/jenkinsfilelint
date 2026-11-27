//! `controller-execution`: steps that run on the Jenkins controller.
//!
//! The controller's own node has the label `built-in`, and `master` before
//! Jenkins 2.307. A build that runs there, `agent { label 'built-in' }` or
//! `node('master')`, runs as the Jenkins process: it can read the
//! controller's files, among them its secrets and every job's
//! configuration. Builds belong on agents.

use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{visit, Arg, Expr, Node, Stmt};
use crate::diag::{Pos, Span};

pub static RULE: Rule = Rule {
    id: "controller-execution",
    summary: "Steps run on the Jenkins controller",
    severity: Severity::High,
};

/// The labels of the controller's node: `built-in`, and `master` before
/// Jenkins 2.307 and on controllers that have not migrated it.
const CONTROLLER: &[&str] = &["built-in", "master"];

pub fn check(stmts: &[Stmt]) -> Vec<Finding> {
    let mut findings = Vec::new();
    visit(stmts, &mut |node| {
        let Node::Expr(e) = node else { return };
        let Some(section) = e.section() else { return };
        match section.name {
            // Declarative: `agent { label 'x' }`, `agent { node { label 'x' } }`
            // and the `label` of `docker`, `dockerfile` and other agent types.
            "agent" => {
                if let Some(body) = section.body {
                    labels(body, 0, &mut findings);
                }
            }
            // Scripted: `node('x') { }` and `node(label: 'x') { }`.
            "node" => {
                let label = section.args.iter().find_map(|a| match a {
                    Arg::Pos(v) => Some(v),
                    Arg::Named { key, value, .. } if key == "label" => Some(value),
                    Arg::Named { .. } => None,
                });
                if let Some(label) = label {
                    report(label, section.pos.line, "node block", &mut findings);
                }
            }
            _ => {}
        }
    });
    findings
}

/// The `label` settings of an `agent` block: its own, and those of the
/// agent type inside it.
fn labels(body: &[Stmt], depth: usize, findings: &mut Vec<Finding>) {
    for s in body {
        let Some(section) = s.section() else { continue };
        if section.name == "label" {
            if let Some(Arg::Pos(label)) = section.args.first() {
                report(label, section.pos.line, "agent", findings);
            }
        } else if let (Some(inner), 0) = (section.body, depth) {
            labels(inner, 1, findings);
        }
    }
}

fn report(label: &Expr, stmt_line: usize, block: &str, findings: &mut Vec<Finding>) {
    // A label that Groovy computes is not known here.
    let Expr::Str {
        value, interp: false, pos, ..
    } = label
    else {
        return;
    };
    let Some((name, only)) = controller(value) else { return };
    let risk = "a build there runs as Jenkins itself, and can read the controller's files, its secrets and every \
                job's configuration";
    let fix = match block {
        "agent" => "agent { label 'linux' }",
        _ => "node('linux') { ... }",
    };
    let (confidence, reach) = if only {
        (Confidence::High, format!("so the steps of this {block} run on it"))
    } else {
        (
            Confidence::Medium,
            format!("and '{value}' lets the steps of this {block} run on it"),
        )
    };
    findings.push(Finding {
        rule: RULE.id,
        severity: Severity::High,
        confidence,
        persona: Persona::Regular,
        message: format!(
            "{name} is the label of the Jenkins controller, {reach}: {risk}. Run them on an agent instead, \
             as in {fix}."
        ),
        span: Span {
            start: *pos,
            // The quotes and the text between them, for a label on one line.
            end: Pos {
                col: pos.col + value.chars().count() + 2,
                ..*pos
            },
        },
        stmt_line,
        fix: None,
        related: Vec::new(),
    });
}

/// The controller's label in a label expression, and whether every node
/// the expression accepts has it: true for `built-in` and
/// `built-in && linux`, false for `built-in || linux`. Expressions with
/// `!`, `->` or `<->` are left alone: `!master` is the opposite.
fn controller(label: &str) -> Option<(&'static str, bool)> {
    if label.contains(['!', '>']) {
        return None;
    }
    let mut found = None;
    let mut only = true;
    for alternative in label.split("||") {
        let name = alternative
            .split("&&")
            .map(|atom| atom.trim_matches(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '"')))
            .find_map(|atom| CONTROLLER.iter().find(|c| **c == atom));
        match name {
            Some(name) => found = found.or(Some(*name)),
            None => only = false,
        }
    }
    found.map(|name| (name, only))
}

#[cfg(test)]
mod tests {
    use super::controller;

    #[test]
    fn label_expressions() {
        assert_eq!(controller("built-in"), Some(("built-in", true)));
        assert_eq!(controller("master"), Some(("master", true)));
        assert_eq!(controller("master && linux"), Some(("master", true)));
        assert_eq!(controller("(built-in)&&\"linux\""), Some(("built-in", true)));
        assert_eq!(controller("linux || master"), Some(("master", false)));
        assert_eq!(controller("master || built-in"), Some(("master", true)));
        assert_eq!(controller("(master || docker) && linux"), Some(("master", false)));
        assert_eq!(controller("!master"), None);
        assert_eq!(controller("linux && !built-in"), None);
        assert_eq!(controller("master -> linux"), None);
        assert_eq!(controller("masters"), None);
        assert_eq!(controller("Master"), None);
        assert_eq!(controller("build-master"), None);
        assert_eq!(controller(""), None);
    }
}
