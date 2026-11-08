# Tessera window — design direction

The window is a **tool**, not a page. It deliberately parts from the website's
warm editorial palette: the site is the publication, the app is the instrument.
The window is Apple-neutral — system greys, true surfaces, one blue accent — so
the content (screenshots, documents, names) carries every bit of colour. The
website's `DESIGN.md` owns the site's own palette; this file owns the window.

## The archetype

Tessera is a **lens over a library**, not a list of recent activity. The home
state shows what the index actually holds: pictures as a justified wall at
their own aspect, documents as quiet rows. A search narrows that same surface.
The tool is summoned, used dozens of times a day, and dismissed. Speed is the
aesthetic; anything that adds a beat between keystroke and answer is a defect.

## Tokens

| Token | Light | Dark | Role |
|---|---|---|---|
| canvas | `#F5F5F7` | `#141416` | window background |
| surface | `#FFFFFF` | `#1D1D1F` | settings boxes, preview sheet, buttons |
| surface-2 | `#EDEDF0` | `#2A2A2D` | lens track, pressed segments, keys |
| inset | `#F2F2F4` | `#18181A` | picture backdrop, tile skeleton |
| raised | `#FFFFFF` | `#47474A` | pressed lens segment |
| header | `rgb(245 245 247 / 88%)` | `rgb(20 20 22 / 88%)` | sticky header wash under blur |
| hover | `rgb(0 0 0 / 4%)` | `rgb(255 255 255 / 6%)` | quiet hover fill |
| line | `#E3E3E8` | `#2E2E31` | 1px hairlines |
| line-strong | `#C9C9CF` | `#424245` | control borders |
| ink | `#1D1D1F` | `#F5F5F7` | headings, names (~15:1) |
| ink-2 | `#424245` | `#C7C7CC` | body, snippets (~10:1) |
| muted | `#545458` | `#A1A1A6` | meta, paths, hints (6.9:1 light) |
| faint | `#6E6E73` | `#8E8E93` | quietest annotations (4.7:1 light) |
| accent | `#0071E3` | `#0A84FF` | selection, focus (4.7:1 light) |
| accent-strong | `#0059B3` | `#4DA3FF` | selected names, links (7.6:1 light) |
| accent-text | `#0059B3` | `#9CC7FF` | accent text on tinted fills |
| accent-soft | `#E3EFFD` | `rgb(10 132 255 / 16%)` | selected document row |
| on-accent | `#FFFFFF` | `#0B1420` | text on an accent or bad fill |
| match | `#FF9F0A` | `#FFB340` | OCR line marks, snippet underline |
| match-wash | `rgb(255 159 10 / 12%)` | `rgb(255 179 64 / 14%)` | the mark's fill |
| ok / warn / bad | `#1B7A3D` / `#8A5A00` / `#C7251C` | `#30D158` / `#FFD60A` / `#FF5F56` | status, both themes |

The accent is a signal, not decoration. It appears only on the selected tile or
row, focus rings, links and the mark's found tile. Everything else is ink,
line, surface or the amber match colour.

**Type.** One family: the system sans stack (`-apple-system`, `SF Pro Text`,
`Segoe UI Variable Text`, `Segoe UI`, `Roboto`). Headings are the same face at
weight 600 with −0.01em tracking; there is no serif in the app. Base is
13px/1.5, meta 12px, the query 24px with −0.015em tracking. **Numbers and
paths are sans too**, with `font-variant-numeric: tabular-nums` wherever values
update. Mono (Cascadia) is reserved for inline `code`; key hints use the sans
face in a bordered `kbd`.

**Shape.** 6px for picture tiles, 8px for buttons and the picture frame, 10px
for document rows and the toast, 12px for settings boxes and the preview
sheet's left corners, full pill for the lens track. Nothing above 12px except
the pill.

