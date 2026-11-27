//! The jenkins-lint command: which files to check, the options, and the
//! output.

use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, ValueEnum};

use crate::baseline::{self, Baseline};
use crate::check::{fix, lint};
use crate::diag::{Report, Severity};
use crate::discover::{expand_paths, path_match, LIBRARY_DIRS};
use crate::output::{
    diagnostics_full, diagnostics_text, file_json, findings_full, findings_text, fixed_text, github_annotations, renderer, UnreadableJson,
};
use crate::remote::{Jenkins, Verdict};
use crate::rules::fix::{Disposition, Mode};
use crate::rules::{self, Confidence, Filter, Finding, Persona};
use crate::sarif::{sarif, Checked as SarifFile};

const NAME: &str = "jenkins-lint";

/// Printed with --verbose when files are checked offline only.
pub const OFFLINE_NOTE: &str = "ℹ Checking offline: step names, step parameters and other plugin-dependent \
     checks are skipped. Pass --remote to also check with your Jenkins.";

#[derive(Debug, Clone, Copy, PartialEq, ValueEnum)]
pub enum Format {
    /// Each error and finding with the code it is about, and a summary
    #[value(alias = "text")]
    Full,
    /// One line per error and finding, with a summary
    Concise,
    /// One object per file
    Json,
    /// SARIF 2.1.0, for code scanning
    Sarif,
    /// GitHub Actions annotations, shown on the lines of a pull request
    Github,
}

impl Format {
    /// Printed for people to read, with a summary.
    fn is_text(self) -> bool {
        matches!(self, Format::Full | Format::Concise)
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Level {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PersonaArg {
    /// What is worth fixing
    Regular,
    /// Adds lower-confidence findings and hardening advice
    Pedantic,
    /// Everything, for a security review
    Auditor,
}

/// Check Jenkinsfiles offline: Groovy syntax, Declarative Pipeline structure
/// and security. With --remote, the files that pass are also validated by
/// your Jenkins.
#[derive(Debug, Parser)]
#[command(
    name = "jenkins-lint",
    version,
    override_usage = "jenkins-lint [check] [OPTIONS] <PATH>...",
    after_help = "Documentation: https://jenkinsci.github.io/jenkins-lint/"
)]
struct Args {
    /// Jenkinsfiles and shared library files, or directories to search. In a
    /// directory: Jenkinsfile and *.jenkinsfile, Jenkinsfile* and *.groovy
    /// files that contain a pipeline, and every *.groovy under vars/ and src/
    #[arg(required = true, value_name = "PATH")]
    paths: Vec<PathBuf>,

    /// Also validate the files that pass offline with this Jenkins, which
    /// knows its own plugins. Without a URL, uses JENKINS_URL, which never
    /// turns this on by itself
    #[arg(long, value_name = "URL", num_args = 0..=1, require_equals = true, default_missing_value = "", help_heading = "Jenkins")]
    remote: Option<String>,
    /// Jenkins username, with --remote [env: JENKINS_USER]
    #[arg(long, value_name = "USER", requires = "remote", help_heading = "Jenkins")]
    username: Option<String>,
    /// Jenkins API token, with --remote [env: JENKINS_TOKEN]
    #[arg(long, value_name = "TOKEN", requires = "remote", help_heading = "Jenkins")]
    token: Option<String>,
    /// Files to send to Jenkins at the same time
    #[arg(short, long, value_name = "N", default_value_t = 8, value_parser = clap::value_parser!(u16).range(1..), help_heading = "Jenkins")]
    jobs: u16,

    /// Apply fixes to the files
    #[arg(long, overrides_with = "no_fix", help_heading = "Fixes")]
    fix: bool,
    #[arg(long, overrides_with = "fix", hide = true)]
    no_fix: bool,
    /// Include the fixes that can change what a pipeline does: review them
    #[arg(long, overrides_with = "no_unsafe_fixes", help_heading = "Fixes")]
    unsafe_fixes: bool,
    #[arg(long, overrides_with = "unsafe_fixes", hide = true)]
    no_unsafe_fixes: bool,
    /// Name each fix that --fix made
    #[arg(long, overrides_with = "no_show_fixes", help_heading = "Fixes")]
    show_fixes: bool,
    #[arg(long, overrides_with = "show_fixes", hide = true)]
    no_show_fixes: bool,
    /// Print what the fixes would change, and change nothing
    #[arg(long, conflicts_with = "remote", help_heading = "Fixes")]
    diff: bool,

