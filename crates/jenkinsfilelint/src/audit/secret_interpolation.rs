//! `secret-interpolation`: a secret that Groovy puts into a shell script.
//!
//! `withCredentials` and `credentials()` bind secrets to environment
//! variables, which Groovy can also read. Interpolated into a step's
//! argument, `sh "curl -u $USER:$PASS ..."`, the secret is in the script
//! text: the process arguments show it, and shell metacharacters in it
//! run. In single quotes, `sh 'curl -u "$USER:$PASS" ...'`, the shell reads
//! it from the environment instead.

use std::collections::HashMap;

use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{Arg, Expr, Stmt, StrPart};
use crate::diag::{Pos, Span};

pub static RULE: Rule = Rule {
    id: "secret-interpolation",
    summary: "A secret is interpolated by Groovy into a shell step",
    severity: Severity::High,
};

/// Steps that run their argument as a script.
const SHELL_STEPS: &[&str] = &["sh", "bat", "powershell", "pwsh"];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// A password, token, passphrase or key.
    Secret,
    /// What `credentials('id')` sets: the secret itself, `user:password`,
    /// or the path of a secret file, depending on the credential.
    Credential,
    /// A user name or key ID: masked in logs, rarely what matters.
    Username,
}

/// A secret that ends up in a string.
#[derive(Debug, Clone)]
struct Use {
    name: String,
    kind: Kind,
    span: Span,
    /// The local variable it went through: `def cmd = "...$PASS..."`.
    via: Option<String>,
    /// Joined with `+` rather than interpolated.
    joined: bool,
}

pub fn check(stmts: &[Stmt]) -> Vec<Finding> {
    let mut w = Walker::default();
    w.block(stmts);
    w.findings
}

#[derive(Default)]
struct Walker {
    /// Secret variables in scope, innermost last.
    secrets: Vec<HashMap<String, Kind>>,
    /// Local variables that hold a string with secrets in it, per block.
    locals: Vec<HashMap<String, Vec<Use>>>,
    findings: Vec<Finding>,
}

impl Walker {
    fn secret(&self, name: &str) -> Option<Kind> {
        self.secrets.iter().rev().find_map(|s| s.get(name).copied())
    }

    fn local(&self, name: &str) -> Option<&Vec<Use>> {
        self.locals.iter().rev().find_map(|l| l.get(name))
    }

    /// Record what a local variable now holds, where it was declared.
    fn set_local(&mut self, name: &str, uses: Vec<Use>) {
        let level = self
            .locals
            .iter()
            .rposition(|l| l.contains_key(name))
            .unwrap_or(self.locals.len() - 1);
        if uses.is_empty() {
            self.locals[level].remove(name);
        } else {
            self.locals[level].insert(name.to_string(), uses);
        }
    }

    fn block(&mut self, stmts: &[Stmt]) {
        self.scoped(environment_credentials(stmts), |w| {
            for s in stmts {
                w.stmt(s);
            }
        });
    }