**Elevation.** One hairline and one soft shadow, never stacked heavy pairs:
`--shadow-sm` for the pressed lens, `--shadow-sheet` for the preview,
`--shadow-float` for the toast. Surfaces are otherwise flat; depth comes from
the canvas/surface contrast and the picture wall's own colour.

## The mark

Four rounded tiles in a 2×2 mosaic; three are ink and one is accent. A tessera
is a mosaic piece, and the accent tile is the one that was found. The same SVG
serves the 20px header mark and the 40px first-run mark; there is no wordmark
anywhere in the window.

**The app icon** is the same mark on the instrument's plate: a graphite
rounded square (`#1D1D1F`, 21% corner) carrying three `#F5F5F7` tiles and one
`#0071E3` tile, with a faint (8%) white hairline to hold the plate edge on a
dark taskbar. `icons/icon.svg` is the source; the PNG and ICO files are
generated from the same geometry. Changing an icon file does not rerun the
Windows resource build — touch `tauri.conf.json` before rebuilding.

## The four lenses

One pill track, four words: **all**, **pictures**, **words**, **documents**.
The order is the library's own taxonomy, not a menu.

| Lens | Asks | Arms |
|---|---|---|
| all | everything, with a reason on every result | keyword, image, meaning |
| pictures | pictures that look like the description | image vectors only |
| words | text visible inside pictures | OCR text and meaning |
| documents | PDF, Word and text files | full text and meaning |

The samurai test is the contract: a samurai photograph answers *pictures*, a
screenshot containing the word answers *words*, a PDF about samurai answers
*documents*. `all` shows all three and says why each one is there. A lens is a
scope, never a filter applied after the fact.

## Interaction rules

1. **Keyboard actions never animate.** `Ctrl+K`, `Esc`, `Ctrl+1..4`, arrow-key
   selection — all instant. Tiles and rows repaint at keystroke speed.
2. **Hover and selection change colour only.** No movement, no scale on rows or
   tiles. A selected tile draws a 2px accent ring outside the picture, in the
   canvas gap, so the image is never tinted or trimmed.
3. **Pressable chrome answers the press.** Buttons get `scale(0.98)` and the
   preview sheet slides 12px at 180ms. Rows and tiles are excluded.
4. **Motion budget under 300 ms.** Only the toast, the preview sheet, the press
   scale, thumbnail opacity and hover colour animate. No page-load
   choreography.
5. **Ease-out, never ease-in.** `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`
   for the sheet, press and toast; `ease` for colour.
6. **Only transform, opacity and colour transition.** Never width, height or
   `all`.
7. **`prefers-reduced-motion` collapses movement.** Colour stays; the sheet
   appears in place and the toast does not slide.
8. **Hovers are gated behind `@media (hover: hover) and (pointer: fine)`.**

## Accessibility rules

- **The surface is one listbox.** `#library-results` is a labelled listbox;
  every tile and row is `role="option"` with one roving `tabindex="0"`. The
  input points at the selection with `aria-activedescendant`, so arrows from
  the field move selection without moving focus.
- **Pictures are decoration, names are the answer.** The tile's `<img>` has
  `alt=""` and the mark layer is `aria-hidden`; the tile's name, folder and
  reason are real text in the option. A picture never becomes the only label
  for a file.
- **Marks are not the only evidence.** An amber box on a picture is repeated by
  the OCR line in the preview, which carries the highlighted words. Colour is
  never the only carrier of meaning.
- **Focus is always visible and never trapped.** One `:focus-visible` style
  (2px accent) on lenses, tiles, rows, settings controls and the sheet. The
  preview is a non-modal complementary pane; `Esc` closes it and no overlay
  ever swallows Tab.
- **Captions appear on focus as well as hover.** A tile's name, folder and
  reason are reachable by keyboard, not just by pointer.
- **Text meets WCAG AA in both themes.** Muted and faint tokens clear 4.5:1 on
  canvas and surface; on tinted fills the text swaps to `muted` or
  `accent-text`, never `faint`.
- **State is never colour-only.** Selection adds a wash or ring and an accent
  name; a failed tile says "could not be read" in words; the miss state names
  the cause and offers the other lenses.
