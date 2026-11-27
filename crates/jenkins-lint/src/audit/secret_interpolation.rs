//! `secret-interpolation`: a secret that Groovy puts into a shell script.
//!
//! `withCredentials` and `credentials()` bind secrets to environment
//! variables, which Groovy can also read. Interpolated into a step's
//! argument, `sh "curl -u $USER:$PASS ..."`, the secret is in the script
//! text: the process arguments show it, and shell metacharacters in it
//! run. In single quotes, `sh 'curl -u "$USER:$PASS" ...'`, the shell reads
//! it from the environment instead.

use std::collections::HashMap;

use super::fix::{Disposition, Fix};
use super::taint::{self, Flow, How, Label, Sink, Use};
use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{Arg, Expr};

pub static RULE: Rule = Rule {
    id: "secret-interpolation",
    summary: "A secret is interpolated by Groovy into a shell step",
    severity: Severity::High,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    /// A password, token, passphrase or key.
    Secret,
    /// What `credentials('id')` sets: the secret itself, `user:password`,
    /// or the path of a secret file, depending on the credential.
    Credential,
    /// A user name or key ID: masked in logs, rarely what matters.
    Username,
}

pub fn check(flows: &[Flow], fixes: &[Option<Fix>]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for f in flows {
        let Label::Secret(kind) = f.value.label else { continue };
        let (severity, confidence, persona, message) = match f.sink {
            Sink::Script => {
                let (severity, confidence, persona) = match kind {
                    Kind::Secret => (Severity::High, Confidence::High, Persona::Regular),
                    // For a secret file, `credentials()` sets a path, which
                    // is fine to interpolate: `--kubeconfig ${KUBECONFIG}`.
                    Kind::Credential => (Severity::High, Confidence::Medium, Persona::Pedantic),
                    Kind::Username => (Severity::Low, Confidence::High, Persona::Pedantic),
                };
                (severity, confidence, persona, shell_message(&f.value, kind, &f.step))
            }
            // Jenkins warns about a secret interpolated into any step's
            // argument, but outside a script it does less harm.
            Sink::Argument if kind != Kind::Username => (
                Severity::Low,
                Confidence::High,
                Persona::Pedantic,
                format!(
                    "{}, and Groovy interpolates it into an argument of {}, which Jenkins warns about. \
                     Keep secrets out of Groovy strings.",
                    what(&f.value.name, kind),
                    f.step
                ),
            ),
            Sink::Argument => continue,
        };
        findings.push(Finding {
            rule: RULE.id,
            severity,
            confidence,
            persona,
            message,
            span: f.value.span,
            stmt_line: f.stmt_line,
            fix: fix(f, fixes),
        });
    }
    findings
}

/// What `--fix` makes of a script that a secret is interpolated into.
/// Unsafe, as for script-injection: the quoted value is no longer split.
const FIX: Disposition = Disposition::Unsafe;

/// `--fix` rewrites a script that a secret is interpolated into, so that
/// the shell reads the secret from the environment, where
/// `withCredentials` puts it.
fn fix(f: &Flow, fixes: &[Option<Fix>]) -> Option<Fix> {
    if f.sink != Sink::Script || f.value.how != How::Interpolated {
        return None;
    }
    let fix = f.script.and_then(|i| fixes[i].clone())?;
    Some(Fix { disposition: FIX, ..fix })
}

/// `X = credentials('id')` in an `environment` block sets X, and X_USR and
/// X_PSW for a user name and password.
pub fn credentials(name: &str, value: &Expr) -> Option<Vec<(String, Kind)>> {
    let Expr::Call { callee, .. } = value else { return None };
    if callee.method_name() != Some("credentials") {
        return None;
    }
    Some(vec![
        (name.to_string(), Kind::Credential),
        (format!("{name}_USR"), Kind::Username),
        (format!("{name}_PSW"), Kind::Secret),
    ])
}

