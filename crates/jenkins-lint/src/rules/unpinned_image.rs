//! `unpinned-image`: a container image without a fixed version.
//!
//! `agent { docker { image 'maven' } }` pulls `maven:latest`, which is
//! whatever the image's publisher pushed last. A tag such as `maven:3.9.9`
//! is better, but its publisher can still move it. Only a digest,
//! `maven:3.9.9@sha256:...`, names one image.

use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{visit, Arg, Expr, Node, Stmt};
use crate::diag::{Pos, Span};

pub static RULE: Rule = Rule {
    id: "unpinned-image",
    summary: "A container image is used without a tag, with latest, or without a digest",
    severity: Severity::Medium,
};

/// How fixed an image reference is.
#[derive(Debug, PartialEq)]
enum Version<'a> {
    /// No tag: Docker pulls `latest`.
    Untagged,
    Latest,
    /// A tag other than `latest`, without a digest.
    Tag(&'a str),
    /// A digest, `@sha256:...`, with or without a tag.
    Digest,
}

fn version(image: &str) -> Version<'_> {
    if image.contains('@') {
        return Version::Digest;
    }
    // The tag follows the last `:` of the last path component, so that a
    // registry's port, `localhost:5000/maven`, is not taken for one.
    let name = image.rsplit('/').next().unwrap_or(image);
    match name.split_once(':') {
        Some((_, "latest")) => Version::Latest,
        Some((_, tag)) if !tag.is_empty() => Version::Tag(tag),
        _ => Version::Untagged,
    }
}

pub fn check(stmts: &[Stmt]) -> Vec<Finding> {
    let mut findings = Vec::new();
    visit(stmts, &mut |node| {
        let Node::Expr(e) = node else { return };
        // `docker.image('maven').inside { }`
        if let Expr::Call { callee, args, pos, .. } = e {
            if matches!(callee.as_ref(), Expr::Prop { obj, name, .. }
                if name == "image" && matches!(obj.as_ref(), Expr::Ident { name, .. } if name == "docker"))
            {
                if let Some(Arg::Pos(image)) = args.first() {
                    report(image, pos.line, "docker.image(...)", &mut findings);
                }
                return;
            }
        }
        let Some(section) = e.section() else { return };
        match section.name {
            // Declarative: `agent { docker 'maven' }` and
            // `agent { docker { image 'maven' } }`.
            "agent" => {
                for s in section.body.unwrap_or_default() {
                    let Some(docker) = s.section().filter(|s| s.name == "docker") else {
                        continue;
                    };
                    if let Some(Arg::Pos(image)) = docker.args.first() {
                        report(image, docker.pos.line, "a docker agent", &mut findings);
                    }
                    for s in docker.body.unwrap_or_default() {
                        if let Some(Arg::Pos(image)) = s.section().filter(|s| s.name == "image").and_then(|s| s.args.first().copied()) {
                            report(image, s.pos().line, "a docker agent", &mut findings);
                        }
                    }
                }
            }
            // Kubernetes plugin: `containerTemplate(name: 'maven', image: 'maven')`.
            "containerTemplate" => {
                for a in &section.args {
                    if let Arg::Named { key, value, .. } = a {
                        if key == "image" {
                            report(value, section.pos.line, "a containerTemplate", &mut findings);
                        }
                    }
                }
            }
            _ => {}
        }
    });
    findings
}

fn report(image: &Expr, stmt_line: usize, used_by: &str, findings: &mut Vec<Finding>) {
    // An image that Groovy computes is not known here.
    let Expr::Str {
        value, interp: false, pos, ..
    } = image
    else {
        return;
    };
    // `image '$IMAGE'`: a variable that something expands later.
    if value.contains('$') {
        return;
    }
    let base = value.split('@').next().unwrap_or(value);
    let name = match base.rsplit_once('/') {
        Some((repo, last)) => format!("{repo}/{}", last.split(':').next().unwrap_or(last)),
        None => base.split(':').next().unwrap_or(base).to_string(),
    };
    let fix = format!("'{name}:<version>@sha256:<digest>'");
    let (severity, confidence, persona, message) = match version(value) {
        Version::Digest => return,
        Version::Untagged => (
            Severity::Medium,
            Confidence::High,
            Persona::Regular,
            format!(
                "{value} has no tag, so {used_by} pulls {name}:latest, which changes whenever its publisher \
                 pushes: builds can break or run code nobody reviewed. Pin a version tag and a digest, as in {fix}."
            ),
        ),
        Version::Latest => (
            Severity::Medium,
            Confidence::High,
            Persona::Regular,
            format!(
                "{value} is the latest tag, which changes whenever its publisher pushes: builds can break or \
                 run code nobody reviewed. Pin a version tag and a digest, as in {fix}."
            ),
        ),
        Version::Tag(tag) => (
            Severity::Low,
            Confidence::High,
            Persona::Pedantic,
            format!(
                "{value} has the tag {tag} but no digest, and its publisher can move a tag to another image. \
                 Add the digest, as in '{value}@sha256:<digest>'."
            ),
        ),
    };
    findings.push(Finding {
        rule: RULE.id,
        severity,
        confidence,
        persona,
        message,
        span: Span {
            start: *pos,
            // The quotes and the text between them, for an image on one line.
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

#[cfg(test)]
mod tests {
    use super::{version, Version};

    #[test]
    fn versions() {
        assert_eq!(version("maven"), Version::Untagged);
        assert_eq!(version("maven:"), Version::Untagged);
        assert_eq!(version("localhost:5000/maven"), Version::Untagged);
        assert_eq!(version("maven:latest"), Version::Latest);
        assert_eq!(version("ghcr.io/org/tool:latest"), Version::Latest);
        assert_eq!(version("maven:3.9.9"), Version::Tag("3.9.9"));
        assert_eq!(version("localhost:5000/maven:3"), Version::Tag("3"));
        assert_eq!(version("maven@sha256:0123abcd"), Version::Digest);
        assert_eq!(version("maven:3.9.9@sha256:0123abcd"), Version::Digest);
    }
}
