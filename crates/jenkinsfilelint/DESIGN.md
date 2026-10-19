# Offline mode: design notes

## Problem

`jenkinsfilelint` checks a Jenkinsfile by sending it to a Jenkins
controller, either an existing one or a container started with `--local`.
Both are accurate, but both need a controller: credentials or Docker, a
JVM, and seconds to minutes of start-up. That keeps the hook out of many
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
src/main.rs      CLI: text or JSON output, exit codes
python/          jenkinsfilelint_engine: finds the binary installed by the wheel
scripts/corpus_check.py  agreement test against the plugin's own samples
```

The parser is strict where Jenkins users write code every day (command
calls, named arguments, closures, GStrings, maps, lists) and lenient where
declarative validation never looks (class bodies, `switch`, `for` headers),
where it only checks that brackets balance.

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
2. **Distribution.** Done: this crate is published to PyPI as
   `jenkinsfilelint-engine`, maturin binary wheels (`bindings = "bin"`,
   like ruff) for Linux, macOS and Windows plus an sdist. `jenkinsfilelint`
   stays pure Python, depends on the engine pinned to the exact version, and
   runs it for `--offline` and the `jenkinsfilelint-offline` pre-commit
   hook. Keeping the root package pure Python matters because pre-commit
   installs hooks from source: a maturin-built root package would make every
   pre-commit user compile Rust. The dependency is limited to platforms with
   a wheel, so other platforms keep the remote and Docker modes. GitHub
   Releases and Homebrew later.
3. **Editor and web.** JSON output already exists for editors; add a
   WebAssembly build for a web playground.
4. **Groovy coverage.** Keep adding real-world Jenkinsfiles to the test set
   and fix any parse gaps they reveal.