- **Live regions are quiet.** Counts and notes only rewrite when their text
  changed. Settings polls, and its lists rebuild only when their data changes.
- **High contrast is honoured.** `prefers-contrast: more` darkens lines, muted
  and faint tokens in each theme.

## Layout

The header spans the window: lens row on top, one large field below, quiet
notes at the foot of the header. The wall is the width of the window, with
20px side padding; at 720px and below that becomes 12px. Rows are justified:
a full row stretches to the content width, the last row keeps the 168px target
height and stays left aligned. The target height is 168px, the gap 6px.

The preview is a fixed right sheet: `min(600px, 52vw)`, full height, rounded
left corners, a hairline and `--shadow-sheet`. It never pushes or reflows the
wall; below 900px it takes `min(560px, 100vw - 32px)`.

Settings is one centred page, `--page: 640px`, with a 24px gutter. The document
list and the miss state share the same 20px wall gutter.

## Component standards

- **Header**: a translucent wash (`--header`) with an 18px blur and saturation,
  fading into the wall through an 18px gradient instead of a border. The mark,
  the lens track and the count sit on the first row; the settings key is at the
  right. The field is 40px high with a 16px glyph, no box of its own: the
  window's top edge is the field's edge.
- **Lenses**: a `surface-2` pill track, 28px high, 2px padding. The pressed
  segment is `raised` with a 1px inset line and `--shadow-sm`; other segments
  are ink-2. Counts are not repeated in the track; the meta line carries them.
- **Wall**: `display: flex` rows, 6px gap, tiles shrink-wrapped. A tile is a
  button with a frame, a skeleton glyph and name, a caption and an optional
  mark layer. The frame has a 6% black hairline. The image fades in over 120ms
  when its thumbnail arrives; a failed decode keeps the glyph and name.
  Captions are a bottom gradient with name, folder and reason, shown on focus
  and on fine-pointer hover.
- **Document rows**: 56px minimum, 12px gap, a 20px kind glyph, name and page,
  a two-line snippet with amber match runs, folder and reason, and the
  modified time at the right for the home list. Selection is an accent-soft
  wash and an accent-strong name; focus draws inside the row so the scroller
  cannot clip it.
- **Preview sheet**: name and path at the head, the file body, and a foot with
  the kind, page and pixel size plus "show in folder". A picture is shown at
  its own size, capped by the sheet width and `100vh - 330px`; its marks sit in
  a shrink-wrapped box so the fractions line up with the image. Below the
  picture, the OCR lines or page text highlight the query terms. The first OCR
  line is dropped when it only repeats the file name (the head already says it).
- **Miss state**: a title, one sentence of cause, the other three lenses as
  quiet pills, and a clear action. An empty state that only says "empty" is a
  bug.
- **First run**: the mark, "add a folder to begin", one sentence about what
  stays on the machine, and one primary action. No sample rows and no invented
  content.
- **Settings**: one lowercase page with seven sections — folders, look,
  reading, models, storage, shortcuts, about. Each section is a quiet label
  over a `surface` box with a 12px radius and hairline-separated rows. Rows put
  the label and a one-line hint at left and the value or action at right;
  stacked rows get a rule. Removing a folder names it and promises the files
  are not touched.
- **Toast**: a bottom-centre surface card, hairline, 10px radius,
  `--shadow-float` and a 20px blur; errors swap to the bad fill with white
  text. It slides 8px and fades over 200ms, gated on reduced motion.

## The dev mock

`dev-mock.js` provides `window.__TAURI__` **only when the real Tauri API is
absent** (a plain browser opening `index.html`). Under the packaged app it is
inert: the first line checks for the real API and does nothing. It exists so
the window's visual and interaction states can be reviewed without building
the Rust shell, and it mirrors the real payload shapes in `engine.rs` and
`main.rs`, not invented ones. Its fixtures are the only invented content in
the app.