/// The variables one `withCredentials` binding sets: `usernamePassword(...)`
/// or `[$class: 'AmazonWebServicesCredentialsBinding', ...]`.
pub fn bindings(e: &Expr, vars: &mut HashMap<String, Kind>) {
    let mut named: Vec<(String, &Expr)> = Vec::new();
    let binding = match e {
        Expr::Call { callee, args, .. } => {
            for a in args {
                if let Arg::Named { key, value, .. } = a {
                    named.push((key.clone(), value));
                }
            }
            match callee.method_name() {
                Some(name) => name.to_string(),
                None => return,
            }
        }
        Expr::Map { entries, .. } => {
            for (k, v) in entries {
                if let Expr::Str { value, .. } = k {
                    named.push((value.clone(), v));
                }
            }
            named
                .iter()
                .find(|(k, _)| k == "$class")
                .and_then(|(_, v)| match v {
                    Expr::Str { value, .. } => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or_default()
        }
        _ => return,
    };
    // The Git plugin's binding sets fixed names.
    if binding == "gitUsernamePassword" {
        vars.insert("GIT_USERNAME".into(), Kind::Username);
        vars.insert("GIT_PASSWORD".into(), Kind::Secret);
    }
    for (key, value) in named {
        let Expr::Str {
            value: var, interp: false, ..
        } = value
        else {
            continue;
        };
        if let Some(kind) = variable_kind(&binding, &key) {
            vars.insert(var.clone(), kind);
        }
    }
}

/// What a binding's `...Variable` parameter holds. Paths of the files a
/// secret is written to are not secrets.
fn variable_kind(binding: &str, key: &str) -> Option<Kind> {
    if key != "variable" && !key.ends_with("Variable") {
        return None;
    }
    match key {
        "usernameVariable" | "aliasVariable" | "accessKeyVariable" => Some(Kind::Username),
        "keyFileVariable" | "keystoreVariable" => None,
        "variable" if matches!(binding, "file" | "zip" | "dockerCert") => None,
        _ => Some(Kind::Secret),
    }
}

fn what(name: &str, kind: Kind) -> String {
    match kind {
        Kind::Secret => format!("{name} is a secret"),
        Kind::Credential => format!("{name} holds a credential"),
        Kind::Username => format!("{name} is a credential's user name or ID"),
    }
}

fn shell_message(u: &Use, kind: Kind, step: &str) -> String {
    let n = &u.name;
    let example = match step {
        "bat" => format!("bat '... %{n}% ...'"),
        "powershell" | "pwsh" => format!("{step} '... $env:{n} ...'"),
        "docker.build" => format!("docker.build(image, '... \"${n}\" ...')"),
        "run" | "withRun" => format!("{step}('... \"${n}\" ...')"),
        _ => format!("{step} '... \"${n}\" ...'"),
    };
    format!(
        "{}, and {}: the process arguments show it, and shell metacharacters in it run. \
         Use single quotes, as in {example}, so that the shell reads it from the environment.",
        what(n, kind),
        taint::how_it_gets_there(u, step)
    )
}

#[cfg(test)]
mod tests {
    use super::super::{audit, Filter};
    use crate::lexer::tokenize_with_comments;
    use crate::parser::parse;

    fn messages(src: &str, filter: Filter) -> Vec<String> {
        let (tokens, comments) = tokenize_with_comments(src).unwrap();
        audit(src, &parse(tokens).unwrap(), &comments, &[], &filter)
            .into_iter()
            .map(|f| f.message)
            .collect()
    }

    const CREDS: &str = "withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'U', passwordVariable: 'P')]) {";

    #[test]
    fn messages_say_how_to_read_the_secret_in_each_shell() {
        let m = messages(
            &format!("{CREDS}\n sh \"a $P\"\n bat \"a $P\"\n powershell \"a $P\"\n}}"),
            Filter::default(),
        );
        assert!(
            m[0].starts_with("P is a secret, and Groovy interpolates it into the sh script:"),
            "{}",
            m[0]
        );
        assert!(m[0].ends_with("as in sh '... \"$P\" ...', so that the shell reads it from the environment."));
        assert!(m[1].contains("bat '... %P% ...'"), "{}", m[1]);
        assert!(m[2].contains("powershell '... $env:P ...'"), "{}", m[2]);
    }

    #[test]
    fn messages_say_how_the_secret_got_there() {
        let m = messages(&format!("{CREDS}\n def c = \"a $P\"\n sh c\n sh 'a ' + P\n}}"), Filter::default());
        assert!(m[0].contains("Groovy puts it into c, which becomes the sh script"), "{}", m[0]);
        assert!(m[1].contains("+ joins it into the sh script"), "{}", m[1]);
    }

    #[test]
    fn filter() {
        use super::super::{Confidence, Persona, Severity};
        let src = format!("{CREDS}\n sh \"$U:$P\"\n echo \"$P\"\n}}");
        assert_eq!(messages(&src, Filter::default()).len(), 1);
        let pedantic = Filter {
            persona: Persona::Pedantic,
            ..Filter::default()
        };
        assert_eq!(messages(&src, pedantic.clone()).len(), 3);
        let high = Filter {
            min_severity: Severity::High,
            ..pedantic.clone()
        };
        assert_eq!(messages(&src, high).len(), 1);
        let certain = Filter {
            min_confidence: Confidence::High,
            ..pedantic.clone()
        };
        assert_eq!(messages(&src, certain).len(), 3);
        // A secret in a string passed to a call in a placeholder is that
        // call's finding, once; a `when` condition is not a step.
        let nested = format!("{CREDS}\n writeFile file: 'k', text: \"${{readFile file: \"$P\"}}\"\n}}");
        assert_eq!(messages(&nested, pedantic.clone()).len(), 1);
        let when = format!("{CREDS}\n when {{ environment name: 'X', value: \"a $P\" }}\n}}");
        assert!(messages(&when, pedantic).is_empty());
        // An ignore comment for another rule does not hide this one.
        let other = format!("{CREDS}\n // jenkins-lint: ignore[script-injection]\n sh \"$P\"\n}}");
        assert_eq!(messages(&other, Filter::default()).len(), 1);
        // --ignore turns the rule off everywhere; ignoring another rule does not.
        let off = Filter {
            ignore: vec!["secret-interpolation"],
            ..Filter::default()
        };
        assert!(messages(&src, off).is_empty());
        let other_rule = Filter {
            ignore: vec!["script-injection"],
            ..Filter::default()
        };
        assert_eq!(messages(&src, other_rule).len(), 1);
    }
}
