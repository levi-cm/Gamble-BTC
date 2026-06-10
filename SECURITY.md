# Security Policy

## Supported Versions

This project does not have stable releases yet. Security fixes should target the
current main development branch once a public repository exists.

## Reporting a Vulnerability

If the project is public, report vulnerabilities through the repository's
private vulnerability reporting feature when available. Until then, contact the
maintainer directly through the channel where the repository was shared.

Please include:

- A short description of the issue.
- Steps to reproduce it.
- The affected hardware, operating system, Docker version, and driver stack.
- Whether the issue exposes wallet data, local host resources, network access,
  or the unauthenticated HTTP status endpoint.

## Known Security Boundaries

- `.env` contains wallet/runtime configuration and must not be committed.
- The HTTP status endpoint currently has no authentication.
- The container needs access to host DRM devices.
- The Compose file is intended for a trusted local host, not public deployment.
