# Security Policy

## Supported Versions

This project does not have stable releases. Security fixes target the current
`main` branch.

## Reporting a Vulnerability

Report vulnerabilities through GitHub private vulnerability reporting when it
is available for this repository. Otherwise contact the maintainer privately;
do not publish exploit details in an issue.

Please include:

- A short description of the issue.
- Steps to reproduce it.
- The affected hardware, operating system, Docker version, and driver stack.
- Whether the issue exposes wallet data, local host resources, network access,
  or the unauthenticated HTTP status endpoint.

## Known Security Boundaries

- `.env` contains wallet/runtime configuration and must not be committed.
- The HTTP status endpoint has no authentication and defaults to loopback.
- The container needs access to host DRM devices.
- The Compose file is intended for a trusted local host, not public deployment.