    fn scoped(&mut self, secrets: HashMap<String, Kind>, f: impl FnOnce(&mut Self)) {
        self.secrets.push(secrets);
        self.locals.push(HashMap::new());
        f(self);
        self.locals.pop();
        self.secrets.pop();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Expr(e) => self.expr(e, e.pos().line),
            Stmt::Decl { names, value, pos } => {
                if let Some(v) = value {
                    self.expr(v, pos.line);
                }
                // A declaration makes a new variable in this block, which
                // hides one of the same name outside it.
                if let [name] = names.as_slice() {
                    let uses = value.as_ref().map(|v| self.uses(v, true)).unwrap_or_default();
                    self.locals.last_mut().expect("in a block").insert(name.clone(), uses);
                }
            }
            Stmt::MethodDef { body, .. } => self.block(body),
            Stmt::Control { bodies, .. } => {
                for b in bodies {
                    self.block(b);
                }
            }
            Stmt::Other { .. } => {}
        }
    }

    fn expr(&mut self, e: &Expr, line: usize) {
        match e {
            Expr::Call { callee, args, .. } => {
                let step = match callee.as_ref() {
                    Expr::Ident { name, .. } => Some(name.as_str()),
                    _ => None,
                };
                match step {
                    Some("withCredentials") => return self.with_credentials(args),
                    Some(s) if SHELL_STEPS.contains(&s) => self.shell_step(s, args, line),
                    // `when { environment name: 'X', value: "..." }` compares,
                    // it is not a step.
                    Some("environment") => {}
                    Some(s) => self.other_step(s, args, line),
                    None => {}
                }
                self.expr(callee, line);
                for a in args {
                    self.expr(arg_value(a), line);
                }
            }
            Expr::Closure { body, .. } => self.block(body),
            Expr::Assign { target, value, .. } => {
                self.expr(value, line);
                if let Expr::Ident { name, .. } = target.as_ref() {
                    let uses = self.uses(value, true);
                    self.set_local(name, uses);
                }
            }
            Expr::Str { parts, .. } => {
                for p in parts {
                    if let StrPart::Placeholder(ph) = p {
                        if let Some(x) = &ph.expr {
                            self.expr(x, line);
                        }
                    }
                }
            }
            Expr::List { items, .. } => items.iter().for_each(|i| self.expr(i, line)),
            Expr::Map { entries, .. } => {
                for (k, v) in entries {
                    self.expr(k, line);
                    self.expr(v, line);
                }
            }
            Expr::Prop { obj, .. } | Expr::Unary { expr: obj, .. } => self.expr(obj, line),
            Expr::Index { obj, index, .. } => {
                self.expr(obj, line);
                index.iter().for_each(|i| self.expr(i, line));
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs, line);
                self.expr(rhs, line);
            }
            Expr::Ternary { cond, then, other, .. } => {
                self.expr(cond, line);
                self.expr(then, line);
                self.expr(other, line);
            }
            Expr::New { args, .. } => args.iter().for_each(|a| self.expr(arg_value(a), line)),
            Expr::Ident { .. } | Expr::Num { .. } => {}
        }
    }

    /// `withCredentials([bindings]) { body }`: the body sees the variables.
    fn with_credentials(&mut self, args: &[Arg]) {
        let mut vars = HashMap::new();
        let mut bodies = Vec::new();
        for a in args {
            match arg_value(a) {
                Expr::List { items, .. } => items.iter().for_each(|b| bindings(b, &mut vars)),
                Expr::Closure { body, .. } => bodies.push(body),
                other => bindings(other, &mut vars),
            }
        }
        for body in bodies {
            self.scoped(vars.clone(), |w| w.block(body));
        }
    }

    fn shell_step(&mut self, step: &str, args: &[Arg], line: usize) {
        let script = args.iter().find_map(|a| match a {
            Arg::Pos(e) => Some(e),
            Arg::Named { key, value, .. } if key == "script" => Some(value),
            Arg::Named { .. } => None,
        });
        let Some(script) = script else { return };
        for u in dedup(self.uses(script, true)) {
            let (severity, confidence, persona) = match u.kind {
                Kind::Secret => (Severity::High, Confidence::High, Persona::Regular),
                // For a secret file, `credentials()` sets a path, which is
                // fine to interpolate: `--kubeconfig ${KUBECONFIG}`.
                Kind::Credential => (Severity::High, Confidence::Medium, Persona::Pedantic),
                Kind::Username => (Severity::Low, Confidence::High, Persona::Pedantic),
            };
            self.findings.push(Finding {
                rule: RULE.id,
                severity,
                confidence,
                persona,
                message: shell_message(&u, step),
                span: u.span,
                stmt_line: line,
            });
        }
    }

    /// Other steps: Jenkins warns about a secret interpolated into any
    /// step's argument, but outside a script it does less harm.
    fn other_step(&mut self, step: &str, args: &[Arg], line: usize) {
        for a in args {
            let value = arg_value(a);
            if !matches!(value, Expr::Str { .. }) {
                continue;
            }
            for u in dedup(self.uses(value, false)) {
                if u.kind == Kind::Username {
                    continue;
                }
                self.findings.push(Finding {
                    rule: RULE.id,
                    severity: Severity::Low,
                    confidence: Confidence::High,
                    persona: Persona::Pedantic,
                    message: format!(
                        "{}, and Groovy interpolates it into an argument of {step}, which Jenkins warns about. \
                         Keep secrets out of Groovy strings.",
                        what(&u)
                    ),
                    span: u.span,
                    stmt_line: line,
                });
            }
        }
    }

    /// The secrets that end up in the string `e` evaluates to. With `deep`,
    /// also those in strings passed to calls inside its placeholders, as in
    /// `"${f("$PASS")}"`; without, the call reports them as its own step.
    fn uses(&self, e: &Expr, deep: bool) -> Vec<Use> {
        let mut out = Vec::new();
        self.collect_uses(e, deep, &mut out);
        out
    }

    fn collect_uses(&self, e: &Expr, deep: bool, out: &mut Vec<Use>) {
        match e {
            Expr::Str { parts, .. } => {
                for p in parts {
                    if let StrPart::Placeholder(ph) = p {
                        if let Some(x) = &ph.expr {
                            self.refs(x, ph.span, deep, out);
                        }
                    }
                }
            }
            Expr::Binary { op, lhs, rhs, .. } if op == "+" => {
                for side in [lhs, rhs] {
                    match name_of(side) {
                        Some((name, span)) if self.secret(&name).is_some() => out.push(Use {
                            kind: self.secret(&name).unwrap(),
                            name,
                            span,
                            via: None,
                            joined: true,
                        }),
                        _ => self.collect_uses(side, deep, out),
                    }
                }
            }
            Expr::Ident { name, pos } => {
                if let Some(held) = self.local(name) {
                    out.extend(held.iter().map(|u| Use {
                        span: ident_span(*pos, name),
                        via: Some(name.clone()),
                        ..u.clone()
                    }));
                }
            }
            Expr::Ternary { then, other, .. } => {
                self.collect_uses(then, deep, out);
                self.collect_uses(other, deep, out);
            }
            // `"""...""".stripIndent()`, `cmd.trim()`: the string with changes.
            Expr::Call { callee, .. } => {
                if let Expr::Prop { obj, .. } = callee.as_ref() {
                    self.collect_uses(obj, deep, out);
                }
            }
            _ => {}
        }
    }

    /// The secrets that a placeholder's expression refers to: `$PASS`,
    /// `${env.PASS}`, `${PASS.trim()}`, or a local variable holding them.
    fn refs(&self, e: &Expr, span: Span, deep: bool, out: &mut Vec<Use>) {
        if let Some((name, _)) = name_of(e) {
            if let Some(kind) = self.secret(&name) {
                out.push(Use {
                    name,
                    kind,
                    span,
                    via: None,
                    joined: false,
                });
                return;
            }
            if let Some(held) = self.local(&name) {
                out.extend(held.iter().map(|u| Use {
                    span,
                    via: Some(name.clone()),
                    ..u.clone()
                }));
                return;
            }
        }
        let mut visit = |x: &Expr| self.refs(x, span, deep, out);
        match e {
            Expr::Prop { obj, .. } | Expr::Unary { expr: obj, .. } => visit(obj),
            Expr::Call { callee, args, .. } => {
                visit(callee);
                for a in args {
                    let arg = arg_value(a);
                    if deep || !matches!(arg, Expr::Str { .. }) {
                        visit(arg);
                    }
                }
            }
            Expr::Index { obj, index, .. } => {
                visit(obj);
                index.iter().for_each(&mut visit);
            }
            Expr::Binary { lhs, rhs, .. } => {
                visit(lhs);
                visit(rhs);
            }
            Expr::Ternary { then, other, .. } => {
                visit(then);
                visit(other);
            }
            Expr::Str { parts, .. } => {
                for p in parts {
                    if let StrPart::Placeholder(ph) = p {
                        if let Some(x) = &ph.expr {
                            visit(x);
                        }
                    }
                }
            }
            Expr::List { items, .. } => items.iter().for_each(&mut visit),
            _ => {}
        }
    }
}

