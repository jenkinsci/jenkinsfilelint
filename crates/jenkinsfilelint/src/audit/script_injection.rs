//! `script-injection`: input from outside the pipeline that Groovy puts
//! into a shell script.
//!
//! A pull request's title and branch, a build parameter or a webhook's
//! payload is chosen by whoever opens the pull request, starts the build
//! or sends the request. Interpolated by Groovy into a step's script,
//! `sh "echo ${env.CHANGE_TITLE}"`, any shell syntax in it runs. Read from
//! the environment, `sh 'echo "$CHANGE_TITLE"'`, it stays text.

use std::collections::HashMap;

use super::fix::Fix;
use super::taint::{self, Flow, How, Label, Sink, Use};
use super::{Confidence, Finding, Persona, Rule, Severity};
use crate::ast::{visit, Arg, Expr, Node, Stmt};

pub static RULE: Rule = Rule {
    id: "script-injection",
    summary: "Input from outside the pipeline is interpolated by Groovy into a shell step",
    severity: Severity::High,
};

/// How much of a value its sender controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tier {
    /// Any text: a title, a branch name, a display name.
    Arbitrary,
    /// An email address, which can hold shell syntax.
    Email,
    /// A user name, a repository name or URL: little room for shell syntax.
    Structured,
    /// Chosen by people who can push to the repository anyway.
    Writers,
    /// A build parameter the file declares as text.
    Parameter,
    /// A build parameter the file does not declare: text, unless it is a
    /// choice or boolean parameter declared elsewhere.
    UndeclaredParameter,
    /// A choice or boolean parameter, whose value Jenkins checks.
    CheckedParameter,
    /// A webhook variable: what it holds depends on what sends the webhook.
    Webhook,
    /// A webhook variable for a field with a fixed format, such as a commit
    /// SHA, from a webhook whose token is not in the file.
    FixedFormat,
    /// A webhook variable whose `regexpFilter` removes shell syntax.
    Filtered,
    /// Base64-encoded.
    Encoded,
}

/// A value from outside the pipeline.
#[derive(Debug, PartialEq)]
pub struct Input {
    /// What it holds: "the pull request title".
    pub what: &'static str,
    /// Who chooses it: "anyone who opens a pull request".
    pub who: &'static str,
    pub tier: Tier,
}

const PR: &str = "anyone who opens a pull request";
const PR_COMMENT: &str = "anyone who can comment on a pull request";
const MR: &str = "anyone who opens a merge request";
const MR_COMMENT: &str = "anyone who can comment on a merge request";
const CHANGE: &str = "anyone who can upload a change";
const CHANGE_COMMENT: &str = "anyone who can comment on a change";
const PUSH: &str = "anyone who can push to the repository";
const BUILD: &str = "anyone who can start the build";
const WEBHOOK: &str = "whoever sends the webhook";
const OPEN_WEBHOOK: &str = "anyone who can read the webhook token in this file";

macro_rules! inputs {
    ($($name:literal => $what:literal, $who:ident, $tier:ident;)*) => {
        &[$(($name, Input { what: $what, who: $who, tier: Tier::$tier }),)*]
    };
}

