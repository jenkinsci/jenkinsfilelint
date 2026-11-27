# Jenkinsfile Lint

[![CI](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml)
[![PyPI version](https://img.shields.io/pypi/v/jenkins-lint)](https://pypi.org/project/jenkins-lint)

Catch Jenkinsfile syntax errors before they break your CI.

> [!NOTE]
> This project was called `jenkinsfilelint` until 1.7.2, which stays its
> last release. See [Migrating to 2.0](https://jenkinsci.github.io/jenkins-lint/migrating/).

`jenkins-lint` checks your Declarative Pipeline syntax offline, with no
Jenkins at all, or with your Jenkins's own validator
([`/pipeline-model-converter/validate`](https://www.jenkins.io/doc/book/pipeline/development/#linter))
when you configure one. It's primarily a [pre-commit](https://pre-commit.com/)
hook, but also works as a standalone CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

![demo](https://raw.githubusercontent.com/jenkinsci/jenkins-lint/main/demo.gif)

## Quick Start

```bash
pip install jenkins-lint
jenkins-lint Jenkinsfile  # syntax, Declarative structure and security
jenkins-lint .            # every Jenkinsfile and shared library file here
```

That works as is: files are checked offline, and `--remote` also checks them
with your own Jenkins. Then add the pre-commit hook (see [below](#pre-commit-hook)) or keep
using the [CLI](https://jenkinsci.github.io/jenkins-lint/usage/#cli).

## Pre-commit Hook

Add the hook to your `.pre-commit-config.yaml` and install. Once configured,
every commit that touches a Jenkinsfile is automatically validated.

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: # use the latest version
    hooks:
      - id: jenkins-lint
```

```bash
pip install pre-commit
pre-commit install
```

## Documentation

Everything else is on the [documentation site](https://jenkinsci.github.io/jenkins-lint/): [install](https://jenkinsci.github.io/jenkins-lint/install/), [usage](https://jenkinsci.github.io/jenkins-lint/usage/), [pre-commit](https://jenkinsci.github.io/jenkins-lint/pre-commit/), [configuration](https://jenkinsci.github.io/jenkins-lint/configuration/), [security](https://jenkinsci.github.io/jenkins-lint/security/), [how it works](https://jenkinsci.github.io/jenkins-lint/how-it-works/) and the [security audit](https://jenkinsci.github.io/jenkins-lint/audit/).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
