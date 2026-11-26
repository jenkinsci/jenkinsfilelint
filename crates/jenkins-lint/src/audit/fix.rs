//! `--fix`: rewrite a step's script so that the shell, not Groovy, reads
//! the values in it.
//!
//! `sh "deploy --token ${TOKEN}"` becomes `sh 'deploy --token "$TOKEN"'`.
//! A script is rewritten only when every placeholder in it is an
//! environment variable, and its text means the same in single quotes: no
//! quote, backslash or `$` of its own. Anything else is left as it is.

use crate::diag::{Pos, Span};

/// A string literal passed straight to a step as its script, as the walk
/// saw it.
#[derive(Debug, Clone)]
pub struct Script {
    /// The step: `sh`, `bat`, `powershell` or `pwsh`.
    pub step: String,
    /// Where the literal starts: its opening quote.
    pub pos: Pos,
    /// Each placeholder, and the environment variable it reads, if it
    /// only reads one.
    pub placeholders: Vec<(Span, Option<String>)>,
}

/// Whether a fix keeps what the file does. A safe fix does; an unsafe one
/// may change it, and must be reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Disposition {
    Safe,
    Unsafe,
}

impl Disposition {
    pub fn name(self) -> &'static str {
        match self {
            Disposition::Safe => "safe",
            Disposition::Unsafe => "unsafe",
        }
    }
}

/// Which fixes `--fix=MODE` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Safe,
    UnsafeOnly,
    All,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "safe" => Some(Mode::Safe),
            "unsafe-only" => Some(Mode::UnsafeOnly),
            "all" => Some(Mode::All),
            _ => None,
        }
    }

    pub fn applies(self, d: Disposition) -> bool {
        match self {
            Mode::Safe => d == Disposition::Safe,
            Mode::UnsafeOnly => d == Disposition::Unsafe,
            Mode::All => true,
        }
    }
}

/// A rewritten script literal.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// The literal, quotes included.
    pub span: Span,
    /// What replaces it.
    pub text: String,
    /// Set by the rule whose finding it fixes.
    pub disposition: Disposition,
}

/// How a step's shell reads an environment variable, for the steps that
/// `--fix` rewrites. `bat` has no quoting that keeps every value text.
fn reference(step: &str, name: &str) -> Option<String> {
    match step {
        "sh" => Some(format!("\"${name}\"")),
        "powershell" | "pwsh" => Some(format!("${{env:{name}}}")),
        _ => None,
    }
}

/// Text in which a value the shell reads is still run, or where quotes do
/// not mean what they mean elsewhere: here-documents, `[[ ]]` patterns.
fn reruns_values(step: &str, text: &str) -> bool {
    match step {
        "sh" => ["<<", "[[", "eval"].iter().any(|w| text.contains(w)),
        _ => {
            let text = text.to_lowercase();
            ["invoke-expression", "iex"].iter().any(|w| text.contains(w))
        }
    }
}

/// A file's text, by character as the lexer counts positions.
pub struct Source {
    chars: Vec<char>,
    /// Where each line starts.
    lines: Vec<usize>,
}

impl Source {
    pub fn new(src: &str) -> Self {
        let chars: Vec<char> = src.chars().collect();
        let mut lines = vec![0];
        lines.extend(chars.iter().enumerate().filter(|(_, c)| **c == '\n').map(|(i, _)| i + 1));
        Source { chars, lines }
    }

    fn index(&self, p: Pos) -> Option<usize> {
        let i = self.lines.get(p.line.checked_sub(1)?)? + p.col.checked_sub(1)?;
        (i <= self.chars.len()).then_some(i)
    }

    fn pos(&self, i: usize) -> Pos {
        let line = self.lines.partition_point(|&start| start <= i);
        Pos {
            line,
            col: i - self.lines[line - 1] + 1,
        }
    }
}

