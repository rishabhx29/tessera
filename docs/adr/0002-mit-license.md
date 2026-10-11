# ADR-0002: MIT license

- Status: Accepted
- Date: 2026-10-11

## Context

The project's goals are users, stars, contributors, and legitimacy — not
revenue. The license is a growth decision, not a legal preference.

## Decision

MIT.

## Alternatives considered

- **GPL-3.0** (gyotaku's choice) — chosen by neighbors to prevent closed forks.
  For us that downside is nearly irrelevant while the upside (frictionless
  reuse in company tools, blog posts, scripts, distributions) is real.
- **AGPL-3.0** — strong network-copyleft. Meaningless for a desktop tool whose
  value is local execution; adds contribution friction.

## Consequences

- Maximum adoption and contribution ease; companies can embed it without legal
  review.
- Someone may ship a closed fork; we accept that in exchange for reach.
- All dependencies must stay MIT/Apache-2.0/BSD-compatible; new deps get a
  license note in the PR.
