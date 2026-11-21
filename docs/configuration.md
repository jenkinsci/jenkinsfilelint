# Configuration

Supply credentials via environment variables (recommended) or CLI flags:

| Env Variable               | CLI Flag        | Required |
|----------------------------|-----------------|----------|
| `JENKINS_URL`              | `--jenkins-url` | No *     |
| `JENKINS_USER`             | `--username`    | No **    |
| `JENKINS_TOKEN`            | `--token`       | No **    |
| `JENKINSFILELINT_SERVER_IMAGE` | —          | No       |
| `JENKINSFILELINT_ENGINE`   | —               | No ***   |

\* Required for remote mode. Without any Jenkins setting, files are checked offline.
\*\* Only required if your Jenkins requires authentication (not used in ``--local`` or ``--offline`` mode).
\*\*\* Path of an engine binary for offline mode to use instead of the installed one.

> [!TIP]
> Even if your Jenkins allows anonymous access for validation, using an API token is recommended for production setups.

CLI flags override env vars. There is no config file.

## Local Docker image

By default, ``--local`` mode uses the official image
``ghcr.io/jenkinsci/jenkinsfilelint-server:latest``, which includes the
following plugins to cover common Declarative Pipeline constructs:

| Plugin | Common pattern enabled |
|--------|------------------------|
| `docker-workflow` | ``agent { docker '…' }``, ``docker.image('…')`` |
| `git` | ``git url: '…'`` |
| `credentials-binding` | ``withCredentials([…]) { … }`` |
| `timestamper` | ``options { timestamps() }`` |
| `ws-cleanup` | ``post { always { cleanWs() } }`` |
| `pipeline-utility-steps` | ``readJSON``, ``readYAML``, ``findFiles`` |

> This list is a curated starting point — not exhaustive. If your Jenkinsfile
> uses a plugin not listed here (e.g. ``kubernetes``, ``pipeline-github-lib``),
> please [open an issue](https://github.com/jenkinsci/jenkinsfilelint/issues/new)
> to propose adding it, or use a custom image (see below).

### Custom image

Build a custom image matching your production Jenkins setup and point to it
via ``JENKINSFILELINT_SERVER_IMAGE``:

```bash
# Export plugin list from your Jenkins (Script Console or Jenkins CLI)
# and generate plugins.txt + a Dockerfile, then:
docker build -t my-company/jenkinsfilelint-server:custom .
JENKINSFILELINT_SERVER_IMAGE=my-company/jenkinsfilelint-server:custom \
  jenkinsfilelint --local Jenkinsfile
```

> The [existing Dockerfile](https://github.com/jenkinsci/jenkinsfilelint/blob/main/docker/Dockerfile) and [plugins.txt](https://github.com/jenkinsci/jenkinsfilelint/blob/main/docker/plugins.txt)
> are a good starting template — fork them and adjust the plugin list to match
> your production Jenkins.
>
> A full Jenkins plugin list can be 100+ entries, which increases build time
> and image size. Only add what you need.
