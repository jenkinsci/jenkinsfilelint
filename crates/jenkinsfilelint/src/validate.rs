//! Declarative Pipeline structure checks.
//!
//! Error messages reuse the wording of the official validator
//! (pipeline-model-definition plugin) so that offline and online results
//! read the same.
//!
//! Anything that depends on which plugins a controller has installed
//! (step names, step parameters, extra agent types) is not checked here.

use std::collections::HashSet;

use crate::ast::{Arg, Expr, Section, Stmt};
use crate::diag::{Diagnostic, Pos, Report};

const TOP_SECTIONS: &[&str] = &[
    "agent",
    "stages",
    "post",
    "environment",
    "options",
    "parameters",
    "triggers",
    "tools",
    "libraries",
];
const STAGE_SECTIONS: &[&str] = &[
    "agent",
    "environment",
    "input",
    "options",
    "post",
    "tools",
    "when",
    "steps",
    "stages",
    "parallel",
    "matrix",
    "failFast",
];
const MATRIX_SECTIONS: &[&str] = &[
    "axes",
    "excludes",
    "stages",
    "agent",
    "environment",
    "input",
    "options",
    "post",
    "tools",
    "when",
];
const POST_CONDITIONS: &[&str] = &[
    "always",
    "changed",
    "fixed",
    "regression",
    "aborted",
    "success",
    "unsuccessful",
    "unstable",
    "failure",
    "notBuilt",
    "cleanup",
];
const AGENT_TYPES: &[&str] = &["any", "docker", "dockerfile", "kubernetes", "label", "node", "none"];
const WHEN_CONDITIONS: &[&str] = &[
    "allOf",
    "anyOf",
    "branch",
    "buildingTag",
    "changeRequest",
    "changelog",
    "changeset",
    "environment",
    "equals",
    "expression",
    "isRestartedRun",
    "not",
    "tag",
    "triggeredBy",
];
const WHEN_FLAGS: &[&str] = &["beforeAgent", "beforeInput", "beforeOptions"];
const INPUT_FIELDS: &[&str] = &["message", "id", "ok", "submitter", "submitterParameter", "parameters"];

pub fn validate(stmts: &[Stmt]) -> Report {
    let mut v = Validator::default();
    let mut seen_pipeline = false;
    for stmt in stmts {
        match stmt.section() {
            Some(sec) if sec.name == "pipeline" => {
                if seen_pipeline {
                    v.err(sec.pos, "Multiple occurrences of the pipeline section");
                }
                seen_pipeline = true;
                v.pipeline(&sec);
            }
            _ => match stmt {
                // Shared library steps: def call() { pipeline { ... } }
                Stmt::MethodDef { body, .. } => v.library_body(body),
                other => v.no_nested_pipeline(other),
            },
        }
    }
    v.diags.sort_by_key(|d| d.pos);
    Report {
        diagnostics: v.diags,
        declarative: v.declarative,
    }
}

#[derive(Default)]
struct Validator {
    diags: Vec<Diagnostic>,
    /// A `pipeline` block was checked.
    declarative: bool,
    stage: String,
}

fn list(items: &[&str]) -> String {
    format!("[{}]", items.join(", "))
}

fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_') && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A literal axis value: a plain string, number or boolean.
fn plain_string(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Pos(Expr::Str { value, interp: false, .. }) => Some(value),
        Arg::Pos(Expr::Num { text, .. }) => Some(text),
        Arg::Pos(Expr::Ident { name, .. }) if name == "true" || name == "false" => Some(name),
        _ => None,
    }
}

/// An axis name: a plain string, or a bare word, which Jenkins reads as
/// the same string (`name compiler` is `name 'compiler'`).
fn axis_name(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Pos(Expr::Str { value, interp: false, .. }) => Some(value),
        Arg::Pos(Expr::Ident { name, .. }) => Some(name),
        _ => None,
    }
}

fn arg_expr(arg: &Arg) -> &Expr {
    match arg {
        Arg::Pos(e) => e,
        Arg::Named { value, .. } => value,
    }
}