/// Environment variables that plugins set from a pull request or a
/// change, by name.
static KNOWN: &[(&str, Input)] = inputs! {
    // Branch API: multibranch projects, from GitHub, GitLab, Bitbucket,
    // Gitea and other branch sources.
    "CHANGE_TITLE" => "the pull request title", PR, Arbitrary;
    "CHANGE_BRANCH" => "the name of the pull request's branch", PR, Arbitrary;
    "CHANGE_AUTHOR_DISPLAY_NAME" => "the pull request author's display name", PR, Arbitrary;
    "CHANGE_AUTHOR_EMAIL" => "the pull request author's email address", PR, Email;
    "CHANGE_AUTHOR" => "the pull request author's user name", PR, Structured;
    "CHANGE_FORK" => "the name of the pull request's fork", PR, Structured;
    "CHANGE_TARGET" => "the pull request's target branch", PUSH, Writers;
    "BRANCH_NAME" => "the branch name", PUSH, Writers;
    "TAG_NAME" => "the tag name", PUSH, Writers;
    // Git plugin.
    "GIT_BRANCH" => "the branch name", PUSH, Writers;
    "GIT_LOCAL_BRANCH" => "the branch name", PUSH, Writers;
    // GitHub Pull Request Builder.
    "ghprbPullTitle" => "the pull request title", PR, Arbitrary;
    "ghprbPullDescription" => "the pull request description", PR, Arbitrary;
    "ghprbPullLongDescription" => "the pull request description", PR, Arbitrary;
    "ghprbSourceBranch" => "the name of the pull request's branch", PR, Arbitrary;
    "ghprbActualCommitAuthor" => "the commit author's name", PR, Arbitrary;
    "ghprbActualCommitAuthorEmail" => "the commit author's email address, which Git does not check", PR, Arbitrary;
    "ghprbCommentBody" => "the comment that started the build", PR_COMMENT, Arbitrary;
    "ghprbTriggerAuthor" => "the name of who started the build", PR_COMMENT, Arbitrary;
    "ghprbTriggerAuthorEmail" => "the email address of who started the build", PR_COMMENT, Email;
    "ghprbPullAuthorEmail" => "the pull request author's email address", PR, Email;
    "ghprbPullAuthorLogin" => "the pull request author's user name", PR, Structured;
    "ghprbPullAuthorLoginMention" => "the pull request author's user name", PR, Structured;
    "ghprbTriggerAuthorLogin" => "the user name of who started the build", PR_COMMENT, Structured;
    "ghprbTriggerAuthorLoginMention" => "the user name of who started the build", PR_COMMENT, Structured;
    "ghprbAuthorRepoGitUrl" => "the URL of the pull request's fork", PR, Structured;
    "ghprbTargetBranch" => "the pull request's target branch", PUSH, Writers;
    // GitLab.
    "gitlabMergeRequestTitle" => "the merge request title", MR, Arbitrary;
    "gitlabMergeRequestDescription" => "the merge request description", MR, Arbitrary;
    "gitlabSourceBranch" => "the name of the merge request's branch", MR, Arbitrary;
    "gitlabBranch" => "the name of the branch that started the build", MR, Arbitrary;
    "gitlabUserName" => "the name of who started the build", MR, Arbitrary;
    "gitlabTriggerPhrase" => "the comment that started the build", MR_COMMENT, Arbitrary;
    "gitlabUserEmail" => "the email address of who started the build", MR, Email;
    "gitlabUserUsername" => "the user name of who started the build", MR, Structured;
    "gitlabSourceRepoName" => "the name of the merge request's source project", MR, Structured;
    "gitlabSourceNamespace" => "the namespace of the merge request's source project", MR, Structured;
    "gitlabSourceRepoHomepage" => "the URL of the merge request's source project", MR, Structured;
    "gitlabSourceRepoURL" => "the URL of the merge request's source project", MR, Structured;
    "gitlabSourceRepoSshUrl" => "the URL of the merge request's source project", MR, Structured;
    "gitlabSourceRepoHttpUrl" => "the URL of the merge request's source project", MR, Structured;
    "gitlabMergeRequestLabels" => "the merge request's labels", PUSH, Writers;
    "gitlabMergedByUser" => "the name of who merged the merge request", PUSH, Writers;
    "gitlabMergeRequestAssignee" => "the name of the merge request's assignee", PUSH, Writers;
    "gitlabTargetBranch" => "the merge request's target branch", PUSH, Writers;
    // Gerrit Trigger.
    "GERRIT_CHANGE_SUBJECT" => "the change's subject", CHANGE, Arbitrary;
    "GERRIT_TOPIC" => "the change's topic", CHANGE, Arbitrary;
    "GERRIT_OLD_TOPIC" => "the change's previous topic", CHANGE, Arbitrary;
    "GERRIT_HASHTAGS" => "the change's hashtags", CHANGE, Arbitrary;
    "GERRIT_ADDED_HASHTAGS" => "the change's hashtags", CHANGE, Arbitrary;
    "GERRIT_REMOVED_HASHTAGS" => "the change's hashtags", CHANGE, Arbitrary;
    "GERRIT_CHANGE_OWNER" => "the change owner's name and email address", CHANGE, Arbitrary;
    "GERRIT_CHANGE_OWNER_NAME" => "the change owner's name", CHANGE, Arbitrary;
    "GERRIT_PATCHSET_UPLOADER" => "the uploader's name and email address", CHANGE, Arbitrary;
    "GERRIT_PATCHSET_UPLOADER_NAME" => "the uploader's name", CHANGE, Arbitrary;
    "GERRIT_EVENT_ACCOUNT" => "the name and email address of who caused the event", CHANGE_COMMENT, Arbitrary;
    "GERRIT_EVENT_ACCOUNT_NAME" => "the name of who caused the event", CHANGE_COMMENT, Arbitrary;
    "GERRIT_CHANGE_ABANDONER" => "the name and email address of who abandoned the change", CHANGE, Arbitrary;
    "GERRIT_CHANGE_ABANDONER_NAME" => "the name of who abandoned the change", CHANGE, Arbitrary;
    "GERRIT_CHANGE_RESTORER" => "the name and email address of who restored the change", CHANGE, Arbitrary;
    "GERRIT_CHANGE_RESTORER_NAME" => "the name of who restored the change", CHANGE, Arbitrary;
    "GERRIT_TOPIC_CHANGER" => "the name and email address of who changed the topic", CHANGE, Arbitrary;
    "GERRIT_TOPIC_CHANGER_NAME" => "the name of who changed the topic", CHANGE, Arbitrary;
    "GERRIT_CHANGE_OWNER_EMAIL" => "the change owner's email address", CHANGE, Email;
    "GERRIT_PATCHSET_UPLOADER_EMAIL" => "the uploader's email address", CHANGE, Email;
    "GERRIT_EVENT_ACCOUNT_EMAIL" => "the email address of who caused the event", CHANGE_COMMENT, Email;
    "GERRIT_CHANGE_ABANDONER_EMAIL" => "the email address of who abandoned the change", CHANGE, Email;
    "GERRIT_CHANGE_RESTORER_EMAIL" => "the email address of who restored the change", CHANGE, Email;
    "GERRIT_TOPIC_CHANGER_EMAIL" => "the email address of who changed the topic", CHANGE, Email;
    "GERRIT_CHANGE_OWNER_USERNAME" => "the change owner's user name", CHANGE, Structured;
    "GERRIT_PATCHSET_UPLOADER_USERNAME" => "the uploader's user name", CHANGE, Structured;
    "GERRIT_EVENT_ACCOUNT_USERNAME" => "the user name of who caused the event", CHANGE_COMMENT, Structured;
    "GERRIT_CHANGE_COMMIT_MESSAGE" => "the commit message", CHANGE, Encoded;
    "GERRIT_EVENT_COMMENT_TEXT" => "the comment", CHANGE_COMMENT, Encoded;
    "GERRIT_BRANCH" => "the change's target branch", PUSH, Writers;
    "GERRIT_REFNAME" => "the name of the updated ref", PUSH, Writers;
};

