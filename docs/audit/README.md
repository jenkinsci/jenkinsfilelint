# Security audit (preview)

`jenkinsfilelint --audit` finds ways a pipeline can be attacked or can leak
secrets. It runs offline, without Jenkins, and reads Jenkinsfiles and
shared library files with the same parser as the syntax check.

```console
$ jenkinsfilelint --audit Jenkinsfile
Jenkinsfile:8:36: high: script-injection: CHANGE_BRANCH is the name of the pull request's branch, and Groovy interpolates it into the sh script: ...
  help: https://github.com/jenkinsci/jenkinsfilelint/blob/main/docs/audit/script-injection.md
Jenkinsfile:12:41: high: secret-interpolation: PASS is a secret, and Groovy interpolates it into the sh script: ...
  help: https://github.com/jenkinsci/jenkinsfilelint/blob/main/docs/audit/secret-interpolation.md
2 findings (2 high)
```

The plans and the rules to come are in
[#65](https://github.com/jenkinsci/jenkinsfilelint/issues/65).

## Rules

| Rule | Finds |
|---|---|
| [script-injection](script-injection.md) | A pull request title, branch name, webhook payload or build parameter interpolated by Groovy into a shell step |
| [secret-interpolation](secret-interpolation.md) | A secret interpolated by Groovy into a shell step |

Both follow a value through local variables, environment variables, `+`
and string methods to the step that runs it, the way Groovy evaluates it.

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
| `0` | No findings shown, or `--format sarif` |
| `1` | A finding was shown, or a file could not be read or parsed |
| `2` | Wrong command-line options |