/// Walk every closure in an expression.
fn for_each_closure<'a>(e: &'a Expr, f: &mut dyn FnMut(&'a [Stmt])) {
    match e {
        Expr::Closure { body, .. } => f(body),
        Expr::Call { callee, args, .. } => {
            for_each_closure(callee, f);
            for a in args {
                for_each_closure(arg_expr(a), f);
            }
        }
        Expr::Assign { value, .. } => for_each_closure(value, f),
        Expr::List { items, .. } => items.iter().for_each(|i| for_each_closure(i, f)),
        Expr::Map { entries, .. } => entries.iter().for_each(|(_, v)| for_each_closure(v, f)),
        Expr::Prop { obj, .. } => for_each_closure(obj, f),
        Expr::Paren { inner, .. } => for_each_closure(inner, f),
        _ => {}
    }
}

impl Validator {
    fn err(&mut self, pos: Pos, msg: impl Into<String>) {
        self.diags.push(Diagnostic::error(pos, msg));
    }

    /// Body of a shared library step (`def call() { ... }`), where a
    /// pipeline may sit inside plain control flow.
    fn library_body(&mut self, body: &[Stmt]) {
        for inner in body {
            match (inner.section(), inner) {
                (Some(sec), _) if sec.name == "pipeline" => self.pipeline(&sec),
                (_, Stmt::Control { bodies, .. }) => bodies.iter().for_each(|b| self.library_body(b)),
                _ => self.no_nested_pipeline(inner),
            }
        }
    }

    /// `pipeline { }` anywhere but the top level is an error.
    fn no_nested_pipeline(&mut self, stmt: &Stmt) {
        let mut bodies: Vec<&[Stmt]> = Vec::new();
        match stmt {
            Stmt::Expr(e) => for_each_closure(e, &mut |b| bodies.push(b)),
            Stmt::Decl { value: Some(e), .. } => for_each_closure(e, &mut |b| bodies.push(b)),
            Stmt::Control { bodies: bs, .. } => bodies.extend(bs.iter().map(|b| b.as_slice())),
            Stmt::MethodDef { body, .. } => bodies.push(body),
            _ => {}
        }
        for body in bodies {
            for inner in body {
                if let Some(sec) = inner.section() {
                    if sec.name == "pipeline" && sec.body.is_some() {
                        self.err(sec.pos, "pipeline block must be at the top-level, not within another block.");
                        continue;
                    }
                }
                self.no_nested_pipeline(inner);
            }
        }
    }

