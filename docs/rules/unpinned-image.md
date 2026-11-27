# unpinned-image

A container image is used without a tag, with the `latest` tag, or, for
the pedantic persona, with a tag but no digest.

| Severity | Confidence | Persona |
|---|---|---|
| medium: no tag or `latest`; low: a tag without a digest | high | regular; pedantic for a tag without a digest |

## What it finds

```groovy
agent { docker { image 'maven' } }        // maven is reported
docker.image('node:latest').inside { }    // node:latest is reported
```

It reads the image wherever a pipeline names one to pull:

- Declarative Docker agents: `agent { docker 'maven' }` and
  `agent { docker { image 'maven' } }`, for the pipeline or one stage
- Docker Pipeline: `docker.image('maven')`, with `.inside`, `.run`,
  `.withRun` or `.pull`
- the Kubernetes plugin: `containerTemplate(name: 'maven', image: 'maven')`

An image with a digest, `maven:3.9.9@sha256:...`, is pinned and not
reported. A registry's port, as in `localhost:5000/maven`, is not taken for
a tag.

## Why it matters

- **`latest` is whatever was pushed last.** An image without a tag is
  `latest`. Each build may get a different image: a new major version that
  breaks the build, or, if the publisher's account or registry is
  compromised, an image that runs an attacker's code with the build's
  credentials and workspace.
- **Builds stop being reproducible.** Rerunning last week's build does not
  run last week's tools.
- **A tag can move too.** Its publisher can push another image under
  `maven:3.9.9`. Only a digest names exactly one image, which is why the
  pedantic persona also reports tags without one.

## How to fix it

Use a version tag, and add the image's digest:

```groovy
agent {
    docker {
        image 'maven:3.9.9-eclipse-temurin-21@sha256:<digest>'
    }
}
```

`docker buildx imagetools inspect maven:3.9.9-eclipse-temurin-21` or
`docker inspect --format '{{index .RepoDigests 0}}' <image>` prints the
digest. Keep the tag next to it, so that people can read which version it
is. Dependabot and Renovate can open pull requests that update both.

## What it does not report

- **Images that Groovy computes**: `docker.image(params.IMAGE)`,
  `image "${env.REGISTRY}/maven"`.
- **Images a pipeline builds**: `docker.build('app')` and `dockerfile`
  agents. The `FROM` lines of the Dockerfile are where those are pinned; a
  Dockerfile linter such as [Hadolint](https://github.com/hadolint/hadolint)
  reports them (DL3006, DL3007).
- **Images in shell steps** (`sh 'docker run maven'`) and in pod YAML.

## When to ignore it

When the job is meant to follow the newest image, such as a nightly build
that tests compatibility with it.

```groovy
// jenkins-lint: ignore[unpinned-image] a nightly job that tests the newest image
agent { docker { image 'node:latest' } }
```

## How precise it is

On 17,367 public Jenkinsfiles and shared library steps from GitHub, it
reports 1,177 findings in 625 files with the regular persona. In a random
sample of 50, 49 used an image with no tag or with `latest`. The other one,
`image '$BUILDER_IMAGE'`, a variable in single quotes, is no longer
reported. A few of the 49 name images that the pipeline may build itself
earlier, such as `ubuntu_ci:latest`, which Docker then does not pull.

## References

- Docker: [Pull an image by digest](https://docs.docker.com/reference/cli/docker/image/pull/#pull-an-image-by-digest-immutable-identifier)
- Hadolint: [DL3006](https://github.com/hadolint/hadolint/wiki/DL3006) and
  [DL3007](https://github.com/hadolint/hadolint/wiki/DL3007), the same
  problem in Dockerfiles
- zizmor: [unpinned-images](https://docs.zizmor.sh/audits/#unpinned-images),
  the same problem in GitHub Actions
