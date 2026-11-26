# Configuration

Offline checking needs no configuration. These settings are for `--remote`:

| Env Variable          | CLI Flag         | Used for |
|-----------------------|------------------|----------|
| `JENKINS_URL`         | `--remote URL`   | The server, when `--remote` is given without a URL *  |
| `JENKINS_USER`        | `--username`     | Credentials, if your Jenkins asks for them |
| `JENKINS_TOKEN`       | `--token`        | Credentials, if your Jenkins asks for them |

\* `JENKINS_URL` never turns remote checking on by itself. Jenkins sets it in
every build, so a lint step in a job stays offline unless it passes `--remote`.

> [!TIP]
> Even if your Jenkins allows anonymous access for validation, using an API token is recommended for production setups.

CLI flags override env vars. There is no config file.
