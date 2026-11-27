//! Where values go: the dataflow that the rules about steps read.
//!
//! The walker follows the values the rules care about, secrets and input
//! from outside the pipeline, through a file: into local variables,
//! environment variables and strings, and into the steps that receive
//! them. Each time one reaches a step, it records a flow, and each rule
//! decides which flows are findings.

use std::collections::{HashMap, HashSet};

use super::fix::Script;
use super::script_injection::{self, Input};
use super::secret_interpolation::{self, Kind};
use crate::ast::{visit, Arg, Expr, Node, Stmt, StrPart};
use crate::diag::{Pos, Span};

/// Steps that run their argument as a script.
pub const SHELL_STEPS: &[&str] = &["sh", "bat", "powershell", "pwsh"];

/// What a value is, for the rules.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Label {
    /// From `withCredentials` or `credentials()`.
    Secret(Kind),
    /// From outside the pipeline: a pull request, a build parameter.
    Input(&'static Input),
}

/// How a value gets into a step's argument.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum How {
    /// A GString placeholder: `"... ${X} ..."`.
    Interpolated,
    /// Joined with `+`: `'... ' + X`.
    Joined,
    /// The whole argument: `sh X`.
    Whole,
}

/// A labeled value that ends up in a step's argument.
#[derive(Debug, Clone)]
pub struct Use {
    /// The variable it comes from: `PASS`, `CHANGE_TITLE`, `params.VERSION`.
    pub name: String,
    pub label: Label,
    /// Where it enters the argument: a placeholder, or the name itself.
    pub span: Span,
    pub how: How,
    /// The variable it went through: `cmd` in `def cmd = "...$PASS..."`.
    pub via: Option<String>,
    /// That variable is an environment variable.
    pub via_env: bool,
    /// Passed through something that makes it safe in a shell, such as
    /// `toInteger()` or `URLEncoder.encode`. A secret is still a secret.
    pub escaped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sink {
    /// A script, or a command line that a step runs in a shell.
    Script,
    /// A string argument of any other step.
    Argument,
}

/// A labeled value that reaches a step.
#[derive(Debug, Clone)]
pub struct Flow {
    pub sink: Sink,
    /// The step: `sh`, `echo`, `docker.build`.
    pub step: String,
    pub value: Use,
    /// Where the statement it is in starts.
    pub stmt_line: usize,
    /// The script literal it is in, when that literal is the step's
    /// argument itself: an index into the walk's scripts.
    pub script: Option<usize>,
}

/// What a step runs: its script, or the command line it runs in a shell.
pub fn target(step: &str) -> String {
    match step {
        "docker.build" | "run" | "withRun" => format!("the command line that {step} runs in a shell"),
        _ => format!("the {step} script"),
    }
}

/// How a value gets into what a step runs, for messages.
pub fn how_it_gets_there(u: &Use, step: &str) -> String {
    let target = target(step);
    // A shell step's argument is its script; Docker's join a command line.
    let (is, becomes) = if SHELL_STEPS.contains(&step) {
        ("is", "becomes")
    } else {
        ("goes into", "goes into")
    };
    match (&u.via, u.how) {
        (Some(var), How::Whole) => format!("Groovy puts it into {var}, which {becomes} {target}"),
        (Some(var), How::Joined) => format!("Groovy puts it into {var}, which + joins into {target}"),
        (Some(var), How::Interpolated) => format!("Groovy puts it into {var} and interpolates that into {target}"),
        (None, How::Whole) => format!("it {is} {target}"),
        (None, How::Joined) => format!("+ joins it into {target}"),
        (None, How::Interpolated) => format!("Groovy interpolates it into {target}"),
    }
}

/// Every flow in a file, in the order the walk meets them, and the script
/// literals passed straight to shell steps.
pub fn flows(stmts: &[Stmt]) -> (Vec<Flow>, Vec<Script>) {
    let sources = script_injection::sources(stmts);
    let root = sources
        .names
        .into_iter()
        .map(|(name, input)| (name, Binding::Source(Label::Input(input))))
        .collect();
    let mut w = Walker {
        scopes: vec![root],
        prefixes: sources.prefixes,
        images: docker_images(stmts),
        flows: Vec::new(),
        scripts: Vec::new(),
    };
    w.block(stmts);
    (w.flows, w.scripts)
}

/// What a name refers to.
#[derive(Debug, Clone)]
enum Binding {
    /// A source: `PASS` in `withCredentials`, `CHANGE_TITLE`.
    Source(Label),
    /// A variable that holds these values, maybe none: a local variable,
    /// or an environment variable (`true`) set from them.
    Holds(Vec<Use>, bool),
}

struct Walker {
    /// Names in scope, innermost last; the first holds the inputs every
    /// build can read.
    scopes: Vec<HashMap<String, Binding>>,
    /// Sources known by the start of their name: `pr_` for the variables
    /// that the Generic Webhook Trigger makes from a JSON object `pr`.
    prefixes: Vec<(String, &'static Input)>,
    /// Variables that hold a Docker image: `def img = docker.image(...)`.
    images: HashSet<String>,
    flows: Vec<Flow>,
    scripts: Vec<Script>,
}

impl Walker {
    fn block(&mut self, stmts: &[Stmt]) {
        self.scopes.push(HashMap::new());
        self.environment(stmts);
        for s in stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn scoped(&mut self, names: HashMap<String, Binding>, body: &[Stmt]) {
        self.scopes.push(names);
        self.block(body);
        self.scopes.pop();
    }

    fn bind(&mut self, name: &str, binding: Binding) {
        self.scopes.last_mut().expect("in a block").insert(name.to_string(), binding);
    }

    /// `environment { ... }` in this block: `X = credentials('id')` binds
    /// secrets, and any other variable holds what its value holds.
    fn environment(&mut self, stmts: &[Stmt]) {
        for s in stmts {
            let Stmt::Expr(Expr::Call { callee, args, .. }) = s else { continue };
            if callee.method_name() != Some("environment") {
                continue;
            }
            for a in args {
                let Expr::Closure { body, .. } = arg_value(a) else { continue };
                for s in body {
                    let Stmt::Expr(Expr::Assign { target, value, .. }) = s else {
                        continue;
                    };
                    let Expr::Ident { name, .. } = target.as_ref() else { continue };
                    match secret_interpolation::credentials(name, value) {
                        Some(secrets) => {
                            for (var, kind) in secrets {
                                self.bind(&var, Binding::Source(Label::Secret(kind)));
                            }
                        }
                        None => {
                            let held = self.uses(value, true);
                            self.bind(name, Binding::Holds(held, true));
                        }
                    }
                }
            }
        }
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
                let held = match (names.as_slice(), value) {
                    ([_], Some(v)) => self.uses(v, true),
                    _ => Vec::new(),
                };
                for name in names {
                    self.bind(name, Binding::Holds(held.clone(), false));
                }
            }
            // Its parameters hide the variables of the same name outside.
            Stmt::MethodDef { params, body, .. } => self.scoped(locals(params), body),
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
                match (callee.method_name(), callee.as_ref()) {
                    (Some(name), _) => match name {
                        "withCredentials" => return self.with_credentials(args),
                        "withEnv" => return self.with_env(args, line),
                        // `block` binds the variables of an `environment`
                        // block; their values can still hold steps. In
                        // `when { environment name: 'X', value: "..." }`,
                        // it compares.
                        "environment" => {
                            for a in args {
                                match arg_value(a) {
                                    Expr::Closure { body, .. } => {
                                        for s in body {
                                            match s {
                                                Stmt::Expr(Expr::Assign { value, .. }) => self.expr(value, line),
                                                s => self.stmt(s),
                                            }
                                        }
                                    }
                                    other => self.expr(other, line),
                                }
                            }
                            return;
                        }
                        s if SHELL_STEPS.contains(&s) => {
                            let script = args.iter().find_map(|a| match a {
                                Arg::Pos(e) => Some(e),
                                Arg::Named { key, value, .. } if key == "script" => Some(value),
                                Arg::Named { .. } => None,
                            });
                            if let Some(script) = script {
                                let index = self.script(s, script);
                                self.sink(Sink::Script, s, script, true, line, index);
                            }
                        }
                        s => {
                            for a in args {
                                let value = arg_value(a);
                                if matches!(value, Expr::Str { .. }) {
                                    self.sink(Sink::Argument, s, value, false, line, None);
                                }
                            }
                        }
                    },
                    (None, Expr::Prop { obj, name, .. }) => self.docker(obj, name, args, line),
                    _ => {}
                }
                self.expr(callee, line);
                for a in args {
                    self.expr(arg_value(a), line);
                }
            }
            Expr::Closure { params, body, .. } => self.scoped(locals(params), body),
            Expr::Assign { op, target, value, .. } => {
                self.expr(value, line);
                let mut held = self.uses(value, true);
                match target.as_ref() {
                    Expr::Ident { name, .. } => {
                        if *op == "+=" {
                            let mut old = self.lookup_held(name);
                            old.append(&mut held);
                            held = old;
                        }
                        // Where the variable is declared, or here.
                        let level = self
                            .scopes
                            .iter()
                            .rposition(|s| s.contains_key(name))
                            .unwrap_or(self.scopes.len() - 1);
                        let env = matches!(self.scopes[level].get(name), Some(Binding::Holds(_, true)));
                        self.scopes[level].insert(name.clone(), Binding::Holds(held, env));
                    }
                    // `env.X = ...` sets X for the rest of the build.
                    target => {
                        if let Some(name) = self.name_of(target).and_then(|(n, _)| n.strip_prefix("env.").map(String::from)) {
                            self.scopes[0].insert(name, Binding::Holds(held, true));
                        }
                    }
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

    /// Record the labeled values in `value` as flows into `step`, once per
    /// value and place.
    fn sink(&mut self, sink: Sink, step: &str, value: &Expr, deep: bool, line: usize, script: Option<usize>) {
        let mut seen = Vec::new();
        for u in self.uses(value, deep) {
            let key = (u.name.clone(), u.span);
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            self.flows.push(Flow {
                sink,
                step: step.to_string(),
                value: u,
                stmt_line: line,
                script,
            });
        }
    }

    /// Record a GString passed as a shell step's script, with the
    /// environment variable each placeholder reads.
    fn script(&mut self, step: &str, e: &Expr) -> Option<usize> {
        let Expr::Str {
            interp: true, parts, pos, ..
        } = e
        else {
            return None;
        };
        let placeholders = parts
            .iter()
            .filter_map(|p| match p {
                StrPart::Placeholder(ph) => Some((ph.span, ph.expr.as_ref().and_then(|x| self.environment_name(x)))),
                StrPart::Text(_) => None,
            })
            .collect();
        self.scripts.push(Script {
            step: step.to_string(),
            pos: *pos,
            placeholders,
        });
        Some(self.scripts.len() - 1)
    }

    /// The environment variable that `e` reads, by a name the shell can
    /// read it by: `X` for `env.X`, `env['X']` and `params.X`, and for `X`
    /// when it is an environment variable here.
    fn environment_name(&self, e: &Expr) -> Option<String> {
        let (name, _) = self.name_of(e)?;
        let binding = |var: &str| self.scopes.iter().rev().find_map(|s| s.get(var));
        let var = if let Some(var) = name.strip_prefix("env.") {
            var
        } else if let Some(var) = name.strip_prefix("params.") {
            // A build parameter is an environment variable, unless a
            // variable of the same name hides it.
            match binding(var) {
                None | Some(Binding::Source(Label::Input(_))) => var,
                _ => return None,
            }
        } else {
            match binding(&name) {
                Some(Binding::Source(_) | Binding::Holds(_, true)) => &name,
                Some(Binding::Holds(_, false)) => return None,
                None if self.prefixes.iter().any(|(p, _)| name.starts_with(p.as_str())) || JENKINS_VARIABLES.contains(&name.as_str()) => {
                    &name
                }
                None => return None,
            }
        };
        let shell_name = var.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && var.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        shell_name.then(|| var.to_string())
    }

    /// The Docker Pipeline steps that join their arguments into a shell
    /// command line: `docker.build(name, args)`, and `run(args, command)`
    /// and `withRun(args, command)` on an image. It passes the image name
    /// in the environment.
    fn docker(&mut self, obj: &Expr, method: &str, args: &[Arg], line: usize) {
        let positional = args.iter().filter_map(|a| match a {
            Arg::Pos(e) if !matches!(e, Expr::Closure { .. }) => Some(e),
            _ => None,
        });
        let (step, command_line): (&str, Vec<&Expr>) = match method {
            "build" if is_ident(obj, "docker") => ("docker.build", positional.skip(1).take(1).collect()),
            "run" | "withRun" if self.is_image(obj) => (method, positional.take(2).collect()),
            _ => return,
        };
        for e in command_line {
            self.sink(Sink::Script, step, e, true, line, None);
        }
    }

    fn is_image(&self, e: &Expr) -> bool {
        match e {
            Expr::Ident { name, .. } => self.images.contains(name),
            _ => is_image_call(e),
        }
    }

    /// `withCredentials([bindings]) { body }`: the body sees the variables.
    fn with_credentials(&mut self, args: &[Arg]) {
        let mut vars = HashMap::new();
        let mut bodies = Vec::new();
        for a in args {
            match arg_value(a) {
                Expr::List { items, .. } => items.iter().for_each(|b| secret_interpolation::bindings(b, &mut vars)),
                Expr::Closure { body, .. } => bodies.push(body),
                other => secret_interpolation::bindings(other, &mut vars),
            }
        }
        let names: HashMap<String, Binding> = vars
            .into_iter()
            .map(|(name, kind)| (name, Binding::Source(Label::Secret(kind))))
            .collect();
        for body in bodies {
            self.scoped(names.clone(), body);
        }
    }

    /// `withEnv(["NAME=${value}"]) { body }`: in the body, NAME holds what
    /// the value holds.
    fn with_env(&mut self, args: &[Arg], line: usize) {
        let mut names = HashMap::new();
        let mut bodies = Vec::new();
        for a in args {
            match arg_value(a) {
                Expr::List { items, .. } => {
                    for item in items {
                        self.expr(item, line);
                        if let Some(name) = env_entry_name(item) {
                            names.insert(name, Binding::Holds(self.uses(item, true), true));
                        }
                    }
                }
                Expr::Closure { body, .. } => bodies.push(body),
                other => self.expr(other, line),
            }
        }
        for body in bodies {
            self.scoped(names.clone(), body);
        }
    }

    /// `name_of`, unless `env` or `params` in `env.X` or `params.X` is a
    /// variable of the file, as in `def call(Map params)`: then it is not
    /// the build's.
    fn name_of(&self, e: &Expr) -> Option<(String, Span)> {
        let found = name_of(e)?;
        if let Some((qualifier, _)) = found.0.split_once('.') {
            if self.scopes.iter().any(|s| s.contains_key(qualifier)) {
                return None;
            }
        }
        Some(found)
    }

    fn lookup_held(&self, name: &str) -> Vec<Use> {
        match self.scopes.iter().rev().find_map(|s| s.get(name)) {
            Some(Binding::Holds(held, _)) => held.clone(),
            _ => Vec::new(),
        }
    }

    /// The labeled values that the value of `e` carries. With `deep`, also
    /// those in strings passed to calls inside its placeholders, as in
    /// `"${f("$PASS")}"`; without, the call reports them as its own step.
    fn uses(&self, e: &Expr, deep: bool) -> Vec<Use> {
        let mut out = Vec::new();
        self.collect(e, None, How::Whole, deep, &mut out);
        out
    }

    /// `at` is the placeholder that `e` is in, where its values enter the
    /// string; outside one, they are where their names are.
    fn collect(&self, e: &Expr, at: Option<Span>, how: How, deep: bool, out: &mut Vec<Use>) {
        if let Some((name, span)) = self.name_of(e) {
            if self.lookup(&name, at.unwrap_or(span), how, out) {
                return;
            }
        }
        match e {
            Expr::Str { parts, .. } => {
                for p in parts {
                    if let StrPart::Placeholder(ph) = p {
                        if let Some(x) = &ph.expr {
                            self.collect(x, Some(at.unwrap_or(ph.span)), How::Interpolated, deep, out);
                        }
                    }
                }
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                match op.as_str() {
                    "+" => {
                        let how = if at.is_none() { How::Joined } else { how };
                        self.collect(lhs, at, how, deep, out);
                        self.collect(rhs, at, how, deep, out);
                    }
                    "?:" => {
                        self.collect(lhs, at, how, deep, out);
                        self.collect(rhs, at, how, deep, out);
                    }
                    // `BRANCH - 'origin/'` removes text.
                    "-" => self.collect(lhs, at, how, deep, out),
                    // `X as Integer`
                    "as" => {
                        let start = out.len();
                        self.collect(lhs, at, how, deep, out);
                        if matches!(rhs.as_ref(), Expr::Ident { name, .. } if NUMBER_TYPES.contains(&name.as_str())) {
                            out[start..].iter_mut().for_each(|u| u.escaped = true);
                        }
                    }
                    // Comparisons, logic and arithmetic: a boolean or a number.
                    _ => {}
                }
            }
            Expr::Ternary { then, other, .. } => {
                self.collect(then, at, how, deep, out);
                self.collect(other, at, how, deep, out);
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Prop { name, .. } = callee.as_ref() {
                    if TELLS_ABOUT.contains(&name.as_str()) {
                        return;
                    }
                }
                let start = out.len();
                match callee.as_ref() {
                    // `X.trim()`: the string, changed.
                    Expr::Prop { obj, .. } => self.collect(obj, at, how, deep, out),
                    callee if at.is_some() => self.collect(callee, at, how, deep, out),
                    _ => {}
                }
                // What a step or function returns, `sh(...)` or
                // `api.get(USER, PASS)`, is not its arguments; in a
                // placeholder, `"${f(X)}"`, it often shows them.
                if at.is_some() {
                    for a in args {
                        let arg = arg_value(a);
                        if deep || !matches!(arg, Expr::Str { .. }) {
                            self.collect(arg, at, how, deep, out);
                        }
                    }
                }
                if escapes(callee, args) {
                    out[start..].iter_mut().for_each(|u| u.escaped = true);
                }
            }
            Expr::Prop { obj, name, .. } => {
                if !TELLS_ABOUT.contains(&name.as_str()) {
                    self.collect(obj, at, how, deep, out);
                }
            }
            Expr::Index { obj, .. } => self.collect(obj, at, how, deep, out),
            Expr::Unary { op, expr, .. } => {
                if *op != "!" {
                    self.collect(expr, at, how, deep, out);
                }
            }
            Expr::List { items, .. } => items.iter().for_each(|i| self.collect(i, at, how, deep, out)),
            Expr::New { args, .. } => {
                for a in args {
                    self.collect(arg_value(a), at, how, deep, out);
                }
            }
            Expr::Assign { value, .. } => self.collect(value, at, how, deep, out),
            Expr::Ident { .. } | Expr::Num { .. } | Expr::Map { .. } | Expr::Closure { .. } => {}
        }
    }

    /// The values a name refers to, if it is a name the walk knows. `env.X`
    /// is looked up as X.
    fn lookup(&self, name: &str, span: Span, how: How, out: &mut Vec<Use>) -> bool {
        let var = name.strip_prefix("env.").unwrap_or(name);
        let source = |label| Use {
            name: var.to_string(),
            label,
            span,
            how,
            via: None,
            via_env: false,
            escaped: false,
        };
        match self.scopes.iter().rev().find_map(|s| s.get(var)) {
            Some(Binding::Source(label)) => out.push(source(*label)),
            Some(Binding::Holds(held, env)) => out.extend(held.iter().map(|u| Use {
                span,
                how,
                via: Some(var.to_string()),
                via_env: *env,
                ..u.clone()
            })),
            None => {
                let input = match var.strip_prefix("params.") {
                    Some(_) => Some(script_injection::undeclared_parameter()),
                    None => self.prefixes.iter().find(|(p, _)| var.starts_with(p.as_str())).map(|(_, i)| *i),
                };
                match input {
                    Some(input) => out.push(source(Label::Input(input))),
                    None => return false,
                }
            }
        }
        true
    }
}

/// Environment variables that Jenkins sets in every build, which Groovy
/// reads by name too.
const JENKINS_VARIABLES: &[&str] = &[
    "BUILD_DISPLAY_NAME",
    "BUILD_ID",
    "BUILD_NUMBER",
    "BUILD_TAG",
    "BUILD_URL",
    "EXECUTOR_NUMBER",
    "JENKINS_HOME",
    "JENKINS_URL",
    "JOB_BASE_NAME",
    "JOB_NAME",
    "JOB_URL",
    "NODE_LABELS",
    "NODE_NAME",
    "WORKSPACE",
    "WORKSPACE_TMP",
];

/// Methods and properties whose result tells something about a string
/// without holding its text: a length, a boolean.
const TELLS_ABOUT: &[&str] = &[
    "size",
    "length",
    "isEmpty",
    "isBlank",
    "asBoolean",
    "toBoolean",
    "isNumber",
    "isInteger",
    "isLong",
    "isDouble",
    "isFloat",
    "isBigInteger",
    "isBigDecimal",
    "startsWith",
    "endsWith",
    "contains",
    "containsKey",
    "equals",
    "equalsIgnoreCase",
    "matches",
    "isCase",
    "hashCode",
    "count",
    "indexOf",
    "lastIndexOf",
    "compareTo",
    "compareToIgnoreCase",
    "any",
    "every",
];

/// Conversions whose result has no shell syntax in it, though it can still
/// hold a secret.
const ESCAPING_METHODS: &[&str] = &[
    "toInteger",
    "toLong",
    "toBigInteger",
    "toBigDecimal",
    "toDouble",
    "toFloat",
    "encodeBase64",
    "encodeBase64Url",
    "encodeHex",
];

const NUMBER_TYPES: &[&str] = &[
    "int",
    "long",
    "short",
    "byte",
    "double",
    "float",
    "boolean",
    "Integer",
    "Long",
    "Short",
    "Byte",
    "Double",
    "Float",
    "Boolean",
    "BigInteger",
    "BigDecimal",
];

/// Whether a call makes its result safe in a shell: a conversion,
/// `URLEncoder.encode(x, ...)`, `x.replaceAll(/[^A-Za-z0-9._-]/, '')`, or
/// the usual quoting for single quotes, `x.replace("'", "'\\''")`.
fn escapes(callee: &Expr, args: &[Arg]) -> bool {
    let Expr::Prop { obj, name, .. } = callee else { return false };
    let text = |i: usize| match args.get(i).map(arg_value) {
        Some(Expr::Str { value, interp: false, .. }) => Some(value.as_str()),
        _ => None,
    };
    match name.as_str() {
        n if ESCAPING_METHODS.contains(&n) => true,
        "encode" => is_ident(obj, "URLEncoder"),
        "replaceAll" | "replace" => match (text(0), text(1)) {
            (Some(pattern), Some(with)) if name == "replaceAll" && script_injection::is_safe_filter(pattern) => {
                with.chars().all(script_injection::is_inert)
            }
            (Some("'"), Some(with)) => with == "'\\''" || with == "'\"'\"'",
            _ => false,
        },
        _ => false,
    }
}

/// Parameters of a method or closure: variables that hold nothing known.
fn locals(names: &[String]) -> HashMap<String, Binding> {
    names.iter().map(|n| (n.clone(), Binding::Holds(Vec::new(), false))).collect()
}

fn arg_value(a: &Arg) -> &Expr {
    match a {
        Arg::Pos(e) | Arg::Named { value: e, .. } => e,
    }
}

fn is_ident(e: &Expr, name: &str) -> bool {
    matches!(e, Expr::Ident { name: n, .. } if n == name)
}

/// A variable read as `NAME`, `env.NAME`, `env['NAME']`, `params.NAME` or
/// `params['NAME']`, and where it is. Environment variables come back as
/// `env.NAME` and build parameters as `params.NAME`.
fn name_of(e: &Expr) -> Option<(String, Span)> {
    let qualifier = |obj: &Expr| match obj {
        Expr::Ident { name, .. } if name == "env" || name == "params" => Some(name.clone()),
        _ => None,
    };
    match e {
        Expr::Ident { name, pos } => Some((name.clone(), ident_span(*pos, name))),
        Expr::Prop { obj, name, pos } => Some((
            format!("{}.{name}", qualifier(obj)?),
            Span {
                start: obj.pos(),
                end: ident_span(Pos { col: pos.col + 1, ..*pos }, name).end,
            },
        )),
        Expr::Index { obj, index, pos } => {
            let q = qualifier(obj)?;
            let [Expr::Str { value, interp: false, .. }] = index.as_slice() else {
                return None;
            };
            Some((
                format!("{q}.{value}"),
                Span {
                    start: obj.pos(),
                    end: *pos,
                },
            ))
        }
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

/// `NAME` in a `withEnv` entry `"NAME=value"`.
fn env_entry_name(e: &Expr) -> Option<String> {
    let text = match e {
        Expr::Str { value, .. } => value,
        Expr::Binary { op, lhs, .. } if op == "+" => match lhs.as_ref() {
            Expr::Str { value, .. } => value,
            _ => return None,
        },
        _ => return None,
    };
    let name = &text[..text.find('=')?];
    let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| name.to_string())
}

/// `docker.image(...)` or `docker.build(...)`.
fn is_image_call(e: &Expr) -> bool {
    let Expr::Call { callee, .. } = e else { return false };
    matches!(callee.as_ref(), Expr::Prop { obj, name, .. } if is_ident(obj, "docker") && (name == "image" || name == "build"))
}

/// Variables that are ever given a Docker image.
fn docker_images(stmts: &[Stmt]) -> HashSet<String> {
    let mut images = HashSet::new();
    visit(stmts, &mut |node| match node {
        Node::Stmt(Stmt::Decl { names, value: Some(v), .. }) if is_image_call(v) => images.extend(names.iter().cloned()),
        Node::Expr(Expr::Assign { target, value, .. }) if is_image_call(value) => {
            if let Expr::Ident { name, .. } = target.as_ref() {
                images.insert(name.clone());
            }
        }
        _ => {}
    });
    images
}
