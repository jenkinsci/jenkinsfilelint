# Install

```bash
pip install jenkins-lint
```

Or as a tool of its own, with `pipx install jenkins-lint` or
`uv tool install jenkins-lint`.

## Without Python

Each [GitHub release](https://github.com/jenkinsci/jenkins-lint/releases)
has the binary for every platform above, the same one as in the wheel:
`jenkins-lint-<target>.tar.gz`, or `.zip` for Windows, each with a
`.sha256` file. For example, on a Linux x86_64 agent (the musl binary is
static, so it runs on any distribution); replace `latest/download` with
`download/v2.0.1`, say, to pin a version:

```bash
target=x86_64-unknown-linux-musl
url=https://github.com/jenkinsci/jenkins-lint/releases/latest/download
curl -fsSLO "$url/jenkins-lint-$target.tar.gz"
curl -fsSLO "$url/jenkins-lint-$target.tar.gz.sha256"
sha256sum -c "jenkins-lint-$target.tar.gz.sha256"
tar xzf "jenkins-lint-$target.tar.gz"
./jenkins-lint-$target/jenkins-lint --version
```

The targets are `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`i686-unknown-linux-gnu`, `armv7-unknown-linux-gnueabihf`, their `-musl`
(`-musleabihf`) counterparts, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc` and `i686-pc-windows-msvc`.

## Requirements

- On Linux (x86_64, aarch64, i686, armv7; glibc or musl), macOS (Intel,
  Apple silicon) and Windows (x86_64, x86): nothing. The package is a
  prebuilt binary, which runs without Python once installed.
- Elsewhere, pip builds it from source, which needs a
  [Rust toolchain](https://rustup.rs).
- For `--remote`: a Jenkins server with the Pipeline plugin installed.
