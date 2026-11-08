# Tessera window — design direction

The window is an **instrument that returns proof**. It does not hand back a
grid of thumbnails and ask the user to guess why each one is there. Every
result carries its evidence on the page: the picture is shown where it was
found, the line is shown beside it, the file is named under it, and the reason
is stated in the same breath. The wall is the answer.

That is the line between Tessera and a gallery. A gallery shows pictures. A
proof sheet shows pictures **and the reason they are on the sheet**. The
website's `DESIGN.md` owns the site; this file owns the window.

## The archetype

Tessera is a **lens over a library**, not a feed of recent activity. The home
state shows what the index holds: pictures as a justified wall at their own
aspect, documents as quiet rows. A search narrows that same surface and adds
the evidence that placed each result.

The tool is summoned, used dozens of times a day, and dismissed. Speed is the
aesthetic; anything that adds a beat between keystroke and answer is a defect.

## Ink and one amber

The window spends exactly two signals. **Ink** is the instrument: text, chrome,
the active lens, the selection ring. **Amber** is the find: OCR boxes, the
matched words, the proof square on every reason, the one lit tile in the mark.
There is no brand blue anywhere in the window. A coloured pixel that is not
amber is either content or a status colour.

| Token | Light | Dark | Role |
|---|---|---|---|
| canvas | `#F3F4F0` | `#0F1112` | window background |
| surface | `#FFFFFF` | `#171A1C` | settings boxes, preview sheet, buttons |
| surface-2 | `#E8EAE3` | `#22262A` | lens track, quiet fills |
| inset | `#EDEFE9` | `#131617` | picture backdrop, tile skeleton |
| raised | `#FFFFFF` | `#2B2F33` | pressed settings segment |
| header | `rgb(243 244 240 / 88%)` | `rgb(15 17 18 / 88%)` | sticky header wash under blur |
| hover | `rgb(0 0 0 / 5%)` | `rgb(255 255 255 / 7%)` | quiet hover fill |
| line | `#DCDFD6` | `#2B3034` | 1px hairlines |
| line-strong | `#C3C7BD` | `#42484D` | control borders, hover hairline |
| ink | `#16181B` | `#F2F3EE` | headings, names, active lens |
| ink-2 | `#3D4146` | `#C2C6C1` | body, OCR lines, snippets |
| muted | `#555A5F` | `#9AA0A4` | meta, folders, page badges |
| faint | `#6A6F74` | `#858B90` | quietest annotations |
| accent | `#16181B` | `#F2F3EE` | focus ring, primary fill |
| accent-strong | `#000000` | `#FFFFFF` | links, selected names |
| accent-text | `#16181B` | `#F2F3EE` | accent text on tinted fills |
| accent-soft | `#E5E7E0` | `#2A2E31` | selected document row |
| on-accent | `#FFFFFF` | `#0F1112` | text on an ink fill |
| match | `#F59E0B` | `#FFB020` | OCR boxes, matched words, proof square |
| match-deep | `#8A4700` | `#FFCB6B` | the reason text that goes with the square |
| match-wash | `rgb(245 158 11 / 16%)` | `rgb(255 176 32 / 16%)` | the mark's fill |
| match-edge | `rgb(0 0 0 / 18%)` | `rgb(0 0 0 / 40%)` | the mark's outer edge |
| ok / warn / bad | `#1E6B3A` / `#7A4A00` / `#B3261E` | `#3DDC84` / `#FFCB6B` / `#FF6B60` | status, both themes |

**Type.** One sans family: the system stack (`-apple-system`, `SF Pro Text`,
`Segoe UI Variable Text`, `Segoe UI`, `Roboto`). Headings are the same face at
weight 600 with −0.01em tracking. Base is 13px/1.5, meta 12px, the query 24px
with −0.015em tracking.

**Mono is the annotation voice.** Counts, paths, page badges, reasons, preview
metadata and keyboard hints are set in `--mono` (Cascadia Code, Cascadia Mono,
Consolas). A mono line is a note written on the specimen, not a second
typeface: it is always 11px and never carries prose. The `mark` tag keeps the
sans face because it is part of the text.

**Shape.** 6px for picture frames, 8px for buttons, 10px for document rows and
the toast, 12px for settings boxes and the preview sheet's left corners, full
pill for the lens track. Nothing above 12px except the pill.

**Elevation.** The window is flat. A 1px hairline holds pictures and controls;
the preview sheet and the toast are the only floating surfaces
(`--shadow-sheet`, `--shadow-float`). There are no drop-shadowed cards.

## The mark

Four rounded tiles in a 2×2 mosaic. Three are ink; **one is amber, and its
position is the active lens**:

| Lens | Lit quadrant |
|---|---|
| all | bottom right |
| pictures | top left |
| words | top right |
| documents | bottom left |

A tessera is a mosaic piece; the lit piece is the question in force. The same
SVG serves the 20px header mark and the 40px first-run mark. There is no
wordmark anywhere in the window.

