# Contributing

Contributions are welcome! Here's how to get started.

## Setup

```bash
pip install ./crates/jenkinsfilelint   # the offline engine; needs a Rust toolchain
pip install -e ".[dev]"
pre-commit install
```

The first line builds `jenkinsfilelint-engine` from this checkout, so the tests
run against the engine you are working on. To try another engine binary
without installing it, set `JENKINSFILELINT_ENGINE` to its path.

Run pre-commit before submitting:

```bash
pre-commit run --all-files
```

## Testing

```bash
pytest tests/ -v --cov=jenkinsfilelint
```

`tests/test_offline.py` also runs `jenkinsfilelint --offline` end to end on the
cases in `crates/jenkinsfilelint/tests/jenkins/`. For the engine itself, see
[crates/jenkinsfilelint/README.md](crates/jenkinsfilelint/README.md#development).

## The offline engine and releases

`crates/jenkinsfilelint` is published to PyPI as `jenkinsfilelint-engine`, a
set of prebuilt wheels. `pyproject.toml` pins it to the exact version in
`crates/jenkinsfilelint/Cargo.toml`, and a test checks that the two agree.
When a change touches the engine, bump the version in both places in the
same PR. Otherwise the next release keeps the engine already on PyPI.

Publishing a GitHub release runs `.github/workflows/release.yml`. It builds
the engine wheels and the `jenkinsfilelint` package, installs them on each
platform, then uploads `jenkinsfilelint-engine` (skipping files already on
PyPI) and only then `jenkinsfilelint`. Pull requests that touch `crates/` or
`pyproject.toml` run the same workflow without uploading.

## Conventions

- Follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, etc.)
- Keep PRs focused — one logical change per PR.
- Update the README if your change affects usage or configuration.