static PARAMETER: Input = Input {
    what: "a build parameter",
    who: BUILD,
    tier: Tier::Parameter,
};
static UNDECLARED_PARAMETER: Input = Input {
    what: "a build parameter",
    who: BUILD,
    tier: Tier::UndeclaredParameter,
};
static CHECKED_PARAMETER: Input = Input {
    what: "a choice or boolean parameter",
    who: BUILD,
    tier: Tier::CheckedParameter,
};
static WEBHOOK_VARIABLE: Input = Input {
    what: "a Generic Webhook Trigger variable",
    who: WEBHOOK,
    tier: Tier::Webhook,
};
static OPEN_WEBHOOK_VARIABLE: Input = Input {
    what: "a Generic Webhook Trigger variable",
    who: OPEN_WEBHOOK,
    tier: Tier::Arbitrary,
};
static FIXED_WEBHOOK_VARIABLE: Input = Input {
    what: "a Generic Webhook Trigger variable",
    who: WEBHOOK,
    tier: Tier::FixedFormat,
};
static FILTERED_WEBHOOK_VARIABLE: Input = Input {
    what: "a Generic Webhook Trigger variable",
    who: WEBHOOK,
    tier: Tier::Filtered,
};

/// The inputs a file can read: the ones every build can have, its build
/// parameters and its webhook variables.
pub struct Sources {
    pub names: HashMap<String, &'static Input>,
    /// Inputs known by the start of their names.
    pub prefixes: Vec<(String, &'static Input)>,
}

pub fn sources(stmts: &[Stmt]) -> Sources {
    let mut names = HashMap::new();
    // The pull request builders set their variables as build parameters.
    for (name, input) in KNOWN {
        names.insert(name.to_string(), input);
        names.insert(format!("params.{name}"), input);
    }
    let mut prefixes = Vec::new();
    visit(stmts, &mut |node| {
        let Node::Expr(e) = node else { return };
        match declaration(e) {
            // `parameters { string(...) }`, `parameters([string(...)])`, or
            // `[$class: 'ParametersDefinitionProperty', parameterDefinitions: [...]]`.
            Some("parameters" | "ParametersDefinitionProperty") => {
                let lists = match e {
                    Expr::Call { args, .. } => args.iter().map(arg_value).collect(),
                    _ => field(e, "parameterDefinitions").into_iter().collect::<Vec<_>>(),
                };
                for decl in lists.into_iter().flat_map(items) {
                    if let Some((name, input)) = parameter(decl) {
                        names.insert(format!("params.{name}"), input);
                        names.insert(name, input);
                    }
                }
            }
            Some("GenericTrigger") => {
                // A token written in the file lets anyone who can read it
                // send any payload.
                let open = matches!(field(e, "token"), Some(Expr::Str { interp: false, .. }));
                for (key, lower) in [
                    ("genericVariables", false),
                    ("genericRequestVariables", false),
                    ("genericHeaderVariables", true),
                ] {
                    for var in field(e, key).map(items).unwrap_or_default() {
                        let Some((name, input)) = webhook_variable(var, lower, open) else {
                            continue;
                        };
                        // `pr` from a JSON object also sets `pr_title`, and
                        // a request parameter `ref` also sets `ref_0`.
                        prefixes.push((format!("{name}_"), input));
                        names.insert(name, input);
                    }
                }
            }
            _ => {}
        }
    });
    Sources { names, prefixes }
}

/// What `params.X` holds when the file does not declare X.
pub fn undeclared_parameter() -> &'static Input {
    &UNDECLARED_PARAMETER
}

