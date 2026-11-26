# Contributing

Contributions are welcome! Here's how to get started.

## Setup

jenkins-lint is written in Rust, in `crates/jenkins-lint`. You need a
[Rust toolchain](https://rustup.rs), and Python for pre-commit and a few
scripts.

```bash
cd crates/jenkins-lint
cargo build
cargo run -- path/to/Jenkinsfile
pip install pre-commit && pre-commit install
```

Run pre-commit before submitting:

```bash
pre-commit run --all-files
```

## Testing

```bash
cd crates/jenkins-lint
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                         # unit tests, and tests/cli.rs end to end
python3 scripts/corpus_check.py    # the official validator's test files
```

`cargo test` also checks every case in `tests/jenkins/` against what
Jenkins's own validator said about it. `tests/` at the root holds the
Python tests of the release scripts, and the Docker integration tests, which
check `--remote` against a real Jenkins built from `docker/`:

```bash
pytest tests/ -m "not docker"
pytest tests/ -m docker            # needs Docker; builds and starts docker/
```

A change to the parser, the validator or an audit rule should also be checked
on public Jenkinsfiles. `corpus/diff.py` compares the engine with the one on
`main` on about 17,000 public files, and asks Jenkins which of them was
right. [corpus/README.md](corpus/README.md#checking-a-change) shows how.
Paste its summary into the pull request.

## Releases

`crates/jenkins-lint` is published to PyPI as `jenkins-lint`: one wheel per
platform with the binary, and an sdist. The root `pyproject.toml` is only
for pre-commit, which installs a hook's repository from source. It has the
same version as `crates/jenkins-lint/Cargo.toml` and pins `jenkins-lint` to
it, so that a hook installs the wheel of its tag.

To release, bump the version in `Cargo.toml` and in both places in the root
`pyproject.toml`, then publish a GitHub release tagged `v<version>`.
`crates/jenkins-lint/scripts/check_version.py` checks that these agree, and
the release workflow runs it first.

Publishing the release runs `.github/workflows/release.yml`. It builds the
wheels and the sdist, installs each wheel on its platform, uploads them, and
finally installs `jenkins-lint` from PyPI on Linux, macOS and Windows. Pull
requests that touch `crates/` or `pyproject.toml` run the same workflow
without uploading.

To rehearse a release, run it by hand with `publish: testpypi`:

```bash
gh workflow run release.yml --ref <branch> -f publish=testpypi
```

The same jobs then upload to TestPyPI (environment `testpypi`, which needs a
TestPyPI trusted publisher) and install from there. A version is uploaded
once: a second rehearsal of the same version skips the upload.

The trusted publisher of `jenkins-lint` on PyPI and TestPyPI is: owner
`jenkinsci`, repository `jenkins-lint`, workflow `release.yml`, environment
`pypi` (or `testpypi`).

## Conventions

- Follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, etc.)
- Keep PRs focused — one logical change per PR.
- Update the README if your change affects usage or configuration.
