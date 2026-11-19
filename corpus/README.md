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

# Start the Jenkins of local mode, which keeps running, then compare:
jenkinsfilelint --local Jenkinsfile
python3 corpus/compare.py --copy /tmp/look
```

`compare.py` uses the engine built in `crates/jenkinsfilelint/target/release`
(or `JENKINSFILELINT_ENGINE`), keeps Jenkins's answers in
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
case in `crates/jenkinsfilelint/tests/jenkins/`, checked with Jenkins, before
the engine changes.

## Results

On 2026-11-19, against the Jenkins of `ghcr.io/jenkinsci/jenkinsfilelint-server`,
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
repository.
