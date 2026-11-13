# Security policy

tuilane is early-development, local-first software with planned plugin and agent integrations. Avoid storing critical credentials or relying on unimplemented isolation claims. This document describes reporting expectations; it does not assert a mature security program or audited plugin sandbox.

## Reporting a vulnerability

**Do not disclose exploitable vulnerability details in public issues or discussions.**

Preferred reporting channel: the repository's GitHub **Security → Report a vulnerability** feature **if the maintainers have enabled private vulnerability reporting**. This documentation archive cannot verify whether that setting is enabled.

If no private reporting option is available, the maintainer must establish a verified private contact channel before accepting sensitive vulnerability details. Do not assume GitHub issues are private, and do not place credentials, personal data, proof-of-concept exploits or private database dumps into public tickets.

For non-sensitive security improvements, ordinary issues and pull requests are appropriate.

## Information useful in a private report

- Affected version/commit and supported platform/terminal.
- Clear reproduction steps and expected versus actual security boundary.
- Potential impact, attack prerequisites, and any safe minimal proof of concept.
- Whether private data, privilege escalation, filesystem/network access or plugin permissions are involved.

Redact passwords, API tokens, personal data and real user vault contents.

## Supported versions

No public support window or stable release has been verified from the supplied documentation. Once releases exist, maintainers should specify supported version ranges here and keep them aligned with actual patch/backport policy.

## Security boundaries

- Trusted, statically linked Rust plugins share process privileges; a service broker or Rust trait alone does not sandbox arbitrary code.
- External plugins and AI tool requests must pass appropriate authorization and isolation checks before privileged operations become supported.
- Security-sensitive code requires focused negative tests, disclosure of relevant limitations, and maintainer review.

See [engineering standards](docs/coding-standards.md) and [architecture](docs/architecture.md).
