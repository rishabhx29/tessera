# Security Policy

## Supported versions

Tessera is pre-alpha; no releases are supported yet. When v0.1 ships, the latest
release line is the supported one.

## Reporting a vulnerability

Report vulnerabilities privately via GitHub Security Advisories (the
**Report a vulnerability** button on the repository's Security tab). Do not
open public issues for security problems. Expect an acknowledgement within
7 days.

## What matters in this threat model

Tessera is a local-first desktop tool: the index lives in a single local SQLite
file and no user data is transmitted anywhere. The highest-severity bug classes,
given that design:

- Anything that reads or writes outside the designated index and cache directories.
- Model download integrity — checksums are pinned; anything that bypasses
  verification is a security bug.
- Anything that can modify, move, or delete a user's files. (v0.4 will touch
  files, and will be opt-in, dry-run-first, and undoable by design.)

## Please don't include private content in reports

The index contains the extracted text of your screenshots and documents.
Sanitize anything you attach to an issue or advisory.