**The app icon** is the same mark on the instrument's plate: a `#16181B`
rounded square (21% corner) carrying three `#F2F3EE` tiles and one `#FFB020`
tile, with a faint (8%) white hairline to hold the plate edge on a dark
taskbar. `icons/icon.svg` is the source; the PNG and ICO files are generated
from the same geometry. Changing an icon file does not rerun the Windows
resource build — touch `tauri.conf.json` before rebuilding.

## The four lenses

One pill track, four words: **all**, **pictures**, **words**, **documents**.
The order is the library's own taxonomy, not a menu.

| Lens | Asks | Arms | Result material |
|---|---|---|---|
| all | everything, with a reason on every result | keyword, image, meaning | picture wall and document rows |
| pictures | pictures that look like the description | image vectors only | picture wall |
| words | text visible inside pictures | OCR text and meaning | evidence cards |
| documents | PDF, Word and text files | full text and meaning | document rows |

The samurai test is the contract: a samurai photograph answers *pictures*, a
screenshot containing the word answers *words*, a PDF about samurai answers
*documents*. `all` shows all three and says why each one is there. A lens is a
scope, never a filter applied after the fact.

The three result materials exist so the user never has to ask which question
was answered:

1. **The picture wall** is for likeness. Each picture keeps its true aspect and
   carries a permanent plate naming it.
2. **The evidence card** is for text inside a picture. It crops the picture
   around the matched line, overlays the amber box, and prints the OCR line
   directly under it.
3. **The document row** is for files whose own pages hold the words. It shows
   the page badge and the matched snippet.

## Interaction rules

1. **Keyboard actions never animate.** `Ctrl+K`, `Esc`, `Ctrl+1..4`, arrow-key
   selection — all instant. Tiles and cards repaint at keystroke speed.
2. **Hover changes one hairline, never content.** The plate is always visible;
   hover only darkens the frame outline. Nothing is revealed on hover that a
   keyboard user cannot see.
3. **Selection is an ink ring in the gap.** A selected tile or card draws a 2px
   ink ring outside a 2px canvas gap, so the picture is never tinted, scaled or
   trimmed. Document rows use the `accent-soft` wash and an ink name.
4. **Pressable chrome answers the press.** Buttons get `scale(0.98)`; the
   preview sheet slides 12px at 180ms. Tiles, cards and rows are excluded.
5. **Motion budget under 300 ms.** Only the toast, the preview sheet, the press
   scale, the thumbnail develop and colour changes animate. No page-load
   choreography.
6. **Ease-out, never ease-in.** `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`
   for the sheet, press, toast and develop; `ease` for colour.
7. **Only transform, opacity, filter and colour transition.** `filter` is used
   only for the develop-in. Never width, height or `all`.
8. **`prefers-reduced-motion` removes movement and blur.** The sheet appears in
   place, the toast does not slide, and a picture appears fully developed.
9. **Hovers are gated behind `@media (hover: hover) and (pointer: fine)`.**
10. **The frame develops, it does not blink.** A thumbnail starts at 30%
    opacity behind an 8px blur and resolves over 200ms.

## Result anatomy

**Picture tile.** The frame sits in normal flow at the justified height; the
plate adds 38px beneath it. The plate has two lines: the file name (and a mono
page badge when there is one), then the folder and the reason. The reason is
prefixed by a 5px amber square and set in mono; on the home state it is absent
and the line carries the folder alone. The full path is the option's native
title. A failed decode leaves the glyph and "no preview" inside the frame; the
plate still names the file.

**Evidence card.** A 3:2 frame crops the source around the first row mark. The
crop keeps the card's aspect, clamps inside the picture, and never shows more
than a 0.3-wide slice, so the 480px thumbnail is not stretched past legibility.
The amber marks ride on the same box as the full picture and are clipped by the
frame, which keeps their fractions exact. Under the frame: name and page,
folder and reason, then up to two lines of the OCR text with the matched words
in amber. With no usable mark the card shows the whole picture and still prints
the line; the text is the answer even when the geometry is missing.

**Document row.** 56px minimum, a 20px kind glyph, name and a mono page chip,
a two-line snippet with amber match runs, and a footer of folder, reason and
(for the home list) relative time. The reason carries the same amber square.

**Preview sheet.** A non-modal right sheet, `min(600px, 52vw)`, full height,
rounded left corners. The head names the file and its mono path; the body shows
the full picture with the amber marks, then a mono `matched line` label and the
OCR lines, or the page text for a document. Matched lines are set in ink. The
foot carries kind, page and pixel size in mono, and "show in folder". The first
OCR line is dropped when it only repeats the file name.

## Accessibility rules

- **The surface is one listbox.** `#library-results` is a labelled listbox;
  every tile, card and row is `role="option"` with one roving `tabindex="0"`.
  The input points at the selection with `aria-activedescendant`, so arrows
  from the field move selection without moving focus.
