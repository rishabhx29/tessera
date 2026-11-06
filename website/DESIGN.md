# Tessera website: design system and editorial direction

## 1. Direction

An editorial, light theme built for trust and reading. The site is mostly paper,
ink and hairlines. Structure comes from rules and spacing, not from a stack of
cards.

- Canvas: warm paper (`#FAF9F5`).
- Ink: near-black (`#181715`) for headings, warm grey for body text.
- Accent: one terracotta (`#C2410C`), used for action and emphasis, never as
  decoration.
- Surfaces: white only where a panel needs to lift, such as the result grid or
  the code block.
- Honesty: no fabricated app screenshots. The result grid is labelled as an
  illustration. The video is a labelled placeholder until real footage exists.

## 2. References studied

| Reference | What was taken |
| --- | --- |
| [gyotaku.app](https://gyotaku.app/) and [xevrion/gyotaku](https://github.com/xevrion/gyotaku) | A demo video in the hero, quiet product framing, and an open-source proof story instead of marketing claims. |
| Daylight Computer (Awwwards SOTD) | Warm paper canvas and charcoal ink. |
| Stripe Press | Serif display type with plain technical copy. |
| Linear | Hairline structure, keyboard badges, compact status pills. |
| Raycast | The search result pattern: one box, mixed result types, visible match reasons. |
| Shift 5 (Non-Linear Studio, Awwwards SOTD) | Proof blocks with units, sample sizes and caveats. |
| Reflect | Calm local-first privacy language. |
| Resend | A developer build block that does not pretend to be a product download. |

Awwwards tools were used for reference: `search_sites`, `analyze_page_structure`,
`capture_live_site`, `capture_component`, and `search_motion`. Their output
informed pacing and hierarchy, not any copied layout.

## 3. Palette

All text colors were checked against the canvas with WCAG contrast in mind.
Primary and secondary text exceed AAA. Muted and faint text are used only for
metadata at 11 to 13 px and should not carry essential copy alone.

| Token | Hex | Role |
| --- | --- | --- |
| `canvas` | `#FAF9F5` | Page background |
| `surface` | `#FFFFFF` | Lifted panel |
| `subtle` | `#F3F1EB` | Quiet fill, badge background |
| `inset` | `#EBE8E0` | Bar tracks, video frame background |
| `line` | `#E5E2D9` | Hairline rules |
| `line-strong` | `#D1CDC0` | Stronger border, control edge |
| `ink.primary` | `#181715` | Headings and essential text (15.4:1) |
| `ink.secondary` | `#45423D` | Body copy (8.8:1) |
| `ink.muted` | `#59554E` | Captions and metadata (6.6:1) |
| `ink.faint` | `#6B665E` | Small labels (5.0:1) |
| `accent` | `#C2410C` | Primary action and emphasis (5.1:1) |
| `accent.hover` | `#9A3412` | Primary action hover (7.3:1) |
| `accent.subtle` | `#FFF7ED` | Badge and notice fill |

## 4. Typography

System stacks only. No font requests, no loading shift.

- Serif: `Charter, "Bitstream Charter", "Sitka Text", Cambria, Georgia, serif`
- Sans: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`
- Mono: `ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, monospace`

| Level | Size | Use |
| --- | --- | --- |
| Hero | `clamp(2.5rem, 1.4rem + 3.9vw, 4.35rem)` | One per page |
| Section heading | `clamp(1.85rem, 1.15rem + 2.15vw, 3rem)` | `SectionHeading` |
| Feature heading | 21 to 24 px serif | Privacy pillars, stage headings |
| Body | 14 to 17 px sans | Explanations |
| Metadata | 10 to 13 px mono or sans | Units, labels, captions |

Headings use `text-wrap: balance`; paragraphs use `text-wrap: pretty`.

## 5. Layout and spacing

- One content column: `max-w-6xl` (1152 px) with visible `border-x` hairlines.
- Every section closes with a full-width `border-b`.
- Section padding: `py-16` mobile, `py-24` desktop.
- Grids use rules (`border-t`, `border-b`, `divide-y`) instead of card gaps.
  The exceptions are the hero illustration, the video frame, and the code
  block, which are panels with a real edge.
- Mobile breakpoint is checked at 390 px. The page must not scroll sideways.
- No card shadow is used for hierarchy. `shadow-elevated` belongs only to the
  hero illustration.

## 6. Motion

- One easing token: `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`.
- Interface transitions: 140 to 160 ms. Nothing decorative runs longer.
- Press feedback: 0.98 scale on buttons, no movement on focus.
- `swap-in` (160 ms opacity and 3 px rise) is used only when illustration
  results change.
- Hover styles are gated behind `@media (hover: hover) and (pointer: fine)` by a
  Tailwind variant plugin, so a touch device cannot leave a stuck hover state.
- Under `prefers-reduced-motion: reduce`, scroll behavior becomes instant and
  animation and transition durations collapse. Colour feedback remains.
- No autoplay video. The demo is click to play, with controls and captions.

## 7. Video and proof

The demo section is built to receive a real recording without a redesign:

- Video: `public/media/demo.mp4`, H.264 MP4, 1280x720 or 1920x1080.
- Poster: `public/media/demo-poster.png`.
- Captions: `public/media/demo.en.vtt`, WebVTT, on by default.
- The player is `<video controls playsInline preload="metadata">` with a poster,
  width and height, and a captions track.
- `src/demo.ts` holds the paths and `DEMO_IS_PLACEHOLDER`. While the flag is
  true, the player carries a placeholder badge and the caption says there is no
  app footage. Set it to false only after real footage and a real poster are in
  place.
- The current slate is a generated title card. It is not app footage and must
  never be described as such.
- Full drop-in instructions, including the ffmpeg poster command, are in
  [`README.md`](./README.md).

Measured numbers are presented with their context:

- 5,000 files: 65 ms p50, 80 ms p95, target met.
- 50,000 files: 457 ms p50, 523 ms p95, target missed. Shown with a miss badge
  and a dark bar, never as a win.
- OCR: 34.4 ms p50, labelled as a single sample on a different machine.
- Description search: 0.7015 R@1 at 300 photos and 0.3552 at 5,000, with the
  caption-library caveat.
- Index size: 22.7 MB per 1,000 at 5,000 files and 13.7 MB at 50,000.

## 8. Components

1. **`Navbar`**: hairline header, section links, release status pill, GitHub
   link, Releases action, accessible mobile menu with Escape to close.
2. **`Hero`**: status line, one headline, plain sub-line, two actions, honest
   release note, and a result-grid illustration marked "Illustration, not a
   screenshot".
3. **`ProblemSection`**: three named failure modes, each paired with what
   Tessera does, then a comparison table that works on mobile.
4. **`DemoSection`**: video player, placeholder badge, honest caption,
   recording beats, and an interactive illustration with two scenarios and
   format filters.
5. **`HowItWorks`**: five accessible tabs (Watch, Extract, Embed, Fuse, Answer)
   with arrow, Home and End key support, plus a spec panel per stage.
6. **`ProofSection`**: the 65 ms result, a shared-scale bar chart with a 100 ms
   target line, five stat rows including the 50,000-file miss, and a caveats
   panel.
7. **`PrivacySection`**: four pillars in a 2x2 hairline grid, and a clearly
   labelled "Planned, not built" MCP note.
8. **`DownloadSection`**: "Not published yet" status, Follow releases action,
   installer and model facts, per-platform status, and a build block with an
   `aria-live` copy confirmation.
9. **`FAQSection`**: eight accessible disclosures, with the download question
   open first.
10. **`Footer`**: short description, documentation and project links, and the
    privacy line.

## 9. Rules

| Do | Don't |
| --- | --- |
| Use paper, ink, hairlines and one terracotta accent. | Use neon gradients, glassmorphism or a dark mode. |
| Label illustrations and placeholders as such. | Present a mockup or slate as app footage. |
| Quote `docs/performance.md` with machine, sample and caveat. | Round up, project, or turn a target into a result. |
| Say the first release is being prepared. | Claim a shipped v0.1 or a downloadable build. |
| Keep controls at least 44 px high on primary actions. | Hide focus rings or animate keyboard actions. |
| Keep hover behind a hover-capable pointer check. | Let touch taps stick in hover states. |
