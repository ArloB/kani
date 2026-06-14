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

## After release

- Update the `latest` Docker tag.
- Announce in GitHub Discussions.

<!-- TODO: document the CI release workflow steps in detail -->
