# Tessera window — design direction

The window is a **tool**, not a page. It deliberately parts from the website's
warm editorial palette: the site is the publication, the app is the instrument.
The app is Apple-neutral — system greys, true surfaces, one blue accent — so
the content (screenshots, documents, names) carries every bit of colour. The
website's `DESIGN.md` owns the site's own palette; this file owns the window.

## The archetype

Tessera's window is a **launcher-class surface** (Spotlight, Raycast): it is
summoned, used dozens of times a day, and dismissed. That decides almost every
rule below. Speed is the aesthetic; anything that adds a beat between
keystroke and answer is a defect, however pretty.

## Tokens

| Token | Light | Dark | Role |
|---|---|---|---|
| canvas | `#F5F5F7` | `#141416` | window background |
| surface | `#FFFFFF` | `#1D1D1F` | search bar, result rows, preview, home panel |
| surface-2 | `#EDEDF0` | `#2A2A2D` | lead tiles, filter track, hover fills |
| inset | `#F2F2F4` | `#18181A` | image backdrop, log well |
| raised | `#FFFFFF` | `#47474A` | pressed filter segment |
| hover | `rgb(0 0 0 / 4%)` | `rgb(255 255 255 / 6%)` | quiet row hover |
| line | `#E3E3E8` | `#2E2E31` | 1px hairlines |
| line-strong | `#C9C9CF` | `#424245` | input borders |
| ink | `#1D1D1F` | `#F5F5F7` | headings, result names (~15:1) |
| ink-2 | `#424245` | `#C7C7CC` | body, snippets (~10:1) |
| muted | `#545458` | `#A1A1A6` | meta, paths, hints (6.9:1 light) |
| faint | `#6E6E73` | `#8E8E93` | quietest annotations (4.7:1 light) |
| accent | `#0071E3` | `#0A84FF` | selection, matches, focus (4.7:1 light) |
| accent-strong | `#0059B3` | `#4DA3FF` | selected names, hover (7.6:1 light) |
| accent-text | `#0059B3` | `#9CC7FF` | accent text on tinted fills |
| accent-soft | `#E3EFFD` | `rgb(10 132 255 / 16%)` | selected row wash |
| on-accent | `#FFFFFF` | `#0B1420` | text on an accent or bad fill |
| ok / warn / bad | `#1B7A3D` / `#8A5A00` / `#C7251C` | `#30D158` / `#FFD60A` / `#FF5F56` | status, both themes |

The accent is a signal, not decoration. It appears only on: the selected row
or tile, matched text, the pressed filter count, focus rings, the live state.
Anything else is ink, line or surface.

**Type.** One family: the system sans stack (`-apple-system`, `SF Pro Text`,
`Segoe UI Variable Text`, `Segoe UI`, `Roboto`). Headings are the same face at
weight 600 with −0.01em tracking; there is no serif in the app. Base is
13px/1.5; the query is 15px. **Numbers and paths are sans too**, with
`font-variant-numeric: tabular-nums` wherever values update — times, counts,
latency and sizes all read as interface, not as code. Mono (Cascadia) is
reserved for the watcher log and inline `code`, the two places where character
alignment is the point.

**Shape.** 14px for the search bar and result rows, 10px for lead tiles and
image frames, 8px for buttons and segmented controls, 6px for chips, full pill
only for status and the filter track. Nothing above 16px.

**Elevation.** One hairline and one soft shadow, never stacked heavy pairs:
`--shadow-sm` for the pressed segment and floating chips, `--shadow-float` for
the toast. Surfaces are otherwise flat; depth comes from the canvas/surface
contrast.

## The mark

Four rounded tiles in a 2×2 mosaic; three are ink and one is accent. A tessera
is a mosaic piece, and the accent tile is the one that was found. The same SVG
serves the 24px rail mark and the 28px first-run mark; there is no wordmark
anywhere in the window.

**The app icon** is the same mark on the instrument's plate: a graphite
rounded square (`#1D1D1F`, 21% corner) carrying three `#F5F5F7` tiles and one
`#0071E3` tile, with a faint (8%) white hairline to hold the plate edge on a
dark taskbar. The mark fills 66% of the plate, keeping the in-window mark's
gap-to-tile proportion. `icons/icon.svg` is the source; `32x32.png`,
`128x128.png`, `256x256.png` and `icon.ico` (16–256px) are generated from the
same geometry. Changing any icon file does not rerun the Windows resource
build — touch `tauri.conf.json` before rebuilding or the exe keeps the old
icon.

## Interaction rules (launcher-class)

1. **Keyboard actions never animate.** `Ctrl+K`, `Esc`, `Ctrl+1/2/3`,
   arrow-key selection — all instant, zero transition. Result rows and tiles
   have no transition at all: selection is a repaint at keystroke speed.
2. **Hover and selection change colour only.** No movement, no scale on result
   rows. Selection is a background wash (`accent-soft`) plus an
   `accent-strong` name; an image tile additionally gets a 2px accent ring
   drawn *outside* the picture with a 2px gap, so the image is never tinted or
   trimmed to show state.
3. **Pressable things answer the press.** Small buttons get `scale(0.98)` and
   tiles `scale(0.985)` on `:active` at 120ms ease-out. Result rows, home rows
   and tiles are excluded: a full-width row dipping under the pointer reads as
   the list jumping.