    /// Split a block into named sections, reporting duplicates and
    /// statements that are not sections at all.
    fn sections<'a>(&mut self, body: &'a [Stmt], allowed: &[&str], unknown: &dyn Fn(&str) -> String, in_stage: bool) -> Vec<Section<'a>> {
        let invalid = if in_stage {
            "Not a valid stage section definition"
        } else {
            "Not a valid section definition"
        };
        let mut out: Vec<Section<'a>> = Vec::new();
        let mut seen = HashSet::new();
        for stmt in body {
            let Some(sec) = stmt.section() else {
                let what = match stmt {
                    Stmt::Expr(e) => e.render(),
                    Stmt::Decl { names, .. } => names.join(", "),
                    Stmt::MethodDef { name, .. } => name.clone(),
                    _ => "statement".into(),
                };
                self.err(stmt.pos(), format!("{invalid}: \"{what}\". Some extra configuration is required."));
                continue;
            };
            if !allowed.contains(&sec.name) {
                let msg = match sec.name {
                    "properties" => "The \u{2018}properties\u{2019} section has been renamed as of version 0.8. Use \u{2018}options\u{2019} instead.".into(),
                    "jobProperties" => "The \u{2018}jobProperties\u{2019} section has been renamed as of version 0.7. Use \u{2018}properties\u{2019} instead.".into(),
                    "notifications" => "The \u{2018}notifications\u{2019} section has been removed as of version 0.6. Use \u{2018}post\u{2019} for all post-build actions.".into(),
                    "postBuild" => "The \u{2018}postBuild\u{2019} section has been renamed as of version 0.6. Use \u{2018}post\u{2019} for all post-build actions.".into(),
                    name => unknown(name),
                };
                self.err(sec.pos, msg);
                continue;
            }
            if sec.args.is_empty() && sec.body.is_none() {
                self.err(
                    sec.pos,
                    format!("{invalid}: \"{}\". Some extra configuration is required.", sec.name),
                );
                continue;
            }
            if !seen.insert(sec.name) {
                self.err(sec.pos, format!("Multiple occurrences of the {} section", sec.name));
                continue;
            }
            out.push(sec);
        }
        out
    }

    fn pipeline(&mut self, sec: &Section) {
        self.declarative = true;
        let Some(body) = sec.body else {
            self.err(sec.pos, "Expected a block with the \"pipeline\" step");
            return;
        };
        let secs = self.sections(body, TOP_SECTIONS, &|n| format!("Undefined section \"{n}\""), false);
        for required in ["stages", "agent"] {
            if !secs.iter().any(|s| s.name == required) && !self.diags.iter().any(|d| d.message.contains(&format!("\"{required}\""))) {
                self.err(sec.pos, format!("Missing required section \"{required}\""));
            }
        }
        for s in &secs {
            self.section(s, false);
        }
    }

    /// Sections shared by the pipeline, stages and matrix.
    fn section(&mut self, s: &Section, nested: bool) {
        match s.name {
            "agent" => self.agent(s),
            "stages" => self.stages(s, nested),
            "post" => self.post(s),
            "environment" => self.environment(s),
            "options" => self.directive_list(s, "options", "Options", "Expected an option", Some("option")),
            "triggers" => self.directive_list(s, "triggers", "Triggers", "Expected a trigger", Some("trigger")),
            "parameters" => self.directive_list(s, "parameters", "Build parameters", "Expected a build parameter definition", None),
            "tools" => self.tools(s),
            "libraries" => self.libraries(s),
            "when" => self.when(s),
            "input" => self.input(s),
            _ => {}
        }
    }

    fn agent(&mut self, s: &Section) {
        if let Some(body) = s.body {
            if !s.args.is_empty() {
                self.err(s.pos, "Only \"agent none\", \"agent any\" or \"agent {...}\" are allowed.");
                return;
            }
            let types: Vec<Section> = body.iter().filter_map(|st| st.section()).collect();
            if types.is_empty() {
                self.err(s.pos, format!("No agent type specified. Must be one of {}", list(AGENT_TYPES)));
                return;
            }
            if types.len() > 1 || types.len() != body.len() {
                self.err(s.pos, "Only one agent type is allowed per agent section");
                return;
            }
            let t = &types[0];
            if !AGENT_TYPES.contains(&t.name) {
                // Plugins can add agent types, so this cannot be a hard error offline.
                self.diags.push(Diagnostic::warning(
                    t.pos,
                    format!(
                        "Agent type \"{}\" is not built in (built-in types: {}). Ignore this if a plugin provides it.",
                        t.name,
                        list(AGENT_TYPES)
                    ),
                ));
                return;
            }
            self.agent_params(t);
            return;
        }
        match s.args.as_slice() {
            [Arg::Pos(Expr::Ident { name, .. })] if name == "any" || name == "none" => {}
            _ => self.err(s.pos, "Only \"agent none\", \"agent any\" or \"agent {...}\" are allowed."),
        }
    }

    fn agent_params(&mut self, t: &Section) {
        let (required, allowed): (&[&str], &[&str]) = match t.name {
            "node" => (&["label"], &["label", "customWorkspace"]),
            "docker" => (
                &["image"],
                &[
                    "image",
                    "args",
                    "alwaysPull",
                    "customWorkspace",
                    "label",
                    "registryCredentialsId",
                    "registryUrl",
                    "reuseNode",
                    "containerPerStageRoot",
                ],
            ),
            "dockerfile" => (
                &[],
                &[
                    "additionalBuildArgs",
                    "args",
                    "customWorkspace",
                    "dir",
                    "filename",
                    "label",
                    "registryCredentialsId",
                    "registryUrl",
                    "reuseNode",
                    "containerPerStageRoot",
                ],
            ),
            _ => return,
        };
        let Some(body) = t.body else {
            if t.args.is_empty() {
                self.err(t.pos, format!("No argument for agent key \"{}\"", t.name));
            }
            return;
        };
        let mut present = HashSet::new();
        for st in body {
            match st.section() {
                Some(p) if allowed.contains(&p.name) => {
                    present.insert(p.name);
                    if p.args.is_empty() {
                        self.err(p.pos, format!("No argument for agent key \"{}\"", p.name));
                    } else if p.args.len() > 1 {
                        self.err(p.pos, format!("Too many arguments for agent key \"{}\"", p.name));
                    }
                }
                Some(p) => self.err(
                    p.pos,
                    format!(
                        "Invalid config option \"{}\" for agent type \"{}\". Valid config options are {}",
                        p.name,
                        t.name,
                        list(allowed)
                    ),
                ),
                None => self.err(st.pos(), "Expected to find \u{2018}key \"value\"\u{2019}"),
            }
        }
        for r in required {
            if !present.contains(r) {
                self.err(t.pos, format!("Missing required parameter for agent type \"{}\": {r}", t.name));
            }
        }
    }

    fn stages(&mut self, s: &Section, in_nested: bool) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for stages");
            return;
        };
        self.stage_list(s.pos, body, in_nested);
    }

    /// The stages of one `stages`, `parallel` or matrix `stages` block.
    /// Like Jenkins, stage names only have to be unique among siblings.
    fn stage_list(&mut self, pos: Pos, body: &[Stmt], in_nested: bool) {
        let mut count = 0;
        let mut names = HashSet::new();
        for stmt in body {
            match stmt.section() {
                Some(st) if st.name == "stage" => {
                    count += 1;
                    if let [Arg::Pos(Expr::Str { value, .. })] = st.args.as_slice() {
                        if !names.insert(value.as_str()) {
                            self.err(st.pos, format!("Duplicate stage name: \"{value}\""));
                        }
                    }
                    self.stage(&st, in_nested);
                }
                Some(st) if st.name == "failFast" => {}
                _ => self.err(stmt.pos(), "Expected a stage"),
            }
        }
        if count == 0 && body.iter().all(|s| s.section().is_some_and(|x| x.name != "stage")) {
            self.err(pos, "No stages specified");
        }
    }

    fn stage(&mut self, s: &Section, in_nested: bool) {
        let name = match s.args.as_slice() {
            [Arg::Pos(Expr::Str { value, .. })] => value.clone(),
            [] => {
                self.err(s.pos, "Expected a stage name but did not find any");
                return;
            }
            _ => {
                self.err(s.pos, "Expected string literal");
                return;
            }
        };
        let Some(body) = s.body else {
            self.err(s.pos, "Stage does not have a block");
            return;
        };
        self.stage = name.clone();
        let secs = self.sections(
            body,
            STAGE_SECTIONS,
            &|n| {
                format!(
                    "Unknown stage section \"{n}\". Starting with version 0.5, steps in a stage must be in a \u{2018}steps\u{2019} block."
                )
            },
            true,
        );
        let kinds: Vec<&str> = secs
            .iter()
            .map(|x| x.name)
            .filter(|n| matches!(*n, "steps" | "stages" | "parallel" | "matrix"))
            .collect();
        if kinds.is_empty() {
            self.err(
                s.pos,
                format!("Expected one of \"steps\", \"stages\", or \"parallel\" for stage \"{name}\""),
            );
        } else if kinds.len() > 1 {
            self.err(
                s.pos,
                format!("Only one of \"matrix\", \"parallel\", \"stages\", or \"steps\" allowed for stage \"{name}\""),
            );
        }
        let fans_out = kinds.iter().any(|k| *k == "parallel" || *k == "matrix");
        if fans_out {
            if in_nested {
                self.err(
                    s.pos,
                    "\"matrix\" or \"parallel\" cannot be nested inside another \"matrix\" or \"parallel\".",
                );
            }
            if secs.iter().any(|x| x.name == "agent") {
                self.err(
                    s.pos,
                    format!("\"agent\" is not allowed in stage \"{name}\" as it contains parallel or matrix stages"),
                );
            }
            if secs.iter().any(|x| x.name == "tools") {
                self.err(
                    s.pos,
                    format!("\"tools\" is not allowed in stage \"{name}\" as it contains parallel or matrix stages"),
                );
            }
        }
        for x in &secs {
            match x.name {
                "steps" => match x.body {
                    Some(b) => self.steps(x.pos, b),
                    None => self.err(x.pos, "Expected a block for steps"),
                },
                "parallel" => match x.body {
                    Some(b) => self.stage_list(x.pos, b, true),
                    None => self.err(x.pos, "Expected a block for parallel"),
                },
                "matrix" => self.matrix(x),
                "stages" => self.stages(x, in_nested),
                "failFast" => {}
                _ => self.section(x, in_nested),
            }
        }
    }

    fn steps(&mut self, pos: Pos, body: &[Stmt]) {
        if body.is_empty() {
            self.err(pos, "No steps specified for branch");
            return;
        }
        let only_step = body.len() == 1;
        for stmt in body {
            self.step(stmt, only_step);
        }
    }

    fn step(&mut self, stmt: &Stmt, only_step: bool) {
        let Stmt::Expr(e) = stmt else {
            self.err(stmt.pos(), "Expected a step");
            return;
        };
        let (callee, args) = match e {
            Expr::Call { callee, args, .. } => (callee.as_ref(), args.as_slice()),
            _ => {
                self.err(e.pos(), "Expected a step");
                return;
            }
        };
        let name = match callee {
            Expr::Ident { name, .. } => name.as_str(),
            Expr::Prop { .. } => {
                self.err(e.pos(), "Method calls on objects not allowed outside \"script\" blocks.");
                return;
            }
            _ => {
                self.err(e.pos(), "Expected a step");
                return;
            }
        };
        match name {
            "script" => {
                if !matches!(args.last(), Some(Arg::Pos(Expr::Closure { .. }))) {
                    self.err(e.pos(), "script step without a block");
                }
                return;
            }
            "$" => {
                self.err(
                    e.pos(),
                    "${...} cannot be used as a value directly. Did you mean \"${...}\"? (add quotes)",
                );
                return;
            }
            "pipeline" => {
                self.err(e.pos(), "pipeline block must be at the top-level, not within another block.");
                return;
            }
            "stage" => {
                self.err(
                    e.pos(),
                    "Invalid step \"stage\" used - not allowed in this context - The stage step cannot be used in Declarative Pipelines",
                );
                return;
            }
            "properties" => {
                self.err(e.pos(), "Invalid step \"properties\" used - not allowed in this context - The properties step cannot be used in Declarative Pipelines");
                return;
            }
            "parallel" if only_step => {
                self.parallel_step(e.pos(), args);
                return;
            }
            "parallel" => {
                self.err(
                    e.pos(),
                    "Invalid step \"parallel\" used - not allowed in this context - The parallel step can only be used as the only top-level step in a stage's step block",
                );
                return;
            }
            _ => {}
        }
        for a in args {
            self.method_args(arg_expr(a));
        }
        // Block-scoped steps such as dir('x') { ... } hold more steps.
        for a in args {
            if let Expr::Closure { body, pos, .. } = arg_expr(a) {
                if name == "parallel" {
                    continue;
                }
                self.steps(*pos, body);
            }
        }
    }

    /// `parallel a: { ... }, b: { ... }, failFast: true` as the only step.
    fn parallel_step(&mut self, pos: Pos, args: &[Arg]) {
        if args.is_empty() {
            let stage = self.stage.clone();
            self.err(
                pos,
                format!("Expected one of \"steps\", \"stages\", or \"parallel\" for stage \"{stage}\""),
            );
            return;
        }
        for a in args {
            match a {
                Arg::Named { key, value, pos } if key == "failFast" => {
                    if !matches!(value, Expr::Ident { name, .. } if name == "true" || name == "false") {
                        self.err(*pos, "Expected a boolean with failFast");
                    }
                }
                Arg::Named {
                    value: Expr::Closure { body, pos, .. },
                    ..
                } => self.steps(*pos, body),
                Arg::Named { pos, .. } => self.err(*pos, "Expected closure or failFast"),
                Arg::Pos(Expr::Map { entries, pos }) => {
                    for (_, v) in entries {
                        if let Expr::Closure { body, pos, .. } = v {
                            self.steps(*pos, body);
                        }
                    }
                    let _ = pos;
                }
                Arg::Pos(e) => self.err(e.pos(), "Expected closure or failFast"),
            }
        }
    }

    /// Checks on values passed to steps and directives outside `script`.
    fn method_args(&mut self, e: &Expr) {
        match e {
            Expr::Num { text, pos } => {
                let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                let is_int = !text.contains('.') && !text.contains(['e', 'E']) && !text.starts_with("0x");
                if is_int && !digits.is_empty() && digits.parse::<i64>().is_err() {
                    self.err(*pos, "BigIntegers cannot be used as constants in Declarative. The maximum value for an integer is 9,223,372,036,854,775,807 and the minimum value for an integer is -9,223,372,036,854,775,808.");
                }
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = callee.as_ref() {
                    let reason = match name.as_str() {
                        "node" => Some("The node step cannot be called as an argument to a method in Declarative Pipelines"),
                        "stage" => Some("The stage step cannot be used in Declarative Pipelines"),
                        "properties" => Some("The properties step cannot be used in Declarative Pipelines"),
                        "parallel" => Some("The parallel step can only be used as the only top-level step in a stage's step block"),
                        _ => None,
                    };
                    if let Some(reason) = reason {
                        self.err(
                            e.pos(),
                            format!("Invalid step \"{name}\" used - not allowed in this context - {reason}"),
                        );
                        return;
                    }
                }
                for a in args {
                    self.method_args(arg_expr(a));
                }
            }
            Expr::List { items, .. } => items.iter().for_each(|i| self.method_args(i)),
            Expr::Map { entries, .. } => entries.iter().for_each(|(_, v)| self.method_args(v)),
            Expr::Unary { expr, .. } => self.method_args(expr),
            _ => {}
        }
    }

    fn post(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for post");
            return;
        };
        if body.is_empty() {
            self.err(s.pos, "post can not be empty");
            return;
        }
        let mut seen = HashSet::new();
        for stmt in body {
            let cond = match stmt.section() {
                Some(c) if c.body.is_some() && c.args.is_empty() => c,
                _ => {
                    self.err(
                        stmt.pos(),
                        format!(
                            "The \u{2018}post\u{2019} section can only contain build condition names with code blocks. Valid condition names are {}",
                            list(POST_CONDITIONS)
                        ),
                    );
                    continue;
                }
            };
            if !POST_CONDITIONS.contains(&cond.name) {
                self.err(
                    cond.pos,
                    format!(
                        "Invalid condition \"{}\" - valid conditions are {}",
                        cond.name,
                        list(POST_CONDITIONS)
                    ),
                );
                continue;
            }
            if !seen.insert(cond.name) {
                self.err(cond.pos, format!("Duplicate build condition name: \"{}\"", cond.name));
                continue;
            }
            self.steps(cond.pos, cond.body.unwrap());
        }
    }

    fn environment(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for environment");
            return;
        };
        if body.is_empty() {
            self.err(s.pos, "No variables specified for environment");
            return;
        }
        let mut seen = HashSet::new();
        for stmt in body {
            let Stmt::Expr(Expr::Assign {
                op: "=",
                target,
                value,
                pos,
            }) = stmt
            else {
                let what = match stmt {
                    Stmt::Expr(e) => e.render(),
                    _ => "statement".into(),
                };
                self.err(
                    stmt.pos(),
                    format!("\"{what}\" is not a valid environment expression. Use \"key = value\" pairs with valid Java/shell keys."),
                );
                continue;
            };
            let Expr::Ident { name, .. } = target.as_ref() else {
                self.err(
                    *pos,
                    format!(
                        "\"{}\" is not a valid environment expression. Use \"key = value\" pairs with valid Java/shell keys.",
                        target.render()
                    ),
                );
                continue;
            };
            if !is_valid_identifier(name) {
                self.err(*pos, format!("\"{name}\" is not a valid identifier and cannot be used for an environment variable. Identifiers must start with a letter or underscore and can contain only letters, numbers or underscores."));
            }
            if !seen.insert(name.clone()) {
                self.err(*pos, format!("Duplicate environment variable name: \"{name}\""));
            }
            self.env_value(value);
        }
    }

    fn env_value(&mut self, value: &Expr) {
        match value {
            Expr::Str { .. } | Expr::Num { .. } => {}
            Expr::Ident { name, .. } if name == "true" || name == "false" || name == "null" => {}
            Expr::Call { callee, .. } if matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "$") => {
                self.err(
                    value.pos(),
                    "${...} cannot be used as a value directly. Did you mean \"${...}\"? (add quotes)",
                );
            }
            Expr::Call { .. } => {}
            Expr::Binary { op, lhs, rhs, pos } => {
                if op != "+" {
                    self.err(
                        *pos,
                        "Environment variable values can only be joined together with \u{2018}+\u{2019}s.",
                    );
                    return;
                }
                for side in [lhs.as_ref(), rhs.as_ref()] {
                    match side {
                        Expr::Str { .. } => {}
                        Expr::Binary { .. } => self.env_value(side),
                        _ => self.err(
                            side.pos(),
                            "Environment variable values to be concatenated together must be single or double quoted.",
                        ),
                    }
                }
            }
            _ => self.err(
                value.pos(),
                "Environment variable values must either be single quoted, double quoted, or function calls.",
            ),
        }
    }

    /// options, triggers and parameters: a list of calls without blocks.
    fn directive_list(&mut self, s: &Section, section: &str, label: &str, expected: &str, dup_kind: Option<&str>) {
        let Some(body) = s.body else {
            self.err(s.pos, format!("Expected a block for {section}"));
            return;
        };
        if body.is_empty() {
            self.err(s.pos, format!("{section} can not be empty"));
            return;
        }
        let mut seen = HashSet::new();
        for stmt in body {
            let Some(d) = stmt.section() else {
                if let Stmt::Expr(Expr::Map { pos, .. }) = stmt {
                    self.err(*pos, format!("{label} cannot be defined as maps"));
                } else {
                    self.err(stmt.pos(), expected);
                }
                continue;
            };
            if d.body.is_some() {
                self.err(d.pos, format!("{label} definitions cannot have blocks"));
                continue;
            }
            if section == "options" && (d.name == "parameters" || d.name == "triggers") {
                self.err(
                    d.pos,
                    format!(
                        "The \"{0}\" job property can't be used in the \"options\" directive. Use the \"{0} {{...}}\" directive instead.",
                        d.name
                    ),
                );
                continue;
            }
            for a in &d.args {
                let mut has_closure = false;
                for_each_closure(arg_expr(a), &mut |_| has_closure = true);
                if has_closure {
                    self.err(d.pos, "Method call arguments cannot contain closures");
                }
                self.method_args(arg_expr(a));
            }
            if let Some(kind) = dup_kind {
                if !seen.insert(d.name) {
                    self.err(d.pos, format!("Duplicate {kind} name: \"{}\"", d.name));
                }
            }
        }
    }

    fn tools(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for tools");
            return;
        };
        if body.is_empty() {
            self.err(s.pos, "No tools specified");
            return;
        }
        for stmt in body {
            match stmt.section() {
                Some(t) if t.body.is_some() => self.err(t.pos, "Tools definitions cannot have blocks"),
                Some(t) if t.args.is_empty() => self.err(t.pos, "Expected to find \u{2018}someTool \"someVersion\"\u{2019}"),
                Some(t) if t.args.len() > 1 => self.err(t.pos, format!("Too many arguments for tool \"{}\"", t.name)),
                Some(_) => {}
                None => self.err(stmt.pos(), "Expected to find \u{2018}someTool \"someVersion\"\u{2019}"),
            }
        }
    }

    fn libraries(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for libraries");
            return;
        };
        if body.is_empty() {
            self.err(s.pos, "libraries can not be empty");
            return;
        }
        for stmt in body {
            match stmt.section() {
                Some(l) if l.name == "lib" && !l.args.is_empty() => {}
                _ => {
                    let got = match stmt {
                        Stmt::Expr(e) => e.render(),
                        _ => "statement".into(),
                    };
                    self.err(stmt.pos(), format!("Expected a \"lib(...)\" but got {got}"));
                }
            }
        }
    }

    fn input(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for input");
            return;
        };
        let mut has_message = false;
        let mut seen = HashSet::new();
        for stmt in body {
            match stmt.section() {
                Some(f) if INPUT_FIELDS.contains(&f.name) => {
                    has_message |= f.name == "message";
                    if !seen.insert(f.name) {
                        self.err(f.pos, format!("Multiple occurrences of the {} section", f.name));
                    }
                }
                Some(f) => self.err(f.pos, format!("Not a valid field for input: \"{}\"", f.name)),
                None => self.err(stmt.pos(), "Expected a block"),
            }
        }
        if !has_message {
            self.err(s.pos, "No message specified for input");
        }
    }

    fn when(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for when");
            return;
        };
        let conds: Vec<&Stmt> = body
            .iter()
            .filter(|st| !st.section().is_some_and(|x| WHEN_FLAGS.contains(&x.name)))
            .collect();
        if conds.is_empty() {
            self.err(s.pos, "Empty when closure, remove the property or add some content.");
            return;
        }
        for c in conds {
            self.when_condition(c);
        }
    }

    fn when_condition(&mut self, stmt: &Stmt) {
        let Some(c) = stmt.section() else {
            self.err(stmt.pos(), "Expected a when condition");
            return;
        };
        if !WHEN_CONDITIONS.contains(&c.name) {
            self.err(
                c.pos,
                format!(
                    "Unknown conditional {}. Valid conditionals are: {}",
                    c.name,
                    WHEN_CONDITIONS.join(", ")
                ),
            );
            return;
        }
        match c.name {
            "allOf" | "anyOf" | "not" => {
                if !c.args.is_empty() {
                    self.err(
                        c.pos,
                        format!("No additional arguments are allowed for children when condition \"{}\".", c.name),
                    );
                }
                let children = c.body.unwrap_or(&[]);
                if children.is_empty() {
                    self.err(
                        c.pos,
                        format!("Nested when condition \"{}\" requires at least one child condition.", c.name),
                    );
                    return;
                }
                if c.name == "not" && children.len() != 1 {
                    self.err(c.pos, "Nested when condition \"not\" requires exactly 1 child condition(s).");
                }
                for child in children {
                    self.when_condition(child);
                }
            }
            "expression" => {
                if c.body.is_none() {
                    self.err(c.pos, "Expected a block for expression");
                }
            }
            _ => {
                if c.body.is_some() {
                    self.err(
                        c.pos,
                        format!("No children when conditions are allowed for when condition \"{}\".", c.name),
                    );
                }
            }
        }
    }

    fn matrix(&mut self, s: &Section) {
        let Some(body) = s.body else {
            self.err(s.pos, "Expected a block for matrix");
            return;
        };
        let secs = self.sections(body, MATRIX_SECTIONS, &|n| format!("Undefined section \"{n}\""), false);
        for required in ["axes", "stages"] {
            if !secs.iter().any(|x| x.name == required) {
                self.err(s.pos, format!("Missing required section \"{required}\""));
            }
        }
        for x in &secs {
            match x.name {
                "axes" => {
                    let axes = x.body.unwrap_or(&[]);
                    if axes.is_empty() {
                        self.err(x.pos, "No axis directives specified");
                    }
                    self.axes(axes, false);
                }
                "excludes" => {
                    let excludes = x.body.unwrap_or(&[]);
                    if excludes.is_empty() {
                        self.err(x.pos, "No exclude directives specified");
                    }
                    for e in excludes {
                        match e.section() {
                            Some(ex) if ex.name == "exclude" => {
                                let axes = ex.body.unwrap_or(&[]);
                                if axes.is_empty() {
                                    self.err(ex.pos, "No axis directives specified");
                                }
                                self.axes(axes, true);
                            }
                            _ => self.err(e.pos(), "Expected an axis"),
                        }
                    }
                }
                "stages" => match x.body {
                    Some(b) => self.stage_list(x.pos, b, true),
                    None => self.err(x.pos, "Expected a block for stages"),
                },
                _ => self.section(x, true),
            }
        }
    }

    fn axes(&mut self, axes: &[Stmt], exclude: bool) {
        let mut names = HashSet::new();
        for stmt in axes {
            let Some(axis) = stmt.section().filter(|a| a.name == "axis") else {
                self.err(stmt.pos(), "Expected an axis");
                continue;
            };
            let fields = axis.body.unwrap_or(&[]);
            let mut has_name = false;
            let mut value_kinds = 0;
            for f in fields {
                let Some(field) = f.section() else {
                    self.err(f.pos(), "Expected an axis");
                    continue;
                };
                match field.name {
                    "name" => {
                        has_name = true;
                        match field.args.as_slice() {
                            [a] => match axis_name(a) {
                                Some(n) => {
                                    if !is_valid_identifier(n) {
                                        self.err(field.pos, format!("\"{n}\" is not a valid identifier and cannot be used for an environment variable. Identifiers must start with a letter or underscore and can contain only letters, numbers or underscores."));
                                    }
                                    if !names.insert(n.to_string()) {
                                        self.err(field.pos, format!("Duplicate axis name: \"{n}\""));
                                    }
                                }
                                None => self.err(field.pos, "Expected string literal"),
                            },
                            _ => self.err(field.pos, "Expected string literal"),
                        }
                    }
                    "values" | "notValues" => {
                        value_kinds += 1;
                        if field.name == "notValues" && !exclude {
                            self.err(field.pos, "Expected an axis");
                        }
                        let mut vals = HashSet::new();
                        for a in &field.args {
                            match plain_string(a) {
                                Some(v) => {
                                    if !vals.insert(v.to_string()) {
                                        self.err(field.pos, format!("Duplicate axis value: \"{v}\""));
                                    }
                                }
                                None => self.err(field.pos, format!("Expected string literal but got {}", arg_expr(a).render())),
                            }
                        }
                    }
                    _ => self.err(field.pos, "Expected an axis"),
                }
            }
            if !has_name {
                self.err(axis.pos, "Missing required section \"name\"");
            }
            if value_kinds == 0 {
                self.err(axis.pos, "Missing required section \"values\"");
            } else if value_kinds > 1 && exclude {
                self.err(axis.pos, "Exclude axis must contain either \"values\" or \"notValues\"");
            }
        }
    }
}
