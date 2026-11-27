//! The jenkins-lint command: which files to check, the options, and the
//! output.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use crate::audit::fix::{Disposition, Mode};
use crate::audit::{self, Confidence, Filter, Finding, Persona};
use crate::diag::{Report, Severity};
use crate::discover::{expand_paths, path_match, LIBRARY_DIRS};
use crate::remote::{Jenkins, Verdict};
use crate::{audit_json, audit_text, diagnostics_text, fix_source, fixed_text, json_escape, lint};

const NAME: &str = "jenkins-lint";
const DEFAULT_JOBS: usize = 8;

/// Printed with --verbose when files are checked offline only.
pub const OFFLINE_NOTE: &str = "ℹ Checking offline: step names, step parameters and other plugin-dependent \
     checks are skipped. Pass --remote to also check with your Jenkins.";

const HELP: &str = "Check Jenkinsfiles offline: Groovy syntax, Declarative Pipeline structure and
security. With --remote, the files that pass are also validated by your Jenkins.

Usage: jenkins-lint [check] [OPTIONS] <PATH>...

Arguments:
  <PATH>...  Jenkinsfiles and shared library files, or directories to search.
             In a directory, picks Jenkinsfile and *.jenkinsfile, Jenkinsfile*
             and *.groovy files that contain a pipeline, and every *.groovy
             under vars/ and src/.

Options:
  --remote [URL]         After checking offline, also validate the files with
                         this Jenkins, which knows its own plugins: step names
                         and parameters, agent and tool types. Without a URL,
                         uses JENKINS_URL. JENKINS_URL alone never turns this on.
  --username USER        Jenkins username (or JENKINS_USER), with --remote
  --token TOKEN          Jenkins API token (or JENKINS_TOKEN), with --remote
  -j, --jobs N           Files to send to Jenkins at the same time (default: 8)
  --persona P            How many security findings to show: regular (default),
                         pedantic (adds hardening advice) or auditor (all)
  --min-severity S       Leave out findings below low, medium or high
  --min-confidence C     Leave out findings below low, medium or high
  --ignore RULE[,RULE]   Turn off a rule in every file, such as unpinned-library
  --fix                  Rewrite the files with the fixes, then report what
                         is left. Use --no-fix to disable.
  --unsafe-fixes         Include the fixes that can change what a pipeline
                         does: review them. Use --no-unsafe-fixes to disable.
  --show-fixes           Name each fix that --fix made. Use --no-show-fixes
                         to disable.
  --diff                 Print what the fixes would change, and change nothing
  --format F             text (default), json or sarif. sarif exits with 0
                         when there are findings, so that it can be uploaded.
  --skip PATTERN         Skip files that match this glob, such as 'vars/*.groovy'
  --include PATTERN      Check only files that match one of these globs
  -v, --verbose          Say what is checked, skipped and found
  -h, --help             Print this help
  -V, --version          Print the version

`jenkins-lint check PATH` is the same as `jenkins-lint PATH`.
Documentation: https://jenkinsci.github.io/jenkins-lint/";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Format {
    Text,
    Json,
    Sarif,
}

#[derive(Debug)]
pub struct Options {
    pub paths: Vec<PathBuf>,
    /// The Jenkins to validate with, or None to check offline only.
    pub remote: Option<String>,
    pub username: Option<String>,
    pub token: Option<String>,
    pub jobs: usize,
    pub filter: Filter,
    /// --fix: rewrite the files with the fixes.
    pub fix: bool,
    /// --unsafe-fixes: the fixes include those that can change behaviour.
    pub unsafe_fixes: bool,
    /// --show-fixes: name each fix made.
    pub show_fixes: bool,
    /// --diff: print what the fixes would change instead.
    pub diff: bool,
    pub format: Format,
    pub skip: Vec<String>,
    pub include: Vec<String>,
    pub verbose: bool,
}

/// A usage error: the message, printed with the name of the command.
fn usage_error(message: impl AsRef<str>) -> ExitCode {
    eprintln!("Usage: {NAME} [check] [OPTIONS] <PATH>...");
    eprintln!("{NAME}: error: {}", message.as_ref());
    eprintln!("Try '{NAME} --help' for more information.");
    ExitCode::from(2)
}