/// The name of a call, `name(...)`, or the class of a map, `[$class: 'name', ...]`.
fn declaration(e: &Expr) -> Option<&str> {
    match e {
        Expr::Call { callee, .. } => callee.method_name(),
        Expr::Map { .. } => match field(e, "$class")? {
            Expr::Str { value, .. } => Some(value),
            _ => None,
        },
        _ => None,
    }
}

/// A named argument of a call, or a key of a map.
fn field<'a>(e: &'a Expr, key: &str) -> Option<&'a Expr> {
    match e {
        Expr::Call { args, .. } => args.iter().find_map(|a| match a {
            Arg::Named { key: k, value, .. } if k == key => Some(value),
            _ => None,
        }),
        Expr::Map { entries, .. } => entries.iter().find_map(|(k, v)| match k {
            Expr::Str { value, .. } if value == key => Some(v),
            _ => None,
        }),
        _ => None,
    }
}

/// The text of a field that is a plain string.
fn text(e: &Expr, key: &str) -> Option<String> {
    match field(e, key)? {
        Expr::Str { value, interp: false, .. } => Some(value.clone()),
        _ => None,
    }
}

/// The expressions a declaration lists: the items of `[a, b]`, the
/// statements of `{ a; b }`, or the expression itself.
fn items(e: &Expr) -> Vec<&Expr> {
    match e {
        Expr::List { items, .. } => items.iter().collect(),
        Expr::Closure { body, .. } => body
            .iter()
            .filter_map(|s| match s {
                Stmt::Expr(e) => Some(e),
                _ => None,
            })
            .collect(),
        e => vec![e],
    }
}

/// `--fix` rewrites the script a value is interpolated into, so that the
/// shell reads it from the environment. A value joined with `+`, or a whole
/// argument, is left to the user: the script is not one literal.
fn fix(f: &Flow, fixes: &[Option<Fix>]) -> Option<Fix> {
    if f.value.how != How::Interpolated {
        return None;
    }
    f.script.and_then(|i| fixes[i].clone())
}

