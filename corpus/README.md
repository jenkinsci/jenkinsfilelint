# Real-world corpus

Public Jenkinsfiles and shared library steps from GitHub, to check the
offline engine against Jenkins itself at scale: every file Jenkins's own
validator accepts should pass offline, and every error it reports should be
reported offline too, with the same message where the validator words it.

`collect.py` writes `manifest.tsv`, which lists the files, one per line:
the Git blob hash, the repository, the commit and the path. It pins exactly
which version of each file was checked. The files themselves are other
people's code and stay out of this repository; `fetch.py` downloads them
into a local cache.

The manifest is not in Git yet: at about 3 MB, every pre-commit user
would download it with each release, since pre-commit fetches the whole
tree of the version it runs. It will be published with the documentation
site instead.

## Running it

```console
# Once, or to refresh the manifest (about 25 minutes, needs `gh` logged in):
python3 corpus/collect.py

# Download the files the manifest lists into corpus/.cache/files:
python3 corpus/fetch.py

# Start a Jenkins with the plugins in tests/jenkins-server/, which keeps running, then compare:
python3 tests/jenkins_server.py start
python3 corpus/compare.py --copy /tmp/look
```

`compare.py` uses the jenkins-lint built in `crates/jenkins-lint/target/release`
(or `JENKINS_LINT_ENGINE`), keeps Jenkins's answers in
`corpus/.cache/jenkins.jsonl` so that later runs only ask about new files,
and writes every file's verdicts to `corpus/.cache/report.tsv`.

| Category | Means |
|---|---|
| `agree` | Same verdict, and the same first error if any |
| `syntax` | Both find a Groovy syntax error, worded differently |
| `needs-plugins` | Jenkins reports what only its plugins know: step parameters, agent, option, trigger and tool types |
| `false-error` | The engine reports an error that Jenkins does not: a bug |
| `missed` | Jenkins reports an error that the engine does not |
| `other-message` | Both report errors, but different ones |

Each `false-error`, `missed` or `other-message` file becomes a small test
case in `crates/jenkins-lint/tests/jenkins/`, checked with Jenkins, before
the engine changes.

## Checking a change

`diff.py` shows what a change to the engine changes on these files. It
builds the engine of `origin/main` (or `--base REF`) and the engine of the
checkout, runs both on every file, and lists the differences:

```console
$ python3 corpus/diff.py --base a846990
17,373 files. Base: a846990eff2b (a846990). Head: 4b2d3f010b4e.

Verdicts
  now rejected               34
  now accepted              128
  error changed              48

Checked with Jenkins
  fixed                     177
  still right                30
  still wrong                 2
  no answer                   1

Findings                            added removed changed
  script-injection (auditor)          85      42       0
  script-injection (pedantic)         11       0       0
  script-injection (regular)         113     165       0
  secret-interpolation (pedantic)      33      32       0
  secret-interpolation (regular)      54       4       0
  of these, in files whose verdict changed: 296 added, 243 removed, 0 changed
```

That is the change of #71, which fixed what `compare.py` found. Every
file that changed verdict either now agrees with Jenkins or is one of the
known gaps.

- **Verdicts** count the files whose validation errors changed.
- **Checked with Jenkins** asks Jenkins about those files, or reads its
  answers from the cache of `compare.py`:
  - a file is "fixed" when the engine now agrees with Jenkins, and "broken"
    when it no longer does;
  - `diff.py` exits with 1 when a file broke;
  - `--no-jenkins` skips the check.
- **Findings** count the audit findings added, removed or changed, by rule and
  by the least thorough persona that shows them. A file that no longer parses
  loses its findings, as Jenkins does not run it.

The summary names no repository, so it can go in a pull request.
`.cache/diff/changes.tsv` lists every difference with its file.

### How precise a rule is

To measure a rule's precision, add `--sample N`. The script then writes
`.cache/diff/sample-<rule>.md`, with N added findings of each rule picked at
random and the code around them. Check each one by hand, and count how many
are real. `--seed` picks another sample.

With `--base none`, every finding counts as added, which samples all of a
rule's findings rather than only its new ones.

## Results

On 2026-11-19, against the Jenkins of `tests/jenkins-server/`,
the 17,373 files of the manifest compared as follows:

| Category | Files |
|---|---|
| `agree` | 13,025 |
| `syntax` | 407 |
| `needs-plugins` | 3,906 |
| `known` | 11 |
| `false-error` | 0 |
| `missed` | 6 |
| `other-message` | 11 |
| no answer from Jenkins | 7 |

The six missed errors are Java written in `class` bodies, which the engine
does not read closely, and `"${A B C}"`, which Jenkins reads in a way not
yet understood. Seven files got an error from Jenkins with no message.

## How the files are chosen

GitHub code search returns at most 1,000 results per query, so
`collect.py` searches by file size: files named `Jenkinsfile` in about a
hundred size ranges from a few bytes to 400 KB, and `vars/*.groovy` files of
shared libraries in ten. Files with the same content are listed once, and
files in private repositories, which search shows to the user logged in,
are left out.

## Security findings

Some of these files have real vulnerabilities. Report them to their
maintainers privately, not in public issues, and do not name them in this
repository. `changes.tsv` and the samples of `diff.py` name the files: keep
them out of pull requests and issues, and paste only the summary.