/// The options, read from the command line and the environment: an `env`
/// lookup. Err is the exit code after --help, --version or a usage error.
pub fn parse_args(args: impl IntoIterator<Item = OsString>, env: impl Fn(&str) -> Option<String>) -> Result<Options, ExitCode> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    if args.first().and_then(|a| a.to_str()) == Some("check") {
        args.remove(0);
    }
    let mut o = Options {
        paths: Vec::new(),
        remote: None,
        username: None,
        token: None,
        jobs: DEFAULT_JOBS,
        filter: Filter::default(),
        fix: false,
        unsafe_fixes: false,
        show_fixes: false,
        diff: false,
        format: Format::Text,
        skip: Vec::new(),
        include: Vec::new(),
        verbose: false,
    };
    let mut remote_flag: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].to_string_lossy().into_owned();
        i += 1;
        if arg == "--" {
            o.paths.extend(args[i..].iter().map(PathBuf::from));
            break;
        }
        if !arg.starts_with('-') || arg == "-" {
            o.paths.push(PathBuf::from(&args[i - 1]));
            continue;
        }
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f.to_string(), Some(v.to_string())),
            _ if arg.starts_with("-j") && arg.len() > 2 && !arg.starts_with("--") => ("-j".to_string(), Some(arg[2..].to_string())),
            _ => (arg.clone(), None),
        };
        // The value of an option: after `=`, or the next argument.
        let mut value = |what: &str| -> Result<String, ExitCode> {
            if let Some(v) = inline.clone() {
                return Ok(v);
            }
            if i < args.len() {
                i += 1;
                return Ok(args[i - 1].to_string_lossy().into_owned());
            }
            Err(usage_error(format!("argument {what}: expected one argument")))
        };
        let choice = |flag: &str, v: &str, choices: &[&str]| -> Result<(), ExitCode> {
            if choices.contains(&v) {
                Ok(())
            } else {
                let list: Vec<String> = choices.iter().map(|c| format!("'{c}'")).collect();
                Err(usage_error(format!(
                    "argument {flag}: invalid choice: '{v}' (choose from {})",
                    list.join(", ")
                )))
            }
        };
        match flag.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return Err(ExitCode::SUCCESS);
            }
            "-V" | "--version" => {
                println!("{NAME} {}", env!("CARGO_PKG_VERSION"));
                return Err(ExitCode::SUCCESS);
            }
            "-v" | "--verbose" => o.verbose = true,
            "--remote" => {
                // pre-commit puts the file names right after the hook's args,
                // so --remote only takes the next argument as its URL when it
                // is one: `--remote Jenkinsfile` checks Jenkinsfile.
                let v = match inline.clone() {
                    Some(v) => v,
                    None => match args.get(i).map(|a| a.to_string_lossy().into_owned()) {
                        Some(next) if next.contains("://") => {
                            i += 1;
                            next
                        }
                        _ => String::new(),
                    },
                };
                remote_flag = Some(v);
            }
            "--username" => o.username = Some(value("--username")?),
            "--token" => o.token = Some(value("--token")?),
            "-j" | "--jobs" => {
                let v = value("-j/--jobs")?;
                o.jobs = match v.parse::<i64>() {
                    Ok(n) if n >= 1 => n as usize,
                    Ok(_) => return Err(usage_error("argument -j/--jobs: must be at least 1")),
                    Err(_) => return Err(usage_error(format!("argument -j/--jobs: invalid int value: '{v}'"))),
                };
            }
            "--persona" => {
                let v = value("--persona")?;
                choice("--persona", &v, &["regular", "pedantic", "auditor"])?;
                o.filter.persona = Persona::parse(&v).unwrap_or(Persona::Regular);
            }
            "--min-severity" => {
                let v = value("--min-severity")?;
                choice("--min-severity", &v, &["low", "medium", "high"])?;
                o.filter.min_severity = audit::Severity::parse(&v).unwrap_or(audit::Severity::Low);
            }
            "--min-confidence" => {
                let v = value("--min-confidence")?;
                choice("--min-confidence", &v, &["low", "medium", "high"])?;
                o.filter.min_confidence = Confidence::parse(&v).unwrap_or(Confidence::Low);
            }
            "--ignore" => {
                let rules = value("--ignore")?;
                for name in rules.split(',').map(str::trim) {
                    let Some(id) = audit::rule_id(name) else {
                        let ids: Vec<&str> = audit::RULES.iter().map(|r| r.id).collect();
                        return Err(usage_error(format!(
                            "argument --ignore: unknown rule {name:?}; the rules are {}",
                            ids.join(", ")
                        )));
                    };
                    if !o.filter.ignore.contains(&id) {
                        o.filter.ignore.push(id);
                    }
                }
            }
            // Named and working as in ruff check.
            "--fix" | "--no-fix" | "--unsafe-fixes" | "--no-unsafe-fixes" | "--show-fixes" | "--no-show-fixes" if inline.is_some() => {
                return Err(usage_error(format!("argument {flag}: ignored explicit argument")))
            }
            "--fix" => o.fix = true,
            "--no-fix" => o.fix = false,
            "--unsafe-fixes" => o.unsafe_fixes = true,
            "--no-unsafe-fixes" => o.unsafe_fixes = false,
            "--show-fixes" => o.show_fixes = true,
            "--no-show-fixes" => o.show_fixes = false,
            "--diff" => o.diff = true,
            "--format" => {
                let v = value("--format")?;
                choice("--format", &v, &["text", "json", "sarif"])?;
                o.format = match v.as_str() {
                    "json" => Format::Json,
                    "sarif" => Format::Sarif,
                    _ => Format::Text,
                };
            }
            "--skip" => o.skip.push(value("--skip")?),
            "--include" => o.include.push(value("--include")?),
            _ => return Err(usage_error(format!("unrecognized arguments: {arg}"))),
        }
    }
    if o.paths.is_empty() {
        return Err(usage_error("the following arguments are required: PATH"));
    }
    if o.diff && o.format != Format::Text {
        return Err(usage_error("argument --diff: not allowed with argument --format"));
    }
    if let Some(v) = remote_flag {
        let url = if v.is_empty() { env("JENKINS_URL").unwrap_or_default() } else { v };
        if url.is_empty() {
            return Err(usage_error("argument --remote: no URL given and JENKINS_URL is not set"));
        }
        if o.format != Format::Text || o.diff {
            let flag = if o.diff { "--diff" } else { "--format" };
            return Err(usage_error(format!("argument {flag}: not allowed with argument --remote")));
        }
        o.remote = Some(url);
        o.username = o.username.or_else(|| env("JENKINS_USER"));
        o.token = o.token.or_else(|| env("JENKINS_TOKEN"));
    } else {
        for (flag, set) in [("--username", o.username.is_some()), ("--token", o.token.is_some())] {
            if set {
                return Err(usage_error(format!("argument {flag}: requires --remote")));
            }
        }
    }
    Ok(o)
}

