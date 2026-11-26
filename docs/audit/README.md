# Security audit

`jenkinsfilelint --audit` finds ways a pipeline can be attacked or can leak
secrets. It runs offline, without Jenkins, and reads Jenkinsfiles and
shared library files with the same parser as the syntax check.

```console
$ jenkinsfilelint --audit Jenkinsfile
Jenkinsfile:8:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkinsfilelint/audit/script-injection/
Jenkinsfile:12:41: high: secret-interpolation: PASS is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkinsfilelint/audit/secret-interpolation/
2 findings (2 high)
```

Give it a directory to check a whole repository, `jenkinsfilelint --audit .`.
It picks every `*.groovy` under a `vars/` or `src/` directory, and the
pipelines that the syntax check [finds in a directory](../usage.md#checking-a-directory).

The rules were designed in
[#65](https://github.com/jenkinsci/jenkinsfilelint/issues/65). To suggest a
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
`--fix=all` makes and you review; ❌ no fix.

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
// jenkinsfilelint: ignore[secret-interpolation] the token in this test is fake
sh "deploy --token ${TOKEN}"
```

Several rules go in one comment, separated by commas:
`ignore[secret-interpolation, script-injection]`. Text after the `]` is free.

## Turning off a rule

When a rule does not fit a repository at all, turn it off for every file with
`--ignore`:

```console
$ jenkinsfilelint --audit --ignore unpinned-library Jenkinsfile
```

`--ignore` takes a comma-separated list, `--ignore unpinned-library,unpinned-image`,
and can be given more than once. An unknown rule name is an error, so that a
typo does not go unnoticed. With pre-commit:

```yaml
- id: jenkinsfilelint-audit
  args: [--ignore, unpinned-library]
```

## Fixing findings

`--fix` rewrites the files to fix what it can, then reports what is left.
Like [ruff](https://docs.astral.sh/ruff/linter/#fix-safety) and
[zizmor](https://docs.zizmor.sh/usage/#auto-fixing-results), each fix is
safe or unsafe:

- A **safe** fix keeps what the pipeline does. `--fix` makes the safe fixes.
- An **unsafe** fix can change what it does, so review it before you commit
  it. `--fix=all` makes the safe and unsafe fixes, `--fix=unsafe-only` only
  the unsafe ones.

The fixes for script-injection and secret-interpolation are unsafe: they
quote each value for the shell, which no longer splits it into words. See
[How --fix fixes it](script-injection.md#how-fix-fixes-it) for what they
change and what they leave.

`--diff` prints what the fixes would change, and changes nothing:

```console
$ jenkinsfilelint --audit --fix=all --diff Jenkinsfile
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
$ jenkinsfilelint --audit --fix=all .
Jenkinsfile:8:34: fixed: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
Jenkinsfile:12:41: high: secret-interpolation: TOKEN is a secret, and Groovy interpolates it into the sh script: ...
  help: https://jenkinsci.github.io/jenkinsfilelint/audit/secret-interpolation/
Found 2 findings (1 fixed, 1 remaining).
```

Without `--fix`, nothing changes. A finding that `--fix` would fix starts
with `[*]`, and the last lines say how many each mode would fix:

```text
[*] 1 fixable with --fix.
2 more fixable with --fix=all (unsafe, review the changes).
```

The JSON report gives each finding's `fix`, with its `disposition` and
edits, and the SARIF report carries the fixes for code scanning to suggest.

## Output

- `--format text` (default): one line per finding, and a link to the rule.
- `--format json`: the engine's report, one object per file.
- `--format sarif`: SARIF 2.1.0, for GitHub code scanning and other tools.
  It exits with `0` even when there are findings, so that the report can be
  uploaded.

In GitHub Actions:

```yaml
- run: pipx run jenkinsfilelint --audit --format sarif Jenkinsfile > audit.sarif
- uses: github/codeql-action/upload-sarif@v3
  with:
    sarif_file: audit.sarif
```

In Jenkins, with the Warnings Next Generation plugin:

```groovy
sh 'jenkinsfilelint --audit --format sarif Jenkinsfile > audit.sarif'
recordIssues tool: sarif(pattern: 'audit.sarif')
```

## Exit codes

| Code | Meaning |
|---|---|
| `0` | No findings shown, or `--format sarif`. With `--fix`, every finding was fixed; with `--diff`, no change |
| `1` | A finding was shown, or a file could not be read or parsed. With `--fix`, a finding is left; with `--diff`, a change |
| `2` | Wrong command-line options |