fn arg_value(a: &Arg) -> &Expr {
    match a {
        Arg::Pos(e) | Arg::Named { value: e, .. } => e,
    }
}

/// A variable read as `NAME` or `env.NAME`, and where it is.
fn name_of(e: &Expr) -> Option<(String, Span)> {
    match e {
        Expr::Ident { name, pos } => Some((name.clone(), ident_span(*pos, name))),
        Expr::Prop { obj, name, pos } if matches!(obj.as_ref(), Expr::Ident { name: env, .. } if env == "env") => Some((
            name.clone(),
            Span {
                start: obj.pos(),
                end: ident_span(Pos { col: pos.col + 1, ..*pos }, name).end,
            },
        )),
        _ => None,
    }
}

fn ident_span(start: Pos, name: &str) -> Span {
    Span {
        start,
        end: Pos {
            col: start.col + name.chars().count(),
            ..start
        },
    }
}

/// One finding per secret and place.
fn dedup(mut uses: Vec<Use>) -> Vec<Use> {
    let mut seen = Vec::new();
    uses.retain(|u| {
        let key = (u.name.clone(), u.span);
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    uses
}

/// `environment { TOKEN = credentials('id') }` in this block: it sets
/// TOKEN, and TOKEN_USR and TOKEN_PSW for a user name and password.
fn environment_credentials(stmts: &[Stmt]) -> HashMap<String, Kind> {
    let mut vars = HashMap::new();
    for s in stmts {
        let Stmt::Expr(Expr::Call { callee, args, .. }) = s else { continue };
        if !matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "environment") {
            continue;
        }
        for a in args {
            let Expr::Closure { body, .. } = arg_value(a) else { continue };
            for s in body {
                let Stmt::Expr(Expr::Assign { target, value, .. }) = s else {
                    continue;
                };
                let (Expr::Ident { name, .. }, Expr::Call { callee, .. }) = (target.as_ref(), value.as_ref()) else {
                    continue;
                };
                if matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "credentials") {
                    vars.insert(name.clone(), Kind::Credential);
                    vars.insert(format!("{name}_USR"), Kind::Username);
                    vars.insert(format!("{name}_PSW"), Kind::Secret);
                }
            }
        }
    }
    vars
}

