//! A small syntax tree for Groovy as written in Jenkinsfiles.
//!
//! It keeps what the declarative checks need (calls, closures, literals,
//! assignments) and folds everything else into generic nodes.

use crate::diag::{Pos, Span};

#[derive(Debug, Clone)]
#[allow(dead_code)] // some fields are for the planned step-parameter checks
pub enum Expr {
    Ident {
        name: String,
        pos: Pos,
    },
    /// A string literal. `parts` holds a GString's text and placeholders
    /// in order, and is empty when it has no placeholder. Map keys and
    /// named-argument keys keep only their text.
    Str {
        value: String,
        interp: bool,
        parts: Vec<StrPart>,
        pos: Pos,
    },
    Num {
        text: String,
        pos: Pos,
    },
    List {
        items: Vec<Expr>,
        pos: Pos,
    },
    Map {
        entries: Vec<(Expr, Expr)>,
        pos: Pos,
    },
    Closure {
        params: Vec<String>,
        body: Vec<Stmt>,
        pos: Pos,
    },
    /// `foo(a, b: 1) { }` or the command form `foo a, b: 1`.
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
        parens: bool,
        pos: Pos,
    },
    Prop {
        obj: Box<Expr>,
        name: String,
        pos: Pos,
    },
    Index {
        obj: Box<Expr>,
        index: Vec<Expr>,
        pos: Pos,
    },
    Unary {
        op: &'static str,
        expr: Box<Expr>,
        pos: Pos,
    },
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        pos: Pos,
    },
    Ternary {
        cond: Box<Expr>,
        then: Box<Expr>,
        other: Box<Expr>,
        pos: Pos,
    },
    Assign {
        op: &'static str,
        target: Box<Expr>,
        value: Box<Expr>,
        pos: Pos,
    },
    New {
        ty: String,
        args: Vec<Arg>,
        pos: Pos,
    },
}

/// A piece of a GString: text, or a placeholder.
#[derive(Debug, Clone)]
pub enum StrPart {
    // Not read yet: kept for checks of the text around a placeholder.
    #[allow(dead_code)]
    Text(String),
    Placeholder(Placeholder),
}

/// A `$name` or `${...}` placeholder of a GString.
#[derive(Debug, Clone)]
pub struct Placeholder {
    /// What it evaluates. `$a.b` and `${a.b}` are the expression `a.b`, and
    /// so is the lazy `${ -> a.b }`. A `${...}` that holds statements, such
    /// as `${def v = 1; v}`, is a closure, as Groovy reads it. `${}` is None.
    pub expr: Option<Expr>,
    /// `${...}` rather than `$name`.
    #[allow(dead_code)] // for fixes that rewrite a placeholder
    pub braced: bool,
    /// From the `$` to just past the placeholder.
    pub span: Span,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Arg {
    Pos(Expr),
    Named { key: String, value: Expr, pos: Pos },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Stmt {
    Expr(Expr),
    Decl {
        names: Vec<String>,
        value: Option<Expr>,
        pos: Pos,
    },
    MethodDef {
        name: String,
        body: Vec<Stmt>,
        pos: Pos,
    },
    /// if/for/while/try/switch and friends, with their nested bodies.
    Control {
        kind: &'static str,
        bodies: Vec<Vec<Stmt>>,
        pos: Pos,
    },
    /// import, package, class definitions and other statements the
    /// declarative checks never need to look inside.
    Other {
        kind: &'static str,
        pos: Pos,
    },
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::Ident { pos, .. }
            | Expr::Str { pos, .. }
            | Expr::Num { pos, .. }
            | Expr::List { pos, .. }
            | Expr::Map { pos, .. }
            | Expr::Closure { pos, .. }
            | Expr::Call { pos, .. }
            | Expr::Prop { pos, .. }
            | Expr::Index { pos, .. }
            | Expr::Unary { pos, .. }
            | Expr::Binary { pos, .. }
            | Expr::Ternary { pos, .. }
            | Expr::Assign { pos, .. }
            | Expr::New { pos, .. } => *pos,
        }
    }

    /// Source-like rendering used in error messages.
    pub fn render(&self) -> String {
        match self {
            Expr::Ident { name, .. } => name.clone(),
            Expr::Str { value, interp, .. } => {
                if *interp {
                    format!("\"{value}\"")
                } else {
                    format!("'{value}'")
                }
            }
            Expr::Num { text, .. } => text.clone(),
            Expr::List { items, .. } => {
                format!("[{}]", items.iter().map(|e| e.render()).collect::<Vec<_>>().join(", "))
            }
            Expr::Map { .. } => "[...]".into(),
            Expr::Closure { .. } => "{ ... }".into(),
            Expr::Call { callee, .. } => format!("{}(...)", callee.render()),
            Expr::Prop { obj, name, .. } => format!("{}.{name}", obj.render()),
            Expr::Index { obj, .. } => format!("{}[...]", obj.render()),
            Expr::Unary { op, expr, .. } => format!("{op}{}", expr.render()),
            Expr::Binary { op, lhs, rhs, .. } => format!("{} {op} {}", lhs.render(), rhs.render()),
            Expr::Ternary { .. } => "(... ? ... : ...)".into(),
            Expr::Assign { target, .. } => format!("{} = ...", target.render()),
            Expr::New { ty, .. } => format!("new {ty}(...)"),
        }
    }
}

