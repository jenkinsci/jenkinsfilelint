# jenkins-lint

[![PyPI](https://img.shields.io/pypi/v/jenkins-lint?color=0a7bbb)](https://pypi.org/project/jenkins-lint)
[![CI](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml)
[![License](https://img.shields.io/github/license/jenkinsci/jenkins-lint?color=0a7bbb)](LICENSE)
[![Docs](https://img.shields.io/badge/docs-jenkinsci.github.io-0a7bbb)](https://jenkinsci.github.io/jenkins-lint/)

A fast, offline linter for Jenkins. Pipelines, shared libraries, security.

[**Docs**](https://jenkinsci.github.io/jenkins-lint/) | [**Rules**](https://jenkinsci.github.io/jenkins-lint/rules/) | [**Install**](https://jenkinsci.github.io/jenkins-lint/install/)

![jenkins-lint checking a Jenkinsfile](https://raw.githubusercontent.com/jenkinsci/jenkins-lint/main/docs/assets/demo.gif)

- **Offline and fast.** One Rust binary, no Jenkins, JVM or Docker. It checks
  17,470 real-world Jenkinsfiles, 3.8 million lines, in 13 seconds on one core.
- **Speaks Jenkins.** Errors come in the words of Jenkins's own validator.
  In November 2026, compared with Jenkins on 17,373 public Jenkinsfiles,
  it missed 6 errors and was stricter on 11 files
  ([results](https://github.com/jenkinsci/jenkins-lint/tree/main/corpus#results)).
- **Finds what Jenkins doesn't.** Secrets and pull request input are followed
  through variables to the `sh` step that runs them.
- **Fixes, the ruff way.** `--fix`, `--unsafe-fixes` and `--diff`, with every
  fix marked safe or unsafe.
- **Fits where you lint.** pre-commit, GitHub code scanning and annotations,
  Jenkins Warnings NG, Docker, and a baseline for repositories with history.

## Getting started

```console
$ pip install jenkins-lint      # or: uv tool install jenkins-lint / pipx install jenkins-lint
$ jenkins-lint .                # every Jenkinsfile and shared library file in this repository
```

```console
$ jenkins-lint Jenkinsfile
error: Invalid condition "alway" - valid conditions are [always, changed, fixed, regression, aborted, success, unsuccessful, unstable, failure, notBuilt, cleanup]
  --> Jenkinsfile:19:9
   |
19 |         alway {
   |         ^

high[secret-interpolation]: TOKEN is a secret, and Groovy interpolates it into the sh script: the process arguments show it, and shell metacharacters in it run. Use single quotes, as in sh '... "$TOKEN" ...', so that the shell reads it from the environment.
  --> Jenkinsfile:13:45
   |
13 |                     sh "npm publish --token ${TOKEN}"
   |                                             ^^^^^^^^
   |
   = help: https://jenkinsci.github.io/jenkins-lint/rules/secret-interpolation/

1 finding (1 high)
```

No Python? Download a [static binary](https://jenkinsci.github.io/jenkins-lint/install/#binary)
or run `ghcr.io/jenkinsci/jenkins-lint:2`.

## Security rules

Of 17,470 public Jenkinsfiles and shared library files on GitHub, **2,105 have
a high-severity finding**, and 1,460 put a secret into a shell command.
Every run checks for these, in the same pass as the syntax:

| Rule | Finds |
|---|---|
| [`script-injection`](https://jenkinsci.github.io/jenkins-lint/rules/script-injection/) | A pull request title, branch name, webhook payload or build parameter interpolated into a shell step |
| [`secret-interpolation`](https://jenkinsci.github.io/jenkins-lint/rules/secret-interpolation/) | A secret interpolated into a shell step |
| [`controller-execution`](https://jenkinsci.github.io/jenkins-lint/rules/controller-execution/) | Steps that run on the Jenkins controller |
| [`unpinned-library`](https://jenkinsci.github.io/jenkins-lint/rules/unpinned-library/) | A shared library loaded from a branch, like `@Library('lib@main')` |
| [`unpinned-image`](https://jenkinsci.github.io/jenkins-lint/rules/unpinned-image/) | A container image without a tag, with `latest`, or without a digest |
| [`unrestricted-input`](https://jenkinsci.github.io/jenkins-lint/rules/unrestricted-input/) | An `input` that any user may approve (pedantic) |

A value is followed the way Groovy evaluates it, so the finding points at
where it came from:

```console
high[script-injection]: CHANGE_BRANCH is the name of the pull request's branch, and Groovy puts it into branch and interpolates that into the sh script: ...
 --> Jenkinsfile:8:38
  |
7 |                     def branch = "${env.CHANGE_BRANCH}"
  |                                   -------------------- CHANGE_BRANCH is read here, into branch
8 |                     sh "git checkout ${branch}"
  |                                      ^^^^^^^^^
```

## Use it everywhere

**pre-commit**

```yaml
- repo: https://github.com/jenkinsci/jenkins-lint
  rev: v2.1.0
  hooks:
    - id: jenkins-lint
```

**GitHub Actions**, with findings on the lines of the pull request:

```yaml
- run: pipx run jenkins-lint --output-format github .
```

Or `--output-format sarif` for [code scanning](https://jenkinsci.github.io/jenkins-lint/rules/#output).

**Jenkins**, in the pipeline itself:

```groovy
agent { docker { image 'ghcr.io/jenkinsci/jenkins-lint:2' } }
steps { sh 'jenkins-lint .' }
```

**An existing repository**: `jenkins-lint --write-baseline .` records today's
findings, and from then on only new ones fail.

## Offline, or with your Jenkins

The offline check knows Groovy and Declarative Pipeline. Only your Jenkins
knows its plugins, so `--remote` also sends the files that pass to it:

| | Offline (default) | `--remote` |
|---|---|---|
| Needs | nothing | a Jenkins server |
| Speed | milliseconds | a round trip per file |
| Groovy syntax, Declarative structure | ✓ | ✓ |
| Security rules | ✓ | ✓ |
| Plugin steps, parameters, agent types and tools | | ✓ |

## Learn more

[Usage](https://jenkinsci.github.io/jenkins-lint/usage/) ·
[Configuration](https://jenkinsci.github.io/jenkins-lint/configuration/) ·
[How it works](https://jenkinsci.github.io/jenkins-lint/how-it-works/) ·
[Migrating from jenkinsfilelint](https://jenkinsci.github.io/jenkins-lint/migrating/) ·
[Contributing](CONTRIBUTING.md)

jenkins-lint is MIT licensed, and was `jenkinsfilelint` before 2.0.
