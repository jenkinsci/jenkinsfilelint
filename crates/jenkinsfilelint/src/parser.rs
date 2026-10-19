//! Recursive-descent parser for Groovy as written in Jenkinsfiles.
//!
//! The parser is deliberately lenient about Groovy features that never
//! matter for declarative validation (class bodies, switch bodies, for
//! headers): it checks that their brackets balance and moves on. It is
//! strict about the shapes Jenkins users write every day: command calls
//! with named arguments, trailing closures, GStrings, maps and lists.

use crate::ast::{Arg, Expr, Stmt};
use crate::diag::{Diagnostic, Pos};
use crate::lexer::{TokKind, Token};

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, Diagnostic> {
    let mut p = Parser { toks: tokens, i: 0 };
    let stmts = p.stmts_until(None)?;
    Ok(stmts)
}

const MODIFIERS: &[&str] = &[
    "final",
    "static",
    "private",
    "public",
    "protected",
    "abstract",
    "synchronized",
    "transient",
    "volatile",
];
const PRIMITIVES: &[&str] = &["int", "long", "short", "byte", "char", "boolean", "float", "double", "void"];
const ASSIGN_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "**=", "<<=", ">>=", ">>>=", "&=", "|=", "^=", "?=",
];
/// Identifiers that act as operators or statements and so cannot start a
/// command-call argument.
const NON_ARG_IDENTS: &[&str] = &["in", "instanceof", "as", "else", "catch", "finally"];

struct Parser {
    toks: Vec<Token>,
    i: usize,
}

type PResult<T> = Result<T, Diagnostic>;

