# Jenkinsfile Lint

Catch Jenkinsfile syntax errors before they break your CI.

> [!NOTE]
> This project was called `jenkinsfilelint` until version 1.7. The PyPI package,
> the command and the pre-commit hooks are now named `jenkins-lint`. The old
> hook ids and environment variable names still work.

`jenkins-lint` checks your Declarative Pipeline syntax offline, with no
Jenkins at all, or with your Jenkins's own validator
([`/pipeline-model-converter/validate`](https://www.jenkins.io/doc/book/pipeline/development/#linter))
when you configure one. It's primarily a [pre-commit](https://pre-commit.com/)
hook, but also works as a standalone CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

## Quick Start

```bash
pip install jenkins-lint
jenkins-lint Jenkinsfile  # syntax, Declarative structure and security
jenkins-lint .            # every Jenkinsfile and shared library file here
```

That works as is: files are checked offline, and `--remote` also checks them
with your own Jenkins. Then add the pre-commit hook (see [Pre-commit hook](pre-commit.md)) or keep
using the [CLI](usage.md#cli).

## Security audit rules

<!-- audit-rules -->

## Demo

![demo](https://raw.githubusercontent.com/jenkinsci/jenkins-lint/main/demo.gif)
