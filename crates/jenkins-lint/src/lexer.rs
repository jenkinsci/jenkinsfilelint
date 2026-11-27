//! Tokenizer for the subset of Groovy used in Jenkinsfiles.
//!
//! It knows every Groovy string form (single, double, triple, slashy and
//! dollar-slashy), comments and numbers. A GString's `$name` and `${...}`
//! placeholders are kept as source text with their positions, for the
//! parser to parse.

use crate::diag::{Diagnostic, Pos, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident(String),
    /// A string literal. `interp` is true for a GString that contains `$`,
    /// and for every dollar-slashy string. `parts` holds its text and
    /// placeholders in order, and is empty when it has no placeholder.
    Str {
        value: String,
        interp: bool,
        parts: Vec<RawPart>,
    },
    Num(String),
    Punct(&'static str),
    Eof,
}

/// A piece of a string literal: text, or a placeholder.
#[derive(Debug, Clone, PartialEq)]
pub enum RawPart {
    Text(String),
    Placeholder(RawPlaceholder),
}

/// A `$name` or `${...}` placeholder of a GString, as source text.
#[derive(Debug, Clone, PartialEq)]
pub struct RawPlaceholder {
    /// The dotted name after `$`, or everything between `${` and `}`.
    pub src: String,
    /// Where `src` starts in the file.
    pub src_pos: Pos,
    /// From the `$` to just past the placeholder.
    pub span: Span,
    /// `${...}` rather than `$name`.
    pub braced: bool,
    /// A line break in it, outside strings it holds and not after a
    /// backslash.
    pub multiline: bool,
}

/// Collects the parts of a string literal while it is read.
#[derive(Default)]
struct Parts {
    parts: Vec<RawPart>,
    text: String,
}

impl Parts {
    fn text(&mut self, c: char) {
        self.text.push(c);
    }

    fn placeholder(&mut self, p: RawPlaceholder) {
        if !self.text.is_empty() {
            self.parts.push(RawPart::Text(std::mem::take(&mut self.text)));
        }
        self.parts.push(RawPart::Placeholder(p));
    }

    /// The parts, or none when there was no placeholder.
    fn finish(mut self) -> Vec<RawPart> {
        if !self.parts.is_empty() && !self.text.is_empty() {
            self.parts.push(RawPart::Text(self.text));
        }
        self.parts
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokKind,
    pub pos: Pos,
    /// True when a newline separates this token from the previous one.
    pub nl_before: bool,
}

impl Token {
    pub fn is_punct(&self, p: &str) -> bool {
        matches!(&self.kind, TokKind::Punct(q) if *q == p)
    }
    pub fn is_ident(&self, name: &str) -> bool {
        matches!(&self.kind, TokKind::Ident(n) if n == name)
    }
    pub fn describe(&self) -> String {
        match &self.kind {
            TokKind::Ident(n) => format!("'{n}'"),
            TokKind::Str { .. } => "string".to_string(),
            TokKind::Num(n) => format!("number {n}"),
            TokKind::Punct(p) => format!("'{p}'"),
            TokKind::Eof => "end of file".to_string(),
        }
    }
}

// Longest operators first so that greedy matching works.
const PUNCTS: &[&str] = &[
    ">>>=", "<..<", "**=", "<<=", ">>=", "<=>", "===", "!==", "==~", "..<", "<..", "...", "?.", "*.", ".&", ".@", "::", "==", "!=", "<=",
    ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "**", "->", "=~", "?:", "?=", "..", "<<", "+", "-", "*",
    "/", "%", "=", "<", ">", "!", "~", "?", ":", ".", ",", ";", "(", ")", "[", "]", "{", "}", "@", "&", "|", "^",
];

/// Keywords after which a `/` starts a slashy string rather than a division.
const REGEX_AFTER_KEYWORDS: &[&str] = &["return", "in", "case", "assert", "throw", "else", "new"];

/// Groovy's message for a `$` in a GString that starts neither `${...}` nor
/// a name.
const ILLEGAL_DOLLAR: &str =
    "illegal string body character after dollar sign; either escape a literal dollar sign \"\\$5\" or bracket the value expression \"${5}\"";

/// What may follow `$` in a GString: Java's identifier start, except `$`.
fn is_identifier_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// A byte order mark at the start of the file. Jenkins does not skip it: its
/// validator reads `\u{feff}pipeline { ... }` as a file with no pipeline block.
const LEADING_BOM: &str = "Unexpected byte order mark (U+FEFF) at the start of the file. \
Save the file as UTF-8 without a BOM: Jenkins reads a Jenkinsfile that starts with one \
as if it had no pipeline block.";

/// An error for a character no token can start with. Invisible ones, such
/// as a no-break space, are named by their code point.
fn unexpected_char(c: char) -> String {
    if c.is_whitespace() || c.is_control() || c == '\u{feff}' {
        format!("Unexpected character U+{:04X}", c as u32)
    } else {
        format!("Unexpected character '{c}'")
    }
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(src).run()
}

/// A `//` comment: where it is and the text after the slashes.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub pos: Pos,
    pub text: String,
}

/// Tokenize a whole file, and keep its `//` comments, which can turn off
/// a finding.
pub fn tokenize_with_comments(src: &str) -> Result<(Vec<Token>, Vec<Comment>), Diagnostic> {
    let mut lexer = Lexer::new(src);
    lexer.lex()?;
    Ok((lexer.out, lexer.comments))
}

/// Tokenize text that starts at `start` in the file, such as what is
/// inside a `${...}` placeholder, so that positions point into the file.
pub fn tokenize_at(src: &str, start: Pos) -> Result<Vec<Token>, Diagnostic> {
    let mut lexer = Lexer::new(src);
    lexer.line = start.line;
    lexer.col = start.col;
    lexer.start_of_file = false;
    lexer.run()
}

struct Lexer<'a> {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    src: &'a str,
    out: Vec<Token>,
    /// Nesting of `${ "..." }` inside GStrings, capped like the parser's.
    interp_depth: usize,
    /// Whether the text is a whole file, where a shebang line or a byte
    /// order mark can come first.
    start_of_file: bool,
    comments: Vec<Comment>,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            chars: src.chars().collect(),
            i: 0,
            line: 1,
            col: 1,
            src,
            out: Vec::new(),
            interp_depth: 0,
            start_of_file: true,
            comments: Vec::new(),
        }
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.i + k).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn pos(&self) -> Pos {
        Pos {
            line: self.line,
            col: self.col,
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(k, c)| self.peek(k) == Some(c))
    }

    fn err(&self, pos: Pos, msg: impl Into<String>) -> Diagnostic {
        Diagnostic::error(pos, msg)
    }

    /// Whether the line before the current one is empty.
    fn after_blank_line(&self) -> bool {
        let before: Vec<char> = self.chars[..self.i].iter().rev().take(3).copied().filter(|&c| c != '\r').collect();
        before.len() >= 2 && before[0] == '\n' && before[1] == '\n'
    }

    /// Whether a `/` at this point begins a slashy string.
    fn regex_allowed(&self) -> bool {
        match self.out.last() {
            None => true,
            Some(t) => match &t.kind {
                TokKind::Ident(n) => REGEX_AFTER_KEYWORDS.contains(&n.as_str()),
                TokKind::Str { .. } | TokKind::Num(_) => false,
                TokKind::Punct(p) => !matches!(*p, ")" | "]" | "}" | "++" | "--"),
                TokKind::Eof => false,
            },
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        self.lex()?;
        Ok(self.out)
    }

    fn lex(&mut self) -> Result<(), Diagnostic> {
        let _ = self.src;
        // Shebang line.
        if self.start_of_file && self.starts_with("#!") {
            while let Some(c) = self.peek(0) {
                if c == '\n' {
                    break;
                }
                self.bump();
            }
        }
        let mut nl = false;
        loop {
            // Skip whitespace and comments, remembering newlines.
            loop {
                match self.peek(0) {
                    Some('\n') => {
                        nl = true;
                        self.bump();
                    }
                    // Groovy separates tokens with spaces, tabs and form
                    // feeds only: a no-break space pasted from a web page
                    // is an error there, so it is one here too.
                    Some(' ' | '\t' | '\x0c' | '\r') => {
                        self.bump();
                    }
                    Some('\\') if self.peek(1) == Some('\n') => {
                        // Line continuation.
                        self.bump();
                        self.bump();
                    }
                    Some('\\') if self.peek(1) == Some('\r') && self.peek(2) == Some('\n') => {
                        // Line continuation in a CRLF file.
                        self.bump();
                        self.bump();
                        self.bump();
                    }
                    Some('/') if self.peek(1) == Some('/') => {
                        let pos = self.pos();
                        self.bump();
                        self.bump();
                        let mut text = String::new();
                        while let Some(c) = self.peek(0) {
                            if c == '\n' {
                                break;
                            }
                            text.push(c);
                            self.bump();
                        }
                        self.comments.push(Comment { pos, text });
                    }
                    Some('/') if self.peek(1) == Some('*') => {
                        let start = self.pos();
                        self.bump();
                        self.bump();
                        loop {
                            if self.starts_with("*/") {
                                self.bump();
                                self.bump();
                                break;
                            }
                            match self.bump() {
                                Some('\n') => nl = true,
                                Some(_) => {}
                                None => return Err(self.err(start, "Unterminated comment")),
                            }
                        }
                    }
                    _ => break,
                }
            }
            let pos = self.pos();
            let Some(c) = self.peek(0) else {
                self.out.push(Token {
                    kind: TokKind::Eof,
                    pos,
                    nl_before: true,
                });
                return Ok(());
            };
            let kind = if c == '\'' || c == '"' {
                self.string(pos)?
            } else if c == '$' && self.peek(1) == Some('/') {
                self.dollar_slashy(pos)?
            } else if c == '/' && pos.col == 1 && pos.line > 1 && !self.after_blank_line() {
                // Groovy 2.4's lexer reads nothing at all here: neither a
                // division nor a slashy string may start a line right after
                // another line, though either may after a blank line or with
                // indentation.
                return Err(self.err(
                    pos,
                    "Unexpected '/' at the start of a line: Groovy cannot read a slashy string or a division there. Indent the line, or end the line before with the operator",
                ));
            } else if c == '/' && (self.regex_allowed() || nl) && self.peek(1) != Some('=') {
                // After a line break, `/` starts a new statement's slashy string.
                self.slashy(pos)?
            } else if c.is_ascii_digit() || (c == '.' && self.peek(1).is_some_and(|d| d.is_ascii_digit()) && !self.prev_is_value()) {
                self.number()
            } else if c.is_alphabetic() || c == '_' || c == '$' {
                let mut s = String::new();
                while let Some(c) = self.peek(0) {
                    if c.is_alphanumeric() || c == '_' || c == '$' {
                        s.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                TokKind::Ident(s)
            } else {
                let Some(p) = PUNCTS.iter().find(|p| self.starts_with(p)) else {
                    let message = if c == '\u{feff}' && self.i == 0 && self.start_of_file {
                        LEADING_BOM.to_string()
                    } else {
                        unexpected_char(c)
                    };
                    return Err(self.err(pos, message));
                };
                for _ in 0..p.chars().count() {
                    self.bump();
                }
                TokKind::Punct(p)
            };
            self.out.push(Token { kind, pos, nl_before: nl });
            nl = false;
        }
    }

    fn prev_is_value(&self) -> bool {
        matches!(
            self.out.last().map(|t| &t.kind),
            Some(TokKind::Ident(_))
                | Some(TokKind::Num(_))
                | Some(TokKind::Str { .. })
                | Some(TokKind::Punct(")"))
                | Some(TokKind::Punct("]"))
        )
    }

    fn number(&mut self) -> TokKind {
        let mut s = String::new();
        if self.starts_with("0x") || self.starts_with("0X") || self.starts_with("0b") || self.starts_with("0B") {
            s.push(self.bump().unwrap());
            s.push(self.bump().unwrap());
            while let Some(c) = self.peek(0) {
                if c.is_ascii_hexdigit() || c == '_' {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        } else {
            while let Some(c) = self.peek(0) {
                let exp_sign = (c == '+' || c == '-') && matches!(s.chars().last(), Some('e') | Some('E'));
                let decimal_point = c == '.' && self.peek(1).is_some_and(|d| d.is_ascii_digit()) && !s.contains('.');
                if c.is_ascii_digit() || c == '_' || exp_sign || c == 'e' || c == 'E' || decimal_point {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
        }
        if let Some(c) = self.peek(0) {
            if "gGlLiIdDfF".contains(c) {
                s.push(c);
                self.bump();
            }
        }
        // `1BAR` is not Groovy, but keeping it as one name lets the
        // validator explain why it is not a valid identifier.
        if self.peek(0).is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            while let Some(c) = self.peek(0) {
                if c.is_alphanumeric() || c == '_' || c == '$' {
                    s.push(c);
                    self.bump();
                } else {
                    break;
                }
            }
            return TokKind::Ident(s);
        }
        TokKind::Num(s)
    }

    /// An escape in a quoted string, at its backslash: the character it
    /// stands for, or None for a backslash before a line break, which
    /// continues the string on the next line, even in quotes that do not
    /// span lines. Groovy accepts no other escapes: `"\|"` and `'\s'` do
    /// not compile.
    fn escape(&mut self) -> Result<Option<char>, Diagnostic> {
        let at = self.pos();
        self.bump();
        let Some(e) = self.peek(0) else {
            return Ok(None);
        };
        let c = match e {
            'n' => '\n',
            't' => '\t',
            'r' => '\r',
            'b' => '\u{8}',
            'f' => '\u{c}',
            '\\' | '\'' | '"' | '$' => e,
            '\n' => {
                self.bump();
                return Ok(None);
            }
            '\r' if self.peek(1) == Some('\n') => {
                self.bump();
                self.bump();
                return Ok(None);
            }
            // Octal: up to three digits, the first of three at most 3.
            '0'..='7' => {
                let max = if e <= '3' { 3 } else { 2 };
                let mut code = 0;
                let mut n = 0;
                while n < max && self.peek(0).is_some_and(|d| ('0'..='7').contains(&d)) {
                    code = code * 8 + self.bump().unwrap().to_digit(8).unwrap();
                    n += 1;
                }
                return Ok(char::from_u32(code));
            }
            // `\u0041`, also with more than one u.
            'u' => {
                while self.peek(0) == Some('u') {
                    self.bump();
                }
                let hex: String = (0..4).filter_map(|k| self.peek(k)).take_while(|d| d.is_ascii_hexdigit()).collect();
                if hex.len() < 4 {
                    return Err(self.err(at, "Invalid unicode escape in a string: \\u needs four hexadecimal digits"));
                }
                for _ in 0..4 {
                    self.bump();
                }
                return Ok(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).or(Some('\u{fffd}')));
            }
            other => {
                return Err(self.err(
                    at,
                    format!(
                        "Unexpected escape '\\{other}' in a string. Groovy knows only \\b \\f \\n \\r \\t \\\\ \\' \\\" \\$, octal and \\u escapes: write '\\\\' for a backslash"
                    ),
                ))
            }
        };
        self.bump();
        Ok(Some(c))
    }

    fn string(&mut self, start: Pos) -> Result<TokKind, Diagnostic> {
        let q = self.bump().unwrap();
        let triple = self.peek(0) == Some(q) && self.peek(1) == Some(q);
        if triple {
            self.bump();
            self.bump();
        } else if self.peek(0) == Some(q) {
            // Empty string.
            self.bump();
            return Ok(TokKind::Str {
                value: String::new(),
                interp: false,
                parts: Vec::new(),
            });
        }
        let gstring = q == '"';
        let mut value = String::new();
        let mut interp = false;
        let mut parts = Parts::default();
        loop {
            let Some(c) = self.peek(0) else {
                return Err(self.err(start, "Unterminated string literal"));
            };
            if triple {
                if c == q && self.peek(1) == Some(q) && self.peek(2) == Some(q) {
                    self.bump();
                    self.bump();
                    self.bump();
                    break;
                }
            } else if c == q {
                self.bump();
                break;
            } else if c == '\n' {
                return Err(self.err(start, "Unterminated string literal"));
            }
            if c == '\\' {
                if let Some(c) = self.escape()? {
                    value.push(c);
                    parts.text(c);
                }
                continue;
            }
            if gstring && c == '$' {
                if self.peek(1) == Some('{') {
                    interp = true;
                    let placeholder = self.braced_placeholder(start, &mut value)?;
                    // Only a triple-quoted string spans lines, `${...}` included,
                    // unless a backslash continues the line.
                    if !triple && placeholder.multiline {
                        return Err(self.err(
                            start,
                            "Unterminated string literal: the line ends inside its ${...}. Use a triple-quoted string to span lines",
                        ));
                    }
                    parts.placeholder(placeholder);
                    continue;
                }
                // Like Groovy, a `$` must start `${...}` or a name: `"$(date)"`,
                // `"$1"` and `"cost: $5"` do not compile.
                if !self.peek(1).is_some_and(is_identifier_start) {
                    return Err(self.err(self.pos(), ILLEGAL_DOLLAR));
                }
                interp = true;
                parts.placeholder(self.simple_placeholder(&mut value));
                continue;
            }
            value.push(c);
            parts.text(c);
            self.bump();
        }
        Ok(TokKind::Str {
            value,
            interp,
            parts: parts.finish(),
        })
    }

    /// At `${`: read the placeholder and add its text to `value`.
    fn braced_placeholder(&mut self, start: Pos, value: &mut String) -> Result<RawPlaceholder, Diagnostic> {
        let begin = self.pos();
        let begin_i = self.i;
        let multiline = self.interpolation(start)?;
        // Inside another placeholder, a string is read only to find where
        // that placeholder ends; its value and parts are dropped, and the
        // parser reads the text again. Copying the text there would make
        // deep nesting quadratic.
        let src = if self.interp_depth == 0 {
            value.extend(&self.chars[begin_i..self.i]);
            self.chars[begin_i + 2..self.i - 1].iter().collect()
        } else {
            String::new()
        };
        Ok(RawPlaceholder {
            src,
            src_pos: Pos {
                line: begin.line,
                col: begin.col + 2,
            },
            span: Span {
                start: begin,
                end: self.pos(),
            },
            braced: true,
            multiline,
        })
    }

    /// At `$` and a name: read `$name`, and `.name` after it, as Groovy
    /// does: `"$a.b"` is property `b` of `a`, and the `.` in `"$a."` and
    /// `"$a.1"` is text. A name in a GString has no `$` in it.
    fn simple_placeholder(&mut self, value: &mut String) -> RawPlaceholder {
        let begin = self.pos();
        value.push(self.bump().unwrap());
        let src_pos = self.pos();
        let mut src = String::new();
        loop {
            while let Some(c) = self.peek(0).filter(|c| c.is_alphanumeric() || *c == '_') {
                src.push(c);
                value.push(c);
                self.bump();
            }
            if self.peek(0) == Some('.') && self.peek(1).is_some_and(is_identifier_start) {
                src.push('.');
                value.push('.');
                self.bump();
            } else {
                break;
            }
        }
        RawPlaceholder {
            src,
            src_pos,
            span: Span {
                start: begin,
                end: self.pos(),
            },
            braced: false,
            multiline: false,
        }
    }

    /// Consume `${ ... }` inside a GString, and tell whether a line breaks
    /// in it, outside the strings and comments it holds and not after a
    /// backslash.
    fn interpolation(&mut self, start: Pos) -> Result<bool, Diagnostic> {
        if self.interp_depth >= crate::parser::MAX_DEPTH {
            return Err(self.err(
                self.pos(),
                format!("Nesting is too deep to check (more than {} levels)", crate::parser::MAX_DEPTH),
            ));
        }
        self.interp_depth += 1;
        let result = self.interpolation_inner(start);
        self.interp_depth -= 1;
        result
    }

    fn interpolation_inner(&mut self, start: Pos) -> Result<bool, Diagnostic> {
        self.bump(); // $
        self.bump(); // {
        let mut depth = 1;
        let mut multiline = false;
        // The last meaningful character, to tell `/` as division (after a
        // value) from a slashy string (after an operator or delimiter).
        let mut prev = '{';
        while depth > 0 {
            let Some(c) = self.peek(0) else {
                return Err(self.err(start, "Unterminated ${...} in string"));
            };
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                '\'' | '"' => {
                    let p = self.pos();
                    self.string(p)?;
                    prev = '"';
                    continue;
                }
                '/' if self.peek(1) == Some('/') => {
                    // A line comment; a `}` in it does not end the `${...}`.
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                    continue;
                }
                '/' if self.peek(1) == Some('*') => {
                    // A comment does not change what the next `/` means.
                    while !self.starts_with("*/") {
                        if self.bump().is_none() {
                            return Err(self.err(start, "Unterminated ${...} in string"));
                        }
                    }
                    self.bump();
                    self.bump();
                    continue;
                }
                '/' if !prev.is_alphanumeric() && !matches!(prev, ')' | ']' | '_' | '"') => {
                    // A slashy string, as in `${m[/"/]}`; a `}` or quote in
                    // it does not end the `${...}`.
                    let p = self.pos();
                    self.slashy(p)?;
                    prev = '"';
                    continue;
                }
                '\\' if self.peek(1) == Some('\n') || (self.peek(1) == Some('\r') && self.peek(2) == Some('\n')) => {
                    // A backslash continues the line.
                    while self.bump() != Some('\n') {}
                    continue;
                }
                '\n' => multiline = true,
                _ => {}
            }
            if !c.is_whitespace() {
                prev = c;
            }
            self.bump();
        }
        Ok(multiline)
    }

    fn slashy(&mut self, start: Pos) -> Result<TokKind, Diagnostic> {
        self.bump();
        let mut value = String::new();
        let mut interp = false;
        let mut parts = Parts::default();
        loop {
            match self.peek(0) {
                None => return Err(self.err(start, "Unterminated slashy string")),
                Some('/') => {
                    self.bump();
                    break;
                }
                Some('\\') if self.peek(1) == Some('/') => {
                    self.bump();
                    self.bump();
                    value.push('/');
                    parts.text('/');
                }
                Some('$') if self.peek(1) == Some('{') => {
                    interp = true;
                    parts.placeholder(self.braced_placeholder(start, &mut value)?);
                }
                // A `$` before anything but a name, as in `/^v\d+$/`, is text.
                Some('$') if self.peek(1).is_some_and(is_identifier_start) => {
                    interp = true;
                    parts.placeholder(self.simple_placeholder(&mut value));
                }
                Some(c) => {
                    value.push(c);
                    parts.text(c);
                    self.bump();
                }
            }
        }
        Ok(TokKind::Str {
            value,
            interp,
            parts: parts.finish(),
        })
    }

    fn dollar_slashy(&mut self, start: Pos) -> Result<TokKind, Diagnostic> {
        self.bump();
        self.bump();
        let mut value = String::new();
        let mut parts = Parts::default();
        loop {
            if self.starts_with("/$") {
                self.bump();
                self.bump();
                break;
            }
            if self.starts_with("$$") || self.starts_with("$/") {
                self.bump();
                let c = self.bump().unwrap();
                value.push(c);
                parts.text(c);
                continue;
            }
            if self.starts_with("${") {
                parts.placeholder(self.braced_placeholder(start, &mut value)?);
                continue;
            }
            if self.peek(0) == Some('$') && self.peek(1).is_some_and(is_identifier_start) {
                parts.placeholder(self.simple_placeholder(&mut value));
                continue;
            }
            match self.bump() {
                Some(c) => {
                    value.push(c);
                    parts.text(c);
                }
                None => return Err(self.err(start, "Unterminated dollar-slashy string")),
            }
        }
        Ok(TokKind::Str {
            value,
            interp: true,
            parts: parts.finish(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokKind> {
        tokenize(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn strings() {
        let k = kinds("sh 'a' \"b ${x + '}'} c\"");
        assert_eq!(k[0], TokKind::Ident("sh".into()));
        assert_eq!(
            k[1],
            TokKind::Str {
                value: "a".into(),
                interp: false,
                parts: vec![]
            }
        );
        let TokKind::Str { value, interp, parts } = &k[2] else {
            panic!("{:?}", k[2]);
        };
        assert_eq!((value.as_str(), *interp), ("b ${x + '}'} c", true));
        assert_eq!(parts.len(), 3);
        assert_eq!(k[3], TokKind::Eof);
    }

    /// The text and placeholders of each GString form, where they are in
    /// the file, and what text goes to the parser.
    fn parts(src: &str) -> Vec<RawPart> {
        match tokenize(src).unwrap().remove(0).kind {
            TokKind::Str { parts, .. } => parts,
            other => panic!("{other:?}"),
        }
    }

    fn text(t: &str) -> RawPart {
        RawPart::Text(t.into())
    }

    fn placeholder(src: &str, braced: bool, (line, col): (usize, usize), end: (usize, usize)) -> RawPart {
        let start = Pos { line, col };
        RawPart::Placeholder(RawPlaceholder {
            src: src.into(),
            src_pos: Pos {
                line,
                col: col + if braced { 2 } else { 1 },
            },
            span: Span {
                start,
                end: Pos { line: end.0, col: end.1 },
            },
            braced,
            multiline: src.contains('\n'),
        })
    }

    #[test]
    fn gstring_parts() {
        assert_eq!(
            parts("\"a ${x} $y.z. c\\$d\""),
            vec![
                text("a "),
                placeholder("x", true, (1, 4), (1, 8)),
                text(" "),
                placeholder("y.z", false, (1, 9), (1, 13)),
                text(". c$d"),
            ]
        );
        // `$a$b` is two names; `$a.1` and `$a.b()` stop before what is not a name.
        assert_eq!(
            parts("\"$a$b $c.1 $d.e()\""),
            vec![
                placeholder("a", false, (1, 2), (1, 4)),
                placeholder("b", false, (1, 4), (1, 6)),
                text(" "),
                placeholder("c", false, (1, 7), (1, 9)),
                text(".1 "),
                placeholder("d.e", false, (1, 12), (1, 16)),
                text("()"),
            ]
        );
        // Placeholder text comes from the file, comments and lines included.
        assert_eq!(
            parts("\"\"\"x ${\n  a /* } */\n} y\"\"\""),
            vec![text("x "), placeholder("\n  a /* } */\n", true, (1, 6), (3, 2)), text(" y"),]
        );
        assert_eq!(
            parts("/a ${b} $c $/"),
            vec![
                text("a "),
                placeholder("b", true, (1, 4), (1, 8)),
                text(" "),
                placeholder("c", false, (1, 9), (1, 11)),
                text(" $")
            ]
        );
        assert_eq!(
            parts("$/a $b ${c} $$ $/ x/$"),
            vec![
                text("a "),
                placeholder("b", false, (1, 5), (1, 7)),
                text(" "),
                placeholder("c", true, (1, 8), (1, 12)),
                text(" $ / x")
            ]
        );
        // No placeholder, no parts, even when interpolation is possible.
        assert!(parts("\"plain\"").is_empty());
        assert!(parts("'$a ${b}'").is_empty());
        assert!(parts("/^v\\d+$/").is_empty());
        assert!(parts("$/plain $$/$").is_empty());
    }

    #[test]
    fn tokenize_at_keeps_file_positions() {
        let toks = tokenize_at("a\n  b", Pos { line: 7, col: 30 }).unwrap();
        assert_eq!((toks[0].pos.line, toks[0].pos.col), (7, 30));
        assert_eq!((toks[1].pos.line, toks[1].pos.col), (8, 3));
        // Only a whole file may start with a shebang line or a BOM.
        assert!(tokenize_at("#!x", Pos { line: 1, col: 1 }).is_err());
        let e = tokenize_at("\u{feff}a", Pos { line: 1, col: 1 }).expect_err("BOM");
        assert_eq!(e.message, "Unexpected character U+FEFF");
    }

    #[test]
    fn slashy_vs_division() {
        let k = kinds("x = a / b; y = /re+/");
        assert!(k.contains(&TokKind::Punct("/")));
        assert!(k.contains(&TokKind::Str {
            value: "re+".into(),
            interp: false,
            parts: vec![]
        }));
    }

    #[test]
    fn dollar_in_gstrings() {
        for ok in [
            "\"$x and ${y} and \\$5 and $_z\"",
            "'$(date) $1 $'",
            "/^v\\d+$/",
            "$/a $ b $$ c/$",
            "\"\"\"${x}\n$y\"\"\"",
        ] {
            assert!(tokenize(ok).is_ok(), "{ok}");
        }
        for bad in [
            "\"echo $(date)\"",
            "\"$1\"",
            "\"cost: $5\"",
            "\"a $\"",
            "\"a$$b\"",
            "\"\"\"x $\n\"\"\"",
        ] {
            let e = tokenize(bad).expect_err(bad);
            assert!(
                e.message.starts_with("illegal string body character after dollar sign"),
                "{bad}: {}",
                e.message
            );
        }
        // The error points at the `$`.
        assert_eq!(tokenize("x = \"a $(b)\"").unwrap_err().pos, Pos { line: 1, col: 8 });
    }

    #[test]
    fn only_groovy_whitespace_separates_tokens() {
        assert!(tokenize("a \t\x0c\r\nb").is_ok());
        for (src, name) in [("echo\u{a0}'x'", "U+00A0"), ("a\u{2003}b", "U+2003"), ("a\x0bb", "U+000B")] {
            let e = tokenize(src).expect_err(src);
            assert_eq!(e.message, format!("Unexpected character {name}"));
        }
        // Inside strings and comments anything goes.
        assert!(tokenize("x = 'a\u{a0}b' // c\u{a0}d").is_ok());
    }

    #[test]
    fn byte_order_mark() {
        let e = tokenize("\u{feff}pipeline { }").expect_err("leading BOM");
        assert_eq!((e.pos.line, e.pos.col), (1, 1));
        assert!(
            e.message.starts_with("Unexpected byte order mark (U+FEFF) at the start"),
            "{}",
            e.message
        );
        let e = tokenize("a \u{feff}b").expect_err("BOM inside");
        assert_eq!(e.message, "Unexpected character U+FEFF");
    }

    #[test]
    fn newline_flag() {
        let toks = tokenize("a\nb").unwrap();
        assert!(!toks[0].nl_before);
        assert!(toks[1].nl_before);
    }
}
