# Tessera window — design direction

The window is a **tool**, not a page. It deliberately parts from the website's
warm editorial palette: the site is the publication, the app is the instrument.
The app is neutral graphite and true white with one cobalt accent, so the
content — screenshots, documents, names — carries every bit of colour. The
website's `DESIGN.md` owns the site's own palette; this file owns the window.

## The archetype

Tessera's window is a **launcher-class surface** (Raycast, Spotlight): it is
summoned, used dozens of times a day, and dismissed. That decides almost every
rule below. Speed is the aesthetic; anything that adds a beat between
keystroke and answer is a defect, however pretty.

## Tokens

| Token | Light | Dark | Role |
|---|---|---|---|
| canvas | `#F6F7F9` | `#0B0D11` | window background |
| surface | `#FFFFFF` | `#111419` | search bar, cards, preview |
| surface-2 | `#EEF0F3` | `#191D24` | hover fills, filter track, lead tiles |
| inset | `#E7EAEF` | `#0E1116` | image backdrop inside thumbnails |
| line | `#E3E6EB` | `#222730` | 1px hairlines |
| line-strong | `#CBD1DA` | `#333A47` | input borders |
| ink | `#101318` | `#ECF0F5` | headings, result names (~17:1) |
| ink-2 | `#3B424E` | `#C2C9D4` | body, snippets (~9:1) |
| muted | `#5A6270` | `#8E97A6` | meta, paths, hints (6.1:1 light) |
| faint | `#636C7A` | `#8B94A3` | quietest annotations (5.3:1 light) |
| accent | `#2F5FE3` | `#7AA2FF` | selection, matches, focus (5.5:1 light) |
| accent-strong | `#1E4BC8` | `#9BBAFF` | selected names, primary hover (7.3:1 light) |
| accent-soft | `#EDF2FE` | `#16233F` | selected row wash |
| ok / warn / bad | `#2C6E46` / `#8A5A17` / `#B3261E` | `#7AD3A0` / `#E6B566` / `#F2938D` | status, both themes |

The accent is a signal, not decoration. It appears only on: the selected row
or tile, matched text, the primary action, focus rings, and the live state.
Anything else is ink, line or surface.

**Type.** One family: the system sans stack. Headings are the same face at
weight 600 with −0.02em tracking; there is no serif in the app. Mono
(Cascadia) is reserved for paths, latency, sizes and log output, always with
`tabular-nums` where numbers update. Base is 13px/1.5; the query is 15px.

**Shape.** 12px for panels and the search bar, 8px for buttons, 10px for image
frames, 6px for chips, full pill only for status. No radius above 12px.

## The mark

Four rounded tiles in a 2×2 mosaic; three are ink and one is accent. A tessera
is a mosaic piece, and the accent tile is the one that was found. The same SVG
serves the 24px rail mark and the 28px first-run mark; there is no wordmark
anywhere in the window.

**The app icon** is the same mark on the instrument's plate: a graphite
rounded square (the dark `surface`, 21% corner) carrying three `#ECF0F5`
tiles and one `#2F5FE3` tile, with a faint (8%) white hairline to hold the
plate edge on a dark taskbar. The mark fills 66% of the plate, keeping the
in-window mark's gap-to-tile proportion. `icons/icon.svg` is the source;
`32x32.png`, `128x128.png` and `icon.ico` (16–256px) are generated from the
same geometry. Changing any icon file does not rerun the Windows resource
build — touch `tauri.conf.json` before rebuilding or the exe keeps the old
icon.

## Interaction rules (launcher-class)

1. **Keyboard actions never animate.** `Ctrl+K`, `Esc`, `Ctrl+1/2/3`,
   arrow-key selection — all instant, zero transition. Result rows and tiles
   have no transition at all: selection is a repaint at keystroke speed.
2. **Hover and selection change color only.** No movement, no scale on result
   rows. Selection is a background wash (`accent-soft`) plus an
   `accent-strong` name; an image tile additionally gets a 2px accent ring
   drawn *outside* the picture with a 2px gap, so the image is never tinted or
   trimmed to show state.
3. **Pressable things answer the press.** Small buttons get `scale(0.97)` and
   tiles `scale(0.985)` on `:active` at 120ms ease-out. Result rows are
   excluded: a full-width row dipping under the pointer reads as the list
   jumping.
4. **Motion budget under 300 ms, usually under 200.** Only the toast, a
   thumbnail resolving, and hover color on chrome animate. There is no
   page-load choreography and no view transition.
5. **Ease-out, never ease-in.** `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`
   for the press and the toast; `ease` for color changes.
6. **Only transform, opacity and color transition.** Never width, height or
   `all`.
7. **`prefers-reduced-motion` collapses movement.** Color changes stay;
   transforms, thumbnail fades and the toast slide are gated on
   `prefers-reduced-motion: no-preference`.
8. **Hovers are gated behind `@media (hover: hover) and (pointer: fine)`** so
   touch devices don't get sticky hover states.

## Accessibility rules

