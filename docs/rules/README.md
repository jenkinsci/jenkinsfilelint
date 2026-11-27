# Security rules

Every `jenkins-lint` run also looks for ways a pipeline can be attacked or
can leak secrets. It runs offline, without Jenkins, with the same parser as
the syntax check, in the same run.

```console
$ jenkins-lint Jenkinsfile
Jenkinsfile:8:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/rules/script-injection/
Jenkinsfile:12:41: high: secret-interpolation: PASS is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/rules/secret-interpolation/
2 findings (2 high)
```

Give it a directory to check a whole repository, `jenkins-lint .`. It picks
every `*.groovy` under a `vars/` or `src/` directory, and the pipelines it
[finds in a directory](../usage.md#checking-a-directory).

The rules were designed in
[#65](https://github.com/jenkinsci/jenkins-lint/issues/65). To suggest a
new rule, or to report a finding that is wrong, open an issue.

## Rules

| Rule | Finds | Fix |
|---|---|---|
| [script-injection](script-injection.md) | A pull request title, branch name, webhook payload or build parameter interpolated by Groovy into a shell step | ⚠ |
| [secret-interpolation](secret-interpolation.md) | A secret interpolated by Groovy into a shell step | ⚠ |
| [controller-execution](controller-execution.md) | Steps that run on the Jenkins controller: `agent { label 'built-in' }`, `node('master')` | ❌ |
| [unpinned-library](unpinned-library.md) | A shared library loaded from a branch: `@Library('lib@main') _` | ❌ |
| [unpinned-image](unpinned-image.md) | A container image without a tag, with `latest`, or without a digest | ❌ |
| [unrestricted-input](unrestricted-input.md) | An `input` without `submitter`, which any user may approve (pedantic) | ❌ |

Fix: ✅ a safe fix, which `--fix` makes; ⚠ an unsafe fix, which
`--fix --unsafe-fixes` makes and you review; ❌ no fix.

The first two follow a value through local variables, environment
variables, `+` and string methods to the step that runs it, the way Groovy
evaluates it.

## Choosing what to see

Every finding has a **severity**, how bad it is if it is real, and a
**confidence**, how likely it is to be real. Each finding is also shown from
one **persona** upward:

| `--persona` | Shows |
|---|---|
| `regular` (default) | Findings worth fixing |
| `pedantic` | Also lower-confidence findings and hardening advice |
| `auditor` | Everything, for a security review |

`--min-severity` and `--min-confidence` (`low`, `medium` or `high`) leave out
the rest.

## Turning off a finding

Add a comment on the finding's line, or on the line above its statement:

```groovy
// jenkins-lint: ignore[secret-interpolation] the token in this test is fake
sh "deploy --token ${TOKEN}"
```

Several rules go in one comment, separated by commas:
`ignore[secret-interpolation, script-injection]`. Text after the `]` is free.

## Turning off a rule

When a rule does not fit a repository at all, turn it off for every file with
`--ignore`:

```console
$ jenkins-lint --ignore unpinned-library Jenkinsfile
```

`--ignore` takes a comma-separated list, `--ignore unpinned-library,unpinned-image`,
and can be given more than once. An unknown rule name is an error, so that a
typo does not go unnoticed. With pre-commit:

```yaml
- id: jenkins-lint
  args: [--ignore, unpinned-library]
```

## Fixing findings

`--fix` rewrites the files to fix what it can, then reports what is left.
The fix options are named and work as in
[ruff](https://docs.astral.sh/ruff/linter/#fix-safety): `--fix`,
`--unsafe-fixes`, `--show-fixes`, `--diff`, and `--no-fix`,
`--no-unsafe-fixes` and `--no-show-fixes` to turn one off. Each fix is safe
or unsafe:

- A **safe** fix keeps what the pipeline does. `--fix` makes the safe fixes.
- An **unsafe** fix can change what it does, so review it before you commit
  it. `--fix --unsafe-fixes` makes the safe and the unsafe fixes.

The fixes for script-injection and secret-interpolation are unsafe: they
quote each value for the shell, which no longer splits it into words. See
[How --fix fixes it](script-injection.md#how-fix-fixes-it) for what they
change and what they leave.

`--diff` prints what the fixes would change, and changes nothing:

```console
$ jenkins-lint --diff --unsafe-fixes Jenkinsfile
--- Jenkinsfile
+++ Jenkinsfile
@@ -5,7 +5,7 @@
         stage('Build') {
             steps {
-                sh "git checkout ${env.CHANGE_BRANCH}"
+                sh 'git checkout "$CHANGE_BRANCH"'
             }
         }
     }
$ jenkins-lint --fix --unsafe-fixes --show-fixes .
Jenkinsfile:8:34: fixed: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
Jenkinsfile:12:41: high: secret-interpolation: TOKEN is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkins-lint/rules/secret-interpolation/
Found 2 findings (1 fixed, 1 remaining).
```

Without `--show-fixes`, `--fix` prints only the findings left and the
summary. Without `--fix`, nothing changes. A finding that `--fix` would fix
starts with `[*]`, and the last line says how many, as ruff does:

```text
[*] 1 fixable with the `--fix` option (2 hidden fixes can be enabled with the `--unsafe-fixes` option).
```

With `--unsafe-fixes`, the unsafe fixes are marked and counted too.

The JSON report gives each finding's `fix`, with its `disposition` and
edits, and the SARIF report carries the fixes for code scanning to suggest.

## Output

- `--output-format text` (default): one line per finding, and a link to the rule.
- `--output-format json`: the engine's report, one object per file.
- `--output-format sarif`: SARIF 2.1.0, for GitHub code scanning and other tools.
  Errors that make Jenkins reject a file are results of the rule
  `invalid-pipeline`.
  It exits with `0` even when there are findings, so that the report can be
  uploaded.
- `--output-format github`: GitHub Actions annotations, which show each error
  and finding on its line of the pull request. Errors and high findings are
  errors, warnings and medium findings warnings, and low findings notices.

In GitHub Actions, annotations on the pull request:

```yaml
- run: pipx run jenkins-lint --output-format github .
```

Or the SARIF report, for code scanning:

```yaml
- run: pipx run jenkins-lint --output-format sarif Jenkinsfile > jenkins-lint.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: jenkins-lint.sarif
```

In Jenkins, with the Warnings Next Generation plugin:

```groovy
sh 'jenkins-lint --output-format sarif Jenkinsfile > jenkins-lint.sarif'
recordIssues tool: sarif(pattern: 'jenkins-lint.sarif')
```

## Exit codes

| Code | Meaning |
|---|---|
| `0` | No error and no finding shown, or `--output-format sarif`. With `--fix`, every finding was fixed; with `--diff`, no change |
| `1` | An error or a finding was shown, or a file could not be read. With `--fix`, a finding is left; with `--diff`, a change |
| `2` | Wrong command-line options |
