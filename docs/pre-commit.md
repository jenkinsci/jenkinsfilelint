# Pre-commit Hook

Add the hook to your `.pre-commit-config.yaml` and install. Once configured,
every commit that touches a Jenkinsfile is automatically validated.

## The hook

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

Nothing else to install or configure: no credentials, no Jenkins, no Rust
toolchain. The hook checks offline with a small native engine that ships as a
prebuilt wheel, and reports errors with the same wording as Jenkins's
validator, at the line and column where they are:

```bash
git commit -m "Update Jenkinsfile"
jenkins-lint..........................................................Failed
- hook id: jenkins-lint
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
warns about agent types it does not know. Add `--remote` for those.

`jenkins-lint-offline` and the ids from before the rename (`jenkinsfilelint`,
`jenkinsfilelint-offline`) still work and run the same check.

## Also checking with your Jenkins

```yaml
      - id: jenkins-lint
        args: ["--remote"]
```

With `--remote`, the files that pass offline are also sent to your Jenkins.
It reads the server and credentials from the environment:

```bash
export JENKINS_URL=https://jenkins.example.com
export JENKINS_USER=your-username
export JENKINS_TOKEN=your-api-token
```

## What happens on commit

A valid file passes silently:

```bash
git commit -m "Update Jenkinsfile"
jenkins-lint..........................................................Passed
```

A syntax error blocks the commit with a clear message:

```bash
git commit -m "Update Jenkinsfile"
jenkins-lint..........................................................Failed
- hook id: jenkins-lint
- exit code: 1

Errors encountered validating Jenkinsfile:
WorkflowScript: 17: Expected a step @ line 17, column 11.
             test
             ^
```

Fix the error and re-commit.

## Security checks

The `jenkins-lint` hook runs the [security checks](usage.md#security-checks)
too. `jenkins-lint-audit`, its own hook before 2.0, does the same.
