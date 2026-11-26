# Usage

## Offline, and Jenkins when you ask

Every run checks the files offline: Groovy syntax and Declarative Pipeline
structure, in milliseconds, with nothing but `pip install jenkins-lint`.

`--remote` also sends the files that pass to a Jenkins server, which knows
the plugins it has installed: step names and parameters, plugin agent types
and tools. Files with an offline error are reported without a request.

| | Offline (default) | `--remote URL` |
|---|---|---|
| Needs | nothing else | a Jenkins server, and credentials if it asks for them |
| Speed | milliseconds | a network round trip per file |
| Groovy syntax and Declarative structure | ✓ | ✓ |
| Step names and parameters, plugin agent types, tools | — | ✓ |

Only `--remote` turns remote checking on. `JENKINS_URL` alone never does:
Jenkins sets it in every build, and a lint step in a job should not start
calling the controller by itself. A bare `--remote` uses `JENKINS_URL` as the
URL, and `JENKINS_USER` / `JENKINS_TOKEN` give the credentials.

## CLI

```bash
pip install jenkins-lint

jenkins-lint Jenkinsfile                   # offline
jenkins-lint Jenkinsfile Jenkinsfile.prod tests/Jenkinsfile
jenkins-lint .                             # every Jenkinsfile in this directory
jenkins-lint --remote https://ci.example.com Jenkinsfile
jenkins-lint --remote Jenkinsfile          # the URL in JENKINS_URL
```

With `--verbose`, an offline run starts by saying what it does not check:

```text
ℹ Checking offline: step names, step parameters and other plugin-dependent checks are skipped. Pass --remote to also check with your Jenkins.
```

## Checking a directory

A directory is searched recursively, in every mode:

```bash
jenkins-lint .
jenkins-lint pipelines/ Jenkinsfile
```

| File | Picked |
|---|---|
| `Jenkinsfile` and `*.jenkinsfile`, in any case | Always |
| Other `Jenkinsfile*`, like `Jenkinsfile-prod` | If it contains a pipeline |
| `*.groovy` | If it contains a pipeline |
| `*.groovy` under any `vars/` or `src/` directory | Always |

A file contains a pipeline when it has a `pipeline {`, `node {` or `node(` at
its top level. Comments, strings and the bodies of methods do not count, so
plain Groovy helpers are left out. So are notes and copies such as
`Jenkinsfile.md` and `Jenkinsfile.bak`.

A pipeline with any other name, like `ci/deploy.pipeline`, is not found. Pass
its path as well: `jenkins-lint . ci/deploy.pipeline`.

Directories of tools are left out: `.git`, `.venv`, `venv`, `.tox`,
`node_modules`, `target`, `site`, `__pycache__` and the like, and `build` next
to a build file such as `build.gradle` or `pyproject.toml`. Other hidden
directories, like `.jenkins` and `.ci`, are searched. Symlinks to directories
are not followed. Files are checked in sorted order, and a file found twice is
checked once. [`--include` and `--skip`](#filtering-files) apply to the files
found.

A directory with no files to check is not an error. `--verbose` says so.

## Exit codes

The exit code is `0` when every file passes: Jenkins would accept it and it
has no security finding. It is `1` when any file has an error, a finding, or
could not be checked. See [the exit codes of the security checks](audit/README.md#exit-codes)
for `--fix`, `--diff` and `--format sarif`.

## Security checks

Every run also checks Jenkinsfiles and shared library files for security
problems, in the same pass:

```console
$ jenkins-lint Jenkinsfile
Jenkinsfile:8:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/audit/script-injection/
Jenkinsfile:12:41: high: secret-interpolation: PASS is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/audit/secret-interpolation/
2 findings (2 high)
```

`jenkins-lint check` is the same command, and so is `jenkins-lint audit`, the
name from before 2.0.

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
`// jenkins-lint: ignore[rule]` comment turns off one finding,
`--ignore RULE` turns off a rule everywhere,
`--fix --unsafe-fixes` rewrites the scripts it can fix, for you to review, and
`--format sarif` writes a report for GitHub code scanning. See
[Security audit](audit/README.md) for all of it.

## Filtering files

Use `--include` (whitelist) and `--skip` (blacklist) to control which files are validated:

```bash
# Only validate Jenkinsfiles
jenkins-lint --include 'Jenkinsfile*' Jenkinsfile src/Utils.groovy

# Exclude shared-library helper classes
jenkins-lint --skip '*/src/*.groovy' --skip 'vars/*.groovy' Jenkinsfile src/Utils.groovy
```

These work in every mode and in pre-commit too (with either hook id):

**Exclude non-pipeline Groovy files (shared library helpers):**

```yaml
- id: jenkins-lint
  args: ["--skip=*/src/*.groovy", "--skip=vars/*.groovy"]
```

**Only validate files matching specific patterns:**

```yaml
- id: jenkins-lint
  args: ["--include=Jenkinsfile*", "--include=pipelines/*.groovy"]
```

You can also combine both — `--include` narrows first, then `--skip` removes from that set:

```yaml
- id: jenkins-lint
  args: ["--include=Jenkinsfile*", "--skip=*/src/*.groovy"]
```

## Parallel validation

Files are validated 8 at a time over reused (keep-alive) connections, so a
repository with many pipelines no longer pays one full round trip and TLS
handshake per file. Results are still printed in the order the files were given.

Use `--jobs` / `-j` to change how many requests are in flight at once — for
example to go easier on a shared Jenkins controller:

```yaml
- id: jenkins-lint
  args: ["--jobs=4"]  # use --jobs=1 to validate one file at a time
```

The hook runs as a single process (`require_serial: true`), so `--jobs` is the
total load it puts on Jenkins. Overriding `require_serial: false` in your
config would multiply that by the number of CPUs pre-commit uses.
