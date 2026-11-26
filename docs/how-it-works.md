# How It Works

`jenkins-lint` answers one question: **"Will Jenkins accept this syntax?"**
It checks the file itself, offline, and with `--remote` also POSTs it to a
Jenkins's `/pipeline-model-converter/validate` endpoint.

- **Offline** (always): no Jenkins at all. The
  [`jenkinsfilelint-engine`](https://github.com/jenkinsci/jenkins-lint/tree/main/crates/jenkinsfilelint) package, a dependency of
  `jenkins-lint`, parses the file and checks the Declarative rules itself,
  using the messages of Jenkins's validator. One run checks all files in
  milliseconds.
- **Remote** (`--remote URL`): the files that pass offline are then validated
  by your Jenkins server, with the URL and credentials you give.

**Scripted Pipelines and other Groovy files.** The endpoint only checks
Declarative syntax. For a file with no `pipeline { … }` block (a Scripted
Pipeline, a shared-library `vars/*.groovy` step, an empty file) Jenkins still
parses the Groovy, so syntax errors such as an unclosed brace fail as usual.
If it parses, the file is reported as skipped and does not fail the run
(`⊘ vars/notify.groovy: Skipped (not a Declarative Pipeline)` with `--verbose`).
Offline mode does the same. To keep such files from being checked at all, use
[`--skip` / `--include`](usage.md#filtering-files).