    /// How many security findings to show
    #[arg(long, value_enum, default_value_t = PersonaArg::Regular, help_heading = "Rules")]
    persona: PersonaArg,
    /// Leave out findings below this severity
    #[arg(long, value_enum, value_name = "LEVEL", default_value_t = Level::Low, help_heading = "Rules")]
    min_severity: Level,
    /// Leave out findings below this confidence
    #[arg(long, value_enum, value_name = "LEVEL", default_value_t = Level::Low, help_heading = "Rules")]
    min_confidence: Level,
    /// Turn off rules in every file, such as unpinned-library
    #[arg(long, value_name = "RULE", value_delimiter = ',', value_parser = rule_id, help_heading = "Rules")]
    ignore: Vec<&'static str>,

    /// Skip the files that match this glob, such as 'vars/*.groovy'
    #[arg(long, value_name = "PATTERN", help_heading = "Files")]
    skip: Vec<String>,
    /// Check only the files that match one of these globs
    #[arg(long, value_name = "PATTERN", help_heading = "Files")]
    include: Vec<String>,

    /// Findings in this file do not fail the run [default: .jenkins-lint-baseline.json, up to the Git root]
    #[arg(long, value_name = "FILE", help_heading = "Baseline")]
    baseline: Option<PathBuf>,
    /// Write the findings to the baseline file, so that only new ones fail
    #[arg(long, conflicts_with_all = ["fix", "diff", "no_baseline"], help_heading = "Baseline")]
    write_baseline: bool,
    /// Remove the fixed findings from the baseline file, and add none
    #[arg(long, conflicts_with_all = ["fix", "diff", "no_baseline", "write_baseline"], help_heading = "Baseline")]
    prune_baseline: bool,
    /// Report the findings in the baseline too
    #[arg(long, conflicts_with = "baseline", help_heading = "Baseline")]
    no_baseline: bool,

    /// Output format. sarif exits with 0 when there are findings, so that the
    /// report can be uploaded
    #[arg(long, value_enum, value_name = "FORMAT", default_value_t = Format::Full)]
    output_format: Format,
    /// Say what is checked, skipped and found
    #[arg(short, long)]
    verbose: bool,
}

fn rule_id(name: &str) -> Result<&'static str, String> {
    rules::rule_id(name.trim()).ok_or_else(|| {
        let ids: Vec<&str> = rules::RULES.iter().map(|r| r.id).collect();
        format!("unknown rule; the rules are {}", ids.join(", "))
    })
}

fn level(l: Level) -> (rules::Severity, Confidence) {
    match l {
        Level::Low => (rules::Severity::Low, Confidence::Low),
        Level::Medium => (rules::Severity::Medium, Confidence::Medium),
        Level::High => (rules::Severity::High, Confidence::High),
    }
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
    /// The baseline file to read, and with --write-baseline to write.
    pub baseline: Option<PathBuf>,
    pub write_baseline: bool,
    pub prune_baseline: bool,
}

