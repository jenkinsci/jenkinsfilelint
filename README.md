# jenkins-lint

[![CI](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml/badge.svg)](https://github.com/jenkinsci/jenkins-lint/actions/workflows/main.yml)
[![PyPI version](https://img.shields.io/pypi/v/jenkins-lint)](https://pypi.org/project/jenkins-lint)

Catch Jenkinsfile mistakes and security problems before they reach your CI.

`jenkins-lint` checks Jenkinsfiles and shared library files offline, in
milliseconds, with no Jenkins, Docker or JVM: Groovy syntax and Declarative
Pipeline structure, with the wording of Jenkins's own validator, and security
problems such as secrets and pull request titles interpolated into shell
steps, with fixes. `--remote` also validates the files with your Jenkins,
which knows its plugins. It is one small binary, for pre-commit, CI and the
command line.

> [!NOTE]
> Formerly `jenkinsfilelint`: see [Migrating to 2.0](https://jenkinsci.github.io/jenkins-lint/migrating/).

![demo](docs/assets/demo.gif)

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

Everything else is on the [documentation site](https://jenkinsci.github.io/jenkins-lint/): [install](https://jenkinsci.github.io/jenkins-lint/install/), [usage](https://jenkinsci.github.io/jenkins-lint/usage/), [pre-commit](https://jenkinsci.github.io/jenkins-lint/pre-commit/), [configuration](https://jenkinsci.github.io/jenkins-lint/configuration/), [security](https://jenkinsci.github.io/jenkins-lint/security/), [how it works](https://jenkinsci.github.io/jenkins-lint/how-it-works/) and the [security audit](https://jenkinsci.github.io/jenkins-lint/rules/).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT — see [LICENSE](LICENSE).
