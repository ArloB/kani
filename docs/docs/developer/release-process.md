# Release Process

!!! note "TODO"
    This page is a stub. Full content coming soon.

## Overview

1. All feature work merges into `develop` via PRs.
2. When `develop` is stable and tested, it is merged into `main`.
3. A git tag (`vX.Y.Z`) on `main` triggers the release CI workflow, which builds binaries and
   Docker images and publishes them to GitHub Releases and GHCR.

## Version bump

1. Update `version` in the workspace `Cargo.toml`.
2. Update `CHANGELOG.md` — move entries from `Unreleased` to the new version section.
3. Open a PR: `develop` → `main`.

## Rehearsing a release before tagging

The release workflow runs on every pull request as well as on a tag, so the
artifact matrix is exercised before the tag that ships depends on it
(`pr-run-mode = "upload"` in `dist-workspace.toml`). A PR run builds everything
and publishes nothing:

- `plan.outputs.publishing` is false on a pull request, and that value is passed
  to `docker.yml` as its `publish` input.
- With `publish: false` the image is built for **both** `linux/amd64` and
  `linux/arm64` and then thrown away — no GHCR login, no push, no cosign signing.
  A broken `Dockerfile` still fails the PR, which is the point.
- Because a PR has no tag to announce, the image is named `v0.0.0-rehearsal`, so
  it can never collide with a real release tag or move `latest`.

Keyless cosign signing is the one step a rehearsal cannot cover: it needs a real
GitHub Actions OIDC token, so only a tag run (or a manual
`workflow_dispatch` of **Publish Docker image**, which does push and sign) proves
it. Dispatch against a throwaway prerelease tag such as `v0.9.0-rc.0` if you want
that proof without shipping `latest` — a tag containing `-` is treated as a
prerelease and never moves it.

## After release

- Update the `latest` Docker tag.
- Announce in GitHub Discussions.

<!-- TODO: document the CI release workflow steps in detail -->
