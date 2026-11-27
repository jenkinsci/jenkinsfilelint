//! jenkins-lint checks Jenkinsfiles offline, without a Jenkins controller:
//! Groovy syntax, Declarative Pipeline structure and security rules, with
//! fixes. `--remote` also validates them with a Jenkins server.

pub mod ast;
pub mod baseline;
pub mod check;
pub mod cli;
pub mod diag;
pub mod discover;
pub mod lexer;
pub mod output;
pub mod parser;
pub mod remote;
pub mod rules;
pub mod sarif;
pub mod validate;