impl Options {
    /// The fixes that --fix makes, --diff shows and `[*]` marks.
    fn mode(&self) -> Mode {
        if self.unsafe_fixes {
            Mode::All
        } else {
            Mode::Safe
        }
    }
}

fn skip_reason(path: &Path, o: &Options) -> Option<&'static str> {
    let p = path.to_string_lossy();
    if !o.include.is_empty() && !o.include.iter().any(|pat| path_match(&p, pat)) {
        return Some("does not match include pattern");
    }
    if o.skip.iter().any(|pat| path_match(&p, pat)) {
        return Some("matches skip pattern");
    }
    None
}

fn in_library(path: &Path) -> bool {
    path.parent().is_some_and(|p| {
        p.components()
            .any(|c| c.as_os_str().to_str().is_some_and(|s| LIBRARY_DIRS.contains(&s)))
    })
}

/// What checking one file found.
struct Checked {
    /// Read error, as printed.
    unreadable: Option<String>,
    report: Report,
    findings: Vec<Finding>,
    fixed: Vec<Finding>,
}

impl Checked {
    fn valid(&self) -> bool {
        self.unreadable.is_none() && !self.report.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
}

/// Check one file, and with --fix rewrite it with the fixes made.
fn check_file(path: &Path, o: &Options) -> Checked {
    let mode = o.mode();
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return Checked {
                unreadable: Some(e.to_string()),
                report: Report::default(),
                findings: Vec::new(),
                fixed: Vec::new(),
            }
        }
    };
    let (fixed, findings, text, unreadable) = match fix_source(&src, &o.filter, mode) {
        Ok(f) if o.fix && f.src != src => match std::fs::write(path, &f.src) {
            Ok(()) => (f.fixed, f.remaining, f.src, None),
            Err(e) => (Vec::new(), f.findings, src, Some(e.to_string())),
        },
        Ok(f) => (Vec::new(), f.findings, src, None),
        // A syntax error: lint() below reports it.
        Err(_) => (Vec::new(), Vec::new(), src, None),
    };
    Checked {
        unreadable,
        report: lint(&text),
        findings,
        fixed,
    }
}

