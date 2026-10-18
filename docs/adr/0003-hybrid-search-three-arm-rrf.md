# ADR-0003: Hybrid search — three arms fused with RRF

- Status: Accepted
- Date: 2026-10-11

## Context

The product's core loop is "type what you remember, find the file." Users
remember three different kinds of things:

1. **Words they saw** — an error string, a name, an invoice number →
   needs exact/typo-tolerant lexical search over OCR and document text.
2. **What something looked like** — "the one with the red chart" → needs a
   CLIP text↔image space.
3. **What a document was about** — "the contract with the non-compete" →
   needs text embeddings over document chunks.

Every surveyed solution picks one arm: lexical-only (gyotaku), semantic-only
(most CLIP demos), or lexical+image. The multi-arm hybrid is the differentiator
we own.

## Decision

Three arms — lexical (FTS5 trigram, BM25), image (CLIP), text (MiniLM) —
fused with **Reciprocal Rank Fusion**: merge by rank position, never by raw
score, because BM25 and cosine scores are incomparable quantities. All arms
run by default; the user never picks a mode.

## Alternatives considered

- **Vector-only** — misses exact identifiers, error codes, filenames. The
  survey of comparable projects (qimg, callimachus, foldermcp, ask-my-screenshots)
  converges on hybrid for exactly this reason.
- **Lexical-only** — can't answer description queries, which is half the demo.
- **Score-normalized weighted fusion** — requires cross-arm calibration that
  silently breaks as the corpus grows. Rank fusion degrades gracefully.

## Consequences

- Three model dependencies and three embedding tables — complexity is real and
  contained by the store schema.
- The trigram tokenizer gives typo tolerance for free (`inv0ice` → `invoice`).
- Model versions are pinned in the `meta` table; changing a model invalidates
  that arm's vectors and forces re-indexing, never silent mixing (see
  architecture.md).
- Fusion quality is measurable: the spike and dogfooding sessions include
  arm-ablation numbers before launch.

## Amendment (ticket 01, engine build)

The Consequences line about trigram "typo tolerance for free" is wrong on
mechanism, and building the lexical arm proved it: trigram matching is
*substring* matching, so `inv0ice` (a typo) is not a substring of `invoice`
and matched nothing. Implemented instead: query-side lookalike expansion
(`0`↔`o`, `1`↔`l`↔`i`, `5`↔`s`, `8`↔`b`), one substitution per position,
OR-ed into the MATCH expression. The write-time folded second index
(gyotaku's mechanism, which also survives OCR misreads inside the corpus)
remains a v0.1.x follow-up. The decision itself — three arms, RRF — is
unchanged; only this rationale line is corrected.

## Amendment (ticket 10, the fused search box)

The decision above says RRF; the first cut of it treated each arm's vote as
equal. Ticket 10 built the one search box and the sweep in
`crates/core/tests/image_fusion.rs` measured what equal treatment costs on 56
real screenshots, where the text arm alone gets top-1 0.964: an equal image
voice takes it to **0.375**, a 0.10 voice to 0.857, while a **0.05** voice
holds 0.964 (`docs/performance.md`). So a vote is weighted by evidence the
index already holds — how much text the file's tiles carry:

| evidence (tile text in the file) | lexical | image | text |
|---|---|---|---|
| Full — more than 120 characters | 1.0 | **0.05** | 1.0 |
| Little — 1 to 120 characters | 0.5 | 0.5 | 0.5 |
| None — no tiles at all | 0.0 | 1.0 | 0.0 |

The Full row is measured; the Little and None rows are the honest
split-the-difference and image-alone defaults until something measures them —
they live in `CHOSEN` in `crates/core/src/fusion.rs` so that is a one-edit
change, and nothing here claims more than it measured. The text arm follows
the lexical arm's weight because it is the same signal — the file's own text —
read by a different mechanism.

Mechanics pinned with the weights: a vote whose weight is zero or less is
skipped entirely (no score, and the arm is not named on the hit); a file's
score is the sum of `weight / (60 + position)`, positions zero-based, ties
broken by file id, the list truncated at the limit. The `type:`, `in:`,
`before:` and `after:` filters are parsed from the same arguments and applied
to every arm — as SQL on the lexical arm, as an allowed set on the semantic
arms — because a filter honoured by one arm and not another lets the excluded
files straight back in.

The arithmetic lives in one pure function (`fuse`: ranked lists in, order
out); the offline sweep re-derives its numbers through it rather than through
a copy, so the constant is chosen from what ships. The query path runs the
three arms on threads and hands their lists to that function.
