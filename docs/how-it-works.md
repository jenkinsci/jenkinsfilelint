# How It Works

`jenkins-lint` answers one question: **"Will Jenkins accept this syntax?"**
It either checks the file itself, offline, or POSTs it to a Jenkins's
`/pipeline-model-converter/validate` endpoint.

- **Remote mode**: validates against your existing Jenkins server using the URL
  and credentials you configure. Used whenever a Jenkins setting is present.
- **Local mode** (`--local`): automatically starts a lightweight Jenkins
  container (via Docker or Podman) and validates against it. The container is
  reused across runs for near-instant validation.
- **Offline mode** (`--offline`, and whenever no Jenkins server is
  configured): no Jenkins at all. The
  [`jenkinsfilelint-engine`](https://github.com/jenkinsci/jenkins-lint/tree/main/crates/jenkinsfilelint) package, a dependency of
  `jenkins-lint`, parses the file and checks the Declarative rules itself,
  using the messages of Jenkins's validator. One run checks all files in
  milliseconds.

**Scripted Pipelines and other Groovy files.** The endpoint only checks
Declarative syntax. For a file with no `pipeline { … }` block (a Scripted
Pipeline, a shared-library `vars/*.groovy` step, an empty file) Jenkins still
parses the Groovy, so syntax errors such as an unclosed brace fail as usual.
If it parses, the file is reported as skipped and does not fail the run
(`⊘ vars/notify.groovy: Skipped (not a Declarative Pipeline)` with `--verbose`).
Offline mode does the same. To keep such files from being checked at all, use
[`--skip` / `--include`](usage.md#filtering-files).