/// A call seen as a declarative "section": `name args { body }`.
#[allow(dead_code)]
pub struct Section<'a> {
    pub name: &'a str,
    pub pos: Pos,
    /// Arguments other than a trailing closure.
    pub args: Vec<&'a Arg>,
    /// Body of the trailing closure, when there is one.
    pub body: Option<&'a [Stmt]>,
    pub parens: bool,
}

impl Stmt {
    pub fn pos(&self) -> Pos {
        match self {
            Stmt::Expr(e) => e.pos(),
            Stmt::Decl { pos, .. } | Stmt::MethodDef { pos, .. } | Stmt::Control { pos, .. } | Stmt::Other { pos, .. } => *pos,
        }
    }

    /// View this statement as `name args { body }` when it is a call on a
    /// plain identifier, or a bare identifier.
    pub fn section(&self) -> Option<Section<'_>> {
        match self {
            Stmt::Expr(e) => e.section(),
            _ => None,
        }
    }
}

impl Expr {
    pub fn section(&self) -> Option<Section<'_>> {
        match self {
            Expr::Ident { name, pos } => Some(Section {
                name,
                pos: *pos,
                args: vec![],
                body: None,
                parens: false,
            }),
            Expr::Call { callee, args, parens, pos } => {
                let Expr::Ident { name, .. } = callee.as_ref() else { return None };
                let mut rest: Vec<&Arg> = args.iter().collect();
                let mut body = None;
                if let Some(Arg::Pos(Expr::Closure { body: b, .. })) = rest.last() {
                    body = Some(b.as_slice());
                    rest.pop();
                }
                Some(Section {
                    name,
                    pos: *pos,
                    args: rest,
                    body,
                    parens: *parens,
                })
            }
            _ => None,
        }
    }
}

/// A statement or an expression, for `visit`.
#[derive(Clone, Copy)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

/// Call `f` on every statement and expression in `stmts`, at any depth:
/// in nested blocks, closures and GString placeholders too. A node comes
/// before the nodes inside it.
pub fn visit<'a>(stmts: &'a [Stmt], f: &mut impl FnMut(Node<'a>)) {
    for s in stmts {
        f(Node::Stmt(s));
        match s {
            Stmt::Expr(e) => visit_expr(e, f),
            Stmt::Decl { value, .. } => {
                if let Some(v) = value {
                    visit_expr(v, f);
                }
            }
            Stmt::MethodDef { body, .. } => visit(body, f),
            Stmt::Control { bodies, .. } => bodies.iter().for_each(|b| visit(b, f)),
            Stmt::Other { .. } => {}
        }
    }
}

fn visit_expr<'a>(e: &'a Expr, f: &mut impl FnMut(Node<'a>)) {
    f(Node::Expr(e));
    let args = |args: &'a [Arg], f: &mut _| {
        for a in args {
            match a {
                Arg::Pos(v) | Arg::Named { value: v, .. } => visit_expr(v, f),
            }
        }
    };
    match e {
        Expr::Str { parts, .. } => {
            for p in parts {
                if let StrPart::Placeholder(Placeholder { expr: Some(x), .. }) = p {
                    visit_expr(x, f);
                }
            }
        }
        Expr::List { items, .. } => items.iter().for_each(|i| visit_expr(i, f)),
        Expr::Map { entries, .. } => {
            for (k, v) in entries {
                visit_expr(k, f);
                visit_expr(v, f);
            }
        }
        Expr::Closure { body, .. } => visit(body, f),
        Expr::Call { callee, args: a, .. } => {
            visit_expr(callee, f);
            args(a, f);
        }
        Expr::New { args: a, .. } => args(a, f),
        Expr::Prop { obj, .. } | Expr::Unary { expr: obj, .. } => visit_expr(obj, f),
        Expr::Index { obj, index, .. } => {
            visit_expr(obj, f);
            index.iter().for_each(|i| visit_expr(i, f));
        }
        Expr::Binary { lhs, rhs, .. } => {
            visit_expr(lhs, f);
            visit_expr(rhs, f);
        }
        Expr::Ternary { cond, then, other, .. } => {
            visit_expr(cond, f);
            visit_expr(then, f);
            visit_expr(other, f);
        }
        Expr::Assign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        Expr::Ident { .. } | Expr::Num { .. } => {}
    }
}
