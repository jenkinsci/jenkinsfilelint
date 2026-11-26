# Install

```bash
pip install jenkins-lint
```

## Requirements

- Python 3.10+
- For remote mode: a Jenkins server with the Pipeline plugin installed
- For offline mode: nothing more on Linux (x86_64, aarch64, i686, armv7;
  glibc or musl), macOS (Intel, Apple silicon) and Windows (x86_64, x86),
  where `pip install jenkins-lint` installs a prebuilt engine. Elsewhere,
  `pip install jenkinsfilelint-engine` builds it from source and needs a
  [Rust toolchain](https://rustup.rs); the other modes work without it.
