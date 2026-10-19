//! A small syntax tree for Groovy as written in Jenkinsfiles.
//!
//! It keeps what the declarative checks need (calls, closures, literals,
//! assignments) and folds everything else into generic nodes.

use crate::diag::Pos;

#[derive(Debug, Clone)]
#[allow(dead_code)] // some fields are for the planned step-parameter checks
pub enum Expr {
    Ident {
        name: String,
        pos: Pos,
    },
    Str {
        value: String,
        interp: bool,
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
    Paren {
        inner: Box<Expr>,
        pos: Pos,
    },
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
            | Expr::New { pos, .. }
            | Expr::Paren { pos, .. } => *pos,
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
            Expr::Paren { inner, .. } => format!("({})", inner.render()),
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