/// The options, read from the command line and the environment: an `env`
/// lookup. Err is the exit code after --help, --version or a usage error.
pub fn parse_args(args: impl IntoIterator<Item = OsString>, env: impl Fn(&str) -> Option<String>) -> Result<Options, ExitCode> {
    let mut args: Vec<OsString> = args.into_iter().collect();
    // `jenkins-lint check PATH` is `jenkins-lint PATH`, as in `ruff check`.
    if args.first().and_then(|a| a.to_str()) == Some("check") {
        args.remove(0);
    }
    // pre-commit puts the file names right after a hook's args, so --remote
    // only takes the next argument as its URL when it is one:
    // `--remote Jenkinsfile` checks Jenkinsfile with JENKINS_URL.
    let mut argv = vec![OsString::from(NAME)];
    let mut rest = args.into_iter().peekable();
    while let Some(arg) = rest.next() {
        if arg == "--" {
            argv.push(arg);
            argv.extend(rest.by_ref());
            break;
        }
        if arg == "--remote" {
            match rest.peek().and_then(|a| a.to_str()) {
                Some(next) if next.contains("://") => {
                    let url = rest.next().unwrap_or_default();
                    let mut joined = OsString::from("--remote=");
                    joined.push(url);
                    argv.push(joined);
                }
                _ => argv.push(OsString::from("--remote=")),
            }
            continue;
        }
        argv.push(arg);
    }
    let fail = |e: clap::Error| {
        let _ = e.print();
        ExitCode::from(e.exit_code() as u8)
    };
    let a = Args::try_parse_from(argv).map_err(fail)?;
    let error = |kind: ErrorKind, message: &str| fail(Args::command().error(kind, message));
    if a.diff && !a.output_format.is_text() {
        return Err(error(
            ErrorKind::ArgumentConflict,
            "--diff cannot be used with --output-format json, sarif or github",
        ));
    }
    let remote = match a.remote {
        None => None,
        Some(url) => {
            let url = if url.is_empty() {
                env("JENKINS_URL").unwrap_or_default()
            } else {
                url
            };
            if url.is_empty() {
                return Err(error(
                    ErrorKind::MissingRequiredArgument,
                    "--remote needs a URL: pass --remote=URL or set JENKINS_URL",
                ));
            }
            if !a.output_format.is_text() {
                return Err(error(
                    ErrorKind::ArgumentConflict,
                    "--output-format json, sarif or github cannot be used with --remote",
                ));
            }
            Some(url)
        }
    };
    let (username, token) = match remote {
        Some(_) => (a.username.or_else(|| env("JENKINS_USER")), a.token.or_else(|| env("JENKINS_TOKEN"))),
        None => (None, None),
    };
    if (a.write_baseline || a.prune_baseline) && !a.output_format.is_text() {
        return Err(error(
            ErrorKind::ArgumentConflict,
            "--write-baseline and --prune-baseline cannot be used with --output-format json, sarif or github",
        ));
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let baseline = match a.baseline {
        _ if a.no_baseline => None,
        Some(path) => {
            if !a.write_baseline && !path.is_file() {
                return Err(error(
                    ErrorKind::ValueValidation,
                    &format!("baseline file not found: {}", path.display()),
                ));
            }
            Some(path)
        }
        None => baseline::find(&cwd).or_else(|| a.write_baseline.then(|| baseline::default_path(&cwd))),
    };
    if a.prune_baseline && baseline.as_ref().is_none_or(|p| !p.is_file()) {
        return Err(error(ErrorKind::ValueValidation, "--prune-baseline needs a baseline file"));
    }
    let mut ignore = a.ignore;
    ignore.dedup();
    Ok(Options {
        paths: a.paths,
        remote,
        username,
        token,
        jobs: a.jobs as usize,
        filter: Filter {
            persona: match a.persona {
                PersonaArg::Regular => Persona::Regular,
                PersonaArg::Pedantic => Persona::Pedantic,
                PersonaArg::Auditor => Persona::Auditor,
            },
            min_severity: level(a.min_severity).0,
            min_confidence: level(a.min_confidence).1,
            ignore,
        },
        fix: a.fix,
        unsafe_fixes: a.unsafe_fixes,
        show_fixes: a.show_fixes,
        diff: a.diff,
        format: a.output_format,
        skip: a.skip,
        include: a.include,
        verbose: a.verbose,
        baseline,
        write_baseline: a.write_baseline,
        prune_baseline: a.prune_baseline,
    })
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
    /// The text that `report` and `findings` are about: the file, or what
    /// --fix made of it.
    text: String,
    findings: Vec<Finding>,
    fixed: Vec<Finding>,
    /// How many findings the baseline holds.
    known: usize,
    /// It was read and parsed, so its findings are complete.
    analyzed: bool,
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
                text: String::new(),
                findings: Vec::new(),
                fixed: Vec::new(),
                known: 0,
                analyzed: false,
            }
        }
    };
    let parsed = fix(&src, &o.filter, mode);
    let analyzed = parsed.is_ok();
    let (fixed, findings, text, unreadable) = match parsed {
        Ok(f) if o.fix && f.src != src => match std::fs::write(path, &f.src) {
            Ok(()) => (f.fixed, f.remaining, f.src, None),
            Err(e) => (Vec::new(), f.findings, src, Some(e.to_string())),
        },
        Ok(f) => (Vec::new(), f.findings, src, None),
        // A syntax error: lint() below reports it.
        Err(_) => (Vec::new(), Vec::new(), src, None),
    };
    Checked {
        analyzed: analyzed && unreadable.is_none(),
        unreadable,
        report: lint(&text),
        text,
        findings,
        fixed,
        known: 0,
    }
}

fn load_baseline(o: &Options) -> Result<Option<Baseline>, ExitCode> {
    match &o.baseline {
        None => Ok(None),
        Some(path) => Baseline::load(path).map(Some).map_err(|e| {
            eprintln!("Error reading the baseline {e}");
            ExitCode::from(2)
        }),
    }
}

/// The findings that are new, and how many are in the baseline.
fn split(baseline: &mut Option<Baseline>, path: &Path, c: &mut Checked, write: bool) -> Vec<Finding> {
    let all = std::mem::take(&mut c.findings);
    match baseline {
        // A file that does not parse keeps its entries.
        Some(b) if c.analyzed => {
            let (new, known) = b.split(path, &c.text, all);
            c.known = known.len() + if write { new.len() } else { 0 };
            if write {
                Vec::new()
            } else {
                new
            }
        }
        _ => all,
    }
}

fn fixed_note(baseline: &Option<Baseline>) -> Option<String> {
    let n = baseline.as_ref().map_or(0, Baseline::fixed);
    let (findings, occur) = if n == 1 { ("finding", "occurs") } else { ("findings", "occur") };
    (n > 0).then(|| format!("ℹ {n} {findings} in the baseline no longer {occur}: run with --prune-baseline to remove them"))
}

