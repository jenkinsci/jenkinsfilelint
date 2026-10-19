# jenkinsfilelint-engine

Check a Jenkinsfile without a Jenkins controller: no server, no Docker, no
JVM. One ~500 KB binary that checks Groovy syntax and Declarative Pipeline
structure, and reports errors with the same wording as Jenkins's own
validator.

This is the offline engine behind `jenkinsfilelint --offline` and the
`jenkinsfilelint-offline` pre-commit hook. Most people want
[jenkinsfilelint](https://github.com/jenkinsci/jenkinsfilelint) itself, which
installs this package. The remote and Docker modes of `jenkinsfilelint` stay
the exact check; this one is the fast one. See [DESIGN.md](DESIGN.md).

```console
$ jenkinsfilelint-engine Jenkinsfile
Jenkinsfile:2:5: error: Not a valid section definition: "agent". Some extra configuration is required.
Jenkinsfile:9:9: error: Expected one of "steps", "stages", or "parallel" for stage "Test"
Jenkinsfile:10:13: error: Unknown stage section "step". Starting with version 0.5, steps in a stage must be in a ‘steps’ block.
```

## Install

```bash
pip install jenkinsfilelint-engine
```

PyPI has prebuilt wheels for Linux (x86_64, aarch64; glibc and musl), macOS
(x86_64, arm64) and Windows (x86_64), for any Python 3.10+. The wheel puts the
`jenkinsfilelint-engine` binary in the environment's scripts directory;
`python -m jenkinsfilelint_engine` runs it too, and
`jenkinsfilelint_engine.find_bin()` returns its path. On other platforms pip
builds it from the sdist, which needs a [Rust toolchain](https://rustup.rs).

## Build and run

```bash
cd crates/jenkinsfilelint
cargo build --release
./target/release/jenkinsfilelint-engine path/to/Jenkinsfile [more files...]
./target/release/jenkinsfilelint-engine --format json Jenkinsfile
```

Exit code is `0` when no file has an error, `1` when any does, and `2` for
usage errors. Warnings do not fail the run.

`--format json` prints one object per file:

```json
[{"file": "Jenkinsfile", "declarative": true, "diagnostics": [
  {"line": 2, "column": 5, "severity": "error", "message": "Not a valid section definition: ..."}]},
 {"file": "vars/notify.groovy", "declarative": false, "diagnostics": []},
 {"file": "missing", "error": "No such file or directory (os error 2)"}]
```

`declarative` is `false` when the file has no `pipeline` block (a Scripted
Pipeline, a shared-library step, plain Groovy): its Groovy syntax was checked,
but there was no Declarative structure to check.

## What it checks

- **Groovy syntax**, including every string form (`'...'`, `"${...}"`,
  triple-quoted, slashy, dollar-slashy), command calls with named arguments
  (`sh script: 'ls', returnStdout: true`), trailing closures, `@Library(...) _`,
  method and class definitions.
- **Declarative structure**: required `agent` and `stages`; allowed and
  duplicate sections; each stage has exactly one of `steps`, `stages`,
  `parallel` or `matrix`; no nested `parallel`/`matrix`; `agent` forms and
  built-in agent parameters; `environment` keys and values; `options`,
  `triggers`, `parameters`, `tools`, `libraries`, `input`; `when` conditions
  and their nesting; `post` conditions; `matrix` axes and excludes; steps that
  are blocked in Declarative (`stage`, `properties`, mixed `parallel`);
  `pipeline {}` inside another block.

## What it cannot check

Anything that depends on the plugins installed on a controller:

- whether a step exists and takes the given parameters,
- agent types added by plugins (reported as a warning, not an error),
- tool installations configured on the controller,
- script-security sandbox rules.

Use the remote or `--local` Docker mode of `jenkinsfilelint` for those.

## How close is it to the official validator?

The [pipeline-model-definition plugin](https://github.com/jenkinsci/pipeline-model-definition-plugin)
ships the test Jenkinsfiles for Jenkins's own validator. Running the offline
linter over them:

| | Result |
|---|---|
| Valid files with no false errors | 287 / 287 |
| Invalid files caught | 90 / 90 |
| ... with the same message as Jenkins | 90 / 90 |
| Invalid files that need plugins or a controller to judge | 33 (not counted above) |

Reproduce with:

```bash
cargo build --release
python3 scripts/corpus_check.py   # clones the plugin at a pinned commit
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 scripts/corpus_check.py
pip install .   # builds the wheel with maturin and installs it
```

When the engine changes, bump `version` in `Cargo.toml` and the
`jenkinsfilelint-engine==` pin in the root `pyproject.toml` together.

`cargo test` also lints every case in `tests/jenkins/` and compares the
result with `tests/jenkins/expected.txt`, which records what Jenkins's own
validator said about the same file and explains each known difference.
