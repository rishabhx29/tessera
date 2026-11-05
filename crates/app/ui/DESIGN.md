# Tessera window — design direction

The desktop window shares one brand with the website: warm paper, charcoal
ink, one terracotta accent, hairline rules. The website's `DESIGN.md` owns the
palette and type stacks; this file is the window's reading of it, plus the
interaction rules that belong to a search tool specifically.

## The archetype

Tessera's window is a **launcher-class surface** (Raycast, Spotlight): it is
summoned, used dozens of times a day, and dismissed. That decides almost every
rule below. Speed is the aesthetic; anything that adds a beat between
keystroke and answer is a defect, however pretty.

## Brand tokens (aligned with `website/DESIGN.md`)

| Token | Light | Role |
|---|---|---|
| canvas | `#FAF9F5` | window background — warm archival paper |
| surface | `#FFFFFF` | elevated panels: search bar, results, cards |
| subtle | `#F3F1EB` | secondary surface: hover fills, log, badges |
| inset | `#EBE8E0` | deepest panel: code, image backdrop |
| line | `#E5E2D9` | 1px hairlines |
| line-strong | `#D1CDC0` | input borders, focus-adjacent |
| ink | `#181715` | headings, result names (15.4:1 AAA) |
| ink-2 | `#45423D` | body, snippets (8.8:1 AAA) |
| muted | `#59554E` | meta, paths, hints (6.6:1 AA) |
| faint | `#6B665E` | the quietest annotations (5.0:1 AA) |
| accent | `#C2410C` | terracotta — selection, matches, focus (5.1:1 AA) |
| accent-strong | `#9A3412` | accent hover / pressed (7.3:1 AAA) |
| accent-subtle | `#FFF7ED` | selection background wash |

Dark theme keeps the same structure (paper → deep warm charcoal, accent lifts
to a light terracotta for contrast). Status colors (`ok`/`warn`/`bad`) are
warm-leaning greens, ambers and reds tuned to both themes.

Type: the site's three stacks — **Charter** serif for display headings only
(view titles, first-run); system sans for everything interactive; mono
(Cascadia) for paths, latency, file sizes, and log output, always with
`tabular-nums` where numbers update.

## Interaction rules (launcher-class)

1. **Keyboard actions never animate.** `Ctrl+K`, `Esc`, `Ctrl+1/2/3`,
   arrow-key selection — all instant, zero transition. These run hundreds of
   times a day; a 120 ms fade per press is a tax, not a polish (Raycast ships
   no open/close animation at all).
2. **Hover and selection change color only.** No movement, no scale on result
   rows. The selection is a background wash plus a 3px accent edge.
3. **Pressable things answer the press.** Buttons get `scale(0.97)` on
   `:active` at ~120 ms ease-out. This is the one transform feedback a
   high-frequency UI keeps.
4. **Motion budget under 300 ms, usually under 200.** Only the toast, preview
   pane, and first-run state animate, and each exists to prevent a jarring
   change — never to decorate. Exit faster than enter.
5. **Ease-out or a custom curve, never ease-in.** `--ease-out:
   cubic-bezier(0.23, 1, 0.32, 1)` for entries; `ease` for color changes;
   linear only for indeterminate progress.
6. **Only `transform`, `opacity` and color transition.** Never width, height
   or `all`.
7. **`prefers-reduced-motion` collapses movement to opacity/color.**
   `prefers-reduced-motion: no-preference` gates every transform transition.
8. **Hovers are gated behind `@media (hover: hover) and (pointer: fine)`** so
   touch devices don't get sticky hover states.

## Component standards

- **Search bar**: 48px pill-adjacent field, hairline border that turns
  terracotta with a soft focus wash when focused. Latency readout lives
  inside the field's right edge, mono, tabular — and honest: total wall time
  first, engine time called out separately.
- **Results**: one row per file, name → folder → modality badges → snippet
  with terracotta match runs. Rows are dense (this is a list, not cards);
  the selected row is unmistakable. Arm badges say *why* a file surfaced —
  that is the product's honesty promise, so they are never decorative.
- **Preview pane**: the file at left, its evidence at right. Image previews
  sit on the inset surface; OCR/page text highlights matches. Meta is mono.
- **Empty states**: every empty state states a cause ("no matches", "no
  folders yet", "models need download") and, where one exists, the next
  action. An empty state that only says "empty" is a bug.
- **Activity/Settings**: cards on surface, hairline borders, mono for every
  path and number, status pills for watcher state.

## The dev mock

`dev-mock.js` provides `window.__TAURI__` **only when the real Tauri API is
absent** (a plain browser opening `index.html`). Under the packaged app it is
inert: the first line checks for the real API and does nothing. It exists so
the window's visual and interaction states can be reviewed without building
the Rust shell — the QA rounds correctly kept flagging "the UI is read, not
run" — and it mirrors the real payload shapes in `engine.rs`/`main.rs`, not
invented ones.
