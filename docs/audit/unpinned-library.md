# unpinned-library

A shared library is loaded from a branch, so the code the pipeline runs
changes whenever someone pushes to that branch.

| Severity | Confidence | Persona |
|---|---|---|
| medium | medium | regular |

## What it finds

```groovy
@Library('pipeline-library@main') _      // main is reported
library 'shared@develop'                 // develop is reported
```

It reads the version wherever a pipeline names a library:

- `@Library('lib@main') _`, `@Library(['a@main', 'b@v1']) _` and
  `@Library(value = 'lib@main', changelog = false) _`
- the `library` step: `library 'lib@main'`, `library('lib@main').com.example.Utils`,
  and `library identifier: 'lib@main', retriever: modernSCM(...)`
- the Declarative directive `libraries { lib('lib@main') }`

A version that looks like a release tag (`1.2`, `v1.2.3`, `2.0.0-rc.1`), a
commit SHA, or `refs/tags/...` is pinned and not reported. Any other
version, such as `main`, `master`, `develop` or `feature/x`, is reported as
a branch.

## Why it matters

The [Jenkins documentation](https://www.jenkins.io/doc/book/pipeline/shared-libraries/#global-shared-libraries)
says it plainly about trusted global libraries: "anyone able to push
commits to this SCM repository could obtain unlimited access to Jenkins".

- **A branch is a moving target.** Every build runs whatever the branch
  holds when the build starts. A change merged to the library, by mistake or
  on purpose, reaches every pipeline that loads it, without a change to
  any of them.
- **Trusted libraries run outside the sandbox.** A global library marked
  trusted can call any Java, Groovy or Jenkins API, with the controller's
  permissions. Folder-level libraries are always untrusted and run in the
  sandbox, but they still run in your build, with its credentials.
- **The repository you review is not the one that runs.** Pinning makes a
  library update a change to the Jenkinsfile, which goes through the same
  review as any other change.

## How to fix it

Pin a tag or a commit SHA:

```groovy
@Library('pipeline-library@v1.4.0') _

library 'shared@4f2a9c1e8b7d6a5f4e3d2c1b0a9f8e7d6c5b4a39'
```

Then update the version on purpose, for example with a bot that opens a
pull request when the library publishes a new tag.

If the library's default version should stay a branch, have an
administrator turn off **Allow default version to be overridden** and set
the default version to a tag instead.

## Other personas

- **pedantic:** a library with no version, `@Library('lib') _` or
  `library 'lib'`, with low confidence. It loads the default version that
  the library's configuration in Jenkins sets, which this file does not
  show: it may be a tag or a branch.
- **auditor:** a version that Groovy computes, such as
  `library "lib@${params.VERSION}"` or `library "lib@$BRANCH_NAME"`, with low
  confidence: it may name a branch, and whoever chooses the value chooses
  the code.

## What it does not report

- **Tags that do not look like releases**, such as `stable` or `prod`,
  unless written as `refs/tags/stable`. They are reported as branches,
  since the name alone cannot tell them apart. See
  [When to ignore it](#when-to-ignore-it).
- **Libraries loaded implicitly**: those with **Load implicitly** set in
  Jenkins are not named in the file at all.
- **`lib(...)` outside a `libraries` block**: there it is just a method.

## When to ignore it

- When the version is a tag with a name that does not look like a
  release.
- When the job exists to test the library's branch, such as a library's own
  integration tests.

```groovy
// jenkins-lint: ignore[unpinned-library] this job tests the library's main branch
@Library('pipeline-library@main') _
```

When every pipeline in a repository loads the library from a branch on
purpose, such as the library's own repository, turn the rule off for all of
them with `--ignore unpinned-library`. See
[Turning off a rule](README.md#turning-off-a-rule).

## How precise it is

On 17,367 public Jenkinsfiles and shared library steps from GitHub, it
reports 598 findings in 501 files with the regular persona. In a random
sample of 50, 46 loaded a library from a branch such as `master`, `main`,
`develop`, `release/2.1.0` or a feature branch. The other 4 are no longer
reported: three loaded the library with `retriever: legacySCM(scm)`, from the
Jenkinsfile's own repository and branch, and one was a template placeholder,
`lib@@ref@`.

## References

- Jenkins: [Extending with Shared Libraries](https://www.jenkins.io/doc/book/pipeline/shared-libraries/),
  in particular [Library versions](https://www.jenkins.io/doc/book/pipeline/shared-libraries/#library-versions)
  and [Global Shared Libraries](https://www.jenkins.io/doc/book/pipeline/shared-libraries/#global-shared-libraries)
- zizmor: [unpinned-uses](https://docs.zizmor.sh/audits/#unpinned-uses),
  the same problem for GitHub Actions
