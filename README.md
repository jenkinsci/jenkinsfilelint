# Jenkinsfile Lint

[![CI](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml)
[![codecov](https://codecov.io/gh/jenkinsci/jenkinsfilelint/graph/badge.svg?token=nGrwXORFtI)](https://codecov.io/gh/jenkinsci/jenkinsfilelint)
[![PyPI version](https://img.shields.io/pypi/v/jenkinsfilelint)](https://pypi.org/project/jenkinsfilelint)

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` validates your Declarative Pipeline syntax via Jenkins's
[`/pipeline-model-converter/validate`](https://www.jenkins.io/doc/book/pipeline/development/#linter)
endpoint. It's primarily a [pre-commit](https://pre-commit.com/) hook, but also
works as a standalone CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

![demo](demo.gif)

## Table of Contents

- [Quick Start](#quick-start)
- [Pre-commit Hook](#pre-commit-hook)
- [CLI](#cli)
- [Local mode (no remote Jenkins required)](#local-mode-no-remote-jenkins-required)
- [Filtering files](#filtering-files)
- [Configuration](#configuration)
- [Security](#security)
- [How It Works](#how-it-works)
- [Requirements](#requirements)
- [Contributing](#contributing)
- [License](#license)

## Quick Start

```bash
pip install jenkinsfilelint
```

Then add the pre-commit hook (see [below](#pre-commit-hook)) or use the [CLI](#cli) directly.

## Pre-commit Hook

Add the hook to your `.pre-commit-config.yaml` and install. Once configured,
every commit that touches a Jenkinsfile is automatically validated.

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

jenkinsfilelint Jenkinsfile
jenkinsfilelint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
jenkinsfilelint --local Jenkinsfile
```

## Local mode (no remote Jenkins required)

Pass `--local` to validate using a lightweight Jenkins container that the tool
manages for you. The container runs in unsecured mode on `127.0.0.1` and is
automatically pulled from GitHub Container Registry on first use.

```bash
jenkinsfilelint --local Jenkinsfile          # First run starts the container (~20–40s)
jenkinsfilelint --local Jenkinsfile          # Subsequent runs reuse it (milliseconds)
jenkinsfilelint server stop                  # Stop the container when you're done
```

> [!IMPORTANT]
> Local mode validates **vanilla Declarative Pipeline syntax only**. If your
> production Jenkins has plugins that provide custom options, agents, or steps
> (e.g., custom shared libraries), local mode may not catch errors related to
> those plugins. For authoritative validation, use remote mode pointing at your
> real Jenkins server.
>
> In short: `--local` = fast syntax gate, remote = authoritative validation.

## Filtering files

Use `--include` (whitelist) and `--skip` (blacklist) to control which files are validated:

```bash
# Only validate Jenkinsfiles
jenkinsfilelint --include 'Jenkinsfile*' Jenkinsfile src/Utils.groovy

# Exclude shared-library helper classes
jenkinsfilelint --skip '*/src/*.groovy' --skip 'vars/*.groovy' Jenkinsfile src/Utils.groovy
```

These work in pre-commit too:

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

## Configuration

Supply credentials via environment variables (recommended) or CLI flags:

| Env Variable               | CLI Flag        | Required |
|----------------------------|-----------------|----------|
| `JENKINS_URL`              | `--jenkins-url` | Yes *    |
| `JENKINS_USER`             | `--username`    | No **    |
| `JENKINS_TOKEN`            | `--token`       | No **    |
| `JENKINSFILELINT_SERVER_IMAGE` | —          | No       |

\* Not required in ``--local`` mode.
\*\* Only required if your Jenkins requires authentication (not used in ``--local`` mode).

> [!TIP]
> Even if your Jenkins allows anonymous access for validation, using an API token is recommended for production setups.

CLI flags override env vars. There is no config file.

### Local Docker image

By default, ``--local`` mode uses the official image
``ghcr.io/jenkinsci/jenkinsfilelint-server:latest``. You can override this with
the ``JENKINSFILELINT_SERVER_IMAGE`` environment variable if you maintain a
custom build.

## Security

> [!WARNING]
> Never hardcode credentials in config files — use environment variables.

- **Never** put `--token` or `--username` in `.pre-commit-config.yaml` — use environment variables.
- Use an API token, not your password.
- A regular user token with read access is sufficient — no need for admin privileges.

## How It Works

`jenkinsfilelint` POSTs your Jenkinsfile to Jenkins's
`/pipeline-model-converter/validate` endpoint and reports whether the syntax is
valid. That's it — it only answers: **"Will Jenkins accept this syntax?"**

- **Remote mode**: validates against your existing Jenkins server using the URL
  and credentials you configure.
- **Local mode** (`--local`): automatically starts a lightweight Jenkins
  container (via Docker or Podman) and validates against it. The container is
  reused across runs for near-instant validation.

## Requirements

- Python 3.10+
- For remote mode: a Jenkins server with the Pipeline plugin installed
- For ``--local`` mode: [Docker](https://docker.com) or [Podman](https://podman.io)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