/// A parameter declaration: `string(name: 'X', ...)`, `choice(...)` or
/// `[$class: 'StringParameterDefinition', name: 'X', ...]`.
fn parameter(e: &Expr) -> Option<(String, &'static Input)> {
    let kind = declaration(e)?;
    let name = text(e, "name")?;
    let input = match kind {
        "choice" | "booleanParam" | "ChoiceParameterDefinition" | "BooleanParameterDefinition" => &CHECKED_PARAMETER,
        "string" | "text" | "password" | "StringParameterDefinition" | "TextParameterDefinition" | "PasswordParameterDefinition" => {
            &PARAMETER
        }
        _ => &UNDECLARED_PARAMETER,
    };
    Some((name, input))
}

/// A webhook variable: `[key: 'ref', value: '$.ref', regexpFilter: '...']`.
/// Header names become lower case, and like any key, `-` and spaces in
/// them become `_`.
fn webhook_variable(e: &Expr, lower: bool, open: bool) -> Option<(String, &'static Input)> {
    let key = text(e, "key")?;
    let mut name: String = key.chars().map(|c| if c == '-' || c.is_whitespace() { '_' } else { c }).collect();
    if lower {
        name = name.to_lowercase();
    }
    let input = if text(e, "regexpFilter").is_some_and(|f| is_safe_filter(&f)) {
        &FILTERED_WEBHOOK_VARIABLE
    } else if open {
        &OPEN_WEBHOOK_VARIABLE
    } else if text(e, "value").is_some_and(|path| is_fixed_format(&path)) {
        &FIXED_WEBHOOK_VARIABLE
    } else {
        &WEBHOOK_VARIABLE
    };
    Some((name, input))
}

/// Whether a JSONPath selects a field that webhooks fill with a fixed
/// format: `$.after`, `$.pull_request.number`, `$.object_kind`.
fn is_fixed_format(path: &str) -> bool {
    let last = path.rsplit(['.', '[']).next().unwrap_or("");
    let last = last.trim_end_matches([']', '\'', '"']);
    const FIXED: &[&str] = &[
        "after",
        "before",
        "sha",
        "checkout_sha",
        "hash",
        "id",
        "iid",
        "node_id",
        "number",
        "object_kind",
        "action",
        "state",
        "merged",
        "draft",
        "private",
        "created_at",
        "updated_at",
        "timestamp",
        "total_commits_count",
    ];
    FIXED.contains(&last) || last.ends_with("_id")
}

/// Whether a regular expression whose matches are removed, as in
/// `replaceAll(/[^A-Za-z0-9._-]/, '')`, leaves only characters with no
/// meaning to a shell: one negated character class of letters, digits and
/// `_ - . / : @ , + =`.
pub fn is_safe_filter(pattern: &str) -> bool {
    let pattern = pattern.strip_suffix(['+', '*']).unwrap_or(pattern);
    let Some(class) = pattern.strip_prefix("[^").and_then(|p| p.strip_suffix(']')) else {
        return false;
    };
    let chars: Vec<char> = class.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            // `\w` and `\d`, or an escaped safe character.
            match chars.get(i + 1) {
                Some('w' | 'd') => {}
                Some(&e) if is_inert(e) && !e.is_ascii_alphanumeric() => {}
                _ => return false,
            }
            i += 2;
        } else if chars.get(i + 1) == Some(&'-') && chars.get(i + 2).is_some_and(|&e| e != ']') {
            let end = chars[i + 2];
            if end == '\\' || !(c..=end).all(is_inert) {
                return false;
            }
            i += 3;
        } else {
            if !is_inert(c) {
                return false;
            }
            i += 1;
        }
    }
    !chars.is_empty()
}

/// Whether a character means nothing to a shell, in any quotes: a letter,
/// a digit or one of `_ - . / : @ , + =`.
pub fn is_inert(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_-./:@,+=".contains(c)
}

fn arg_value(a: &Arg) -> &Expr {
    match a {
        Arg::Pos(e) | Arg::Named { value: e, .. } => e,
    }
}

