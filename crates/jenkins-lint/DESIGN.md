# Design notes

## Problem

Jenkins's own validator, `/pipeline-model-converter/validate`, is accurate,
but it needs a controller: a server and credentials, or Docker and a JVM,
and seconds to minutes of start-up. That keeps the hook out of many
pre-commit setups and most editors.

GitHub Actions has `actionlint`, a single binary that catches workflow
mistakes offline. Jenkins has nothing like it. The closest prior work:

- `jenkinsfile-runner lint` embeds Jenkins itself, so it still needs a JVM
  and a large download.
- `jdp` (Rust, 2020) parsed the declarative grammar offline but is
  unmaintained and skipped Groovy.
- `npm-groovy-lint` checks Groovy style, not pipeline structure.

## Goal

A binary that runs in milliseconds, needs nothing installed, and agrees
with the official validator wherever the answer does not depend on
plugins. Where it does depend on plugins, stay silent or warn, never fail
a valid file.

## Why Rust

Both Go and Rust produce small static binaries. Rust won on three points:

1. **Python packaging.** Existing users install with `pip` and run the
   pre-commit hook with `language: python`. `maturin` (used by ruff and uv)
   puts a Rust binary into a platform wheel, so those users get the offline
   mode with no config change.
2. **Parser work.** No off-the-shelf Groovy parser handles Jenkins idioms:
   tree-sitter-groovy failed on 42 of the 293 official valid samples,
   mostly on `sh script: 'x', returnStdout: true` and `@Library('x') _`.
   A hand-written parser is needed in either language, and Rust's enums and
   pattern matching suit syntax trees and diagnostics.
3. **Web playground later.** A paste-your-Jenkinsfile page is the easiest
   way to show the tool; Rust compiles to a small WebAssembly module.

## Architecture

```
src/lexer.rs     tokens; all Groovy string forms; newline tracking
src/parser.rs    recursive-descent parser -> small AST (ast.rs)
src/validate.rs  declarative structure checks, official message wording
src/audit/       security rules, findings, ignore comments, fixes, SARIF;
                 taint.rs follows values from sources to the steps that run them
src/cli.rs       the command: options, output, exit codes
src/discover.rs  the files found in a directory, --include and --skip
src/remote.rs    --remote: Jenkins's validate endpoint
src/main.rs      checking one file: lint, audit, fixes; JSON helpers
scripts/corpus_check.py  agreement test against the plugin's own samples
```

The parser is strict where Jenkins users write code every day (command
calls and command chains, named arguments, closures, GStrings, maps, lists)
and lenient where declarative validation never looks (class bodies,
`switch`, `for` headers), where it only checks that brackets balance.

GString placeholders are parsed as Groovy compiles them, so a mistake inside
`"${...}"` is an error in the file. `$a.b` is an expression; what is inside
`${...}` is a closure body, so it may hold statements (`${def v = 1; v}`) or
start with `->`. Each placeholder keeps where it starts and ends in the file,
which the security checks planned for 1.7.0 (#65) need to point at the exact
value that is interpolated into a shell step.

Messages are copied from the plugin's `Messages.properties`, so a user sees
the same text offline and online.

## Measuring accuracy

`scripts/corpus_check.py` clones the plugin at a pinned commit and runs the
linter over its test resources:

- every file outside `errors/` must produce no error (287/287 today),
- every file in `errors/` whose expectation does not need plugins must be
  caught (90/90) with the expected message (90/90).

Real-world check: 37 Jenkinsfiles from Apache (HBase, Hadoop, Cassandra,
Camel, ZooKeeper, Maven) and Jenkins projects pass clean, and injected
mistakes in them are all caught.

## Next steps

1. **Step catalog (warnings).** Generate a list of steps and their
   parameters from the public Pipeline Steps Reference, ship it inside the
   binary, and warn on unknown steps or parameters. Let users declare
   shared-library steps in a config file.
2. **Distribution.** Done in 2.0: this crate is the `jenkins-lint`
   package on PyPI, maturin binary wheels (`bindings = "bin"`, like ruff)
   for Linux, macOS and Windows plus an sdist. The whole command is Rust,
   `--remote` included (ureq with rustls). The root `pyproject.toml` is
   only for pre-commit, which installs hooks from source: it has no code
   and depends on the exact `jenkins-lint` version, so pre-commit users get
   the wheel instead of compiling Rust. GitHub Releases and Homebrew later.
3. **Editor and web.** JSON output already exists for editors; add a
   WebAssembly build for a web playground.
4. **Groovy coverage.** Keep adding real-world Jenkinsfiles to the test set
   and fix any parse gaps they reveal.
