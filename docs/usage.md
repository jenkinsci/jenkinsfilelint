# Usage

## Which mode?

| | Offline | Remote | Local (`--local`) |
|---|---|---|---|
| Used when | no Jenkins server is configured, or with `--offline` | `JENKINS_URL` (or `--jenkins-url`) is set | `--local` is given |
| Needs | nothing else | a Jenkins server and credentials | Docker or Podman |
| Speed | milliseconds | a network round trip | 20–40 s first run, then fast |
| Groovy syntax and Declarative structure | ✓ | ✓ | ✓ |
| Step names and parameters, plugin agent types, tools | — | ✓ | ✓ (plugins in the image) |

Offline mode is the quick check that works anywhere, including CI images and
machines without Docker. Remote and local mode ask a real Jenkins, so they also
catch mistakes that depend on the plugins it has installed.

Any Jenkins setting (`JENKINS_URL`, `JENKINS_USER` or `JENKINS_TOKEN`, or the
matching flag) selects remote mode. So a CI job that sets `JENKINS_TOKEN` but
lost its `JENKINS_URL` secret fails with "Jenkins URL not provided" instead of
quietly checking less. `--offline` checks offline even if a Jenkins server is
configured.

## CLI

You can also run `jenkinsfilelint` directly on any file or directory:

```bash
pip install jenkinsfilelint

jenkinsfilelint Jenkinsfile                   # offline, or remote if JENKINS_URL is set
jenkinsfilelint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
jenkinsfilelint .                             # every Jenkinsfile in this directory
jenkinsfilelint --local Jenkinsfile           # a Jenkins in Docker
jenkinsfilelint --offline Jenkinsfile         # offline, even if JENKINS_URL is set
```

With `--verbose`, a run that checks offline because no Jenkins server is
configured starts by saying so:

```text
ℹ No Jenkins server configured, so checking offline: step names, step parameters and other plugin-dependent checks are skipped. Set JENKINS_URL to check with Jenkins.
```

## Checking a directory

A directory is searched recursively, in every mode:

```bash
jenkinsfilelint .
jenkinsfilelint --audit .
jenkinsfilelint pipelines/ Jenkinsfile
```

| File | Picked |
|---|---|
| `Jenkinsfile` and `*.jenkinsfile`, in any case | Always |
| Other `Jenkinsfile*`, like `Jenkinsfile-prod` | If it contains a pipeline |
| `*.groovy` | If it contains a pipeline |
| `*.groovy` under any `vars/` or `src/` directory | Always with `--audit` |

A file contains a pipeline when it has a `pipeline {`, `node {` or `node(` at
its top level. Comments, strings and the bodies of methods do not count, so
plain Groovy helpers are left out. So are notes and copies such as
`Jenkinsfile.md` and `Jenkinsfile.bak`.

A pipeline with any other name, like `ci/deploy.pipeline`, is not found. Pass
its path as well: `jenkinsfilelint . ci/deploy.pipeline`.

Directories of tools are left out: `.git`, `.venv`, `venv`, `.tox`,
`node_modules`, `target`, `site`, `__pycache__` and the like, and `build` next
to a build file such as `build.gradle` or `pyproject.toml`. Other hidden
directories, like `.jenkins` and `.ci`, are searched. Symlinks to directories
are not followed. Files are checked in sorted order, and a file found twice is
checked once. [`--include` and `--skip`](#filtering-files) apply to the files
found.

A directory with no files to check is not an error. `--verbose` says so.

## Exit codes

The exit code is `0` when every file is valid and `1` when any file has an
error or could not be checked.

`--audit` has [its own exit codes](audit/README.md#exit-codes).

## Security audit

`--audit` checks Jenkinsfiles and shared library files for security problems
instead of validating them, offline:

```console
$ jenkinsfilelint --audit Jenkinsfile
Jenkinsfile:8:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkinsfilelint/audit/script-injection/
Jenkinsfile:12:41: high: secret-interpolation: PASS is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkinsfilelint/audit/secret-interpolation/
2 findings (2 high)
```

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/jenkinsci/jenkinsfilelint
    rev: # use the latest version
    hooks:
      - id: jenkinsfilelint-audit
```

[script-injection](audit/script-injection.md) finds pull request
titles, branch names, webhook payloads and build parameters that Groovy
interpolates into shell steps, where anyone who opens a pull request or
starts the build can run commands.
[secret-interpolation](audit/secret-interpolation.md) finds secrets
that Groovy interpolates into shell steps.
[controller-execution](audit/controller-execution.md) finds `agent` and
`node` blocks that run their steps on the Jenkins controller.
[unpinned-library](audit/unpinned-library.md) finds shared libraries
loaded from a branch instead of a tag or commit.
[unpinned-image](audit/unpinned-image.md) finds container images
without a tag or with `latest`.
[unrestricted-input](audit/unrestricted-input.md) finds `input` steps
without a `submitter`, which any user may approve, with `--persona pedantic`. `--persona`,
`--min-severity` and `--min-confidence` choose what to see, a
`// jenkinsfilelint: ignore[rule]` comment turns off one finding,
`--ignore RULE` turns off a rule everywhere, and
`--format sarif` writes a report for GitHub code scanning. See
[Security audit](audit/README.md) for all of it.

## Filtering files

Use `--include` (whitelist) and `--skip` (blacklist) to control which files are validated:

```bash
# Only validate Jenkinsfiles
jenkinsfilelint --include 'Jenkinsfile*' Jenkinsfile src/Utils.groovy

# Exclude shared-library helper classes
jenkinsfilelint --skip '*/src/*.groovy' --skip 'vars/*.groovy' Jenkinsfile src/Utils.groovy
```

These work in every mode and in pre-commit too (with either hook id):

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

## Parallel validation

Files are validated 8 at a time over reused (keep-alive) connections, so a
repository with many pipelines no longer pays one full round trip and TLS
handshake per file. Results are still printed in the order the files were given.

Use `--jobs` / `-j` to change how many requests are in flight at once — for
example to go easier on a shared Jenkins controller:

```yaml
- id: jenkinsfilelint
  args: ["--jobs=4"]  # use --jobs=1 to validate one file at a time
```

The hook runs as a single process (`require_serial: true`), so `--jobs` is the
total load it puts on Jenkins. Overriding `require_serial: false` in your
config would multiply that by the number of CPUs pre-commit uses, and in
`--local` mode would race to start the container.