- **Pictures are decoration; text is the answer.** Every `<img>` has `alt=""`
  and every mark layer is `aria-hidden`. The plate, the OCR line and the
  reason are real text inside the option. A picture never becomes the only
  label for a file.
- **Marks are not the only evidence.** An amber box is repeated in words by
  the reason, the OCR snippet and the preview label. Colour never carries
  meaning alone.
- **Plates are permanent.** Name, folder and reason do not wait for hover or
  focus, so keyboard and low-vision users get the same information as a
  pointer user. Hover adds only a hairline.
- **Focus is always visible and never trapped.** One `:focus-visible` style
  (2px ink) on lenses, tiles, cards, rows, settings controls and the sheet. The
  preview is a non-modal complementary pane; `Esc` closes it and no overlay
  ever swallows Tab.
- **Selection is announced.** `aria-selected` and `aria-activedescendant` track
  the same index in every lens, including the mixed `all` lens.
- **Text meets WCAG AA in both themes.** `ink` and `ink-2` clear AAA on canvas;
  `muted` and `faint` clear AA. On tinted fills the text swaps to `muted` or
  `ink-2`, never `faint`. `match` is a fill, never body text; `match-deep`
  carries the reason.
- **State is never colour-only.** A failed frame says "no preview"; the miss
  state names the cause and offers the other lenses; selection changes both a
  ring and a name colour.
- **Live regions are quiet.** Counts and notes only rewrite when their text
  changed. Settings polls, and its lists rebuild only when their data changes.
- **High contrast is honoured.** `prefers-contrast: more` darkens lines, muted
  and faint tokens in each theme.

## Layout

The header spans the window: lens row on top, one large field below, quiet
notes at the foot of the header. The wall is the width of the window, with 20px
side padding; at 720px and below that becomes 12px.

Picture rows are justified at a 160px target height and 6px horizontal gap; a
full row stretches to the content width, the last row stays left aligned. The
plate adds 38px under each picture, and the wall leaves 12px between rows.

The evidence grid is `repeat(auto-fill, minmax(280px, 1fr))` with a 24px row
gap and an 18px column gap, so a 1000px column holds three cards.

The preview is a fixed right sheet that never pushes or reflows the wall; below
900px it takes `min(560px, 100vw - 32px)`. Settings is one centred page,
`--page: 640px`, with a 24px gutter. The document list and the miss state share
the same 20px wall gutter.

## Component standards

- **Header**: a translucent wash (`--header`) with an 18px blur and saturation,
  fading into the wall through an 18px gradient instead of a border. The mark,
  the lens track and the count sit on the first row; the settings key is at the
  right. The field is 40px high with a 16px glyph, no box of its own: the
  window's top edge is the field's edge. The meta count is mono.
- **Lenses**: a `surface-2` pill track, 28px high, 2px padding. The pressed
  segment is a solid ink pill with canvas-coloured text; other segments are
  ink-2. Counts are not repeated in the track; the meta line carries them.
- **Wall**: `display: flex` rows, 6px gap, tiles shrink-wrapped. A tile is a
  button with a frame in flow and a plate below. The frame has a 6% black
  hairline, the picture develops over 200ms, and the fallback is the kind glyph
  plus "no preview". The plate is 38px, two lines, never hidden.
- **Evidence**: a grid of cards. Each has a 3:2 frame with a clipped full-size
  picture positioned by the crop geometry, the amber mark layer, and a plate of
  name, meta and OCR text. Selection and focus ring the frame, not the text.
- **Document rows**: 56px minimum, 12px gap, a 20px kind glyph, name and a mono
  page chip, a two-line snippet with amber match runs, folder and reason, and
  the modified time at the right for the home list. Selection is an
  accent-soft wash and an ink name; focus draws inside the row so the scroller
  cannot clip it.
- **Preview sheet**: name and mono path at the head, the file body, and a mono
  foot with kind, page and pixel size plus "show in folder". A picture is shown
  at its own size, capped by the sheet width and `100vh - 330px`; its marks sit
  in a shrink-wrapped box so the fractions line up with the image. Below the
  picture, the `matched line` label and the OCR lines highlight the query
  terms; matched lines are ink.
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

## Do not

- Do not add a third accent colour, a gradient text fill, or a blue focus ring.
- Do not hide the plate, the reason or the page badge on hover.
- Do not put a reason behind a tooltip; a tooltip is not proof.
- Do not animate the response to a keystroke.
- Do not crop a picture without a mark and claim the crop is evidence.
- Do not show a card, row or tile with no file name in text.

## The dev mock

`dev-mock.js` provides `window.__TAURI__` **only when the real Tauri API is
absent** (a plain browser opening `index.html`). Under the packaged app it is
inert: the first line checks for the real API and does nothing. It exists so
the window's visual and interaction states can be reviewed without building
the Rust shell, and it mirrors the real payload shapes in `engine.rs` and
`main.rs`, not invented ones. Its fixtures are the only invented content in
the app.
