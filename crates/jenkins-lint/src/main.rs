use std::process::ExitCode;

use jenkins_lint::{check, cli};

fn main() -> ExitCode {
    // The parser recurses: check on a thread with a stack of known size.
    match std::thread::Builder::new().stack_size(check::STACK_SIZE).spawn(run) {
        Ok(handle) => handle.join().unwrap_or(ExitCode::from(2)),
        Err(e) => {
            eprintln!("jenkins-lint: cannot start a thread: {e}");
            ExitCode::from(2)
        }
    }
}

fn run() -> ExitCode {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    // args_os: a file name that is not UTF-8 must not panic.
    match cli::parse_args(std::env::args_os().skip(1), env) {
        Ok(options) => cli::run(options),
        Err(code) => code,
    }
}
