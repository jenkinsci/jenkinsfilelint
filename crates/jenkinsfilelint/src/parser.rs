//! Recursive-descent parser for Groovy as written in Jenkinsfiles.
//!
//! The parser is deliberately lenient about Groovy features that never
//! matter for declarative validation (class bodies, switch bodies, for
//! headers): it checks that their brackets balance and moves on. It is
//! strict about the shapes Jenkins users write every day: command calls
//! with named arguments, trailing closures, GStrings, maps and lists.

use crate::ast::{Arg, Expr, Placeholder, Stmt, StrPart};
use crate::diag::{Diagnostic, Pos};
use crate::lexer::{self, RawPart, RawPlaceholder, TokKind, Token};

pub fn parse(tokens: Vec<Token>) -> Result<Vec<Stmt>, Diagnostic> {
    let mut p = Parser {
        toks: tokens,
        i: 0,
        in_group: false,
        depth: 0,
        command_start: 0,
    };
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
/// The precedence of comparisons, `in`, `instanceof` and `as`.
const RELATIONAL: u8 = 7;

const NON_ARG_IDENTS: &[&str] = &["in", "instanceof", "as", "else", "catch", "finally"];

struct Parser {
    toks: Vec<Token>,
    i: usize,
    /// Inside `( )` or `[ ]`, where a newline does not end an expression
    /// (a closure body inside them starts a fresh statement context).
    in_group: bool,
    /// How many nested statements and expressions are being parsed. Capped
    /// at MAX_DEPTH so that absurd nesting gives an error, not a crash.
    depth: usize,
    /// Where a command expression may start: a statement, the value of an
    /// assignment, a command's first argument. Only there does Groovy take
    /// a number with letters after it, `6ce8`, as `6 ce8`.
    command_start: usize,
}

type PResult<T> = Result<T, Diagnostic>;

/// Nesting limit, counted in parser levels (a closure costs about three).
/// Real Jenkinsfiles stay far below it; main.rs gives the linter a stack
/// big enough for it on every platform.
pub const MAX_DEPTH: usize = 1000;

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
    /// Run `f` one nesting level deeper, or fail when that passes MAX_DEPTH.
    fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        if self.depth >= MAX_DEPTH {
            return Err(Diagnostic::error(
                self.peek().pos,
                format!("Nesting is too deep to check (more than {MAX_DEPTH} levels)"),
            ));
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        result
    }

    /// Parse with `in_group` set to `value`, restoring it afterwards.
    fn grouped<T>(&mut self, value: bool, f: impl FnOnce(&mut Self) -> PResult<T>) -> PResult<T> {
        let saved = std::mem::replace(&mut self.in_group, value);
        let r = f(self);
        self.in_group = saved;
        r
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
        self.nested(Self::stmt_inner)
    }

    fn stmt_inner(&mut self) -> PResult<Stmt> {
        let t = self.peek().clone();
        let pos = t.pos;
        if t.is_punct("@") {
            self.annotation()?;
            while self.eat(";") {}
            if self.at_eof() || self.peek().is_punct("}") {
                return Ok(Stmt::Other { kind: "annotation", pos });
            }
            // An annotation belongs to a declaration or an import: a call
            // such as `pipeline { }` right after `@Library('lib')` does not
            // compile, which is why the usual form is `@Library('lib') _`.
            // A bare name declares a variable too: `@Field X = []`,
            // `@Library('lib') l2`.
            let after = self.peek_at(1);
            let bare_name = matches!(self.peek().kind, TokKind::Ident(_))
                && (after.nl_before || matches!(after.kind, TokKind::Eof | TokKind::Punct("=" | ";")));
            let declares = self.peek().is_punct("@")
                || bare_name
                || matches!(&self.peek().kind, TokKind::Ident(w)
                    if matches!(w.as_str(), "def" | "import" | "package" | "class" | "interface" | "enum" | "trait")
                        || MODIFIERS.contains(&w.as_str()))
                || self.looks_like_typed_decl();
            // `@NonCPS` then `name(a, b) { }` defines a method, as `def` would.
            if !declares
                && matches!(self.peek().kind, TokKind::Ident(_))
                && self.peek_at(1).is_punct("(")
                && matches!(self.peek_at(2).kind, TokKind::Ident(_) | TokKind::Punct(")"))
            {
                let at = self.i;
                let name = self.ident_name().unwrap_or_default();
                self.next();
                self.skip_balanced("(", ")")?;
                if self.peek().is_punct("{") {
                    let body = self.block()?;
                    return Ok(Stmt::MethodDef { name, body, pos });
                }
                self.i = at;
            }
            if !declares {
                return Err(self.unexpected("a declaration after the annotation, as in @Library('lib') _"));
            }
            return self.stmt();
        }
        if t.is_punct("{") {
            // A block is a statement only with a label, `L: { }`: Groovy
            // cannot tell a bare one from a closure.
            let first = self.i == 0 || matches!(self.toks[self.i - 1].kind, TokKind::Punct("{" | "->" | ";"));
            let labeled = self.toks.get(self.i.wrapping_sub(1)).is_some_and(|p| p.is_punct(":"));
            if !labeled && !self.closure_has_params() {
                return Err(Diagnostic::error(
                    pos,
                    if first {
                        "Ambiguous expression could be either a parameterless closure expression or an isolated open code block; solution: Add an explicit closure parameter list, e.g. {it -> ...}, or force it to be treated as an open block by giving it a label, e.g. L:{...}"
                    } else {
                        "Ambiguous expression could be a parameterless closure expression, an isolated open code block, or it may continue a previous statement; solution: Add an explicit parameter list, e.g. {it -> ...}, or force it to be treated as an open block by giving it a label, e.g. L:{...}, and also either remove the previous newline, or add an explicit semicolon ';'"
                    },
                ));
            }
            let e = self.closure()?;
            return Ok(Stmt::Expr(e));
        }
        if t.is_punct("(") && self.looks_like_multi_assign() {
            return self.multi_assign();
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
                self.grouped(true, |p| p.expr())?;
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
                // Groovy 2.4 has no try-with-resources: it came with Groovy 3.
                if self.peek().is_punct("(") {
                    return Err(self.unexpected("'{': Groovy 2.4 has no try-with-resources"));
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
            _ if MODIFIERS.contains(&word.as_str()) && self.modifiers_then_type_decl() => {
                while !self.peek().is_ident("class")
                    && !self.peek().is_ident("interface")
                    && !self.peek().is_ident("enum")
                    && !self.peek().is_ident("trait")
                {
                    self.next();
                }
                self.type_decl(pos)
            }
            _ if word == "def" || MODIFIERS.contains(&word.as_str()) => self.declaration(pos),
            _ if self.looks_like_typed_decl() => self.declaration(pos),
            _ if self.peek_at(1).is_punct(":") && !self.peek_at(1).nl_before && word != "default" => {
                // Statement label: `outer: for (...)`, or `outer:` on a
                // line of its own before the statement it names.
                self.next();
                self.next();
                self.stmt()
            }
            _ => Ok(Stmt::Expr(self.expr_stmt()?)),
        }
    }

    /// `public class X`, `abstract class X`, `final enum X`, ...
    fn modifiers_then_type_decl(&self) -> bool {
        let mut k = 0;
        while matches!(&self.peek_at(k).kind, TokKind::Ident(w) if MODIFIERS.contains(&w.as_str())) {
            k += 1;
        }
        matches!(&self.peek_at(k).kind, TokKind::Ident(w) if matches!(w.as_str(), "class" | "interface" | "enum" | "trait"))
    }

    /// `(a, b) = expr`: Groovy multiple assignment to existing variables.
    fn looks_like_multi_assign(&self) -> bool {
        let mut k = 1;
        loop {
            if !matches!(self.peek_at(k).kind, TokKind::Ident(_)) {
                return false;
            }
            k += 1;
            if self.peek_at(k).is_punct(",") {
                k += 1;
            } else {
                break;
            }
        }
        self.peek_at(k).is_punct(")") && self.peek_at(k + 1).is_punct("=")
    }

    fn multi_assign(&mut self) -> PResult<Stmt> {
        let pos = self.expect("(")?.pos;
        let mut items = Vec::new();
        loop {
            let t = self.next();
            if let TokKind::Ident(name) = t.kind {
                items.push(Expr::Ident { name, pos: t.pos });
            }
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        self.expect("=")?;
        let value = self.expr_stmt()?;
        Ok(Stmt::Expr(Expr::Assign {
            op: "=",
            target: Box::new(Expr::List { items, pos }),
            value: Box::new(value),
            pos,
        }))
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

    /// `String x = ...`, `List<String> xs`, `void call() {}`, and with a
    /// qualified type, `java.util.regex.Matcher m`. Like Groovy, take a
    /// name for a type when it is a primitive or starts with a capital:
    /// `a.b.C d` declares d, and `a.b.c d` calls c.
    fn looks_like_typed_decl(&self) -> bool {
        let TokKind::Ident(first) = &self.peek().kind else { return false };
        let mut ty = first;
        let mut k = 1;
        // Qualified type name.
        while self.peek_at(k).is_punct(".") {
            let TokKind::Ident(next) = &self.peek_at(k + 1).kind else { break };
            ty = next;
            k += 2;
        }
        let typeish = (k == 1 && PRIMITIVES.contains(&ty.as_str())) || ty.chars().next().is_some_and(|c| c.is_uppercase());
        if !typeish {
            return false;
        }
        // Generic arguments.
        if self.peek_at(k).is_punct("<") {
            match self.generic_args_end(k) {
                Some(end) => k = end,
                None => return false,
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
        if !after.is_punct(",") {
            return after.is_punct("=") || after.is_punct("(") || after.is_punct(";") || after.nl_before;
        }
        // After a comma, a declaration continues with plain variable names
        // (`String a, b = 'x'`), while a command call takes any arguments:
        // `Utils.notify currentBuild, status: 'ok'` declares nothing.
        let mut k = k + 1;
        while self.peek_at(k).is_punct(",") {
            if !matches!(self.peek_at(k + 1).kind, TokKind::Ident(_)) {
                return false;
            }
            let next = self.peek_at(k + 2);
            if next.is_punct("=") || next.is_punct(";") || next.nl_before || matches!(next.kind, TokKind::Eof) {
                return true;
            }
            k += 2;
        }
        false
    }

    /// When the token `k` ahead is the `<` of generic type arguments, as in
    /// `List<Map<String, ?>>`, the offset just past their closing `>`.
    /// None when what follows is not a list of types.
    fn generic_args_end(&self, mut k: usize) -> Option<usize> {
        let mut depth = 0usize;
        loop {
            match &self.peek_at(k).kind {
                TokKind::Punct("<") => depth += 1,
                TokKind::Punct(">") => depth -= 1,
                TokKind::Ident(_) | TokKind::Punct("," | "." | "?" | "[" | "]" | "&") => {}
                _ => return None,
            }
            k += 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }

    /// Consume generic type arguments when the next token is their `<`.
    fn skip_generic_args(&mut self) -> PResult<()> {
        if self.peek().is_punct("<") {
            let Some(end) = self.generic_args_end(0) else {
                return Err(Diagnostic::error(self.peek().pos, "Unclosed generic type"));
            };
            self.i += end;
        }
        Ok(())
    }

    fn skip_type(&mut self) -> PResult<()> {
        self.qualified_name()?;
        self.skip_generic_args()?;
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
        // `def class X { }`
        if let Some(w) = self.ident_name() {
            if matches!(w.as_str(), "class" | "interface" | "enum" | "trait") {
                return self.type_decl(pos);
            }
        }
        // Type parameters of a generic method: `def <T> T call(Closure<T> body)`.
        if self.peek().is_punct("<") {
            self.skip_generic_args()?;
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
        let name_at = self.i;
        let (name, quoted) = match self.next().kind {
            TokKind::Ident(n) => (n, false),
            TokKind::Str { value, .. } => (value, true),
            _ => {
                self.i -= 1;
                return Err(self.unexpected("a name"));
            }
        };
        // Only a method's name may be quoted: `def 'run tests'() { }`.
        if quoted && (!self.peek().is_punct("(") || self.peek().nl_before) {
            self.i = name_at;
            return Err(self.unexpected("a variable name"));
        }
        if self.peek().is_punct("(") && !self.peek().nl_before {
            // Method definition. On the next line, `(a, b) = f()` is a
            // statement of its own.
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
    /// `step arg1, key: value` and its command chains: `a b c d` is
    /// `a(b).c(d)`, and a name with no argument after it reads a property,
    /// so `a b c` is `a(b).c`. A chain may also start with a call in
    /// parentheses, `library('lib@main') _` is `library('lib@main')._`, or
    /// with a string, which names the method: `"a"${n}"b"` is
    /// `"a"($({ n }))."b"`.
    fn expr_stmt(&mut self) -> PResult<Expr> {
        self.command_start = self.i;
        let at = self.peek().pos;
        let e = self.expr()?;
        let calls_constructor = |e: &Expr| matches!(e, Expr::Call { callee, .. } if matches!(callee.as_ref(), Expr::Ident { name, .. } if name == "this" || name == "super"));
        if calls_constructor(&e)
            || (matches!(e, Expr::Ident { ref name, .. } if name == "this" || name == "super") && self.starts_command_arg())
        {
            return Err(Diagnostic::error(
                at,
                "Constructor call must be the first statement in a constructor.",
            ));
        }
        // `a[0] "x"` and `env.(name) "x"` are command calls too, and so is
        // a string with arguments, which names the method: `'echo' 'hi'`
        // is `echo('hi')`.
        let commandable = matches!(e, Expr::Ident { .. } | Expr::Prop { .. } | Expr::Index { .. } | Expr::Str { .. });
        // A chain also continues after a call with parentheses or a closure:
        // `library('x') _`, `list.collect { it } join ','`.
        let called = match &e {
            Expr::Call { parens: true, .. } => true,
            Expr::Call { args, .. } => matches!(args.last(), Some(Arg::Pos(Expr::Closure { .. }))),
            _ => false,
        };
        let chained_call = called && self.chain_name().is_some();
        if (commandable && self.starts_command_arg()) || chained_call {
            let mut call = if chained_call { e } else { self.command_call(e)? };
            while let Some(name) = self.chain_name() {
                let pos = self.next().pos;
                let prop = Expr::Prop {
                    obj: Box::new(call),
                    name,
                    pos,
                };
                call = if self.peek().is_punct("(") && !self.peek().nl_before {
                    // `foo(1) bar(2)`, `if (a) or (b) or (c) { }`
                    let args = self.call_args()?;
                    let call = Expr::Call {
                        callee: Box::new(prop),
                        args,
                        parens: true,
                        pos,
                    };
                    if self.peek().is_punct("{") && !self.peek().nl_before {
                        attach_closure(call, self.closure()?)
                    } else {
                        call
                    }
                } else if self.starts_chain_arg() {
                    self.command_call(prop)?
                } else if self.peek().is_punct("{") && !self.peek().nl_before {
                    // `a b c { }` passes the closure to c.
                    Expr::Call {
                        callee: Box::new(prop),
                        args: vec![Arg::Pos(self.closure()?)],
                        parens: false,
                        pos,
                    }
                } else {
                    prop
                };
            }
            return Ok(call);
        }
        // A brace on the next line still belongs to a simple expression:
        // `name`, `name(...)`, and even `'x'`, `1` or `[1]`, as in Groovy;
        // not to `a + b` or a command, `foo a`.
        if self.peek().is_punct("{") && self.peek().nl_before {
            if let Expr::Ident { .. }
            | Expr::Prop { .. }
            | Expr::Index { .. }
            | Expr::Str { .. }
            | Expr::Num { .. }
            | Expr::List { .. }
            | Expr::Map { .. }
            | Expr::Call { parens: true, .. } = e
            {
                let closure = self.closure()?;
                return Ok(attach_closure(e, closure));
            }
        }
        Ok(e)
    }

    /// The next name in a command chain: a word, a string, as in
    /// `foo 'a' 'b'` (`foo('a').'b'`), or a number, on the same line.
    fn chain_name(&self) -> Option<String> {
        if !self.starts_command_arg() {
            return None;
        }
        match &self.peek().kind {
            TokKind::Ident(name) => Some(name.clone()),
            TokKind::Str { value, .. } => Some(value.clone()),
            // `foo bar 1` is `foo(bar).1`.
            TokKind::Num(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// The arguments of `callee` in command form, and a closure after them.
    fn command_call(&mut self, callee: Expr) -> PResult<Expr> {
        let pos = callee.pos();
        let mut args = self.command_args()?;
        if self.peek().is_punct("{") && !self.peek().nl_before {
            args.push(Arg::Pos(self.closure()?));
        }
        Ok(Expr::Call {
            callee: Box::new(callee),
            args,
            parens: false,
            pos,
        })
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

    /// The argument of a name in a command chain may also start with a
    /// prefix operator: `foo a b -c` is `foo(a).b(-c)`, and Groovy reads
    /// `dut run --log-level debug` as `dut(run--).log(-level).debug`.
    fn starts_chain_arg(&self) -> bool {
        let t = self.peek();
        self.starts_command_arg() || !t.nl_before && matches!(t.kind, TokKind::Punct("-" | "+" | "--" | "++"))
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
        self.command_start = self.i;
        loop {
            if self.named_key_ahead() {
                let (key, pos) = self.named_key();
                let value = self.ternary()?;
                args.push(Arg::Named { key, value, pos });
            } else {
                // `string x = 'a'` is `string(x = 'a')`.
                let arg = self.ternary()?;
                let arg = match self.peek().kind {
                    TokKind::Punct(p) if ASSIGN_OPS.contains(&p) && !self.peek().nl_before => {
                        let pos = self.next().pos;
                        Expr::Assign {
                            op: p,
                            target: Box::new(arg),
                            value: Box::new(self.expr()?),
                            pos,
                        }
                    }
                    _ => arg,
                };
                args.push(Arg::Pos(arg));
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
        self.nested(Self::expr_inner)
    }

    fn expr_inner(&mut self) -> PResult<Expr> {
        let lhs = self.ternary()?;
        let t = self.peek();
        if !t.nl_before {
            if let TokKind::Punct(p) = t.kind {
                if ASSIGN_OPS.contains(&p) {
                    if matches!(lhs, Expr::Ternary { .. }) {
                        return Err(self.unexpected("end of statement: a conditional cannot be assigned to"));
                    }
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
        self.nested(Self::ternary_inner)
    }

    fn ternary_inner(&mut self) -> PResult<Expr> {
        let cond = self.binary(0)?;
        if self.peek().is_punct("?") {
            let pos = self.next().pos;
            // Its first branch may assign: `a ? m['k'] = 1 : null`.
            let then = self.expr()?;
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
        if t.nl_before && !self.in_group {
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
                    "<" | ">" | "<=" | ">=" => RELATIONAL,
                    "<<" => 8,
                    ".." | "..<" | "<.." | "<..<" => 9,
                    "+" | "-" => 10,
                    "*" | "/" | "%" => 11,
                    "**" => 12,
                    // Groovy 2.4, which Jenkins runs, has no `!in` or
                    // `!instanceof`: they came with Groovy 3.
                    _ => return None,
                };
                (p.to_string(), prec, 1)
            }
            TokKind::Ident(n) if n == "in" || n == "instanceof" || n == "as" => (n.clone(), RELATIONAL, 1),
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
        let mut compared = false;
        while let Some((op, prec, len)) = self.binary_op() {
            if prec < min_prec {
                break;
            }
            // Comparisons do not chain in Groovy 2.4: `a < b < c`, `a in b
            // in c` and `a > b as Integer` do not compile.
            if prec == RELATIONAL {
                if compared {
                    return Err(Diagnostic::error(
                        self.peek().pos,
                        format!("Unexpected '{op}': Groovy does not chain comparisons, `in`, `instanceof` or `as`; add parentheses"),
                    ));
                }
                compared = true;
            }
            let pos = self.peek().pos;
            for _ in 0..len {
                self.next();
            }
            let rhs = if op == "as" || op == "instanceof" {
                let tpos = self.peek().pos;
                // The type's first name: `Integer` in `x as Integer`.
                let name = match &self.peek().kind {
                    TokKind::Ident(n) => n.clone(),
                    _ => "type".into(),
                };
                self.skip_type()?;
                Expr::Ident { name, pos: tpos }
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
        self.nested(Self::unary_inner)
    }

    fn unary_inner(&mut self) -> PResult<Expr> {
        let t = self.peek().clone();
        if let TokKind::Punct(p) = t.kind {
            if matches!(p, "!" | "-" | "+" | "~" | "++" | "--") {
                self.next();
                // Groovy 2.4 takes no sign or step after another: `- -x` and
                // `-- --x` do not compile, `-(-x)` and `!-x` do.
                if matches!(p, "-" | "+" | "++" | "--") && matches!(self.peek().kind, TokKind::Punct("-" | "+" | "++" | "--")) {
                    return Err(self.unexpected("an expression"));
                }
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
                    // A member named in parentheses. Groovy 2.4 compiles a
                    // name or a plain string there as the member's name:
                    // `this.(echo)` and `this.('echo')` are `this.echo`. Any
                    // other expression names it at run time, like
                    // `env[expr]`. It also takes a block, `a.{ ... }`.
                    if self.peek().is_punct("(") || self.peek().is_punct("{") {
                        let name = if self.peek().is_punct("(") {
                            self.next();
                            let name = self.grouped(true, |p| p.expr())?;
                            self.expect(")")?;
                            name
                        } else {
                            self.closure()?
                        };
                        e = match name {
                            Expr::Str {
                                value: name,
                                interp: false,
                                ..
                            }
                            | Expr::Ident { name, .. } => Expr::Prop {
                                obj: Box::new(e),
                                name,
                                pos: t.pos,
                            },
                            name => Expr::Index {
                                obj: Box::new(e),
                                index: vec![name],
                                pos: t.pos,
                            },
                        };
                        continue;
                    }
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
                // Inside brackets a line break is only space: `parameters\n([...])`.
                TokKind::Punct("(") if !t.nl_before || self.in_group => {
                    let args = self.call_args()?;
                    let pos = e.pos();
                    e = Expr::Call {
                        callee: Box::new(e),
                        args,
                        parens: true,
                        pos,
                    };
                }
                TokKind::Punct("[") if !t.nl_before || self.in_group => {
                    self.next();
                    let index = self.grouped(true, |p| {
                        let mut index = Vec::new();
                        while !p.peek().is_punct("]") {
                            // Named entries make a map: `x[a: 1]`.
                            if p.named_key_ahead() {
                                let (key, kpos) = p.named_key();
                                let value = p.expr()?;
                                let k = Expr::Str {
                                    value: key,
                                    interp: false,
                                    parts: Vec::new(),
                                    pos: kpos,
                                };
                                index.push(Expr::Map {
                                    entries: vec![(k, value)],
                                    pos: kpos,
                                });
                            } else {
                                index.push(p.expr()?);
                            }
                            if !p.eat(",") {
                                break;
                            }
                        }
                        Ok(index)
                    })?;
                    self.expect("]")?;
                    e = Expr::Index {
                        obj: Box::new(e),
                        index,
                        pos: t.pos,
                    };
                }
                // A string is a method name too: `'a.b.C' { }` calls a.b.C, as
                // Job DSL's configure blocks do, and `stage 'S' { }` is
                // `stage('S'({ }))` to Groovy.
                TokKind::Punct("{")
                    if !t.nl_before
                        && (matches!(
                            e,
                            Expr::Ident { .. } | Expr::Prop { .. } | Expr::Index { .. } | Expr::Call { .. } | Expr::Str { .. }
                        ) || self.toks[self.i - 1].is_punct(")")) =>
                {
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
        self.grouped(true, Self::call_args_inner)
    }

    fn call_args_inner(&mut self) -> PResult<Vec<Arg>> {
        self.expect("(")?;
        let mut args = Vec::new();
        while !self.peek().is_punct(")") {
            if self.named_key_ahead() {
                let (key, pos) = self.named_key();
                let value = self.expr()?;
                args.push(Arg::Named { key, value, pos });
            } else if self.peek().is_punct("*") && self.peek_at(1).is_punct(":") {
                // Spread-map argument: `f(*:defaults, x: 1)`.
                self.next();
                self.next();
                self.expr()?;
            } else if self.typed_arg_ahead() {
                // Groovy 2.4 takes a declaration as an argument: `f(String s)`.
                self.skip_type()?;
                args.push(Arg::Pos(self.expr()?));
            } else {
                args.push(Arg::Pos(self.expr()?));
            }
            if !self.eat(",") {
                break;
            }
            self.skip_empty_items(")")?;
        }
        self.expect(")")?;
        Ok(args)
    }

    /// At a `{`: whether a parameter list follows, `{ ->`, `{ it ->` or
    /// `{ String a, b ->`, which makes it a closure even as a statement.
    fn closure_has_params(&self) -> bool {
        let mut k = 1;
        loop {
            match &self.peek_at(k).kind {
                TokKind::Punct("->") => return true,
                TokKind::Ident(_) | TokKind::Punct("," | "." | "<" | ">" | "[" | "]" | "?") => k += 1,
                _ => return false,
            }
        }
    }

    /// A declaration in an argument list, `f(String s)` or `f(a.B<C> d = x)`:
    /// a type, then a name and the end of the argument.
    fn typed_arg_ahead(&self) -> bool {
        let TokKind::Ident(first) = &self.peek().kind else { return false };
        let mut ty = first;
        let mut k = 1;
        while self.peek_at(k).is_punct(".") {
            let TokKind::Ident(next) = &self.peek_at(k + 1).kind else {
                return false;
            };
            ty = next;
            k += 2;
        }
        if !(PRIMITIVES.contains(&ty.as_str()) || ty.chars().next().is_some_and(|c| c.is_uppercase())) {
            return false;
        }
        if self.peek_at(k).is_punct("<") {
            match self.generic_args_end(k) {
                Some(end) => k = end,
                None => return false,
            }
        }
        while self.peek_at(k).is_punct("[") && self.peek_at(k + 1).is_punct("]") {
            k += 2;
        }
        matches!(self.peek_at(k).kind, TokKind::Ident(_)) && matches!(self.peek_at(k + 1).kind, TokKind::Punct("," | ")" | "="))
    }

    /// After the comma that ends an argument, list item or map entry:
    /// Groovy also takes empty ones, `f(1, , 2)` and `[a: 1, , b: 2]`,
    /// though not before the closing bracket, `f(1, , )`.
    fn skip_empty_items(&mut self, close: &str) -> PResult<()> {
        if !self.peek().is_punct(",") {
            return Ok(());
        }
        while self.eat(",") {}
        if self.peek().is_punct(close) {
            return Err(self.unexpected("an expression"));
        }
        Ok(())
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
                // Keywords, not values: `a !in b` does not compile.
                if name == "in" || name == "instanceof" {
                    return Err(self.unexpected("an expression"));
                }
                // The lexer keeps `6ce8` whole, for the validator to explain
                // `1BAR = 'x'`; Groovy reads `6 ce8`, which only a command
                // expression can be.
                if name.starts_with(|c: char| c.is_ascii_digit()) && self.i != self.command_start {
                    return Err(Diagnostic::error(
                        pos,
                        format!("Unexpected '{name}': a name cannot start with a digit"),
                    ));
                }
                self.next();
                Ok(Expr::Ident { name, pos })
            }
            TokKind::Str { value, interp, parts } => {
                self.next();
                let parts = self.str_parts(parts)?;
                Ok(Expr::Str { value, interp, parts, pos })
            }
            TokKind::Num(text) => {
                self.next();
                Ok(Expr::Num { text, pos })
            }
            TokKind::Punct("(") => {
                // Cast: `(String) x`, `(String[]) x`, `(List<String>) x`,
                // `(java.util.List) x`.
                if let Some((close, type_only)) = self.cast_type() {
                    let after = self.peek_at(close + 1);
                    let castable = !after.nl_before
                        && match &after.kind {
                            TokKind::Ident(n) => !NON_ARG_IDENTS.contains(&n.as_str()),
                            TokKind::Str { .. } | TokKind::Num(_) => true,
                            // `(a.b)(x)` and `(a.b)[0]` call or index a
                            // value; only what cannot be a value is a cast.
                            TokKind::Punct(p) => (*p == "(" || *p == "[") && (type_only || close == 2),
                            _ => false,
                        };
                    if castable {
                        self.i += close + 1;
                        return self.unary();
                    }
                }
                // Like Groovy, keep no trace of the parentheses: Jenkins reads
                // `stage(('A'))` and `A = ('a')` as if they were not there.
                self.next();
                let inner = self.grouped(true, |p| p.expr())?;
                self.expect(")")?;
                Ok(inner)
            }
            TokKind::Punct("[") => self.list_or_map(),
            TokKind::Punct("{") => self.closure(),
            _ => Err(self.unexpected("an expression")),
        }
    }

    /// At a `(`: whether a type and nothing else stands between it and its
    /// `)`, as in a cast. Returns the offset of the `)`, and whether the
    /// type can only be a type (it has generic arguments or `[]`), unlike
    /// `(a.b)`, which is also how a property is written.
    fn cast_type(&self) -> Option<(usize, bool)> {
        if !matches!(self.peek_at(1).kind, TokKind::Ident(_)) {
            return None;
        }
        let mut k = 2;
        while self.peek_at(k).is_punct(".") && matches!(self.peek_at(k + 1).kind, TokKind::Ident(_)) {
            k += 2;
        }
        let mut type_only = false;
        if self.peek_at(k).is_punct("<") {
            k = self.generic_args_end(k)?;
            type_only = true;
        }
        while self.peek_at(k).is_punct("[") && self.peek_at(k + 1).is_punct("]") {
            k += 2;
            type_only = true;
        }
        self.peek_at(k).is_punct(")").then_some((k, type_only))
    }

    fn new_expr(&mut self, pos: Pos) -> PResult<Expr> {
        let ty = self.qualified_name()?;
        self.skip_generic_args()?;
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
        self.grouped(true, Self::list_or_map_inner)
    }

    fn list_or_map_inner(&mut self) -> PResult<Expr> {
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
            if self.peek().is_punct("*") && self.peek_at(1).is_punct(":") {
                self.next();
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
                        parts: Vec::new(),
                        pos: kpos,
                    },
                    _ => Expr::Str {
                        value: key,
                        interp: false,
                        parts: Vec::new(),
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
            self.skip_empty_items("]")?;
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

    /// Parse the placeholders of a GString. Groovy compiles what is in them,
    /// so an error there is an error in the file.
    fn str_parts(&mut self, raw: Vec<RawPart>) -> PResult<Vec<StrPart>> {
        raw.into_iter()
            .map(|part| match part {
                RawPart::Text(t) => Ok(StrPart::Text(t)),
                RawPart::Placeholder(p) => self.nested(|me| me.placeholder(p)).map(StrPart::Placeholder),
            })
            .collect()
    }

    /// Parse one placeholder with a parser of its own, at this nesting
    /// depth: `$a.b` is an expression, and `${...}` a closure body, which
    /// may hold statements and a `->`.
    fn placeholder(&mut self, raw: RawPlaceholder) -> PResult<Placeholder> {
        let mut toks = lexer::tokenize_at(&raw.src, raw.src_pos)?;
        if raw.braced {
            let eof = toks.pop().expect("tokens end with Eof");
            let brace = |p: &'static str, line, col| Token {
                kind: TokKind::Punct(p),
                pos: Pos { line, col },
                nl_before: false,
            };
            let (start, end) = (raw.span.start, raw.span.end);
            toks.insert(0, brace("{", start.line, start.col + 1));
            toks.push(brace("}", end.line, end.col - 1));
            toks.push(eof);
        }
        let mut sub = Parser {
            toks,
            i: 0,
            in_group: false,
            depth: self.depth,
            command_start: usize::MAX,
        };
        let expr = if raw.braced { sub.closure()? } else { sub.expr()? };
        if !sub.at_eof() {
            return Err(sub.unexpected("the end of the placeholder"));
        }
        let expr = match expr {
            Expr::Closure { params, mut body, pos } if params.is_empty() && body.len() <= 1 => match body.pop() {
                None => None,
                Some(Stmt::Expr(e)) => Some(e),
                Some(stmt) => Some(Expr::Closure {
                    params,
                    body: vec![stmt],
                    pos,
                }),
            },
            other => Some(other),
        };
        Ok(Placeholder {
            expr,
            braced: raw.braced,
            span: raw.span,
        })
    }

    fn closure(&mut self) -> PResult<Expr> {
        let open = self.expect("{")?.pos;
        let mut params = Vec::new();
        if let Some(arrow) = self.closure_arrow() {
            // The name of a parameter is the last word before its `=`
            // default, the `,` after it or the arrow.
            let mut name = None;
            let mut depth = 0usize;
            let mut in_default = false;
            for _ in 0..arrow {
                let t = self.next();
                match t.kind {
                    TokKind::Punct("(" | "[") => depth += 1,
                    TokKind::Punct(")" | "]") => depth = depth.saturating_sub(1),
                    // Generic arguments of a type: `Map<String, ?> m`.
                    TokKind::Punct("<") if !in_default => depth += 1,
                    TokKind::Punct(">") if !in_default => depth = depth.saturating_sub(1),
                    TokKind::Punct("=") if depth == 0 => in_default = true,
                    TokKind::Punct(",") if depth == 0 => {
                        params.extend(name.take());
                        in_default = false;
                    }
                    TokKind::Ident(n) if depth == 0 && !in_default => name = Some(n),
                    _ => {}
                }
            }
            params.extend(name);
            self.expect("->")?;
        }
        let body = self.grouped(false, |p| p.stmts_until(Some(open)))?;
        self.expect("}")?;
        Ok(Expr::Closure { params, body, pos: open })
    }

    /// Whether a closure starts with parameters: `a, b ->`, `String s ->`,
    /// `Map opts = [:], Object... rest ->` or just `->`. Returns how many
    /// tokens come before the arrow.
    ///
    /// Types and default values are skipped, not parsed: a default may be
    /// any expression with balanced `( )` and `[ ]`. The list ends at the
    /// arrow and may only continue on the next line after a comma.
    fn closure_arrow(&self) -> Option<usize> {
        let mut k = 0;
        // Nesting of `( )` and `[ ]` inside a default value.
        let mut depth = 0usize;
        let mut in_default = false;
        loop {
            let t = self.peek_at(k);
            if depth == 0 {
                if t.is_punct("->") {
                    return Some(k);
                }
                if k > 0 && t.nl_before && !self.peek_at(k - 1).is_punct(",") {
                    return None;
                }
            }
            match &t.kind {
                TokKind::Punct("(" | "[") if in_default => depth += 1,
                TokKind::Punct(")" | "]") if depth > 0 => depth -= 1,
                // Inside a default value anything goes, except what ends
                // a statement or opens another closure.
                TokKind::Punct(";" | "{" | "}" | ")" | "]") | TokKind::Eof => return None,
                _ if depth > 0 => {}
                TokKind::Punct(",") => in_default = false,
                _ if in_default => {}
                TokKind::Punct("=") => in_default = true,
                // Modifiers, types and names: `final Map<String, ?> m`,
                // `String[] a`, `Object... rest`, `@Annotated x`.
                TokKind::Ident(_) | TokKind::Punct("." | "<" | ">" | "?" | "..." | "@") => {}
                TokKind::Punct("[") if self.peek_at(k + 1).is_punct("]") => k += 1,
                _ => return None,
            }
            k += 1;
        }
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

    /// The placeholders of the GString in `def s = <src>`.
    fn placeholders(src: &str) -> Vec<Placeholder> {
        match ok(&format!("def s = {src}")).remove(0) {
            Stmt::Decl {
                value: Some(Expr::Str { parts, .. }),
                ..
            } => parts
                .into_iter()
                .filter_map(|p| match p {
                    StrPart::Placeholder(p) => Some(p),
                    StrPart::Text(_) => None,
                })
                .collect(),
            other => panic!("{other:?}"),
        }
    }

    fn rendered(src: &str) -> Vec<Option<String>> {
        placeholders(src).iter().map(|p| p.expr.as_ref().map(Expr::render)).collect()
    }

    #[test]
    fn gstring_placeholders() {
        assert_eq!(
            rendered(r#""${env.CHANGE_TITLE} $params.B ${ -> x } ${} ${def v = 1; v} ${a.b(c)}""#),
            [
                Some("env.CHANGE_TITLE".into()),
                Some("params.B".into()),
                Some("x".into()),
                None,
                Some("{ ... }".into()),
                Some("a.b(...)".into()),
            ]
        );
        assert_eq!(rendered("/a $b ${c} d/"), [Some("b".into()), Some("c".into())]);
        assert_eq!(rendered("$/a $b.c ${d} $$e/$"), [Some("b.c".into()), Some("d".into())]);
        // Groovy reads what is in `${...}` as a closure body.
        for src in [
            r#""${a b c}""#,
            r#""${if (x) { 1 } else { 2 }}""#,
            r#""${w -> w << x}""#,
            "\"${ a /* } */ }\"",
        ] {
            assert_eq!(placeholders(src).len(), 1, "{src}");
        }
        assert!(placeholders(r#""plain $5""#.replace("$5", "\\$5").as_str()).is_empty());
    }

    #[test]
    fn placeholder_positions_point_into_the_file() {
        let p = &placeholders("\"x ${env.A} $b\"")[..];
        assert_eq!(
            (p[0].span.start, p[0].span.end),
            (Pos { line: 1, col: 12 }, Pos { line: 1, col: 20 })
        );
        let Some(Expr::Prop { obj, .. }) = &p[0].expr else {
            panic!("{:?}", p[0].expr);
        };
        assert_eq!(obj.pos(), Pos { line: 1, col: 14 });
        assert!(!p[1].braced);
        assert_eq!(p[1].expr.as_ref().unwrap().pos(), Pos { line: 1, col: 22 });
        // A placeholder inside a placeholder.
        let outer = &placeholders("\"${\"in ${b}\"}\"")[0];
        let Some(Expr::Str { parts, .. }) = &outer.expr else {
            panic!("{:?}", outer.expr);
        };
        let StrPart::Placeholder(inner) = &parts[1] else {
            panic!("{parts:?}");
        };
        assert_eq!(inner.expr.as_ref().unwrap().pos(), Pos { line: 1, col: 18 });
        // Across lines of a triple-quoted string.
        let p = &placeholders("\"\"\"a\n  ${\n    b.c\n  }\"\"\"")[0];
        assert_eq!((p.span.start, p.span.end), (Pos { line: 2, col: 3 }, Pos { line: 4, col: 4 }));
        assert_eq!(p.expr.as_ref().unwrap().pos(), Pos { line: 3, col: 6 });
    }

    #[test]
    fn errors_in_placeholders() {
        // Jenkins rejects each of these; the error is where it is in the file.
        for (src, line, col) in [
            ("def s = \"${a +}\"", 1, 15),
            ("def s = \"${foo(}\"", 1, 16),
            ("def s = \"x ${a # b}\"", 1, 16),
            ("def s = /a ${b +} c/", 1, 17),
            ("def s = $/a ${b +} c/$", 1, 18),
            ("def s = \"\"\"a\n  ${b +}\"\"\"", 2, 8),
            ("def s = \"${a)}\"", 1, 13),
        ] {
            let e = fails(src);
            assert_eq!((e.pos.line, e.pos.col), (line, col), "{src}: {}", e.message);
        }
    }

    #[test]
    fn dynamic_member_names() {
        let render = |src: &str| match &ok(src)[0] {
            Stmt::Expr(e) => e.render(),
            other => panic!("{src}: {other:?}"),
        };
        // A name or a plain string in parentheses is the member's name.
        assert_eq!(render("env.(name)"), "env.name");
        assert_eq!(render("this.('echo') 'x'"), "this.echo(...)");
        assert_eq!(render("this.((echo))()"), "this.echo(...)");
        // Any other expression names it at run time.
        assert_eq!(render("env.(a.split('=')[0]) = 1"), "env[...] = ...");
        assert_eq!(render("this.(\"$n\") 'x'"), "this[...](...)");
        assert_eq!(render("a?.{ 2 }"), "a[...]");
        // Indexes and dynamic members start command calls and take closures.
        assert_eq!(render("a[0] 'x'"), "a[...](...)");
        assert_eq!(render("m['k'] x: 1"), "m[...](...)");
        assert_eq!(render("a[0] { }"), "a[...](...)");
        assert_eq!(render("this.(n) 'a' b 'c'"), "this.n(...).b(...)");
        for src in [
            "a.\n{ 2 }",
            "env.(n)++",
            "println env\n    .(n)",
            "def p = 'a'.&(n)",
            "println o.@(n)",
        ] {
            ok(src);
        }
        // Still expressions: `a[0] -1` subtracts, and `a[0] [0]` indexes.
        assert_eq!(render("a[0] -1"), "a[...] - 1");
        assert_eq!(render("a[0] [0]"), "a[...][...]");
        for src in ["env.() = 1", "env.(a = 1", "env.(a b) = 1"] {
            fails(src);
        }
    }

    #[test]
    fn command_chains() {
        for src in [
            "a b c",
            "a b c d",
            "take 2.pills of chloroquinine after 6.hours",
            "a b c { echo 'x' }",
            "def x = a b c",
            "x = a b c",
        ] {
            ok(src);
        }
        let Stmt::Expr(Expr::Call { callee, .. }) = &ok("a b c d")[0] else {
            panic!("a b c d");
        };
        assert_eq!(callee.render(), "a(...).c");
        let Stmt::Expr(e) = &ok("echo 'x' y")[0] else {
            panic!("echo 'x' y");
        };
        assert_eq!(e.render(), "echo(...).y");
        // A chain may start with a call in parentheses.
        let Stmt::Expr(e) = &ok("library('lib@main') _")[0] else {
            panic!("library");
        };
        assert_eq!(e.render(), "library(...)._");
        let Stmt::Expr(Expr::Call { callee, .. }) = &ok("foo(1) bar baz")[0] else {
            panic!("foo(1) bar baz");
        };
        assert_eq!(callee.render(), "foo(...).bar");
        ok("foo(1) bar { echo 'x' }");
        // Words that are operators still are.
        ok("def x = foo(1) as String\nif (foo(1) in [1]) { }");
        // Quoted names, and chains that start with a string (#67, #68).
        let Stmt::Expr(e) = &ok("foo 'a' b 'c'")[0] else {
            panic!("foo 'a' b 'c'");
        };
        assert_eq!(e.render(), "foo(...).b(...)");
        let Stmt::Expr(e) = &ok("sh script: \"find -name \"${n}\" | egrep .\"")[0] else {
            panic!("#68");
        };
        assert_eq!(e.render(), "sh(...).$(...). | egrep .");
        for src in [
            "n << 'a.b.C' {\n    x()\n}",
            "'a.b.C' { x() }",
            "\"a${b}\" { x() }",
            "def a = \"x\" foo 1",
            "def a = \"x\"${n}",
            "\"x\" foo",
        ] {
            ok(src);
        }
        // `stage 'S' { }` gives the closure to 'S', as Groovy does.
        let Stmt::Expr(Expr::Call { args, .. }) = &ok("stage 'S' { echo 'x' }")[0] else {
            panic!("stage 'S'");
        };
        assert!(matches!(&args[..], [Arg::Pos(Expr::Call { .. })]), "{args:?}");
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
        ok("(a, b) = [1, 2]\n(a,\n  b) = f()");
        ok("def ok = (a != 'x'\n  &&\n  (b == null || b == 'y'))");
        ok("if (a\n  || b) { c() }");
        ok("echo(a\n  + b)\nl = [a\n  + 'x']");
        ok("outer:\nfor (a in b) { break outer }\ninner: while (true) { break inner }");
        ok("def a = (String[]) b\ndef c = (List<String>) d\ndef e = (java.util.List) f");
        ok("def a = (b.c)[0]\ndef d = (b.c)(1)\ndef e = (b)\ndef f = (b) ? c : d");
        ok("x.each {\n  k, v ->\n  echo k\n}\nx.each { k,\n  v -> echo k }");
        ok("x.each { a = [1, 2]\n  b = a }");
    }

    #[test]
    fn closure_parameters() {
        let params = |src: &str| match ok(src).remove(0) {
            Stmt::Decl {
                value: Some(Expr::Closure { params, body, .. }),
                ..
            } => (params, body.len()),
            other => panic!("{src}: {other:?}"),
        };
        assert_eq!(params("def c = { -> 1 }"), (vec![], 1));
        assert_eq!(params("def c = { a, b -> 1 }"), (vec!["a".to_string(), "b".to_string()], 1));
        assert_eq!(
            params("def c = { Map<String, ?> m = [:], String... rest, int n = f(1, 2) -> 1 }"),
            (vec!["m".to_string(), "rest".to_string(), "n".to_string()], 1)
        );
        assert_eq!(
            params("def c = { final String[] a, b = -1, c = b ? 'x' : 'y' -> 1 }").0,
            ["a", "b", "c"]
        );
        // No arrow on the first line: everything is the body.
        assert_eq!(params("def c = { a = 1\n b = { x -> x } }"), (vec![], 2));
        assert_eq!(params("def c = { a }"), (vec![], 1));
        // Without an arrow, `a, b` is not a parameter list.
        fails("def c = { a, b }");
    }

    #[test]
    fn syntax_errors() {
        fails("pipeline {\n  agent any\n");
        // Two strings with a comma missing between them: `sh 'x' 'y'` is the
        // chain `sh('x').'y'` to Groovy, but nothing follows a string.
        fails("sh 'x' 'y' ,");
        fails("stage('a') { sh 'x' }}");
        fails("sh \"unterminated");
        // A newline still ends the statement outside brackets, and inside
        // a closure body even when the closure sits in brackets.
        fails("def x = a\n  && b");
        fails("f({ ->\n  def z = a\n  && b\n})");
        // The colon of a label stays on the line of its name.
        fails("outer\n: for (a in b) { }");
    }
}
