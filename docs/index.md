# Jenkinsfile Lint

Catch Jenkinsfile syntax errors before they break your CI.

`jenkinsfilelint` checks your Declarative Pipeline syntax offline, with no
Jenkins at all, or with your Jenkins's own validator
([`/pipeline-model-converter/validate`](https://www.jenkins.io/doc/book/pipeline/development/#linter))
when you configure one. It's primarily a [pre-commit](https://pre-commit.com/)
hook, but also works as a standalone CLI tool.

> 📖 Read the [official blog post](https://www.jenkins.io/blog/2026/06/08/jenkinsfilelint-pre-commit/) for the story behind this tool.

## Quick Start

```bash
pip install jenkinsfilelint
jenkinsfilelint Jenkinsfile
jenkinsfilelint .          # every Jenkinsfile in this directory
jenkinsfilelint --audit .  # and look for security problems
```

That works as is: with no Jenkins server configured, files are checked
offline. Then add the pre-commit hook (see [Pre-commit hook](pre-commit.md)) or keep
using the [CLI](usage.md#cli).

## Security audit rules

<!-- audit-rules -->

## Demo

![demo](https://raw.githubusercontent.com/jenkinsci/jenkinsfilelint/main/demo.gif)