pub fn run(o: Options) -> ExitCode {
    let searched = o.paths.iter().any(|p| p.is_dir());
    let (files, empty) = expand_paths(&o.paths);
    let report = o.format != Format::Text;
    if o.verbose {
        for dir in &empty {
            let line = format!("ℹ No files to check found in {}", dir.display());
            if report {
                eprintln!("{line}");
            } else {
                println!("{line}");
            }
        }
    }
    // Nothing to check: don't call Jenkins. A report is still written, so
    // that a tool reading it gets an empty one.
    if files.is_empty() && !report {
        return ExitCode::SUCCESS;
    }
    let selected: Vec<&PathBuf> = files.iter().filter(|f| skip_reason(f, &o).is_none()).collect();
    if o.verbose && o.remote.is_none() && !report && !o.diff {
        println!("{OFFLINE_NOTE}");
    }
    if report || o.diff {
        return write_report(&selected, &o);
    }

    let mode = o.mode();
    let show = o.verbose || searched || files.len() > 1;
    let mut counts = [0usize; 3]; // high, medium, low
    let mut fixed = 0;
    let mut fixable = [0usize; 2]; // safe, unsafe
    let mut passed = true;
    let mut send: HashSet<&Path> = HashSet::new();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for path in &files {
        if let Some(reason) = skip_reason(path, &o) {
            // With --remote, said in file order with the files Jenkins checks.
            if o.verbose && o.remote.is_none() {
                let _ = writeln!(out, "⊘ {}: Skipped ({reason})", path.display());
            }
            continue;
        }
        let name = path.to_string_lossy();
        let c = check_file(path, &o);
        if o.show_fixes {
            let _ = write!(out, "{}", fixed_text(&name, &c.fixed));
        }
        fixed += c.fixed.len();
        let errors = match &c.unreadable {
            Some(e) => format!("Error reading file {name}: {e}\n"),
            None => diagnostics_text(&name, &c.report),
        };
        if !errors.is_empty() {
            let _ = out.flush(); // keep the output in file order when piped
            eprint!("{errors}");
        }
        let _ = write!(out, "{}", audit_text(&name, &c.findings, mode));
        for f in &c.findings {
            counts[match f.severity {
                audit::Severity::High => 0,
                audit::Severity::Medium => 1,
                audit::Severity::Low => 2,
            }] += 1;
            if let Some(fix) = &f.fix {
                fixable[(fix.disposition == Disposition::Unsafe) as usize] += 1;
            }
        }
        passed &= c.valid() && c.findings.is_empty();
        if !c.valid() {
            continue;
        }
        if c.report.declarative && o.remote.is_some() {
            send.insert(path);
        } else if c.report.declarative {
            if show && c.findings.is_empty() {
                let _ = writeln!(out, "✓ {}: Valid", path.display());
            }
        } else if o.verbose || (show && !in_library(path)) {
            // Shared library files are never pipelines: named only with -v.
            let _ = writeln!(out, "⊘ {}: Skipped (not a Declarative Pipeline)", path.display());
        }
    }

    let total: usize = counts.iter().sum();
    let plural = |n: usize| if n == 1 { "" } else { "s" };
    if o.fix {
        let found = fixed + total;
        let _ = writeln!(out, "Found {found} finding{} ({fixed} fixed, {total} remaining).", plural(found));
    } else if total > 0 {
        let parts: Vec<String> = ["high", "medium", "low"]
            .iter()
            .zip(counts)
            .filter(|(_, n)| *n > 0)
            .map(|(s, n)| format!("{n} {s}"))
            .collect();
        let _ = writeln!(out, "{total} finding{} ({})", plural(total), parts.join(", "));
    }
    // As ruff check says it: what --fix would fix, and the unsafe fixes
    // that --unsafe-fixes would add.
    let (applicable, hidden) = if o.unsafe_fixes {
        (fixable[0] + fixable[1], 0)
    } else {
        (fixable[0], fixable[1])
    };
    let hidden_note = |n: usize| {
        let fixes = if n == 1 { "fix" } else { "fixes" };
        format!("{n} hidden {fixes} can be enabled with the `--unsafe-fixes` option")
    };
    if applicable > 0 {
        let more = if hidden > 0 {
            format!(" ({})", hidden_note(hidden))
        } else {
            String::new()
        };
        let _ = writeln!(out, "[*] {applicable} fixable with the `--fix` option{more}.");
    } else if hidden > 0 {
        let _ = writeln!(out, "No fixes available ({}).", hidden_note(hidden));
    }
    drop(out);

    if let Some(url) = &o.remote {
        let jenkins = Jenkins::new(url, o.username.as_deref(), o.token.as_deref());
        passed &= validate_remote(&jenkins, &files, &send, &o, show);
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Validate the files in `send` with Jenkins, up to --jobs at a time, and
/// print each result in the order of `files`. Returns whether all passed.
fn validate_remote(jenkins: &Jenkins, files: &[PathBuf], send: &HashSet<&Path>, o: &Options, show: bool) -> bool {
    let queue: Vec<&PathBuf> = files.iter().filter(|f| send.contains(f.as_path())).collect();
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel::<(usize, Verdict)>();
    let mut all_valid = true;
    let mut printed: HashSet<String> = HashSet::new();
    std::thread::scope(|scope| {
        for _ in 0..o.jobs.min(queue.len()) {
            let tx = tx.clone();
            let (next, queue) = (&next, &queue);
            scope.spawn(move || loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(path) = queue.get(i) else { break };
                let verdict = match std::fs::read_to_string(path) {
                    Ok(src) => jenkins.validate(&src),
                    Err(e) => Verdict::Invalid(format!("Error reading file: {e}")),
                };
                if tx.send((i, verdict)).is_err() {
                    break;
                }
            });
        }
        drop(tx);
        let mut results: Vec<Option<Verdict>> = vec![None; queue.len()];
        let mut index = 0;
        let mut pending = rx.iter();
        for path in files {
            if let Some(reason) = skip_reason(path, o) {
                if o.verbose {
                    println!("⊘ {}: Skipped ({reason})", path.display());
                }
                continue;
            }
            if !send.contains(path.as_path()) {
                continue;
            }
            if o.verbose {
                println!("Validating {}...", path.display());
            }
            while results[index].is_none() {
                match pending.next() {
                    Some((i, v)) => results[i] = Some(v),
                    None => break,
                }
            }
            match results[index].take() {
                Some(Verdict::NotDeclarative) if show => {
                    println!("⊘ {}: Skipped (not a Declarative Pipeline)", path.display())
                }
                Some(Verdict::NotDeclarative) => {}
                Some(Verdict::Valid(message)) => {
                    if show {
                        println!("✓ {}: Valid", path.display());
                    }
                    if o.verbose && !message.is_empty() {
                        println!("  {message}");
                    }
                }
                Some(Verdict::Invalid(message)) => {
                    all_valid = false;
                    let _ = std::io::stdout().flush();
                    // The same connection or credentials error for every file
                    // is printed once.
                    if message.starts_with("Error connecting to Jenkins") {
                        if printed.insert(message.clone()) {
                            eprintln!("✗ {}: {message}", path.display());
                        }
                    } else {
                        eprintln!("✗ {}: rejected by Jenkins", path.display());
                        for line in message.lines() {
                            eprintln!("  {line}");
                        }
                    }
                }
                None => all_valid = false,
            }
            index += 1;
        }
    });
    all_valid
}

/// --format json or sarif, or --diff: the report of every file.
fn write_report(files: &[&PathBuf], o: &Options) -> ExitCode {
    let mode = o.mode();
    let mut unreadable = false;
    let mut flagged = false;
    let mut changed = false;
    let mut items = Vec::new();
    let mut checked = Vec::new();
    for path in files {
        let name = path.to_string_lossy().into_owned();
        if o.diff {
            let src = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{name}: {e}");
                    unreadable = true;
                    continue;
                }
            };
            match fix_source(&src, &o.filter, mode) {
                Ok(f) => {
                    let diff = audit::fix::diff(&name, &src, &f.src);
                    changed |= !diff.is_empty();
                    print!("{diff}");
                }
                Err(d) => {
                    eprintln!("{name}:{}:{}: error: {}", d.pos.line, d.pos.col, d.message);
                    unreadable = true;
                }
            }
            continue;
        }
        let c = check_file(path, o);
        if let Some(e) = &c.unreadable {
            unreadable = true;
            if o.format == Format::Json {
                items.push(format!("{{\"file\":\"{}\",\"error\":\"{}\"}}", json_escape(&name), json_escape(e)));
            } else {
                eprintln!("{name}: {e}");
            }
            continue;
        }
        flagged |= !c.valid() || !c.findings.is_empty();
        match o.format {
            Format::Json => items.push(audit_json(&name, &c.report, &c.findings, &c.fixed)),
            _ => checked.push(audit::sarif::Checked {
                path: name,
                findings: c.findings,
                diagnostics: c.report.diagnostics,
            }),
        }
    }
    match o.format {
        Format::Json => println!("[{}]", items.join(",")),
        Format::Sarif => println!("{}", audit::sarif::sarif(&checked)),
        Format::Text => {}
    }
    let failed = if o.diff { changed } else { flagged && o.format != Format::Sarif };
    if unreadable || failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
