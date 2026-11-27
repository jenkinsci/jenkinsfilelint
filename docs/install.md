# Install

```bash
pip install jenkins-lint
```

Or as a tool of its own, with `pipx install jenkins-lint` or
`uv tool install jenkins-lint`.

## Binary

Every [release](https://github.com/jenkinsci/jenkins-lint/releases) has
the binary for each platform, with a `.sha256` file next to it. The Linux
`musl` builds are static, and run on any Linux:

```bash
curl -fsSL https://github.com/jenkinsci/jenkins-lint/releases/latest/download/jenkins-lint-x86_64-unknown-linux-musl.tar.gz \
  | tar -xz jenkins-lint
./jenkins-lint --version
```

| Platform | File |
|---|---|
| Linux x86_64 | `jenkins-lint-x86_64-unknown-linux-musl.tar.gz` |
| Linux aarch64 | `jenkins-lint-aarch64-unknown-linux-musl.tar.gz` |
| Linux i686, armv7 | `jenkins-lint-i686-unknown-linux-musl.tar.gz`, `jenkins-lint-armv7-unknown-linux-musleabihf.tar.gz` |
| macOS Apple silicon | `jenkins-lint-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `jenkins-lint-x86_64-apple-darwin.tar.gz` |
| Windows x86_64 | `jenkins-lint-x86_64-pc-windows-msvc.zip` |
| Windows x86 | `jenkins-lint-i686-pc-windows-msvc.zip` |

The glibc builds (`*-unknown-linux-gnu`) are there too. To pin a version,
replace `latest/download` with `download/v2.1.0`.

Each file has a build provenance attestation, which shows it was built by
this repository's release workflow:
`gh attestation verify jenkins-lint-x86_64-unknown-linux-musl.tar.gz --repo jenkinsci/jenkins-lint`.

## Docker

The image `ghcr.io/jenkinsci/jenkins-lint` has the static binary on Alpine,
for `linux/amd64` and `linux/arm64`. Tags follow the releases: `2.1.0`,
`2.1`, `2` and `latest`.

```bash
docker run --rm -v "$PWD:/src" ghcr.io/jenkinsci/jenkins-lint:2 jenkins-lint .
```

It has no entrypoint and has a shell, so CI systems can run their steps in
it. In a Jenkinsfile, as a docker agent:

```groovy
pipeline {
    agent {
        docker { image 'ghcr.io/jenkinsci/jenkins-lint:2' }
    }
    stages {
        stage('Lint') {
            steps {
                sh 'jenkins-lint .'
            }
        }
    }
}
```

In a Kubernetes pod template, give it `command: [cat]` and `tty: true`,
as for any other container. To keep a build from changing under you, pin
the image by digest, as [unpinned-image](rules/unpinned-image.md)
recommends: `ghcr.io/jenkinsci/jenkins-lint:2.1.0@sha256:...`.

## Requirements

- On Linux (x86_64, aarch64, i686, armv7; glibc or musl), macOS (Intel,
  Apple silicon) and Windows (x86_64, x86): nothing. The package is a
  prebuilt binary, which runs without Python once installed.
- Elsewhere, pip builds it from source, which needs a
  [Rust toolchain](https://rustup.rs).
- For `--remote`: a Jenkins server with the Pipeline plugin installed.