4. **Motion budget under 300 ms, usually under 200.** Only the toast and
   hover colour on chrome animate. There is no page-load choreography and no
   view transition.
5. **Ease-out, never ease-in.** `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`
   for the press and the toast; `ease` for colour changes.
6. **Only transform, opacity and colour transition.** Never width, height or
   `all`.
7. **`prefers-reduced-motion` collapses movement.** Colour changes stay;
   press scales and the toast slide are gated on
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
- **Text meets WCAG AA in both themes.** Every muted/faint token clears 4.5:1
  on canvas and surface; on tinted fills (surface-2, raised, accent-soft) the
  text swaps to `muted` or `accent-text`, never `faint`. Accent blue is used
  for text only at body weight on white (4.7:1) or above.
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
- **High contrast is honoured.** `prefers-contrast: more` darkens the lines,
  muted and faint tokens in light and dark separately.

## Layout

One centred column, `--column: 720px`; past that the extra width becomes
gutter rather than stretch, because a launcher bar and a snippet line both get
worse when they run the width of a monitor. The preview pane opens as a third
column on wide windows and as a full overlay (right of the rail) below 1040px,
so the result list never compresses into a sliver. The rail is fixed at 64px.

## Component standards

- **Search bar**: 52px field, 14px radius, `surface` on `canvas`, 1px
  `line-strong` border; focus turns the border accent and adds a 3px accent
  wash ring. The magnifier turns accent while focused. Under the field, one
  status row: meta (count and latency, tabular sans, total wall time first,
  engine time called out separately) on the left, the filter control on the
  right; the row wraps instead of squeezing at narrow widths.
- **Primary actions**: the label takes `on-accent` (canvas-dark in the dark
  theme) so it stays legible on the lighter dark-mode accent.
- **Filters**: one segmented control on a `surface-2` track, full pill, 1px
  line. The pressed segment is `raised` with a 1px line and `--shadow-sm`; its
  count is accent-text. A zero count disables its segment rather than hiding
  it.
- **Image results**: a gallery grid (`auto-fill`, 164px minimum, 4:3 frames,
  14px row gap). The picture uses `object-fit: contain` so the part that
  matched is never cropped; the frame sits on `inset` with a hairline. The
  grid keeps a 6px inline gutter so the selection ring drawn outside the first
  and last columns is not clipped by the scroller. The strongest image carries
  a small surface "Best match" chip, while the selection ring stays on the
  selected tile, so "best" and "current" never mean the same thing. Tiles are
  lazy, via `IntersectionObserver`; the image glyph is the skeleton and the
  fallback.
- **Document results**: one dense row per file — 44px lead tile with the kind
  glyph → name → folder → arm/page chips → two-line snippet with accent match
  runs. Rows are borderless, 10px radius, separated by 8px padding, and only
  take a surface wash when selected. The lead tile is a `surface-2` square,
  deliberately the same size and radius as an image frame's lead so mixed
  answers keep one alignment. Arm badges say *why* a file surfaced; that is
  the product's honesty promise, so they are never decorative.
- **Preview pane**: the file at left, its evidence at right. Image previews
  put the cached 320px thumbnail up first, then swap in the full file; they
  sit on the inset surface. OCR/page text highlights matches. The pane is not
  modal — no focus trap — and `Esc` or the close button returns to the list.
- **Home state**: what the index actually holds, not sample content. A
  "Latest files" head with quiet sans counts (files · folders), then up to
  eight real rows ordered by the file's own mtime — kind glyph or thumbnail
  lead, name, folder, relative time. A row opens the same preview as a search
  hit; `ArrowDown` from the empty box walks the list and `Esc` returns to it.
  An empty index says so in words. The panel belongs to an empty query and
  leaves the moment one exists, including while a cold search is still
  loading the models.
- **Rail**: 64px, icon-only, 40px targets. The 24px mark at the head, three
  view icons, the engine dot at the foot. No label text: the window spends its
  width on content, and the names live in `aria-label` and `title`.
- **Empty states**: every empty state states a cause ("no matches", "no
  folders yet", "models need download") and, where one exists, the next
  action. An empty state that only says "empty" is a bug.
- **Activity/Settings**: grouped macOS-Settings layout — a quiet 12px group
  label above a hairline-separated group box on `surface` with a 12px radius.
  Rows put the label and a one-line hint at left, the value right aligned;
  stacked rows get a hairline rule between them. Sans for every path and
  number; status pills for watcher state; the log is the one mono well.
- **Toast**: a bottom-centre surface card, hairline, 10px radius,
  `--shadow-float` and a 20px backdrop blur; errors swap to the `bad` fill
  with `on-accent` text. It slides 6px and fades over 180ms, gated on reduced
  motion.

## The dev mock

`dev-mock.js` provides `window.__TAURI__` **only when the real Tauri API is
absent** (a plain browser opening `index.html`). Under the packaged app it is
inert: the first line checks for the real API and does nothing. It exists so
the window's visual and interaction states can be reviewed without building
the Rust shell — the QA rounds correctly kept flagging "the UI is read, not
run" — and it mirrors the real payload shapes in `engine.rs`/`main.rs`, not
invented ones.
