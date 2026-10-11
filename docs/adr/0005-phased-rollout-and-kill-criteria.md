# ADR-0005: Phased rollout and pre-committed kill criteria

- Status: Accepted
- Date: 2026-10-11

## Context

This category is crowded and its graveyard is specific: at least three
open-source projects shipped the *entire* unified vision at once
(`aperture`: 0 stars, `rememex`: 68 stars then dead, `ask-my-screenshots`:
1 star) while the focused, polished projects earned real traction
(gyotaku: ~300 stars in two weeks; Windrecorder: ~4k over three years of
steady releases). Meanwhile six commercial tools ($15–19) race for the same
mainstream user.

The user's goals for this project: a real user base, stars, and legitimacy.
Not revenue.

## Decision

Build and release **one wedge at a time, each phase a relaunch**:

| Phase | Surface | The demo |
|---|---|---|
| v0.1 | Images (OCR+CLIP) + documents (text layer) | One query, mixed-format results |
| v0.2 | Video files | Search inside recorded meetings |
| v0.3 | Phone camera roll + MCP server | Phone screenshots; agents query the index |
| v0.4 | Auto-organize (dry-run, opt-in) | The folder that tidies itself |

Auto-organize — the most support-heavy, destructive-adjacent feature — is
last, on purpose, and only ever dry-run/opt-in.

### Kill criteria (pre-committed)

Eight weeks after launch: **< 300 stars or < 1,000 downloads → do not build
v0.2–0.4.** Pivot to video-only (the emptiest lane) or stop the project.

### Maintenance policy

Triage ≤ 30 min/week; releases at most every 2 weeks (not more — the cadence
is a floor, not a treadmill); the project stops rather than rots.

## Alternatives considered

- **Ship everything at once** — the graveyard's strategy. It also doubles the
  bug surface and halves polish per feature at exactly the moment first
  impressions are made.
- **Video-first** — emptiest lane, but heaviest compute, weakest dev-audience
  demo, and FrameScout already sits there (unpolished, unpositioned).
- **Phone-camera-roll-first** — most mass-market, but the star-giving audience
  cares least about it; better as the v0.3 marketing hook.

## Consequences

- Each phase reuses the same index; the marginal cost of a phase is its
  pipeline, not a rewrite.
- If v0.1 stalls, the criteria force a decision *early*, while stopping is
  cheap.
- The public roadmap doubles as a promise register: items are checked off only
  when a release contains them.
