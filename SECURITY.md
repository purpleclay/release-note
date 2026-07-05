# Security Policy

## Token Trust Model

release-note uses `GITHUB_TOKEN` and `GITLAB_TOKEN` to resolve contributor information via platform APIs. To prevent credential leakage to unintended hosts, tokens are only attached to requests sent to explicitly trusted hosts.

### Always trusted

| Host           | Platform    |
| -------------- | ----------- |
| `github.com`   | GitHub SaaS |
| `*.github.com` | GitHub SaaS |
| `gitlab.com`   | GitLab SaaS |

### CI environments

When running inside a CI pipeline (`GITHUB_ACTIONS` or `GITLAB_CI`), the platform and API URL are taken from the pipeline's own environment variables. These are considered inherently trusted and tokens are attached normally.

### Self-hosted instances

Self-hosted instances whose hostname begins with `github.` or `gitlab.` (e.g. `github.company.com`, `gitlab.mycorp.io`) require explicit opt-in before a token is attached. Without opt-in, release-note still detects the platform and renders commit URLs, but makes no authenticated API requests. A warning is logged to explain why contributor resolution is degraded.

**Opt-in via environment variable** (comma-separated for multiple hosts):

```sh
export RELEASE_NOTE_TRUSTED_HOST=github.company.com
```

**Opt-in via CLI flag** (repeatable):

```sh
release-note --trusted-host github.company.com
```

This means that a repository with a remote such as `git@github.evil.com:x/y.git` will have its platform detected for URL rendering, but the user's token will never be sent to that host.

> **Limitation:** hosts that do not follow the `github.*` / `gitlab.*` naming convention (e.g. `git.company.com`) are not recognised as a platform at all and receive neither URL rendering nor contributor resolution, regardless of `RELEASE_NOTE_TRUSTED_HOST`. Support for fully custom domains is not currently implemented.

## Reporting a Vulnerability

Please report security vulnerabilities via a [GitHub Security Advisory](https://github.com/purpleclay/release-note/security/advisories/new).
