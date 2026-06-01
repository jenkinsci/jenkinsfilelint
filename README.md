# Jenkinsfile Lint

[![CI](https://github.com/shenxianpeng/jenkinsfilelint/actions/workflows/main.yml/badge.svg)](https://github.com/shenxianpeng/jenkinsfilelint/actions/workflows/main.yml)
[![codecov](https://codecov.io/gh/shenxianpeng/jenkinsfilelint/graph/badge.svg?token=Z9UTXBL2XG)](https://codecov.io/gh/shenxianpeng/jenkinsfilelint)
[![PyPI version](https://img.shields.io/pypi/v/jenkinsfilelint)](https://pypi.org/project/jenkinsfilelint/)

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` sends your Jenkinsfiles to your Jenkins server's `/pipeline-model-converter/validate` endpoint for real syntax validation. It's primarily a [pre-commit](https://pre-commit.com/) hook, but also works as a CLI tool.

## Table of Contents

- [Quick Start](#quick-start)
- [Usage](#usage)
  - [Pre-commit Hook](#pre-commit-hook)
  - [CLI](#cli)
  - [Filtering files](#filtering-files)
- [Configuration](#configuration)
- [Security](#security)
- [How It Works](#how-it-works)
- [Requirements](#requirements)
- [Contributing](#contributing)
- [License](#license)

## Quick Start

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
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

## Usage

### Pre-commit Hook

Once installed, every commit that touches a Jenkinsfile is validated:

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

| Env Variable    | CLI Flag       | Required |
|-----------------|----------------|----------|
| `JENKINS_URL`   | `--jenkins-url`| Yes      |
| `JENKINS_USER`  | `--username`   | No *     |
| `JENKINS_TOKEN` | `--token`      | No *     |

\* Only required if your Jenkins requires authentication.

> [!TIP]
> Even if your Jenkins allows anonymous access for validation, using an API token is recommended for production setups.

CLI flags override env vars. There is no config file.

## Security

> [!WARNING]
> Never hardcode credentials in config files — use environment variables.

- **Never** put `--token` or `--username` in `.pre-commit-config.yaml` — use environment variables.
- Use an API token, not your password.
- A regular user token with read access is sufficient — no need for admin privileges.

## How It Works

`jenkinsfilelint` is a **syntax gate** — it checks that your Declarative Pipeline compiles, not that it's semantically correct.

1. Reads the local Jenkinsfile.
2. POSTs it to `<JENKINS_URL>/pipeline-model-converter/validate`.
3. Jenkins parses the Pipeline and returns `"ok"` or errors.
4. Errors are printed and the tool exits non-zero.

It only answers: **"Will Jenkins accept this syntax?"**

## Requirements

- Python 3.10+
- Jenkins server with the Pipeline plugin

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
