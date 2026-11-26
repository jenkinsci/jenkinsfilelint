# Install

```bash
pip install jenkins-lint
```

Or as a tool of its own, with `pipx install jenkins-lint` or
`uv tool install jenkins-lint`.

## Requirements

- On Linux (x86_64, aarch64, i686, armv7; glibc or musl), macOS (Intel,
  Apple silicon) and Windows (x86_64, x86): nothing. The package is a
  prebuilt binary, which runs without Python once installed.
- Elsewhere, pip builds it from source, which needs a
  [Rust toolchain](https://rustup.rs).
- For `--remote`: a Jenkins server with the Pipeline plugin installed.
