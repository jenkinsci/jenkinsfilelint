# controller-execution

The steps of a pipeline run on the Jenkins controller, because an `agent`
or a `node` block asks for the controller's label.

| Severity | Confidence | Persona |
|---|---|---|
| high | high; medium when the label also accepts other nodes | regular |

## What it finds

```groovy
pipeline {
    agent { label 'built-in' }   // built-in is reported
    ...
}

node('master') {                 // master is reported
    sh 'make'
}
```

The controller's own node has the label `built-in`. Before Jenkins 2.307 it
was `master`, and a controller upgraded from an older version keeps `master`
until an administrator applies the migration.

It finds the label wherever a pipeline chooses its node:

- `agent { label 'built-in' }` and `agent { node { label 'built-in' } }`,
  for the pipeline or for one stage
- the `label` of `docker`, `dockerfile` and other agent types:
  `agent { docker { image 'maven:3'; label 'master' } }`
- `node('master') { }` and `node(label: 'master') { }`, in Scripted
  Pipelines, in `script` blocks and in shared libraries
- label expressions: `'built-in && linux'`, and `'linux || master'`, which
  lets the build run on the controller. The second is reported with medium
  confidence, since it runs there only when Jenkins picks that node.

## Why it matters

A build on the controller has the same access to the controller's file
system as the Jenkins process, as the Jenkins documentation on
[controller isolation](https://www.jenkins.io/doc/book/security/controller-isolation/)
says, which advises not to run any builds there. That gives it:

- **The controller's files.** `$JENKINS_HOME` holds every job's
  configuration, build logs and the keys that decrypt the stored
  credentials. A `sh` step there can read all of it, whatever the job's own
  permissions are.
- **Jenkins itself.** It can change configuration files and plugins, which
  Jenkins loads as its own.
- **Whoever can change the build can do this.** A pull request that edits a
  build script, a test or a dependency runs its code in these steps.

It also makes Jenkins slower and less stable: builds compete with the
controller for memory, CPU and disk.

## How to fix it

Run the steps on an agent, by a label that only agents have:

```groovy
pipeline {
    agent { label 'linux' }
    ...
}

node('linux') {
    sh 'make'
}
```

Then, in **Manage Jenkins > Nodes**, select **Built-In Node**, then
**Configure**, and set the number of executors to 0. No build can run on the controller after that, and
`agent any` and `node { }` without a label always get an agent.

## What it does not report

- **`agent any`, and `node { }` without a label.** They run on the
  controller only when it has executors, which this file does not say.
- **Labels that Groovy computes**: `node(params.AGENT)`,
  `agent { label "${env.NODE_LABEL}" }`.
- **Expressions that leave the controller out**: `'!master'`,
  `'linux && !built-in'`.
- **Other uses of the words**: `when { branch 'master' }`,
  `git branch: 'master'`, a `label:` parameter of another step.

## When to ignore it

- When the job is for the controller: a backup of `$JENKINS_HOME`, or a
  disk space check. Keep such jobs few, and let only administrators change
  them.
- When `master` is the label of an agent, not of the controller. A label
  that says what the agent has, such as `linux` or `docker`, avoids the
  confusion.

```groovy
// jenkins-lint: ignore[controller-execution] this job backs up JENKINS_HOME
agent { label 'built-in' }
```

## How precise it is

On 17,367 public Jenkinsfiles and shared library steps from GitHub, it
reports 910 findings in 544 files with the regular persona. In a random
sample of 40, each was an `agent` or `node` block whose label is `master` or
`built-in`. Whether that node is the controller depends on the Jenkins: a
controller that labels an agent `master` makes a finding wrong, and the file
cannot show it.

## References

- Jenkins: [Controller isolation](https://www.jenkins.io/doc/book/security/controller-isolation/)
- Jenkins: [Built-in node name and label migration](https://www.jenkins.io/doc/upgrade-guide/2.319/#built-in-node-name-and-label-migration)