/// The rewrite of a script, if it is safe.
pub fn plan(src: &Source, script: &Script) -> Option<Fix> {
    let c = &src.chars;
    let start = src.index(script.pos)?;
    let triple = c.get(start..start + 3) == Some(&['"'; 3]);
    if !triple && c.get(start) != Some(&'"') {
        return None;
    }
    let quote = if triple { "'''" } else { "'" };
    let inner = start + quote.len();
    // The text between placeholders must read the same in single quotes.
    let plain = |from: usize, to: usize| c[from..to].iter().all(|c| !matches!(c, '\'' | '"' | '\\' | '$'));
    let mut text = String::from(quote);
    let mut words = String::new();
    let mut i = inner;
    for (span, name) in &script.placeholders {
        let (from, to) = (src.index(span.start)?, src.index(span.end)?);
        if from < i || !plain(i, from) {
            return None;
        }
        let reference = reference(&script.step, name.as_deref()?)?;
        // A value where a command goes is the command: quoted, a command
        // line in it would become one name.
        let before = c[inner..from].iter().rev().find(|c| !matches!(c, ' ' | '\t'));
        if before.is_none_or(|c| matches!(c, '\n' | '\r' | ';' | '|' | '&' | '(' | '{' | '`')) {
            return None;
        }
        // PowerShell reads `$env:X` alone as an argument; joined to other
        // text it can mean something else.
        if script.step != "sh" {
            let apart = |c: Option<&char>| c.is_none_or(|c| c.is_whitespace() || *c == '"');
            if !(from == inner || apart(c.get(from - 1))) || !apart(c.get(to)) {
                return None;
            }
        }
        text.extend(&c[i..from]);
        words.extend(&c[i..from]);
        words.push(' ');
        text.push_str(&reference);
        i = to;
    }
    let close = (i..c.len()).find(|&k| c[k] == '"')?;
    if !plain(i, close) || (triple && c.get(close..close + 3) != Some(&['"'; 3])) {
        return None;
    }
    text.extend(&c[i..close]);
    words.extend(&c[i..close]);
    text.push_str(quote);
    if reruns_values(&script.step, &words) {
        return None;
    }
    Some(Fix {
        span: Span {
            start: script.pos,
            end: src.pos(close + quote.len()),
        },
        text,
        disposition: Disposition::Unsafe,
    })
}

/// A unified diff from `old` to `new`, which have the same lines: a fix
/// never adds or removes one.
pub fn diff(path: &str, old: &str, new: &str) -> String {
    let (old, new): (Vec<&str>, Vec<&str>) = (old.split_inclusive('\n').collect(), new.split_inclusive('\n').collect());
    let changed: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new[i]).collect();
    if changed.is_empty() || old.len() != new.len() {
        return String::new();
    }
    let line = |s: &str| {
        if s.ends_with('\n') {
            s.to_string()
        } else {
            format!("{s}\n\\ No newline at end of file\n")
        }
    };
    let mut out = format!("--- {path}\n+++ {path}\n");
    let context = 3;
    let mut k = 0;
    while k < changed.len() {
        // A hunk: changes no more than two contexts apart, and their context.
        let mut last = k;
        while last + 1 < changed.len() && changed[last + 1] - changed[last] <= 2 * context {
            last += 1;
        }
        let from = changed[k].saturating_sub(context);
        let to = (changed[last] + context + 1).min(old.len());
        out.push_str(&format!("@@ -{},{} +{},{} @@\n", from + 1, to - from, from + 1, to - from));
        let mut i = from;
        while i < to {
            if old[i] == new[i] {
                out.push(' ');
                out.push_str(&line(old[i]));
                i += 1;
                continue;
            }
            let run = (i..to).take_while(|&j| old[j] != new[j]).count();
            for l in &old[i..i + run] {
                out.push('-');
                out.push_str(&line(l));
            }
            for l in &new[i..i + run] {
                out.push('+');
                out.push_str(&line(l));
            }
            i += run;
        }
        k = last + 1;
    }
    out
}

