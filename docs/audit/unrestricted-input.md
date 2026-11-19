# unrestricted-input

An `input` step or stage directive has no `submitter`, so anyone who can
reach the build can approve it.

| Severity | Confidence | Persona |
|---|---|---|
| medium | high | regular |

## What it finds

```groovy
input 'Deploy to production?'           // input is reported
sh './deploy.sh production'
```

It finds:

- the step: `input 'Proceed?'`, `input message: '...', ok: '...'` and
  `def v = input message: '...', parameters: [...]`, in Declarative
  `steps`, Scripted Pipelines and shared libraries
- the Declarative stage directive: `input { message '...' }`
- `submitter: ''` and `submitter ''`, which restrict nothing

## Why it matters

An `input` that guards a deployment or a release looks like an approval
gate. But `submitter`, as the
[Pipeline Syntax](https://www.jenkins.io/doc/book/pipeline/syntax/#input)
says, "Defaults to allowing any user": without it, any user who can reach
the build can approve it. The gate then checks nothing, and the steps
after it run with the build's credentials.

`submitterParameter` records who approved, but does not restrict who may.

## How to fix it

Name the users or groups who may approve, separated by commas:

```groovy
input message: 'Deploy to production?', submitter: 'release-managers,alice'
```

```groovy
stage('Deploy') {
    input {
        message 'Deploy to production?'
        submitter 'release-managers'
    }
    steps {
        sh './deploy.sh production'
    }
}
```

## What it does not report

- **A submitter that Groovy computes**, such as `submitter: approvers`:
  it restricts who may approve, even if the audit cannot tell to whom.
- **Variables and other methods named `input`**: `def input = readFile(...)`,
  `parser.input(...)`.

## When to ignore it

When the `input` is a confirmation, not a security gate: "Run the slow
tests?", "Continue after this warning?". Anyone who can build the job could
as well start another build that skips the question.

```groovy
// jenkinsfilelint: ignore[unrestricted-input] a confirmation, not a security gate
input 'Continue with the slow tests?'
```

## References

- Jenkins: [Pipeline: Input Step](https://www.jenkins.io/doc/pipeline/steps/pipeline-input-step/)
- Jenkins: [Pipeline Syntax, input](https://www.jenkins.io/doc/book/pipeline/syntax/#input)