/// The variables one `withCredentials` binding sets: `usernamePassword(...)`
/// or `[$class: 'AmazonWebServicesCredentialsBinding', ...]`.
fn bindings(e: &Expr, vars: &mut HashMap<String, Kind>) {
    let mut named: Vec<(String, &Expr)> = Vec::new();
    let binding = match e {
        Expr::Call { callee, args, .. } => {
            for a in args {
                if let Arg::Named { key, value, .. } = a {
                    named.push((key.clone(), value));
                }
            }
            match callee.as_ref() {
                Expr::Ident { name, .. } => name.clone(),
                _ => return,
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

fn what(u: &Use) -> String {
    match u.kind {
        Kind::Secret => format!("{} is a secret", u.name),
        Kind::Credential => format!("{} holds a credential", u.name),
        Kind::Username => format!("{} is a credential's user name or ID", u.name),
    }
}

fn shell_message(u: &Use, step: &str) -> String {
    let how = match (&u.via, u.joined) {
        (Some(var), _) => format!("Groovy puts it into {var}, which becomes the {step} script"),
        (None, true) => format!("+ joins it into the {step} script"),
        (None, false) => format!("Groovy interpolates it into the {step} script"),
    };
    let read = match step {
        "bat" => format!("%{}%", u.name),
        "powershell" | "pwsh" => format!("$env:{}", u.name),
        _ => format!("\"${}\"", u.name),
    };
    format!(
        "{}, and {how}: the process arguments show it, and shell metacharacters in it run. \
         Use single quotes, as in {step} '... {read} ...', so that the shell reads it from the environment.",
        what(u)
    )
}

#[cfg(test)]
mod tests {
    use super::super::{audit, Filter};
    use crate::lexer::tokenize_with_comments;
    use crate::parser::parse;

    fn messages(src: &str, filter: Filter) -> Vec<String> {
        let (tokens, comments) = tokenize_with_comments(src).unwrap();
        audit(&parse(tokens).unwrap(), &comments, &filter)
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
        assert_eq!(messages(&src, pedantic).len(), 3);
        let high = Filter {
            min_severity: Severity::High,
            ..pedantic
        };
        assert_eq!(messages(&src, high).len(), 1);
        let certain = Filter {
            min_confidence: Confidence::High,
            ..pedantic
        };
        assert_eq!(messages(&src, certain).len(), 3);
        // A secret in a string passed to a call in a placeholder is that
        // call's finding, once; a `when` condition is not a step.
        let nested = format!("{CREDS}\n writeFile file: 'k', text: \"${{readFile file: \"$P\"}}\"\n}}");
        assert_eq!(messages(&nested, pedantic).len(), 1);
        let when = format!("{CREDS}\n when {{ environment name: 'X', value: \"a $P\" }}\n}}");
        assert!(messages(&when, pedantic).is_empty());
        // An ignore comment for another rule does not hide this one.
        let other = format!("{CREDS}\n // jenkinsfilelint: ignore[script-injection]\n sh \"$P\"\n}}");
        assert_eq!(messages(&other, Filter::default()).len(), 1);
    }
}