impl Parser {
    fn peek(&self) -> &Token {
        &self.toks[self.i.min(self.toks.len() - 1)]
    }
    fn peek_at(&self, k: usize) -> &Token {
        &self.toks[(self.i + k).min(self.toks.len() - 1)]
    }
    fn next(&mut self) -> Token {
        let t = self.peek().clone();
        if self.i < self.toks.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn at_eof(&self) -> bool {
        matches!(self.peek().kind, TokKind::Eof)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.peek().is_punct(p) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, p: &str) -> PResult<Token> {
        if self.peek().is_punct(p) {
            Ok(self.next())
        } else {
            Err(self.unexpected(&format!("'{p}'")))
        }
    }
    fn unexpected(&self, wanted: &str) -> Diagnostic {
        let t = self.peek();
        if matches!(t.kind, TokKind::Eof) {
            Diagnostic::error(t.pos, format!("Unexpected end of file, expected {wanted}"))
        } else {
            Diagnostic::error(t.pos, format!("Unexpected {}, expected {wanted}", t.describe()))
        }
    }
    fn ident_name(&self) -> Option<String> {
        match &self.peek().kind {
            TokKind::Ident(n) => Some(n.clone()),
            _ => None,
        }
    }

    /// Consume a bracketed group starting at the current `open` token.
    fn skip_balanced(&mut self, open: &str, close: &str) -> PResult<()> {
        let start = self.expect(open)?.pos;
        let mut depth = 1;
        while depth > 0 {
            if self.at_eof() {
                return Err(Diagnostic::error(start, format!("Unclosed '{open}'")));
            }
            let t = self.next();
            if t.is_punct(open) {
                depth += 1;
            } else if t.is_punct(close) {
                depth -= 1;
            }
        }
        Ok(())
    }

    // ---------------------------------------------------------------
    // Statements
    // ---------------------------------------------------------------

    /// Parse statements until `}` (when `close` is Some) or end of file.
    fn stmts_until(&mut self, close: Option<Pos>) -> PResult<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            while self.eat(";") {}
            if self.peek().is_punct("}") {
                if close.is_some() {
                    return Ok(out);
                }
                return Err(Diagnostic::error(self.peek().pos, "Unexpected '}' with no matching '{'"));
            }
            if self.at_eof() {
                if let Some(open) = close {
                    return Err(Diagnostic::error(open, "Unclosed '{' - missing '}'"));
                }
                return Ok(out);
            }
            let stmt = self.stmt()?;
            out.push(stmt);
            let t = self.peek();
            if !(t.nl_before || t.is_punct(";") || t.is_punct("}") || matches!(t.kind, TokKind::Eof)) {
                return Err(self.unexpected("end of statement"));
            }
        }
    }

    fn block(&mut self) -> PResult<Vec<Stmt>> {
        let open = self.expect("{")?.pos;
        let body = self.stmts_until(Some(open))?;
        self.expect("}")?;
        Ok(body)
    }

    /// Body of if/for/while: a block or a single statement.
    fn body(&mut self) -> PResult<Vec<Stmt>> {
        if self.peek().is_punct("{") {
            self.block()
        } else {
            Ok(vec![self.stmt()?])
        }
    }

    fn annotation(&mut self) -> PResult<()> {
        self.expect("@")?;
        self.qualified_name()?;
        if self.peek().is_punct("(") && !self.peek().nl_before {
            self.skip_balanced("(", ")")?;
        }
        Ok(())
    }

    fn qualified_name(&mut self) -> PResult<String> {
        let mut name = match self.next().kind {
            TokKind::Ident(n) => n,
            _ => {
                self.i -= 1;
                return Err(self.unexpected("a name"));
            }
        };
        while self.peek().is_punct(".") {
            if let TokKind::Ident(n) = &self.peek_at(1).kind {
                name.push('.');
                name.push_str(n);
                self.i += 2;
            } else {
                break;
            }
        }
        Ok(name)
    }

    fn rest_of_line(&mut self) {
        self.next();
        while !self.peek().nl_before && !self.peek().is_punct(";") && !self.at_eof() {
            self.next();
        }
    }

    fn stmt(&mut self) -> PResult<Stmt> {
        let t = self.peek().clone();
        let pos = t.pos;
        if t.is_punct("@") {
            self.annotation()?;
            while self.eat(";") {}
            if self.at_eof() || self.peek().is_punct("}") {
                return Ok(Stmt::Other { kind: "annotation", pos });
            }
            return self.stmt();
        }
        if t.is_punct("{") {
            // A bare block or closure statement.
            let e = self.closure()?;
            return Ok(Stmt::Expr(e));
        }
        let Some(word) = self.ident_name() else {
            return Ok(Stmt::Expr(self.expr_stmt()?));
        };
        match word.as_str() {
            "import" | "package" => {
                self.rest_of_line();
                Ok(Stmt::Other { kind: "import", pos })
            }
            "class" | "interface" | "enum" | "trait" => self.type_decl(pos),
            "if" => {
                self.next();
                self.expect("(")?;
                self.expr()?;
                self.expect(")")?;
                let mut bodies = vec![self.body()?];
                if self.peek().is_ident("else") {
                    self.next();
                    bodies.push(self.body()?);
                }
                Ok(Stmt::Control { kind: "if", bodies, pos })
            }
            "for" | "while" => {
                self.next();
                self.skip_balanced("(", ")")?;
                let bodies = vec![self.body()?];
                Ok(Stmt::Control { kind: "loop", bodies, pos })
            }
            "do" => {
                self.next();
                let bodies = vec![self.body()?];
                if !self.peek().is_ident("while") {
                    return Err(self.unexpected("'while'"));
                }
                self.next();
                self.skip_balanced("(", ")")?;
                Ok(Stmt::Control { kind: "loop", bodies, pos })
            }
            "try" => {
                self.next();
                let mut bodies = Vec::new();
                if self.peek().is_punct("(") {
                    self.skip_balanced("(", ")")?;
                }
                bodies.push(self.block()?);
                let mut handled = false;
                while self.peek().is_ident("catch") {
                    self.next();
                    self.skip_balanced("(", ")")?;
                    bodies.push(self.block()?);
                    handled = true;
                }
                if self.peek().is_ident("finally") {
                    self.next();
                    bodies.push(self.block()?);
                    handled = true;
                }
                if !handled {
                    return Err(self.unexpected("'catch' or 'finally'"));
                }
                Ok(Stmt::Control { kind: "try", bodies, pos })
            }
            "switch" => {
                self.next();
                self.skip_balanced("(", ")")?;
                self.skip_balanced("{", "}")?;
                Ok(Stmt::Control {
                    kind: "switch",
                    bodies: vec![],
                    pos,
                })
            }
            "synchronized" if self.peek_at(1).is_punct("(") => {
                self.next();
                self.skip_balanced("(", ")")?;
                let bodies = vec![self.block()?];
                Ok(Stmt::Control {
                    kind: "synchronized",
                    bodies,
                    pos,
                })
            }
            "return" | "throw" => {
                self.next();
                let n = self.peek();
                if !(n.nl_before || n.is_punct(";") || n.is_punct("}") || matches!(n.kind, TokKind::Eof)) {
                    self.expr()?;
                }
                Ok(Stmt::Other { kind: "return", pos })
            }
            "break" | "continue" => {
                self.next();
                if matches!(self.peek().kind, TokKind::Ident(_)) && !self.peek().nl_before {
                    self.next();
                }
                Ok(Stmt::Other { kind: "break", pos })
            }
            "assert" => {
                self.next();
                self.expr()?;
                if self.eat(":") || self.eat(",") {
                    self.expr()?;
                }
                Ok(Stmt::Other { kind: "assert", pos })
            }
            _ if word == "def" || MODIFIERS.contains(&word.as_str()) => self.declaration(pos),
            _ if self.looks_like_typed_decl() => self.declaration(pos),
            _ if self.peek_at(1).is_punct(":") && !self.peek_at(2).nl_before && word != "default" => {
                // Statement label, e.g. `outer: for (...)`.
                self.next();
                self.next();
                self.stmt()
            }
            _ => Ok(Stmt::Expr(self.expr_stmt()?)),
        }
    }

    fn type_decl(&mut self, pos: Pos) -> PResult<Stmt> {
        while !self.peek().is_punct("{") {
            if self.at_eof() {
                return Err(self.unexpected("'{'"));
            }
            self.next();
        }
        self.skip_balanced("{", "}")?;
        Ok(Stmt::Other { kind: "class", pos })
    }

    /// `String x = ...`, `List<String> xs`, `void call() {}`.
    fn looks_like_typed_decl(&self) -> bool {
        let TokKind::Ident(ty) = &self.peek().kind else { return false };
        let typeish = PRIMITIVES.contains(&ty.as_str()) || ty.chars().next().is_some_and(|c| c.is_uppercase());
        if !typeish {
            return false;
        }
        let mut k = 1;
        // Qualified type name.
        while self.peek_at(k).is_punct(".") && matches!(self.peek_at(k + 1).kind, TokKind::Ident(_)) {
            k += 2;
        }
        // Generic arguments.
        if self.peek_at(k).is_punct("<") {
            let mut depth = 0;
            loop {
                let t = self.peek_at(k);
                if t.is_punct("<") {
                    depth += 1;
                } else if t.is_punct(">") {
                    depth -= 1;
                } else if t.is_punct(">>=") || matches!(t.kind, TokKind::Eof) || t.is_punct("{") || t.is_punct(";") {
                    return false;
                }
                k += 1;
                if depth == 0 {
                    break;
                }
            }
        }
        // Array type.
        while self.peek_at(k).is_punct("[") && self.peek_at(k + 1).is_punct("]") {
            k += 2;
        }
        let name = self.peek_at(k);
        if !matches!(name.kind, TokKind::Ident(_)) || name.nl_before {
            return false;
        }
        let after = self.peek_at(k + 1);
        after.is_punct("=") || after.is_punct("(") || after.is_punct(",") || after.is_punct(";") || after.nl_before
    }

    fn skip_type(&mut self) -> PResult<()> {
        self.qualified_name()?;
        if self.peek().is_punct("<") {
            let mut depth = 0;
            loop {
                let t = self.next();
                if t.is_punct("<") {
                    depth += 1;
                } else if t.is_punct(">") {
                    depth -= 1;
                } else if matches!(t.kind, TokKind::Eof) {
                    return Err(Diagnostic::error(t.pos, "Unclosed generic type"));
                }
                if depth == 0 {
                    break;
                }
            }
        }
        while self.peek().is_punct("[") && self.peek_at(1).is_punct("]") {
            self.next();
            self.next();
        }
        Ok(())
    }

    fn declaration(&mut self, pos: Pos) -> PResult<Stmt> {
        let mut saw_def = false;
        while let Some(w) = self.ident_name() {
            if w == "def" || MODIFIERS.contains(&w.as_str()) {
                saw_def = true;
                self.next();
            } else {
                break;
            }
        }
        // Multiple assignment: def (a, b) = [1, 2]
        if self.peek().is_punct("(") {
            self.skip_balanced("(", ")")?;
            self.expect("=")?;
            let value = self.expr()?;
            return Ok(Stmt::Decl {
                names: vec![],
                value: Some(value),
                pos,
            });
        }
        // Optional type: present when two names follow each other.
        let two_names = matches!(self.peek().kind, TokKind::Ident(_))
            && (matches!(self.peek_at(1).kind, TokKind::Ident(_)) && !self.peek_at(1).nl_before
                || self.peek_at(1).is_punct("<")
                || self.peek_at(1).is_punct(".")
                || (self.peek_at(1).is_punct("[") && self.peek_at(2).is_punct("]")));
        if two_names || !saw_def {
            self.skip_type()?;
        }
        let name = match self.next().kind {
            TokKind::Ident(n) => n,
            TokKind::Str { value, .. } => value,
            _ => {
                self.i -= 1;
                return Err(self.unexpected("a name"));
            }
        };
        if self.peek().is_punct("(") {
            // Method definition.
            self.skip_balanced("(", ")")?;
            if self.peek().is_ident("throws") {
                self.next();
                self.skip_type()?;
                while self.eat(",") {
                    self.skip_type()?;
                }
            }
            let body = self.block()?;
            return Ok(Stmt::MethodDef { name, body, pos });
        }
        let mut names = vec![name];
        let mut value = None;
        loop {
            if self.eat("=") {
                value = Some(self.expr_stmt()?);
            }
            if !self.eat(",") {
                break;
            }
            match self.next().kind {
                TokKind::Ident(n) => names.push(n),
                _ => {
                    self.i -= 1;
                    return Err(self.unexpected("a variable name"));
                }
            }
        }
        Ok(Stmt::Decl { names, value, pos })
    }

    /// An expression statement, including Groovy's command form
    /// `step arg1, key: value`.
    fn expr_stmt(&mut self) -> PResult<Expr> {
        let e = self.expr()?;
        let commandable = matches!(e, Expr::Ident { .. } | Expr::Prop { .. });
        if commandable && self.starts_command_arg() {
            let pos = e.pos();
            let mut args = self.command_args()?;
            if self.peek().is_punct("{") && !self.peek().nl_before {
                args.push(Arg::Pos(self.closure()?));
            }
            return Ok(Expr::Call {
                callee: Box::new(e),
                args,
                parens: false,
                pos,
            });
        }
        // A brace on the next line still belongs to `name` / `name(...)`.
        if self.peek().is_punct("{") && self.peek().nl_before {
            if let Expr::Ident { .. } | Expr::Call { parens: true, .. } = e {
                let closure = self.closure()?;
                return Ok(attach_closure(e, closure));
            }
        }
        Ok(e)
    }

    fn starts_command_arg(&self) -> bool {
        let t = self.peek();
        if t.nl_before {
            return false;
        }
        match &t.kind {
            TokKind::Ident(n) => !NON_ARG_IDENTS.contains(&n.as_str()),
            TokKind::Str { .. } | TokKind::Num(_) => true,
            TokKind::Punct(p) => matches!(*p, "!" | "~"),
            TokKind::Eof => false,
        }
    }

    fn named_key_ahead(&self) -> bool {
        matches!(self.peek().kind, TokKind::Ident(_) | TokKind::Str { .. } | TokKind::Num(_)) && self.peek_at(1).is_punct(":")
    }

    fn named_key(&mut self) -> (String, Pos) {
        let t = self.next();
        self.next(); // ':'
        let key = match t.kind {
            TokKind::Ident(n) | TokKind::Num(n) => n,
            TokKind::Str { value, .. } => value,
            _ => String::new(),
        };
        (key, t.pos)
    }

    fn command_args(&mut self) -> PResult<Vec<Arg>> {
        let mut args = Vec::new();
        loop {
            if self.named_key_ahead() {
                let (key, pos) = self.named_key();
                let value = self.ternary()?;
                args.push(Arg::Named { key, value, pos });
            } else {
                args.push(Arg::Pos(self.ternary()?));
            }
            if !self.eat(",") {
                break;
            }
        }
        Ok(args)
    }

    // ---------------------------------------------------------------
    // Expressions
    // ---------------------------------------------------------------

    pub fn expr(&mut self) -> PResult<Expr> {
        let lhs = self.ternary()?;
        let t = self.peek();
        if !t.nl_before {
            if let TokKind::Punct(p) = t.kind {
                if ASSIGN_OPS.contains(&p) {
                    let pos = self.next().pos;
                    let value = self.expr_stmt()?;
                    return Ok(Expr::Assign {
                        op: p,
                        target: Box::new(lhs),
                        value: Box::new(value),
                        pos,
                    });
                }
            }
        }
        Ok(lhs)
    }

    fn ternary(&mut self) -> PResult<Expr> {
        let cond = self.binary(0)?;
        if self.peek().is_punct("?") {
            let pos = self.next().pos;
            let then = self.ternary()?;
            self.expect(":")?;
            let other = self.ternary()?;
            return Ok(Expr::Ternary {
                cond: Box::new(cond),
                then: Box::new(then),
                other: Box::new(other),
                pos,
            });
        }
        if self.peek().is_punct("?:") {
            let pos = self.next().pos;
            let other = self.ternary()?;
            return Ok(Expr::Binary {
                op: "?:".into(),
                lhs: Box::new(cond),
                rhs: Box::new(other),
                pos,
            });
        }
        Ok(cond)
    }

    fn binary_op(&self) -> Option<(String, u8, usize)> {
        let t = self.peek();
        if t.nl_before {
            return None;
        }
        let (op, prec, len) = match &t.kind {
            TokKind::Punct(p) => {
                let prec = match *p {
                    "||" => 1,
                    "&&" => 2,
                    "|" => 3,
                    "^" => 4,
                    "&" => 5,
                    "==" | "!=" | "<=>" | "===" | "!==" | "=~" | "==~" => 6,
                    "<" | ">" | "<=" | ">=" => 7,
                    "<<" => 8,
                    ".." | "..<" | "<.." | "<..<" => 9,
                    "+" | "-" => 10,
                    "*" | "/" | "%" => 11,
                    "**" => 12,
                    "!" => {
                        // `!in` and `!instanceof`
                        let n = self.peek_at(1);
                        if n.is_ident("in") || n.is_ident("instanceof") {
                            return Some((format!("!{}", if n.is_ident("in") { "in" } else { "instanceof" }), 7, 2));
                        }
                        return None;
                    }
                    _ => return None,
                };
                (p.to_string(), prec, 1)
            }
            TokKind::Ident(n) if n == "in" || n == "instanceof" || n == "as" => (n.clone(), 7, 1),
            _ => return None,
        };
        // `>` `>` is a right shift.
        if op == ">" && self.peek_at(1).is_punct(">") && !self.peek_at(1).nl_before {
            if self.peek_at(2).is_punct(">") {
                return Some((">>>".into(), 8, 3));
            }
            return Some((">>".into(), 8, 2));
        }
        Some((op, prec, len))
    }

    fn binary(&mut self, min_prec: u8) -> PResult<Expr> {
        let mut lhs = self.unary()?;
        while let Some((op, prec, len)) = self.binary_op() {
            if prec < min_prec {
                break;
            }
            let pos = self.peek().pos;
            for _ in 0..len {
                self.next();
            }
            let rhs = if op == "as" || op == "instanceof" || op == "!instanceof" {
                let tpos = self.peek().pos;
                self.skip_type()?;
                Expr::Ident {
                    name: "type".into(),
                    pos: tpos,
                }
            } else {
                self.binary(prec + 1)?
            };
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                pos,
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let t = self.peek().clone();
        if let TokKind::Punct(p) = t.kind {
            if matches!(p, "!" | "-" | "+" | "~" | "++" | "--") {
                self.next();
                let e = self.unary()?;
                return Ok(Expr::Unary {
                    op: p,
                    expr: Box::new(e),
                    pos: t.pos,
                });
            }
            if p == "*" {
                // Spread argument `*list`.
                self.next();
                let e = self.unary()?;
                return Ok(Expr::Unary {
                    op: "*",
                    expr: Box::new(e),
                    pos: t.pos,
                });
            }
        }
        let e = self.primary()?;
        self.postfix(e)
    }

    fn postfix(&mut self, mut e: Expr) -> PResult<Expr> {
        loop {
            let t = self.peek().clone();
            match &t.kind {
                TokKind::Punct(p @ ("." | "?." | "*." | ".&" | ".@" | "::" | "??.")) => {
                    self.next();
                    let name = match self.next().kind {
                        TokKind::Ident(n) => n,
                        TokKind::Str { value, .. } => value,
                        _ => {
                            self.i -= 1;
                            return Err(self.unexpected(&format!("a name after '{p}'")));
                        }
                    };
                    e = Expr::Prop {
                        obj: Box::new(e),
                        name,
                        pos: t.pos,
                    };
                }
                TokKind::Punct("(") if !t.nl_before => {
                    let args = self.call_args()?;
                    let pos = e.pos();
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        parens: true,
                        pos,
                    };
                }
                TokKind::Punct("[") if !t.nl_before => {
                    self.next();
                    let mut index = Vec::new();
                    while !self.peek().is_punct("]") {
                        index.push(self.expr()?);
                        if !self.eat(",") {
                            break;
                        }
                    }
                    self.expect("]")?;
                    e = Expr::Index {
                        obj: Box::new(e),
                        index,
                        pos: t.pos,
                    };
                }
                TokKind::Punct("{") if !t.nl_before && matches!(e, Expr::Ident { .. } | Expr::Prop { .. } | Expr::Call { .. }) => {
                    let closure = self.closure()?;
                    e = attach_closure(e, closure);
                }
                TokKind::Punct(p @ ("++" | "--")) if !t.nl_before => {
                    self.next();
                    e = Expr::Unary {
                        op: p,
                        expr: Box::new(e),
                        pos: t.pos,
                    };
                }
                _ => return Ok(e),
            }
        }
    }

    fn call_args(&mut self) -> PResult<Vec<Arg>> {
        self.expect("(")?;
        let mut args = Vec::new();
        while !self.peek().is_punct(")") {
            if self.named_key_ahead() {
                let (key, pos) = self.named_key();
                let value = self.expr()?;
                args.push(Arg::Named { key, value, pos });
            } else if self.peek().is_punct("*:") {
                self.next();
                self.expr()?;
            } else {
                args.push(Arg::Pos(self.expr()?));
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(args)
    }

    fn primary(&mut self) -> PResult<Expr> {
        let t = self.peek().clone();
        let pos = t.pos;
        match t.kind {
            TokKind::Ident(name) => {
                if name == "new" {
                    self.next();
                    return self.new_expr(pos);
                }
                self.next();
                Ok(Expr::Ident { name, pos })
            }
            TokKind::Str { value, interp } => {
                self.next();
                Ok(Expr::Str { value, interp, pos })
            }
            TokKind::Num(text) => {
                self.next();
                Ok(Expr::Num { text, pos })
            }
            TokKind::Punct("(") => {
                // Cast: `(String) x`
                if matches!(self.peek_at(1).kind, TokKind::Ident(_)) && self.peek_at(2).is_punct(")") {
                    let after = self.peek_at(3);
                    let castable = !after.nl_before
                        && match &after.kind {
                            TokKind::Ident(n) => !NON_ARG_IDENTS.contains(&n.as_str()),
                            TokKind::Str { .. } | TokKind::Num(_) => true,
                            TokKind::Punct(p) => *p == "(" || *p == "[",
                            _ => false,
                        };
                    if castable {
                        self.i += 3;
                        return self.unary();
                    }
                }
                self.next();
                let inner = self.expr()?;
                self.expect(")")?;
                Ok(Expr::Paren {
                    inner: Box::new(inner),
                    pos,
                })
            }
            TokKind::Punct("[") => self.list_or_map(),
            TokKind::Punct("{") => self.closure(),
            _ => Err(self.unexpected("an expression")),
        }
    }

    fn new_expr(&mut self, pos: Pos) -> PResult<Expr> {
        let ty = self.qualified_name()?;
        if self.peek().is_punct("<") {
            let mut depth = 0;
            loop {
                let t = self.next();
                if t.is_punct("<") {
                    depth += 1;
                } else if t.is_punct(">") {
                    depth -= 1;
                } else if matches!(t.kind, TokKind::Eof) {
                    return Err(Diagnostic::error(t.pos, "Unclosed generic type"));
                }
                if depth == 0 {
                    break;
                }
            }
        }
        if self.peek().is_punct("[") {
            // Array creation: new String[3] or new String[] { ... }
            while self.peek().is_punct("[") {
                self.skip_balanced("[", "]")?;
            }
            if self.peek().is_punct("{") {
                self.skip_balanced("{", "}")?;
            }
            return Ok(Expr::New { ty, args: vec![], pos });
        }
        let mut args = self.call_args()?;
        if self.peek().is_punct("{") && !self.peek().nl_before {
            // Anonymous class body.
            self.skip_balanced("{", "}")?;
            args.clear();
        }
        Ok(Expr::New { ty, args, pos })
    }

    fn list_or_map(&mut self) -> PResult<Expr> {
        let pos = self.expect("[")?.pos;
        if self.peek().is_punct(":") && self.peek_at(1).is_punct("]") {
            self.next();
            self.next();
            return Ok(Expr::Map { entries: vec![], pos });
        }
        let mut items = Vec::new();
        let mut entries = Vec::new();
        let mut is_map = false;
        while !self.peek().is_punct("]") {
            if self.peek().is_punct("*:") || (self.peek().is_punct("*") && self.peek_at(1).is_punct(":")) {
                while !self.peek().is_punct(":") {
                    self.next();
                }
                self.next();
                let v = self.expr()?;
                entries.push((Expr::Ident { name: "*".into(), pos }, v));
                is_map = true;
            } else if self.named_key_ahead() {
                let kt = self.peek().clone();
                let (key, kpos) = self.named_key();
                let k = match kt.kind {
                    TokKind::Str { interp, .. } => Expr::Str {
                        value: key,
                        interp,
                        pos: kpos,
                    },
                    _ => Expr::Str {
                        value: key,
                        interp: false,
                        pos: kpos,
                    },
                };
                let v = self.expr()?;
                entries.push((k, v));
                is_map = true;
            } else {
                let e = self.expr()?;
                if self.eat(":") {
                    let v = self.expr()?;
                    entries.push((e, v));
                    is_map = true;
                } else {
                    items.push(e);
                }
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect("]")?;
        if is_map {
            if !items.is_empty() {
                return Err(Diagnostic::error(pos, "Cannot mix list items and map entries"));
            }
            Ok(Expr::Map { entries, pos })
        } else {
            Ok(Expr::List { items, pos })
        }
    }

    fn closure(&mut self) -> PResult<Expr> {
        let open = self.expect("{")?.pos;
        let mut params = Vec::new();
        // Look ahead for `a, b ->` or `->`.
        let mut k = 0;
        let mut has_arrow = false;
        loop {
            let t = self.peek_at(k);
            match &t.kind {
                TokKind::Punct("->") => {
                    has_arrow = true;
                    break;
                }
                TokKind::Ident(_) => {}
                TokKind::Punct("," | "<" | ">" | "." | "[" | "]" | "=") => {}
                TokKind::Str { .. } | TokKind::Num(_) => {}
                _ => break,
            }
            k += 1;
        }
        if has_arrow {
            for _ in 0..k {
                if let TokKind::Ident(n) = &self.next().kind {
                    params.push(n.clone());
                }
            }
            self.expect("->")?;
        }
        let body = self.stmts_until(Some(open))?;
        self.expect("}")?;
        Ok(Expr::Closure { params, body, pos: open })
    }
}

/// Turn `name` or `name(args)` plus a closure into one call.
fn attach_closure(e: Expr, closure: Expr) -> Expr {
    match e {
        Expr::Call {
            callee,
            mut args,
            parens,
            pos,
        } => {
            args.push(Arg::Pos(closure));
            Expr::Call { callee, args, parens, pos }
        }
        other => {
            let pos = other.pos();
            Expr::Call {
                callee: Box::new(other),
                args: vec![Arg::Pos(closure)],
                parens: false,
                pos,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn ok(src: &str) -> Vec<Stmt> {
        parse(tokenize(src).unwrap()).unwrap_or_else(|e| panic!("{src}: {}:{} {}", e.pos.line, e.pos.col, e.message))
    }
    fn fails(src: &str) -> Diagnostic {
        match tokenize(src) {
            Err(e) => e,
            Ok(t) => parse(t).expect_err(src),
        }
    }

    #[test]
    fn jenkins_idioms() {
        ok("sh script: 'ls', returnStdout: true");
        ok("@Library('x@main') _");
        ok("@Library('x@main')\nimport foo.Bar");
        ok("build job: 'x', parameters: [string(name: 'a', value: 'b')]");
        ok("stage('a') { sh 'x' }");
        ok("x = [1,2].collect { it * 2 }");
        ok("def f(a) { return a }");
        ok("node('linux') { checkout scm }");
        ok("def m = [:]\nm['a'] = 1");
        ok("String s = \"${env.X}\"\nList<String> l = []");
        ok("if (a) { b() } else if (c) { d() } else e()");
        ok("try { x() } catch (Exception e) { y() } finally { z() }");
        ok("parallel a: { sh 'x' }, b: { sh 'y' }");
        ok("withCredentials([usernamePassword(credentialsId: 'c', usernameVariable: 'U', passwordVariable: 'P')]) {\n sh 'x'\n}");
        ok("def x = y ?: 'z'\ndef r = a ? b : c");
        ok("items.each { k, v -> echo \"${k}\" }");
        ok("def s = sh(script: 'git rev-parse HEAD', returnStdout: true).trim()");
        ok("pipeline\n{\n agent any\n}");
        ok("env.FOO = 'bar'\ncurrentBuild.result = 'FAILURE'");
        ok("x = foo\n  .bar()\n  .baz()");
        ok("assert m, \"not found\"\nassert x : 'y'");
        ok("MY_VERSION = readFile 'Version'\ndef v = readFile file: 'x'");
    }

    #[test]
    fn syntax_errors() {
        fails("pipeline {\n  agent any\n");
        fails("sh 'x' 'y'");
        fails("stage('a') { sh 'x' }}");
        fails("sh \"unterminated");
    }
}
