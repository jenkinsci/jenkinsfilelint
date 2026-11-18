# Jenkinsfile Lint

[![CI](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml)
[![codecov](https://codecov.io/gh/jenkinsci/jenkinsfilelint/graph/badge.svg?token=nGrwXORFtI)](https://codecov.io/gh/jenkinsci/jenkinsfilelint)
[![PyPI version](https://img.shields.io/pypi/v/jenkinsfilelint)](https://pypi.org/project/jenkinsfilelint)

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` checks your Declarative Pipeline syntax offline, with no
Jenkins at all, or with your Jenkins's own validator
([`/pipeline-model-converter/validate`](https://www.jenkins.io/doc/book/pipeline/development/#linter))
when you configure one. It's primarily a [pre-commit](https://pre-commit.com/)
hook, but also works as a standalone CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

![demo](https://raw.githubusercontent.com/jenkinsci/jenkinsfilelint/main/demo.gif)

## Table of Contents

- [Quick Start](#quick-start)
- [Which mode?](#which-mode)
- [Pre-commit Hook](#pre-commit-hook)
  - [Offline mode (no Jenkins, Docker or JVM)](#offline-mode-no-jenkins-docker-or-jvm)
  - [Remote mode (with a Jenkins server)](#remote-mode-with-a-jenkins-server)
  - [Local mode (with Docker, no Jenkins server needed)](#local-mode-with-docker-no-jenkins-server-needed)
  - [What happens on commit](#what-happens-on-commit)
- [CLI](#cli)
- [Filtering files](#filtering-files)
- [Parallel validation](#parallel-validation)
- [Configuration](#configuration)
  - [Local Docker image](#local-docker-image)
- [Security](#security)
- [How It Works](#how-it-works)
- [Requirements](#requirements)
- [Contributing](#contributing)
- [License](#license)

## Quick Start

```bash
pip install jenkinsfilelint
jenkinsfilelint Jenkinsfile
```

That works as is: with no Jenkins server configured, files are checked
offline. Then add the pre-commit hook (see [below](#pre-commit-hook)) or keep
using the [CLI](#cli).

## Which mode?

| | Offline | Remote | Local (`--local`) |
|---|---|---|---|
| Used when | no Jenkins server is configured, or with `--offline` | `JENKINS_URL` (or `--jenkins-url`) is set | `--local` is given |
| Needs | nothing else | a Jenkins server and credentials | Docker or Podman |
| Speed | milliseconds | a network round trip | 20–40 s first run, then fast |
| Groovy syntax and Declarative structure | ✓ | ✓ | ✓ |
| Step names and parameters, plugin agent types, tools | — | ✓ | ✓ (plugins in the image) |

Offline mode is the quick check that works anywhere, including CI images and
machines without Docker. Remote and local mode ask a real Jenkins, so they also
catch mistakes that depend on the plugins it has installed.

Any Jenkins setting (`JENKINS_URL`, `JENKINS_USER` or `JENKINS_TOKEN`, or the
matching flag) selects remote mode. So a CI job that sets `JENKINS_TOKEN` but
lost its `JENKINS_URL` secret fails with "Jenkins URL not provided" instead of
quietly checking less. `--offline` checks offline even if a Jenkins server is
configured.

## Pre-commit Hook

Add the hook to your `.pre-commit-config.yaml` and install. Once configured,
every commit that touches a Jenkinsfile is automatically validated.

### Offline mode (no Jenkins, Docker or JVM)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest version
    hooks:
      - id: jenkinsfilelint-offline
```

```bash
pip install pre-commit
pre-commit install
```

Nothing else to install or configure: no credentials, no container, no Rust
toolchain. The check runs `jenkinsfilelint --offline`, which uses a small
native engine that ships as a prebuilt wheel. The `jenkinsfilelint` hook does
the same when no Jenkins server is configured; `jenkinsfilelint-offline` stays
offline even when `JENKINS_URL` is set in your shell. It reports errors with
the same wording as Jenkins's validator, at the line and column where they
are:

```bash
git commit -m "Update Jenkinsfile"
jenkinsfilelint (offline)................................................Failed
- hook id: jenkinsfilelint-offline
- exit code: 1

Jenkinsfile:4:9: error: Expected one of "steps", "stages", or "parallel" for stage "Build"
Jenkinsfile:5:13: error: Unknown stage section "step". Starting with version 0.5, steps in a stage must be in a ‘steps’ block.
```

It checks Groovy syntax and the Declarative structure: required and allowed
sections, stages, `agent`, `environment`, `options`, `triggers`, `parameters`,
`when`, `post`, `matrix` and so on. On the test files of Jenkins's own
validator it passes all 287 valid files and catches all 90 invalid ones that
do not depend on plugins. It cannot know which plugins your Jenkins has, so it
does not check step names and parameters or tool installations, and it only
warns about agent types it does not know. Use remote or local mode for those.

### Remote mode (with a Jenkins server)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkinsfilelint
```

Set credentials via environment variables, then install:

```bash
export JENKINS_URL=https://jenkins.example.com
export JENKINS_USER=your-username
export JENKINS_TOKEN=your-api-token

pip install pre-commit
pre-commit install
```

### Local mode (with Docker, no Jenkins server needed)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkinsfilelint
        args: ["--local"]
```

Docker (or Podman) is the only requirement — no credentials needed:

```bash
pip install pre-commit
pre-commit install
```

> The first commit pulls a minimal Jenkins container (~20–40s cold start).
> Subsequent commits reuse the running container and complete in milliseconds.

> [!IMPORTANT]
> The pre-built image includes plugins for the most common Declarative
> Pipeline patterns (see [full list](#local-docker-image)). If your production
> Jenkins has additional plugins (e.g., ``kubernetes``), local mode may still
> report false positives for those constructs. For full fidelity, use remote
> mode pointing at your real Jenkins, or [build a custom image](#custom-image)
> that mirrors your production plugin set.

When you're done, stop the container:

```bash
jenkinsfilelint server stop
```

### What happens on commit

A valid file passes silently:

```bash
git commit -m "Update Jenkinsfile"
jenkinsfilelint..........................................................Passed
```

A syntax error blocks the commit with a clear message:

```bash
git commit -m "Update Jenkinsfile"
jenkinsfilelint..........................................................Failed
- hook id: jenkinsfilelint
- exit code: 1

Errors encountered validating Jenkinsfile:
WorkflowScript: 17: Expected a step @ line 17, column 11.
             test
             ^
```

Fix the error and re-commit.

## CLI

You can also run `jenkinsfilelint` directly on any file:

```bash
pip install jenkinsfilelint

jenkinsfilelint Jenkinsfile                   # offline, or remote if JENKINS_URL is set
jenkinsfilelint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
jenkinsfilelint --local Jenkinsfile           # a Jenkins in Docker
jenkinsfilelint --offline Jenkinsfile         # offline, even if JENKINS_URL is set
```

With `--verbose`, a run that checks offline because no Jenkins server is
configured starts by saying so:

```text
ℹ No Jenkins server configured, so checking offline: step names, step parameters and other plugin-dependent checks are skipped. Set JENKINS_URL to check with Jenkins.
```

The exit code is `0` when every file is valid and `1` when any file has an
error or could not be checked.

## Filtering files

Use `--include` (whitelist) and `--skip` (blacklist) to control which files are validated:

```bash
# Only validate Jenkinsfiles
jenkinsfilelint --include 'Jenkinsfile*' Jenkinsfile src/Utils.groovy

# Exclude shared-library helper classes
jenkinsfilelint --skip '*/src/*.groovy' --skip 'vars/*.groovy' Jenkinsfile src/Utils.groovy
```

These work in every mode and in pre-commit too (with either hook id):

**Exclude non-pipeline Groovy files (shared library helpers):**

```yaml
- id: jenkinsfilelint
  args: ["--skip=*/src/*.groovy", "--skip=vars/*.groovy"]
```

**Only validate files matching specific patterns:**

```yaml
- id: jenkinsfilelint
  args: ["--include=Jenkinsfile*", "--include=pipelines/*.groovy"]
```

You can also combine both — `--include` narrows first, then `--skip` removes from that set:

```yaml
- id: jenkinsfilelint
  args: ["--include=Jenkinsfile*", "--skip=*/src/*.groovy"]
```

## Parallel validation

Files are validated 8 at a time over reused (keep-alive) connections, so a
repository with many pipelines no longer pays one full round trip and TLS
handshake per file. Results are still printed in the order the files were given.

Use `--jobs` / `-j` to change how many requests are in flight at once — for
example to go easier on a shared Jenkins controller:

```yaml
- id: jenkinsfilelint
  args: ["--jobs=4"]  # use --jobs=1 to validate one file at a time
```

The hook runs as a single process (`require_serial: true`), so `--jobs` is the
total load it puts on Jenkins. Overriding `require_serial: false` in your
config would multiply that by the number of CPUs pre-commit uses, and in
`--local` mode would race to start the container.

## Configuration

Supply credentials via environment variables (recommended) or CLI flags:

| Env Variable               | CLI Flag        | Required |
|----------------------------|-----------------|----------|
| `JENKINS_URL`              | `--jenkins-url` | No *     |
| `JENKINS_USER`             | `--username`    | No **    |
| `JENKINS_TOKEN`            | `--token`       | No **    |
| `JENKINSFILELINT_SERVER_IMAGE` | —          | No       |
| `JENKINSFILELINT_ENGINE`   | —               | No ***   |

\* Required for remote mode. Without any Jenkins setting, files are checked offline.
\*\* Only required if your Jenkins requires authentication (not used in ``--local`` or ``--offline`` mode).
\*\*\* Path of an engine binary for offline mode to use instead of the installed one.

> [!TIP]
> Even if your Jenkins allows anonymous access for validation, using an API token is recommended for production setups.

CLI flags override env vars. There is no config file.

### Local Docker image

By default, ``--local`` mode uses the official image
``ghcr.io/jenkinsci/jenkinsfilelint-server:latest``, which includes the
following plugins to cover common Declarative Pipeline constructs:

| Plugin | Common pattern enabled |
|--------|------------------------|
| `docker-workflow` | ``agent { docker '…' }``, ``docker.image('…')`` |
| `git` | ``git url: '…'`` |
| `credentials-binding` | ``withCredentials([…]) { … }`` |
| `timestamper` | ``options { timestamps() }`` |
| `ws-cleanup` | ``post { always { cleanWs() } }`` |
| `pipeline-utility-steps` | ``readJSON``, ``readYAML``, ``findFiles`` |

> This list is a curated starting point — not exhaustive. If your Jenkinsfile
> uses a plugin not listed here (e.g. ``kubernetes``, ``pipeline-github-lib``),
> please [open an issue](https://github.com/jenkinsci/jenkinsfilelint/issues/new)
> to propose adding it, or use a custom image (see below).

#### Custom image

Build a custom image matching your production Jenkins setup and point to it
via ``JENKINSFILELINT_SERVER_IMAGE``:

```bash
# Export plugin list from your Jenkins (Script Console or Jenkins CLI)
# and generate plugins.txt + a Dockerfile, then:
docker build -t my-company/jenkinsfilelint-server:custom .
JENKINSFILELINT_SERVER_IMAGE=my-company/jenkinsfilelint-server:custom \
  jenkinsfilelint --local Jenkinsfile
```

> The [existing Dockerfile](docker/Dockerfile) and [plugins.txt](docker/plugins.txt)
> are a good starting template — fork them and adjust the plugin list to match
> your production Jenkins.
>
> A full Jenkins plugin list can be 100+ entries, which increases build time
> and image size. Only add what you need.

## Security

> [!WARNING]
> Never hardcode credentials in config files — use environment variables.

- **Never** put `--token` or `--username` in `.pre-commit-config.yaml` — use environment variables.
- Use an API token, not your password.
- A regular user token with read access is sufficient — no need for admin privileges.

## How It Works

`jenkinsfilelint` answers one question: **"Will Jenkins accept this syntax?"**
It either checks the file itself, offline, or POSTs it to a Jenkins's
`/pipeline-model-converter/validate` endpoint.

- **Remote mode**: validates against your existing Jenkins server using the URL
  and credentials you configure. Used whenever a Jenkins setting is present.
- **Local mode** (`--local`): automatically starts a lightweight Jenkins
  container (via Docker or Podman) and validates against it. The container is
  reused across runs for near-instant validation.
- **Offline mode** (`--offline`, and whenever no Jenkins server is
  configured): no Jenkins at all. The
  [`jenkinsfilelint-engine`](crates/jenkinsfilelint) package, a dependency of
  `jenkinsfilelint`, parses the file and checks the Declarative rules itself,
  using the messages of Jenkins's validator. One run checks all files in
  milliseconds.

**Scripted Pipelines and other Groovy files.** The endpoint only checks
Declarative syntax. For a file with no `pipeline { … }` block (a Scripted
Pipeline, a shared-library `vars/*.groovy` step, an empty file) Jenkins still
parses the Groovy, so syntax errors such as an unclosed brace fail as usual.
If it parses, the file is reported as skipped and does not fail the run
(`⊘ vars/notify.groovy: Skipped (not a Declarative Pipeline)` with `--verbose`).
Offline mode does the same. To keep such files from being checked at all, use
[`--skip` / `--include`](#filtering-files).

## Requirements

- Python 3.10+
- For remote mode: a Jenkins server with the Pipeline plugin installed
- For ``--local`` mode: [Docker](https://docker.com) or [Podman](https://podman.io)
- For offline mode: nothing more on Linux (x86_64, aarch64, i686, armv7;
  glibc or musl), macOS (Intel, Apple silicon) and Windows (x86_64, x86),
  where `pip install jenkinsfilelint` installs a prebuilt engine. Elsewhere,
  `pip install jenkinsfilelint-engine` builds it from source and needs a
  [Rust toolchain](https://rustup.rs); the other modes work without it.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
