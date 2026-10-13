# ADR-0009: Model delivery — pins in the binary, HTTPS with a loopback exception

- Status: Accepted
- Date: 2026-10-13
- Implements: [ADR-0004](0004-model-choices.md) (delivery), [ADR-0008](0008-final-ocr-stack.md) (which artifacts)

## Context

ADR-0004 decided that models download once on first run and are verified
against pinned SHA-256 checksums, and ADR-0008 fixed which artifacts: SigLIP-base
int8, MiniLM int8, and PP-OCRv6 det-tiny + rec-small. It left three questions
open, and all three are expensive to reverse:

1. **Where does the pin live at runtime?** A checksum that ships beside the
   bytes it checks is a checksum the bytes can influence.
2. **What does the engine do with a file that does not match its pin** — fail,
   or quietly re-download whatever the directory now contains?
3. **How is the download path tested** without downloading 257 MB on every CI
   run, and without a test double standing in for the code that does the
   security check?

`SECURITY.md` names model-download integrity as a bug class: *"checksums are
pinned; anything that bypasses verification is a security bug."* That is the
standard these decisions are measured against.

## Decision

1. **The pins are compiled into the binary.** `crates/core/src/models/lock.json`
   is embedded with `include_str!`. It is derived from `spike/model-lock.json`
   (the spike's measured record) and keeps only the models v0.1 runs;
   MobileCLIP2 and CLIP ViT-B/32 stay in the spike file as the evidence for
   rejecting them. A manifest that cannot be trusted — no checksum, a short or
   non-hex checksum, a zero size, a duplicate set — **does not load**.
2. **Bytes that do not match their pin never run, and are replaced rather than
   blocking.** A mismatching download is a loud error naming the file, what was
   expected and what arrived. A file already on disk that does not match is
   deleted and re-fetched, and the install report says so. A pin moving is a
   routine event; making every user hand-delete files to get past one is a bug
   report waiting to happen, and the unsafe outcome — *using* unverified bytes —
   is prevented either way.
3. **Every source is HTTPS, with one exception: loopback.** A model fetched from
   `127.0.0.1` never crosses a network, so TLS protects nothing there. This is
   what lets the download path be tested against a real local HTTP server — so
   the production fetch code is what the tests exercise, not a double. A test
   asserts that every pinned source is HTTPS, so the exception can never be used
   for a real model.
4. **Verification is by content, and the pinned byte count is checked too.** The
   count distinguishes a truncated transfer from a tampered file, which have
   different causes and different fixes. Redirects are followed (model hosts use
   them); the checksum is what makes that safe, since bytes are verified wherever
   they came from.
5. **Vector spaces are versioned per arm.** The version token is a digest over a
   set's pins, sorted by name, recorded in `meta` beside the vectors. Changing the
   SigLIP pin marks the image arm outstanding and leaves the text arm alone.
   Blending vectors from two models is never an option: it looks exactly like
   "search got worse for no reason".

## Alternatives considered

- **Pins in a plain `models.json` next to the models.** Simpler to update
  without a rebuild. Rejected: it makes the pin editable by whatever wrote the
  directory, which is the threat `SECURITY.md` names.
- **Hard-fail on any mismatching file, telling the user to delete it.** Strictest
  reading of "fails loudly". Rejected as too brittle for a pin bump during
  development; the loudness is preserved in the report and in the error.
- **A `Fetch` trait so tests could serve bytes without a socket.** Would have
  meant the security-relevant code under test was the fake, not the fetch. A
  loopback server costs about forty lines and tests the real thing.
- **A `--insecure-http` flag for testing.** Rejected: a flag that disables a
  security check is a flag that ships.

## Consequences

- Changing a model version means editing the lock, rebuilding, and re-embedding
  the affected arm. That friction is the feature.
- Verification reads the installed bytes each time it is asked ("are these
  still the files we pinned?"), which is why it happens on the indexing path and
  never on the query path — a keyword search does not touch the models directory.
- `cargo test --test models -- --ignored` re-verifies one pinned checksum
  against upstream over the network, so pins are provable rather than trusted
  forever. It is ignored by default: CI does not depend on the network.
- A second Tessera instance downloading the same set concurrently can have its
  in-flight partial swept by the other; the download fails and is retried. No
  wrong file can be installed, because nothing is installed without matching its
  pin.