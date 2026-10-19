//! Tokenizer for the subset of Groovy used in Jenkinsfiles.
//!
//! It knows every Groovy string form (single, double, triple, slashy and
//! dollar-slashy), comments and numbers. GString interpolation is kept as
//! raw text: the declarative checks only need to know that a string is
//! interpolated, not what is inside it.

use crate::diag::{Diagnostic, Pos};

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident(String),
    /// A string literal. `interp` is true for a GString that contains `$`.
    Str {
        value: String,
        interp: bool,
    },
    Num(String),
    Punct(&'static str),
    Eof,
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

pub fn tokenize(src: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(src).run()
}

struct Lexer<'a> {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
    src: &'a str,
    out: Vec<Token>,
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
        let _ = self.src;
        // Shebang line.
        if self.starts_with("#!") {
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
                    Some(c) if c.is_whitespace() => {
                        self.bump();
                    }
                    Some('\\') if self.peek(1) == Some('\n') => {
                        // Line continuation.
                        self.bump();
                        self.bump();
                    }
                    Some('/') if self.peek(1) == Some('/') => {
                        while let Some(c) = self.peek(0) {
                            if c == '\n' {
                                break;
                            }
                            self.bump();
                        }
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
                return Ok(self.out);
            };
            let kind = if c == '\'' || c == '"' {
                self.string(pos)?
            } else if c == '$' && self.peek(1) == Some('/') {
                self.dollar_slashy(pos)?
            } else if c == '/' && self.regex_allowed() && self.peek(1) != Some('=') {
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
                    return Err(self.err(pos, format!("Unexpected character '{c}'")));
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
            });
        }
        let gstring = q == '"';
        let mut value = String::new();
        let mut interp = false;
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
                self.bump();
                if let Some(e) = self.bump() {
                    value.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        other => other,
                    });
                }
                continue;
            }
            if gstring && c == '$' {
                if self.peek(1) == Some('{') {
                    interp = true;
                    value.push_str(&self.interpolation(start)?);
                    continue;
                }
                if self.peek(1).is_some_and(|d| d.is_alphabetic() || d == '_') {
                    interp = true;
                }
            }
            value.push(c);
            self.bump();
        }
        Ok(TokKind::Str { value, interp })
    }

    /// Consume `${ ... }` inside a GString and return its raw text.
    fn interpolation(&mut self, start: Pos) -> Result<String, Diagnostic> {
        let mut text = String::new();
        text.push(self.bump().unwrap()); // $
        text.push(self.bump().unwrap()); // {
        let mut depth = 1;
        while depth > 0 {
            let Some(c) = self.peek(0) else {
                return Err(self.err(start, "Unterminated ${...} in string"));
            };
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                '\'' | '"' => {
                    let p = self.pos();
                    let before = self.i;
                    self.string(p)?;
                    text.extend(&self.chars[before..self.i]);
                    continue;
                }
                _ => {}
            }
            text.push(c);
            self.bump();
        }
        Ok(text)
    }

    fn slashy(&mut self, start: Pos) -> Result<TokKind, Diagnostic> {
        self.bump();
        let mut value = String::new();
        let mut interp = false;
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
                }
                Some('$') if self.peek(1) == Some('{') => {
                    interp = true;
                    value.push_str(&self.interpolation(start)?);
                }
                Some(c) => {
                    value.push(c);
                    self.bump();
                }
            }
        }
        Ok(TokKind::Str { value, interp })
    }

    fn dollar_slashy(&mut self, start: Pos) -> Result<TokKind, Diagnostic> {
        self.bump();
        self.bump();
        let mut value = String::new();
        loop {
            if self.starts_with("/$") {
                self.bump();
                self.bump();
                break;
            }
            if self.starts_with("$$") || self.starts_with("$/") {
                self.bump();
                value.push(self.bump().unwrap());
                continue;
            }
            match self.bump() {
                Some(c) => value.push(c),
                None => return Err(self.err(start, "Unterminated dollar-slashy string")),
            }
        }
        Ok(TokKind::Str { value, interp: true })
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
        assert_eq!(
            kinds("sh 'a' \"b ${x + '}'} c\""),
            vec![
                TokKind::Ident("sh".into()),
                TokKind::Str {
                    value: "a".into(),
                    interp: false
                },
                TokKind::Str {
                    value: "b ${x + '}'} c".into(),
                    interp: true
                },
                TokKind::Eof
            ]
        );
    }

    #[test]
    fn slashy_vs_division() {
        let k = kinds("x = a / b; y = /re+/");
        assert!(k.contains(&TokKind::Punct("/")));
        assert!(k.contains(&TokKind::Str {
            value: "re+".into(),
            interp: false
        }));
    }

    #[test]
    fn newline_flag() {
        let toks = tokenize("a\nb").unwrap();
        assert!(!toks[0].nl_before);
        assert!(toks[1].nl_before);
    }
}
