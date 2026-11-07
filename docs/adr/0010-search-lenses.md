# ADR-0010: Search lenses select the arms and the kinds

- Status: Accepted
- Date: 2026-11-08

## Context

The three-arm search answers "anything" in one box. That works until the user
knows what kind of thing they remember. A folder holds `samurai.jpg`, a
photograph of a samurai, and `samurai-notes.png`, a screenshot that contains
the word. A query for "samurai" returns both, and nothing on the screen says
which one looks like a samurai and which one says the word. The user cannot
ask "what does the picture look like" separately from "what words are inside
it", so the two answers arrive as one confused pile.

The engine already has the three signals this needs: keyword, image and
meaning. What is missing is a way for the user to say which question they are
asking.

## Decision

The search box has four lenses: `all`, `pictures`, `words`, `documents`. A
lens selects the arms it runs and the file kinds it searches.

| Lens | Arms | Kinds |
|---|---|---|
| `all` | keyword, image, meaning | every kind |
| `pictures` | image only | image |
| `words` | keyword, meaning | image, with visible body text |
| `documents` | keyword, meaning | pdf, docx, text, with visible body text |

Three rules make the answers honest:

1. **`pictures` does not run text arms.** A visual search is ordered by cosine
   similarity alone. Text evidence does not reorder it.
2. **`words` and `documents` search visible body text.** A file whose only
   extracted text is its own name is not a content match. The test is one
   newline in the tile text: `with_filename` puts the name on its own line, so
   a name-only file has one line. Such a file appears only in `all`, where its
   reason says "name matches".
3. **A filter narrows, never widens.** `type:` intersects the lens kinds. An
   empty intersection returns no rows plus a note that names the conflict.

`all` keeps the existing evidence-weighted RRF (ADR-0003). Every hit in `all`
carries the arms that voted, so the app can label why it matched.

The lens is not persisted. Every open starts at `all`.

## Alternatives considered

- **Exactly three lenses, no `all`.** The user must classify a query before
  typing it. A plain term like "invoice" would search only one lens. That is a
  worse default, and the three questions remain present under four lenses.
- **Client-side filtering of one fused result list.** The current All/Images/
  Docs chips do this. It cannot separate a visual match from a text match,
  because both are already fused and the chip only hides kinds.
- **Separate windows or modes per question.** More chrome, and the user still
  has to choose before searching. One field with four quiet segments is the
  smaller surface.
- **Persisting the lens.** A lens is a question, not a setting. Restoring the
  last question on launch would make the next search silently narrower than
  the user thinks.

## Consequences

- `fusion::search` takes a `SearchMode`. The CLI gains `--mode` so the lenses
  can be measured from a terminal.
- Missing models produce lens-aware notes. An arm the lens deliberately turns
  off produces no note.
- The app must label hits in `all` ("looks like this", "says ...", "name
  matches", "mentions this"). In the other lenses the lens is the reason.
- Geometry for marks is a separate decision. This ADR only decides which arms
  answer and which kinds appear.
- Changing the arms a lens runs is a one-line change in `SearchMode` plus the
  weights table. That is deliberate: the lenses are the product, the arms are
  the implementation.