pub fn check(flows: &[Flow], fixes: &[Option<Fix>]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for f in flows {
        let Label::Input(input) = f.value.label else { continue };
        if f.sink != Sink::Script || f.value.escaped {
            continue;
        }
        let (severity, confidence, persona) = match input.tier {
            Tier::Arbitrary => (Severity::High, Confidence::High, Persona::Regular),
            Tier::Parameter => (Severity::Medium, Confidence::High, Persona::Regular),
            Tier::UndeclaredParameter => (Severity::Medium, Confidence::Medium, Persona::Regular),
            Tier::Email => (Severity::Medium, Confidence::Medium, Persona::Pedantic),
            Tier::Structured => (Severity::Low, Confidence::High, Persona::Pedantic),
            Tier::Webhook => (Severity::High, Confidence::Medium, Persona::Regular),
            Tier::CheckedParameter | Tier::Filtered => (Severity::Low, Confidence::High, Persona::Auditor),
            Tier::Writers | Tier::FixedFormat | Tier::Encoded => (Severity::Low, Confidence::Low, Persona::Auditor),
        };
        findings.push(Finding {
            rule: RULE.id,
            severity,
            confidence,
            persona,
            message: message(&f.value, input, &f.step),
            span: f.value.span,
            stmt_line: f.stmt_line,
            fix: fix(f, fixes),
        });
    }
    findings
}

fn message(u: &Use, input: &Input, step: &str) -> String {
    let who = input.who;
    let risk = match input.tier {
        Tier::Arbitrary | Tier::Parameter => format!("{who} chooses it, and shell syntax in it runs on the agent"),
        Tier::UndeclaredParameter => {
            format!("unless it is a choice or boolean parameter, {who} chooses it, and shell syntax in it runs on the agent")
        }
        Tier::Email => format!("{who} chooses it, and an email address can hold shell syntax"),
        Tier::Structured => format!("{who} chooses it, though its format leaves little room for shell syntax"),
        Tier::Writers => format!("{who} chooses it"),
        Tier::CheckedParameter => "Jenkins accepts only its declared values, so it is safe while it keeps its type".into(),
        Tier::Webhook => format!("{who} chooses what the webhook's payload holds, and shell syntax in it runs on the agent"),
        Tier::FixedFormat => format!("{who} chooses it, though the field it comes from usually has a fixed format"),
        Tier::Filtered => "its regexpFilter leaves no shell syntax in it".into(),
        Tier::Encoded => format!("{who} chooses it, but it is Base64-encoded, which is safe until something decodes it"),
    };
    // Build parameters are environment variables too.
    let fix = match &u.via {
        Some(v) if !u.via_env => {
            let var = v.to_uppercase();
            format!(
                "Pass {v} in the environment instead: withEnv([\"{var}=${{{v}}}\"]) {{ {} }}",
                read(step, &var)
            )
        }
        Some(v) => format!("Read {v} from the environment instead: {}", read(step, v)),
        None => format!(
            "Read it from the environment instead: {}",
            read(step, u.name.strip_prefix("params.").unwrap_or(&u.name))
        ),
    };
    format!(
        "{} is {}, and {}: {risk}. {fix}.",
        u.name,
        input.what,
        taint::how_it_gets_there(u, step)
    )
}

/// How a step reads environment variable `var` safely.
fn read(step: &str, var: &str) -> String {
    match step {
        "bat" => format!("in bat, %{var}% is expanded before cmd reads the line, so turn on delayed expansion and use !{var}!"),
        "powershell" | "pwsh" => format!("{step} '... $env:{var} ...'"),
        "docker.build" => format!("docker.build(image, '... \"${var}\" ...')"),
        "run" | "withRun" => format!("{step}('... \"${var}\" ...')"),
        _ => format!("{step} '... \"${var}\" ...'"),
    }
}

#[cfg(test)]
mod tests {
    use super::is_safe_filter;

    #[test]
    fn safe_filters() {
        for f in ["[^a-zA-Z0-9]", "[^A-Za-z0-9._-]", "[^\\w.\\-/]+", "[^0-9]", "[^\\w]*", "[^a-z+-9]"] {
            assert!(is_safe_filter(f), "{f}");
        }
        for f in [
            "refs/heads/",
            "[^a-z ]",
            "[^\\s\\w]",
            "[^!-~]",
            "[^a-z$]",
            "[a-z]",
            "[^]",
            "[^\\W]",
            "[^a-z]x",
        ] {
            assert!(!is_safe_filter(f), "{f}");
        }
    }
}
