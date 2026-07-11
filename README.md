# Jenkinsfile Lint

[![CI](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkinsfilelint/actions/workflows/main.yml)
[![codecov](https://codecov.io/gh/jenkinsci/jenkinsfilelint/graph/badge.svg?token=nGrwXORFtI)](https://codecov.io/gh/jenkinsci/jenkinsfilelint)
[![PyPI version](https://img.shields.io/pypi/v/jenkinsfilelint)](https://pypi.org/project/jenkinsfilelint/)

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` sends your Jenkinsfiles to your Jenkins server's `/pipeline-model-converter/validate` endpoint for real syntax validation. It's primarily a [pre-commit](https://pre-commit.com/) hook, but also works as a CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

![demo](demo.gif)

## Table of Contents

- [Quick Start](#quick-start)
- [Usage](#usage)
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

### With a remote Jenkins server

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkinsfilelint
```

```bash
export JENKINS_URL=https://jenkins.example.com
export JENKINS_USER=your-username
export JENKINS_TOKEN=your-api-token

pip install pre-commit
pre-commit install
```

### With local Docker (no remote Jenkins required)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkinsfilelint
        args: ["--local"]
```

```bash
# Docker (or Podman) is the only requirement — no env vars needed
pip install pre-commit
pre-commit install
```

The first commit will pull the container image and start Jenkins (~20–40s cold
start). Subsequent commits reuse the running container and complete in
milliseconds.

## Usage

### Pre-commit Hook

Once installed, every commit that touches a Jenkinsfile is validated.

**Remote mode** (default — requires ``JENKINS_URL``):

```bash
git commit -m "Update Jenkinsfile"
jenkinsfilelint..........................................................Passed
```

If the file has a syntax error the commit is blocked:

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

Fix the error, re-commit, and it passes.

### CLI

```bash
pip install jenkinsfilelint

jenkinsfilelint Jenkinsfile
jenkinsfilelint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
```

### Local mode (no remote Jenkins required)

If you have Docker (or Podman) installed, you can validate without any remote
Jenkins server. The tool automatically manages a minimal Jenkins container on
your machine:

```bash
# First run: starts a Jenkins container (~20–40s cold start)
jenkinsfilelint --local Jenkinsfile

# Subsequent runs: reuses the running container (milliseconds)
jenkinsfilelint --local Jenkinsfile

# Stop the container when you're done
jenkinsfilelint server stop
```

The container runs in unsecured mode, listening only on ``127.0.0.1`` so it is
safe for local use. The image is hosted on GitHub Container Registry and is
automatically pulled on first use.

**Server lifecycle commands:**

```bash
jenkinsfilelint server status   # Check if the container is running
jenkinsfilelint server restart  # Restart the container
```

> [!IMPORTANT]
> Local mode validates **vanilla Declarative Pipeline syntax only**. If your
> production Jenkins has plugins that provide custom options, agents, or steps
> (e.g., custom shared libraries), local mode may not catch errors related to
> those plugins. For authoritative validation, use the regular remote mode
> pointing at your real Jenkins server.
>
> In short: `--local` = fast syntax gate, remote = authoritative validation.

### Filtering files

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

`jenkinsfilelint` is a **syntax gate** — it checks that your Declarative Pipeline syntax is valid.

### Remote mode (default)

1. Reads the local Jenkinsfile.
2. POSTs it to `<JENKINS_URL>/pipeline-model-converter/validate`.
3. Jenkins parses the Pipeline and returns `"ok"` or errors.
4. Errors are printed and the tool exits non-zero.

It only answers: **"Will Jenkins accept this syntax?"**

### Local mode (`--local`)

Same validation, zero infrastructure:

1. Checks if a local Jenkins container is already running (identified by label).
2. If not, starts one with ``docker run -d`` (or ``podman``).
3. Waits for Jenkins to become ready by polling ``/login``.
4. Uses the **exact same** validate endpoint at ``http://127.0.0.1:<port>``.
5. Container stays running — subsequent invocations are near-instant.

This gives you 100% validation fidelity without maintaining a remote Jenkins
server. The only requirement is Docker or Podman on your machine.

## Requirements

- Python 3.10+
- For remote mode: a Jenkins server with the Pipeline plugin installed
- For ``--local`` mode: [Docker](https://docker.com) or [Podman](https://podman.io)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