/// The text of a file with these fixes made.
pub fn apply(src: &str, fixes: &[&Fix]) -> String {
    let source = Source::new(src);
    let mut edits: Vec<(usize, usize, &str)> = fixes
        .iter()
        .filter_map(|f| Some((source.index(f.span.start)?, source.index(f.span.end)?, f.text.as_str())))
        .collect();
    edits.sort();
    edits.dedup();
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (from, to, text) in edits {
        if from < at {
            continue;
        }
        out.extend(&source.chars[at..from]);
        out.push_str(text);
        at = to;
    }
    out.extend(&source.chars[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(step: &str, src: &str, names: &[Option<&str>]) -> Option<String> {
        // Placeholders as the lexer reports them: `$` up to the end of the
        // name or the closing brace.
        let chars: Vec<char> = src.chars().collect();
        let mut placeholders = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] == '$' {
                let end = if chars.get(i + 1) == Some(&'{') {
                    i + chars[i..].iter().position(|c| *c == '}').unwrap() + 1
                } else {
                    i + 1
                        + chars[i + 1..]
                            .iter()
                            .take_while(|c| c.is_alphanumeric() || **c == '_' || **c == '.')
                            .count()
                };
                let at = |k: usize| Pos { line: 1, col: k + 1 };
                placeholders.push(Span {
                    start: at(i),
                    end: at(end),
                });
                i = end;
            } else {
                i += 1;
            }
        }
        let start = src.find(['"', '\'']).unwrap();
        let s = Script {
            step: step.to_string(),
            pos: Pos { line: 1, col: start + 1 },
            placeholders: placeholders.into_iter().zip(names.iter().map(|n| n.map(String::from))).collect(),
        };
        let fix = plan(&Source::new(src), &s)?;
        Some(apply(src, &[&fix]))
    }

    #[test]
    fn rewrites_shell_scripts() {
        assert_eq!(
            script("sh", r#"sh "curl -u ${USER}:$PASS x""#, &[Some("USER"), Some("PASS")]).as_deref(),
            Some(r#"sh 'curl -u "$USER":"$PASS" x'"#)
        );
        assert_eq!(
            script("sh", "sh \"\"\"\n  make ${env.V}\n\"\"\"", &[Some("V")]).as_deref(),
            Some("sh '''\n  make \"$V\"\n'''")
        );
        assert_eq!(
            script("pwsh", r#"pwsh "Deploy -Token ${TOKEN}""#, &[Some("TOKEN")]).as_deref(),
            Some("pwsh 'Deploy -Token ${env:TOKEN}'")
        );
    }

    #[test]
    fn leaves_what_it_cannot_rewrite_safely() {
        for (step, src) in [
            ("bat", r#"bat "deploy ${T}""#),
            ("sh", r#"sh "echo '${T}'""#),
            ("sh", r#"sh "echo \"${T}\"""#),
            ("sh", r#"sh "a\tb ${T}""#),
            ("sh", r#"sh "cat <<EOF ${T}""#),
            ("sh", r#"sh "[[ a == ${T} ]]""#),
            ("sh", r#"sh "eval ${T}""#),
            ("pwsh", r#"pwsh "iex ${T}""#),
            ("pwsh", r#"pwsh "a-${T}""#),
            ("pwsh", r#"pwsh "${T}-a""#),
            ("sh", "sh '''a ${T}'''"),
            ("sh", r#"sh "${T} --version""#),
            ("sh", r#"sh "make && ${T}""#),
            ("sh", "sh \"\"\"\n    ${T}\n\"\"\""),
        ] {
            assert_eq!(script(step, src, &[Some("T")]), None, "{src}");
        }
        assert_eq!(script("sh", r#"sh "a ${T} ${u}""#, &[Some("T"), None]), None);
    }

    #[test]
    fn diffs() {
        let old = "a\nb\nc\nd\ne\nf\ng\nh\ni\nj\nk\nl";
        let new = "a\nB\nc\nd\ne\nf\ng\nh\ni\nj\nk\nL";
        assert_eq!(
            diff("J", old, new),
            "--- J\n+++ J\n@@ -1,5 +1,5 @@\n a\n-b\n+B\n c\n d\n e\n@@ -9,4 +9,4 @@\n i\n j\n k\n-l\n\\ No newline at end of file\n+L\n\\ No newline at end of file\n"
        );
        assert_eq!(diff("J", old, old), "");
        assert_eq!(Mode::parse("all").map(|m| m.applies(Disposition::Safe)), Some(true));
        assert!(!Mode::UnsafeOnly.applies(Disposition::Safe) && Mode::Safe.applies(Disposition::Safe));
        assert_eq!((Mode::parse("some"), Disposition::Unsafe.name()), (None, "unsafe"));
    }

    #[test]
    fn positions() {
        let s = Source::new("ab\r\ncd\n");
        assert_eq!(s.index(Pos { line: 2, col: 2 }), Some(5));
        assert_eq!(s.pos(5), Pos { line: 2, col: 2 });
        assert_eq!(s.index(Pos { line: 9, col: 1 }), None);
        assert_eq!(s.index(Pos { line: 0, col: 1 }), None);
    }
}
