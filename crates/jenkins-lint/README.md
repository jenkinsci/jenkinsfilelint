# jenkins-lint

Catch Jenkinsfile mistakes before they break your CI, without a Jenkins
controller: no server, no Docker, no JVM. One small binary checks Groovy
syntax, Declarative Pipeline structure and security, and reports errors with
the same wording as Jenkins's own validator. `--remote` also validates the
files with your Jenkins, which knows its plugins.

![demo](https://raw.githubusercontent.com/jenkinsci/jenkins-lint/main/docs/assets/demo.gif)

```console
$ jenkins-lint Jenkinsfile
Jenkinsfile:2:5: error: Not a valid section definition: "agent". Some extra configuration is required.
Jenkinsfile:9:9: error: Expected one of "steps", "stages", or "parallel" for stage "Test"
Jenkinsfile:14:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/rules/script-injection/
1 finding (1 high)
```

## Install

```bash
pip install jenkins-lint      # or: pipx install jenkins-lint, uv tool install jenkins-lint
```

PyPI has prebuilt wheels for Linux (x86_64, aarch64, i686, armv7; glibc and
musl), macOS (x86_64, arm64) and Windows (x86_64, x86). They hold only the
`jenkins-lint` binary, which needs no Python to run. On other platforms pip
builds it from the sdist, which needs a [Rust toolchain](https://rustup.rs).

As a [pre-commit](https://pre-commit.com/) hook:

```yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: # the latest version
    hooks:
      - id: jenkins-lint
```

Documentation: https://jenkinsci.github.io/jenkins-lint/. Source and
design notes: https://github.com/jenkinsci/jenkins-lint.

## Build and run

```bash
cd crates/jenkins-lint
cargo build --release
./target/release/jenkins-lint path/to/Jenkinsfile [more files or directories...]
./target/release/jenkins-lint --format json Jenkinsfile
```

Exit code is `0` when no file has an error or a finding, `1` when any does,
and `2` for usage errors. Warnings do not fail the run.

`--format json` prints one object per file:

```json
[{"file": "Jenkinsfile", "declarative": true, "diagnostics": [
  {"line": 2, "column": 5, "severity": "error", "message": "Not a valid section definition: ..."}],
  "findings": [], "fixed": []},
 {"file": "vars/notify.groovy", "declarative": false, "diagnostics": [], "findings": [], "fixed": []},
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
  `pipeline {}` inside the block of another step (`node { pipeline { } }`).

## What it cannot check

Anything that depends on the plugins installed on a controller:

- whether a step exists and takes the given parameters,
- agent types added by plugins (reported as a warning, not an error),
- tool installations configured on the controller,
- script-security sandbox rules.

Use `jenkins-lint --remote` with your Jenkins for those.

## How close is it to the official validator?

The [pipeline-model-definition plugin](https://github.com/jenkinsci/pipeline-model-definition-plugin)
ships the test Jenkinsfiles for Jenkins's own validator. Running jenkins-lint
over them:

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

A release bumps `version` in `Cargo.toml`, and the version and the
`jenkins-lint==` pin in the root `pyproject.toml` with it:
`scripts/check_version.py` checks that they agree.

`cargo test` also lints every case in `tests/jenkins/` and compares the
result with `tests/jenkins/expected.txt`, which records what Jenkins's own
validator said about the same file and explains each known difference.
