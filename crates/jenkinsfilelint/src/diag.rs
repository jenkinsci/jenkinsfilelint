use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub pos: Pos,
    pub severity: Severity,
    pub message: String,
}

/// The result of checking one file.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub diagnostics: Vec<Diagnostic>,
    /// A `pipeline { }` block was found and checked. False for scripted
    /// Pipelines, plain Groovy and files that do not parse.
    pub declarative: bool,
}

impl Diagnostic {
    pub fn error(pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            pos,
            severity: Severity::Error,
            message: message.into(),
        }
    }
    pub fn warning(pos: Pos, message: impl Into<String>) -> Self {
        Diagnostic {
            pos,
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warning => write!(f, "warning"),
        }
    }
}
