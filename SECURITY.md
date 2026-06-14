# Security Policy

## Reporting a Vulnerability

Please **do not** open a public GitHub issue for security vulnerabilities.

Report a vulnerability through:

- **GitHub private security advisory:** [Report a vulnerability](../../security/advisories/new)

### What to include

- A clear description of the vulnerability and its potential impact
- Steps to reproduce (version, configuration, request/response if applicable)
- Any proof-of-concept code, if available

### Response timeline

| Milestone | Target |
|-----------|--------|
| Acknowledgement | 72 hours |
| Initial assessment | 14 days |
| Fix / coordinated disclosure | 90 days from report |

We practice coordinated disclosure: we ask that you not publish details publicly until 90 days
after the report, or until a fix is released, whichever comes first.

## Scope

In scope: the Kani server binary, REST API, auth layer, WASM runtime host, and Docker image.

Out of scope: third-party extensions installed by users, the content of external websites accessed
through extensions, and user-created configuration.
