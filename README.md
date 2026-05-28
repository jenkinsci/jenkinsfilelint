# Jenkinsfile Lint

[![CI](https://github.com/shenxianpeng/jenkinsfilelint/actions/workflows/main.yml/badge.svg)](https://github.com/shenxianpeng/jenkinsfilelint/actions/workflows/main.yml)
[![codecov](https://codecov.io/gh/shenxianpeng/jenkinsfilelint/graph/badge.svg?token=Z9UTXBL2XG)](https://codecov.io/gh/shenxianpeng/jenkinsfilelint)
[![PyPI version](https://img.shields.io/pypi/v/jenkinsfilelint )](https://pypi.org/project/jenkinsfilelint/)

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` validates Jenkinsfiles through your real Jenkins instance and works as a CLI or pre-commit hook.

## Table of Contents

- [Features](#features)
- [Installation](#installation)
- [Usage](#usage)
- [Security Notes](#security-notes)
- [How It Works](#how-it-works)
- [Requirements](#requirements)
- [License](#license)

## Features

- Validates Jenkinsfiles using Jenkins REST API
- Works as a pre-commit hook
- Supports both command-line usage and environment variables for configuration
- Requires Jenkins credentials for validation
- Supports skipping files that are not Jenkins pipelines (e.g., pure Groovy helper classes)
- Machine-readable JSON and GitHub Actions annotation output for CI/CD integration

## Installation

### Using pip

```bash
pip install jenkinsfilelint
```

### Using pre-commit

Add this to your `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
    rev: # or specific version tag
    hooks:
      - id: jenkinsfilelint
```

## Usage

> [!IMPORTANT]
> jenkinsfilelint requires Jenkins credentials to be set via environment variables for validation:
>
> - `JENKINS_URL`: Your Jenkins server URL (required)
> - `JENKINS_USER`: Your Jenkins username (required unless anonymous access is enabled)
> - `JENKINS_TOKEN`: Your Jenkins API token (required unless anonymous access is enabled)

### Command Line

Validation requires Jenkins credentials. Set them using environment variables:

```bash
export JENKINS_URL=https://your-jenkins-instance.com
export JENKINS_USER=your-username
export JENKINS_TOKEN=your-api-token

jenkinsfilelint path/to/Jenkinsfile
```

Or using command-line arguments:

```bash
jenkinsfilelint path/to/Jenkinsfile \
  --jenkins-url https://your-jenkins-instance.com \
  --username your-username \
  --token your-api-token
```

Validate multiple files:

```bash
jenkinsfilelint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
```

### Skipping Files

In Jenkins shared libraries, some Groovy files are pure Groovy helper classes, not Jenkins pipeline scripts. Use the `--skip` option to exclude files from validation:

```bash
# Skip a specific file pattern
jenkinsfilelint --skip '*/src/*.groovy' Jenkinsfile src/Utils.groovy

# Skip multiple patterns
jenkinsfilelint --skip '*/src/*.groovy' --skip 'vars/*.groovy' Jenkinsfile src/Utils.groovy vars/deploy.groovy
```

The `--skip` option accepts glob patterns and can be used multiple times.

### Including Only Specific Files

Use the `--include` option to validate only files that match specified patterns (whitelist approach). Files not matching any include pattern are silently skipped:

```bash
# Only validate Jenkinsfiles (files starting with "Jenkinsfile")
jenkinsfilelint --include 'Jenkinsfile*' Jenkinsfile Jenkinsfile.prod src/Utils.groovy

# Only validate pipeline groovy files in a specific folder
jenkinsfilelint --include 'pipelines/*.groovy' pipelines/deploy.groovy src/Utils.groovy

# Use multiple include patterns
jenkinsfilelint --include 'Jenkinsfile*' --include 'pipelines/*.groovy' \
  Jenkinsfile pipelines/deploy.groovy src/Utils.groovy
```

The `--include` and `--skip` options can be combined: `--include` first narrows the set of files to consider, then `--skip` further excludes files within that set.

### Machine-Readable Output

Use `--format` to get structured output for CI/CD automation. When specified, all human-readable output is suppressed.

#### JSON (`--format json`)

Outputs a JSON array of per-file results to stdout:

```bash
jenkinsfilelint Jenkinsfile --format json
```

```json
[
  {
    "file": "Jenkinsfile",
    "valid": false,
    "message": "WorkflowScript: 12: Expected a stage @ line 12, column 5."
  }
]
```

Exit code is `0` when all files are valid, `1` otherwise.

#### GitHub Annotations (`--format github`)

Emits [GitHub Actions workflow annotations](https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions#setting-an-error-message) for invalid files. Line numbers are parsed from Jenkins error messages:

```bash
jenkinsfilelint Jenkinsfile --format github
```

```text
::error file=Jenkinsfile,line=12::WorkflowScript: 12: Expected a stage @ line 12, column 5.
```

Valid files produce no output. This format is ideal for:
- **GitHub Actions** — inline annotations in PR diffs
- **Jenkins pipeline self-validation** — parseable by pipeline scripts
- **External validators** (ODS[^1], custom CI systems) — structured, machine-friendly output

[^1]: [Open Delivery Spec](https://github.com/open-delivery-spec) — an open-source delivery specifications and overnance in the AI era.

### Pre-commit Hook

Create or update `.pre-commit-config.yaml` in your repository:

```yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
    rev: # or specific version tag
    hooks:
      - id: jenkinsfilelint
```

Or using command-line arguments:

```yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
    rev: # or specific version tag
    hooks:
      - id: jenkinsfilelint
        args: ["--jenkins-url=https://your-jenkins-instance.com", "--username=your-username", "--token=your-api-token"]
```

To skip certain files (e.g., pure Groovy classes in Jenkins shared libraries):

```yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
    rev: # or specific version tag
    hooks:
      - id: jenkinsfilelint
        args: ["--skip=*/src/*.groovy", "--skip=vars/*.groovy"]
```

To validate only specific files:

```yaml
repos:
  - repo: https://github.com/shenxianpeng/jenkinsfilelint
    rev: # or specific version tag
    hooks:
      - id: jenkinsfilelint
        args: ["--include=Jenkinsfile*", "--include=pipelines/*.groovy"]
```

> [!WARNING]
> Using `args` to pass credentials directly in the configuration file is not recommended for security reasons.
> Consider using environment variables instead. For more details, see [pre-commit/pre-commit#758](https://github.com/pre-commit/pre-commit/issues/758#issuecomment-505935221).

Then install the pre-commit hook:

```bash
pre-commit install
```

## Security Notes

- Do not commit Jenkins tokens to `.pre-commit-config.yaml` or `.env` .
- Prefer environment variables or local secret managers.
- Avoid using administrator tokens for local linting.

## How It Works

> **What it is:** A Jenkinsfile syntax gate — not a Groovy formatter, not a static analyzer.

The linter sends the Jenkinsfile to your Jenkins instance's `/pipeline-model-converter/validate` endpoint for validation. If it passes here, it will pass on the server. Jenkins credentials (URL, username, and API token) are required.

## Requirements

- Python 3.10+
- Jenkins server with Pipeline plugin installed

## License

MIT License
