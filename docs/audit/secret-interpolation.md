# secret-interpolation

A secret from `withCredentials` or `credentials()` is interpolated by Groovy
into the script of a `sh`, `bat`, `powershell` or `pwsh` step, or into a
command line that Docker Pipeline runs in a shell.

| Severity | Confidence | Persona |
|---|---|---|
| high | high | regular |

## What it finds

```groovy
withCredentials([usernamePassword(credentialsId: 'nexus',
                                  usernameVariable: 'USER',
                                  passwordVariable: 'PASS')]) {
    sh "curl -u ${USER}:${PASS} https://nexus.example.com"   // PASS is reported
}
```

The double quotes make this a Groovy string, so Groovy replaces `${PASS}`
with the password before the step runs. Jenkins then receives a script with
the password written into it.

It also finds the secret when it gets there another way:

- `sh "... $PASS ..."` and `sh "... ${env.PASS} ..."`
- `sh "... ${PASS.trim()} ..."`, or any expression that uses the secret
- `sh 'curl -u admin:' + PASS`, joined with `+`
- `def cmd = "... ${PASS} ..."` and later `sh cmd` or `sh "${cmd}"`, and
  `def pw = env.PASS` and later `sh "... ${pw} ..."`
- an environment variable set from the secret, with `environment`,
  `withEnv` or `env.X = ...`, and later interpolated
- `sh script: "...", returnStdout: true`, and `"""..."""` strings, with or
  without `.stripIndent()`
- the arguments of `docker.build(image, args)`, and of `run(args, command)`
  and `withRun(args, command)` on an image, which Docker Pipeline joins
  into a `sh` command line

The secrets it knows:

- **`withCredentials` bindings:**
  - `string`, `usernameColonPassword` and `usernamePassword`;
  - the passphrase of `sshUserPrivateKey`, the password of `certificate`, and
    the secret key of `AmazonWebServicesCredentialsBinding`;
  - any other binding's `...Variable` parameter;
  - `GIT_PASSWORD` from `gitUsernamePassword`.
- **`environment` sections**, at the pipeline or stage level: `X_PSW` for
  `X = credentials('id')`.

## Why it matters

The [Jenkins documentation](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#interpolation-of-sensitive-environment-variables)
says it plainly: Groovy string interpolation should never be used with
credentials.

- **The secret is copied into the process arguments**, where any process on
  the agent can read it, for example with `ps`.
- **Shell metacharacters in the secret run as shell syntax.** A password
  with `$(...)`, a backtick or `;` in it changes the command. A secret
  someone else can set becomes a way to run commands on the agent.
- **Jenkins masks secrets in the build log, but not in the process arguments
  or in the script file** that `sh` writes to the agent's disk.

Recent Jenkins versions also print a warning in the build log when a step
receives an interpolated secret.

## How to fix it

Pass the script in single quotes, so that Groovy leaves it alone and the
shell reads the secret from its environment, where `withCredentials` and
`environment` put it:

```groovy
withCredentials([usernamePassword(credentialsId: 'nexus',
                                  usernameVariable: 'USER',
                                  passwordVariable: 'PASS')]) {
    sh 'curl -u "$USER:$PASS" https://nexus.example.com'
}
```

| Step | Read the secret as |
|---|---|
| `sh` | `"$PASS"`, in double quotes inside the single-quoted Groovy string |
| `bat` | `%PASS%` |
| `powershell`, `pwsh` | `$env:PASS` |

When the script also needs a Groovy value, escape the secret's `$` instead,
so that only the Groovy value is interpolated:

```groovy
sh "curl -u \"\$USER:\$PASS\" ${env.NEXUS_URL}"
```

## How `--fix` fixes it

`jenkinsfilelint --audit --fix=all` makes this change for you. Like the
fix for script-injection, it is unsafe, so review it, with `--diff` first:

```groovy
sh "curl -u ${USER}:${PASS} https://nexus.example.com"     // before
sh 'curl -u "$USER":"$PASS" https://nexus.example.com'     // after
```

In `powershell` and `pwsh`, a secret becomes `${env:PASS}`. `bat` steps are
left to you, and so is a script that also needs a Groovy value, such as a
local variable or a method call: the escaped form above is the fix for it.
The rules for what `--fix` rewrites are the same as for
[script-injection](script-injection.md#how-fix-fixes-it).

## What it does not report

- **Paths to secret files.** `file(variable: 'KUBECONFIG')`,
  `sshUserPrivateKey(keyFileVariable: 'KEY')` and the keystore of
  `certificate` hold a path, not the secret. `--kubeconfig ${KUBECONFIG}` is
  fine.
- **Secrets used outside their `withCredentials` block or stage.** There,
  Groovy does not read them as secrets.
- **Strings in single quotes and escaped dollars** (`\$PASS`). The shell
  reads those, not Groovy.

## Other personas

- **pedantic:**
  - **User names and key IDs** (`usernameVariable`, `X_USR`, `accessKeyVariable`):
    low severity, since they are rarely what an attacker wants.
  - **`X` for `X = credentials('id')`:** high severity, but medium
    confidence. For a user name and password, `X` holds `user:password`, but
    for a secret file it holds a path.
  - **Secrets interpolated into other steps**, such as `echo` or `writeFile`:
    low severity. Jenkins warns about these too, but outside a script they
    do less harm.
- **auditor:** the same as pedantic.

## When to ignore it

When the value is not really secret, for example a fake token in a test
pipeline:

```groovy
// jenkinsfilelint: ignore[secret-interpolation] the token in this test is fake
sh "deploy --token ${TOKEN}"
```

## References

- Jenkins: [Interpolation of sensitive environment variables](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#interpolation-of-sensitive-environment-variables)
- Jenkins: [Injection via interpolation](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#injection-via-interpolation)
- Jenkins: [Handling credentials](https://www.jenkins.io/doc/book/pipeline/jenkinsfile/#handling-credentials)
