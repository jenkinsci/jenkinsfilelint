# Pre-commit Hook

Add the hook to your `.pre-commit-config.yaml` and install. Once configured,
every commit that touches a Jenkinsfile is automatically validated.

## Offline mode (no Jenkins, Docker or JVM)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: # use the latest version
    hooks:
      - id: jenkins-lint-offline
```

```bash
pip install pre-commit
pre-commit install
```

Nothing else to install or configure: no credentials, no container, no Rust
toolchain. The check runs `jenkins-lint --offline`, which uses a small
native engine that ships as a prebuilt wheel. The `jenkins-lint` hook does
the same when no Jenkins server is configured; `jenkins-lint-offline` stays
offline even when `JENKINS_URL` is set in your shell. It reports errors with
the same wording as Jenkins's validator, at the line and column where they
are:

```bash
git commit -m "Update Jenkinsfile"
jenkins-lint (offline)................................................Failed
- hook id: jenkins-lint-offline
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

## Remote mode (with a Jenkins server)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkins-lint
```

Set credentials via environment variables, then install:

```bash
export JENKINS_URL=https://jenkins.example.com
export JENKINS_USER=your-username
export JENKINS_TOKEN=your-api-token

pip install pre-commit
pre-commit install
```

## Local mode (with Docker, no Jenkins server needed)

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkins-lint
    rev: # use the latest or a specific version, e.g. v1.4.0
    hooks:
      - id: jenkins-lint
        args: ["--local"]
```

Docker (or Podman) is the only requirement — no credentials needed:

```bash
pip install pre-commit
pre-commit install
```

> The first commit pulls a minimal Jenkins container (~20–40s cold start).
> Subsequent commits reuse the running container and complete in milliseconds.

<!-- Keeps the two quotes apart, so that the callout renders. -->

> [!IMPORTANT]
> The pre-built image includes plugins for the most common Declarative
> Pipeline patterns (see [full list](configuration.md#local-docker-image)). If your production
> Jenkins has additional plugins (e.g., ``kubernetes``), local mode may still
> report false positives for those constructs. For full fidelity, use remote
> mode pointing at your real Jenkins, or [build a custom image](configuration.md#custom-image)
> that mirrors your production plugin set.

When you're done, stop the container:

```bash
jenkins-lint server stop
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

## Security audit

The `jenkins-lint-audit` hook runs the [security audit](usage.md#security-audit).