pub fn run(o: Options) -> ExitCode {
    let searched = o.paths.iter().any(|p| p.is_dir());
    let (files, empty) = expand_paths(&o.paths);
    let report = !o.format.is_text();
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
    let mut baseline = match load_baseline(&o) {
        Ok(b) => b,
        Err(code) => return code,
    };
    if report || o.diff {
        return write_report(&selected, &o, &mut baseline);
    }

    let mode = o.mode();
    let show = o.verbose || searched || files.len() > 1;
    let mut counts = [0usize; 3]; // high, medium, low
    let mut fixed = 0;
    let mut fixable = [0usize; 2]; // safe, unsafe
    let mut passed = true;
    let mut known = 0;
    let mut send: HashSet<&Path> = HashSet::new();
    let stdout = std::io::stdout();
    let full = o.format == Format::Full;
    let out_renderer = renderer(stdout.is_terminal());
    let err_renderer = renderer(std::io::stderr().is_terminal());
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
        let mut c = check_file(path, &o);
        c.findings = split(&mut baseline, path, &mut c, o.write_baseline);
        known += c.known;
        if o.show_fixes {
            let _ = write!(out, "{}", fixed_text(&name, &c.fixed));
        }
        fixed += c.fixed.len();
        let errors = match &c.unreadable {
            Some(e) => format!("Error reading file {name}: {e}\n"),
            None if full => diagnostics_full(&name, &c.text, &c.report, &err_renderer),
            None => diagnostics_text(&name, &c.report),
        };
        if !errors.is_empty() {
            let _ = out.flush(); // keep the output in file order when piped
            eprint!("{errors}");
        }
        let found = if full {
            findings_full(&name, &c.text, &c.findings, mode, &out_renderer)
        } else {
            findings_text(&name, &c.findings, mode)
        };
        let _ = write!(out, "{found}");
        for f in &c.findings {
            counts[match f.severity {
                rules::Severity::High => 0,
                rules::Severity::Medium => 1,
                rules::Severity::Low => 2,
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
        let more = if known > 0 {
            format!(", {known} in the baseline")
        } else {
            String::new()
        };
        let _ = writeln!(out, "{total} finding{} ({}){more}", plural(total), parts.join(", "));
    } else if known > 0 && !o.write_baseline {
        let _ = writeln!(out, "No new findings, {known} in the baseline");
    }
    if let Some(b) = &baseline {
        if o.write_baseline {
            match b.write() {
                Ok(n) => {
                    let _ = writeln!(out, "Wrote {n} finding{} to {}", plural(n), b.path.display());
                }
                Err(e) => {
                    eprintln!("Error writing the baseline {e}");
                    passed = false;
                }
            }
        } else if o.prune_baseline {
            match b.prune() {
                Ok(n) => {
                    let _ = writeln!(out, "Removed {n} fixed finding{} from {}", plural(n), b.path.display());
                }
                Err(e) => {
                    eprintln!("Error writing the baseline {e}");
                    passed = false;
                }
            }
        } else if let Some(note) = fixed_note(&baseline) {
            let _ = writeln!(out, "{note}");
        }
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

/// --output-format json, sarif or github, or --diff: the report of every
/// file.
fn write_report(files: &[&PathBuf], o: &Options, baseline: &mut Option<Baseline>) -> ExitCode {
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
            match fix(&src, &o.filter, mode) {
                Ok(f) => {
                    let diff = rules::fix::diff(&name, &src, &f.src);
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
        let mut c = check_file(path, o);
        let all = std::mem::take(&mut c.findings);
        let (new, known) = match baseline {
            Some(b) if c.analyzed => b.split(path, &c.text, all),
            _ => (all, Vec::new()),
        };
        c.findings = new;
        if let Some(e) = &c.unreadable {
            unreadable = true;
            if o.format == Format::Json {
                items.push(serde_json::to_value(UnreadableJson { file: &name, error: e }).unwrap_or_default());
            } else {
                eprintln!("{name}: {e}");
            }
            continue;
        }
        flagged |= !c.valid() || !c.findings.is_empty();
        match o.format {
            Format::Json => items.push(serde_json::to_value(file_json(&name, &c.report, &c.findings, &c.fixed)).unwrap_or_default()),
            Format::Github => print!("{}", github_annotations(&name, &c.report, &c.findings)),
            _ => checked.push(SarifFile {
                path: name,
                findings: c.findings,
                baselined: known,
                diagnostics: c.report.diagnostics,
            }),
        }
    }
    match o.format {
        Format::Json => println!("{}", serde_json::Value::Array(items)),
        Format::Sarif => println!("{}", sarif(&checked)),
        Format::Full | Format::Concise | Format::Github => {}
    }
    if let Some(note) = fixed_note(baseline) {
        eprintln!("{note}");
    }
    let failed = if o.diff { changed } else { flagged && o.format != Format::Sarif };
    if unreadable || failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
