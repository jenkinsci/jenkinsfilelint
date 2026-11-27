# Migrating to 2.0

jenkins-lint 2.0 is the project that was called `jenkinsfilelint` until
1.7.2, rebuilt around the offline check and rewritten in Rust. 2.0 keeps no
aliases for the old names and options: change them once, with the table
below. `jenkinsfilelint` 1.7.2 stays on PyPI as its last release, and is not
updated.

## Old and new

| 1.x | 2.0 |
|---|---|
| `pip install jenkinsfilelint` | `pip install jenkins-lint` (or `pipx install`, `uv tool install`) |
| `jenkinsfilelint Jenkinsfile` | `jenkins-lint Jenkinsfile`, or `jenkins-lint check Jenkinsfile` |
| `repo: https://github.com/jenkinsci/jenkinsfilelint` | `repo: https://github.com/jenkinsci/jenkins-lint` |
| hooks `jenkinsfilelint`, `jenkinsfilelint-offline`, `jenkinsfilelint-audit` | hook `jenkins-lint` |
| Checks with Jenkins when `JENKINS_URL` is set | Checks offline; `--remote` also checks with Jenkins |
| `--jenkins-url URL` | `--remote URL` |
| `--offline` | the default: remove the option |
| `--local`, `jenkinsfilelint server ...` | removed: use `--remote URL` with your own Jenkins |
| `--audit`, a run of its own | every run checks security too: remove the option |
| `// jenkinsfilelint: ignore[rule]` | `// jenkins-lint: ignore[rule]` |
| `JENKINSFILELINT_ENGINE`, `JENKINSFILELINT_SERVER_IMAGE` | removed |
| help links `…/jenkinsfilelint/audit/<rule>/` | `…/jenkins-lint/rules/<rule>/` |
| SARIF rule `syntax-error` | `invalid-pipeline`, for every error Jenkins would report |
| a Python package and `jenkinsfilelint-engine` | one binary wheel, which runs without Python |

An option that 2.0 does not know stops the run with exit code 2, so a
leftover `--offline` or `--audit` shows up at once.

New in 2.0: `--fix`, `--unsafe-fixes`, `--show-fixes` and `--diff`, named
and working as in ruff, rewrite the scripts that the security checks can fix.

## What to change

**pre-commit.** Point the repo at the new name, use the `jenkins-lint` hook
and update `rev`:

```yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: v2.0.0
    hooks:
      - id: jenkins-lint
```

A hook that set `JENKINS_URL` to check with Jenkins now checks offline.
To keep the Jenkins check, add `args: ["--remote"]` (see
[Pre-commit hook](pre-commit.md#also-checking-with-your-jenkins)).

**CI scripts.** Install `jenkins-lint` and run `jenkins-lint`. A job that ran
the syntax check and `--audit` separately needs one run now. A job that
relied on `JENKINS_URL` to pick remote checking adds `--remote`: Jenkins sets
`JENKINS_URL` in every build, which is why it no longer decides the mode.

**Security findings now fail a plain check.** Before 2.0 only `--audit`
reported them. If a run starts failing on a finding you accept, turn it off
with a `// jenkins-lint: ignore[rule] why` comment, or for every file with
`--ignore RULE`. See [Security rules](rules/README.md).

**Shared library files.** A directory search now checks `vars/*.groovy` and
`src/**/*.groovy` too, as `--audit` did, so a syntax error there fails the
run. Leave them out with `--skip 'vars/*.groovy'` if you need to.

**Docker mode.** `--local` started a Jenkins container with a fixed set of
plugins. The offline check now covers what it was for, without Docker, and
`--remote` checks against your own Jenkins and its real plugins.

**Platforms without a prebuilt wheel.** `pip install jenkins-lint` builds it
from source there, which needs a [Rust toolchain](https://rustup.rs). The
wheels cover Linux (x86_64, aarch64, i686, armv7; glibc and musl), macOS
and Windows.