- **The list is one tab stop.** `#results` is a labelled listbox; the selected
  row carries `aria-selected` and the roving `tabindex="0"`, every other row
  is `-1`. The input points at the selection with `aria-activedescendant`,
  so arrow keys from the field move the selection without moving focus.
- **Pictures are decoration, names are the answer.** A thumbnail tile is
  `aria-hidden` and its `<img>` has `alt=""`: the row's name, folder, badges
  and snippet already say what the file is and why it matched. A picture
  never becomes the only label for a result.
- **Focus is always visible and never trapped.** One `:focus-visible` style
  (2px accent, drawn inside a result row so the scrolling list cannot clip
  it). The search bar draws its own accent ring on `:focus-within` and the
  input drops its duplicate outline. The preview is a non-modal complementary
  pane; `Esc` closes it, and no overlay ever swallows Tab.
- **State is never colour-only.** Selection adds a background wash and an
  accent file name; image selection adds a ring; failure states say so in
  words; the watcher pill is labelled "watching"/"paused". The rail's engine
  dot keeps its state in a tooltip and an `sr-only` live label, and the
  icon-only rail buttons carry `aria-label`, `aria-keyshortcuts` and `title`
  so a sighted mouse user still gets the name.
- **Status messages keep their shape.** A note is a full bordered message with
  a status dot, never a coloured side stripe.
- **Live regions are quiet.** Search meta, notes and the watcher log only
  rewrite when their text actually changed, so a polling pane does not
  re-announce unchanged content every 1.8 s.

## Component standards

- **Search bar**: 48px field, 12px radius, `surface` on `canvas`, 1px
  `line-strong` border; focus turns the border accent and adds a 3px accent
  wash ring. The magnifier turns accent while focused. The latency readout is
  a quiet mono line under the field — total wall time first, engine time
  called out separately — outside the input so query and numbers never
  compete for space. The search column stops at 1040px: past that the extra
  width becomes gutter rather than stretch, because a launcher bar and a
  snippet line both get worse when they run the width of a monitor.
- **Primary actions**: the label takes the canvas colour rather than white, so
  it stays dark on the lighter dark-mode accent and legible in both themes.
- **Filters**: one segmented control on a `surface-2` track with a hairline
  border. The pressed segment is the raised white one with a 1px line and a
  1px shadow; its count is the only accent. A zero count disables its segment
  rather than hiding it.
- **Image results**: a gallery grid (`auto-fill`, 168px minimum, 4:3 frames).
  The picture uses `object-fit: contain` so the part that matched is never
  cropped; the frame sits on `inset` with a hairline. The grid keeps a 6px
  inline gutter so the selection ring drawn outside the first and last
  columns is not clipped by the scroller. The strongest image carries a small
  surface "Best match" chip at 8px, while the selection ring stays on the
  selected tile, so "best" and "current" never mean the same thing. Tiles are
  lazy, via `IntersectionObserver`; the image glyph is the skeleton and the
  fallback.
- **Document results**: one dense row per file — 40px lead tile with the kind
  glyph → name → folder (mono) → arm/page chips → two-line snippet with
  accent match runs. The lead tile is a `surface-2` square, deliberately the
  same size and radius as an image frame's lead so mixed answers keep one
  alignment. Arm badges say *why* a file surfaced; that is the product's
  honesty promise, so they are never decorative.
- **Preview pane**: the file at left, its evidence at right. Image previews
  put the cached 320px thumbnail up first, then swap in the full file; they
  sit on the inset surface. OCR/page text highlights matches. Meta is mono.
  The pane is not modal — no focus trap — and `Esc` or the close button
  returns to the list.
- **Home state**: what the index actually holds, not sample content. A
  "Latest files" head with mono counts (files · folders), then up to eight
  real rows ordered by the file's own mtime — kind glyph or thumbnail lead,
  name, folder (mono), relative time. A row opens the same preview as a
  search hit; `ArrowDown` from the empty box walks the list and `Esc`
  returns to it. An empty index says so in words. The panel belongs to an
  empty query and leaves the moment one exists, including while a cold
  search is still loading the models.
- **Rail**: 64px, icon-only, 40px targets. The 24px mark at the head, three
  view icons, the engine dot at the foot. No label text: the window spends
  its width on content, and the names live in `aria-label` and `title`.
- **Empty states**: every empty state states a cause ("no matches", "no
  folders yet", "models need download") and, where one exists, the next
  action. An empty state that only says "empty" is a bug.
- **Activity/Settings**: cards on surface with 12px radius and hairline
  borders; section titles are 13px sans, not tracked uppercase. Mono for every
  path and number; status pills for watcher state.

## The dev mock

`dev-mock.js` provides `window.__TAURI__` **only when the real Tauri API is
absent** (a plain browser opening `index.html`). Under the packaged app it is
inert: the first line checks for the real API and does nothing. It exists so
the window's visual and interaction states can be reviewed without building
the Rust shell — the QA rounds correctly kept flagging "the UI is read, not
run" — and it mirrors the real payload shapes in `engine.rs`/`main.rs`, not
invented ones.
