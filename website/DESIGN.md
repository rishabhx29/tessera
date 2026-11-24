# Tessera website: design system and editorial direction

## 1. Direction

Warm editorial. Paper, ink and one terracotta accent. Structure comes from
hairlines, spacing and type, not from a stack of cards.

- Canvas: warm paper, with a slightly deeper paper for quiet bands.
- Display: Instrument Serif at 400, roman and italic. It carries the voice.
- Body: Inter Tight Variable. Mono: JetBrains Mono Variable for numbers,
  labels and command lines.
- Accent: terracotta `#C2410C` for action and emphasis only.
- One dark band: the install section, where the page stops describing and
  starts asking for a command.
- Honesty: the hero story is a coded simulation, labelled as such on the
  stage and in its accessible name; the photos in it are cropped from the real
  app. The demo is a fixed sample and says so. The illustration panels are
  labelled by their content, not passed off as footage.

## 2. References studied

| Reference | What was taken |
| --- | --- |
| [gyotaku.app](https://gyotaku.app/) | The shape of the proof: real app stills, quiet framing, a demo you can try, and measured numbers instead of claims. |
| [xevrion/gyotaku](https://github.com/xevrion/gyotaku) | Open-source proof story, install notes, and the reminder that the search box is the product. |
| Awwwards SOTD editorial sites | Warm paper canvases, large serif display, generous section rhythm. |
| Stripe Press | Serif display type with plain technical copy. |
| Linear | Hairline structure, keyboard badges, compact status pills. |
| Raycast | The search result pattern: one box, mixed result types, visible match reasons. |
| Resend | A developer build block that does not pretend to be a product download. |
| Gallery references (`minimal.gallery`, `component.gallery`, `navbar.gallery`, `footer.design`, `cta.gallery`) | Navigation, footer and call to action patterns. |
| Motion references (`kinetics.colorion.co`, `animejs.com`, `motion-primitives.com`) | Scroll pacing, reveal timing and how much motion is enough. |

Awwwards tools were used for reference: `search_sites`, `analyze_page_structure`,
`capture_live_site`, `capture_component` and `search_motion`. They informed
pacing and hierarchy, not any copied layout.

## 3. Palette

Tokens are declared in `tailwind.config.js` and mirrored as CSS variables for
the few raw uses. Contrast figures below are computed against paper `#F6F2EA`.

| Token | Hex | Role | Contrast |
| --- | --- | --- | --- |
| `paper` | `#F6F2EA` | Page background | |
| `paper.deep` | `#EDE6D8` | Quiet band fill | |
| `paper.card` | `#FCFAF6` | Lifted panel | |
| `ink` | `#16140F` | Headings and essential text | 16.5:1 |
| `ink.soft` | `#3E392F` | Body copy | 10.3:1 |
| `ink.muted` | `#6B6557` | Metadata at 11 to 13 px | 5.2:1 |
| `accent` | `#C2410C` | Primary action, small emphasis | 4.6:1 |
| `accent.deep` | `#8E2D06` | Accent text on tinted fills | 7.4:1 |
| `accent.soft` | `#F7E6D8` | Tinted fill behind accent text | |
| `line` | `#DFD7C8` | Hairline rules | |
| `line.strong` | `#C6BDAB` | Stronger edge | |
| `night` | `#12110E` | Install band | |
| `night.soft` | `#1B1915` | Panel inside the night band | |
| `night.ink` | `#F2EDE3` | Text on night | 16.2:1 |
| `night.muted` | `#A69E8D` | Secondary text on night | 7.1:1 |
| `sage` | `#5E7358` | "Nothing" in the data table | 4.6:1 |
| `slate` | `#4C5A66` | Reserved | |

Accent on accent.soft measures 4.3:1, so accent text on a tinted surface uses
`accent.deep` instead. Night body copy never goes below `night.muted`.

## 4. Typography

Fonts are self-hosted through `@fontsource`, so there is no runtime request to a
font CDN and no layout shift while a webfont arrives.

- Display: `Instrument Serif` 400, roman and italic.
- Body: `Inter Tight Variable`, with `ss01`, `cv05` and `cv11` enabled.
- Mono: `JetBrains Mono Variable`.

| Level | Size | Use |
| --- | --- | --- |
| Hero | `clamp(2.9rem, 7.2vw, 6rem)` | One per page |
| Section heading, large | `clamp(2.6rem, 5.4vw, 4.9rem)` | `SectionHead`, size `large` |
| Section heading | `clamp(2.2rem, 4.1vw, 3.7rem)` | `SectionHead`, default size |
| Stage heading | `clamp(2.1rem, 3.6vw, 3.2rem)` | Product story |
| Body | 15 to 17 px sans | Explanations |
| Metadata | 9 to 13 px mono or sans | Units, labels, captions |

Display tracking is `-0.02em`, never tighter. Headings use `text-wrap: balance`;
paragraphs use `text-wrap: pretty`. Body line length stays under about 44ch in
the hero and 65ch elsewhere.

## 5. Layout and spacing

- One shell: `max-width: 1280px`, centered, `20px` inline padding that grows to
  `32px` at 640 and `48px` at 1024.
- The shell can carry `border-x` hairlines, so the page reads as a bounded
  column on wide screens.
- Section padding: `py-20`, `py-24` at 640, `py-32` at 768.
- Sticky nav height is `4.5rem`; anchors land 76 px down.
- Grids use rules (`border-t`, `border-b`, `divide-y`) before they use cards.
  Cards, where they exist, are 8 to 12 px radius with a single hairline border.
- Mobile checkpoints: 320, 390 and 1440 px. The page must not scroll sideways at
  any of them. Tables become stacked lists below `md`; the install platform list
  is a definition-style grid at every width.

## 6. Motion

- Lenis drives the page: `lerp 0.09`, wheel `0.9`, touch `1.15`, anchors at a
  `-76px` offset. GSAP ScrollTrigger updates from the Lenis scroll event.
- Reveals use `[data-reveal]` and `[data-reveal-group]`: 26 px rise for single
  blocks, 22 px with a `0.07s` stagger for groups, `expo.out`, once.
- The from-state is applied only when the trigger fires (`immediateRender:
  false`). Content is visible by default, so a missed trigger, a hidden tab or a
  headless render can never leave a section blank. Blocks already on screen when
  the page hydrates keep their prerendered state instead of replaying.
- The hero intro is CSS: two clipped lines rise, the lede and buttons fade up,
  the app window settles, all from the first paint. It never waits for
  hydration, and `backwards` fill means no animation style is left behind.
- The hero story is one coded loop, not a video: a friend's question types
  itself, the reply lands, Ctrl + Alt + T summons a Tessera window over the
  chat, the query types, results arrive, the grid selects itself and the
  album drops back into the chat. A single phase clock drives all of it.
  When the question lands, a floating card states the punchline: don't scroll
  12,400 photos, launch Tessera and describe the picture. Playback pauses on
  hover (mouse only), when the stage leaves the viewport, when the tab is
  hidden, and from the visible Pause button. Each step is a CSS transition or
  a small bubble-in, never a scrubbed timeline.
- The hero tile field is one Three.js shader on one plane, lazily imported
  after idle on desktop only. It clamps device pixel ratio, stops drawing when
  it leaves the viewport and renders one still frame when reduced motion is set.
  Below 1024 px, or on a coarse pointer, three.js is never fetched.
- Under `prefers-reduced-motion: reduce`, Lenis and GSAP never start, the tile
  field draws a still, the hero story renders its final frame statically with
  no pause control, and CSS transitions and animations collapse to their end
  state. The page scrolls natively.
- Interface transitions stay at 140 to 200 ms. Buttons scale to `0.98` on press.
  There are no marquees or parallax text layers. The only autoplaying motion
  is the hero story, which stops on hover, offscreen, a hidden tab or the
  Pause button, and never autoplays video.

## 7. Proof and numbers

- The hero story is a replica of the app window. The chat, the shortcut and
  the send are simulated; neutral chat copy ("Maya", the camera roll line)
  keeps the scenario generic. Its photos are crops of a real app screenshot,
  captioned "Photos cropped from the real app", and the result counter keeps
  the measured "60 shown · 52 ms". The stage sits in a rounded frame over an
  original painted Bali scene, credited "Painted backdrop" beside the stage.
- The demo is ten fixed files, five of them sample photos, filtered in the
  page by typed words. It is labelled as a fixed sample with no engine and no
  simulated timings.

Numbers come from `docs/performance.md` and carry their machine and sample:

- 5,000 files: 65 ms p50, 80 ms p95, target met.
- 50,000 files: 457 ms p50, 523 ms p95, target missed. Shown first, with a miss
  badge. Never presented as a win.
- OCR: 34.4 ms p50, labelled as one sample on a different machine.
- Description search: 0.7015 R@1 at 300 photos and 0.3552 at 5,000, with the
  caption-library caveat.
- Index size: 22.7 MB per 1,000 at 5,000 files and 13.7 MB at 50,000.

## 8. Components

1. **`Nav`**: hairline header, section links, a download pill, a star link, a
   scroll progress rule, and a mobile menu with `aria-expanded` and Escape to
   close.
2. **`Hero`**: serif headline with an italic accent line, plain lede, two
   actions, a shortcut hint, and `HeroStage`, the chat-to-send story in a
   rounded frame over a painted backdrop, with its pause control. The tile
   field sits behind it.
3. **`Problem`**: two contrasting file cards, three named failure modes, and a
   comparison table that becomes stacked rows below `md`.
4. **`ProductStory`**: five stages from watch to answer, a sticky stage rail on
   desktop with `aria-current`, and one illustration per stage.
5. **`Demo`**: a fixed ten-file sample with real thumbnails, a typed query,
   query chips, match highlighting, a live result count and arm badges.
6. **`Numbers`**: the miss first, then OCR, description search and index size,
   with fixed bars and visible targets. Bars never animate in, so they cannot
   be missed.
7. **`Privacy`**: a data table that becomes a list below `md`, plus four
   promises on hairline rules.
8. **`Install`**: the night band. The download, the version, the model size,
   first-run steps and the unsigned-installer note. Its primary button points
   at the stable release asset, and a secondary link goes to Releases for
   checksums.
9. **`Faq`**: a button accordion with `aria-expanded`, `aria-controls` and a
   pinned region. One item starts open.
10. **`Footer`**: product, project and document columns, a closing display line
    and the plain bottom strip.

## 9. Accessibility

- The skip link is the first tab stop.
- Focus is visible everywhere: 2 px accent outline, 3 px offset.
- The mobile menu reports state and closes on Escape.
- The demo result count sits in a polite live region. Query chips report
  `aria-pressed`. Thumbnails are decorative because the file name is adjacent.
- Tables carry captions, and their mobile replacements keep the reading order.
- Images either have a real description or `alt=""` plus adjacent text.
- The hero story is decorative to assistive tech (`aria-hidden`, no focusable
  inner controls) and carries one figure label describing the whole sequence.
  It has a Pause control, and reduced motion shows the final frame without one.
- Every text pair used for body copy meets 4.5:1. Metadata that does not carry
  essential copy stays at `ink.muted` or above.

## 10. Rules

| Do | Don't |
| --- | --- |
| Use paper, ink, hairlines and one terracotta accent. | Use neon gradients, glassmorphism or a decorative dark section. |
| Keep the night band for install only. | Flip the whole page dark for style. |
| Label the sample demo and the illustrations as such. | Present a sample or a mockup as app footage. |
| Credit the painted backdrop and the real-app photo crops beside the stage. | Stage a mockup without saying it is one, or use real people's photos. |
| Quote `docs/performance.md` with machine, sample and caveat. | Round up, project, or turn a target into a result. |
| Point the download at the release asset that exists. | Link a download before the workflow has published the asset. |
| Say the installer is unsigned and how SmartScreen behaves. | Hide the warning or call the build signed. |
| Load no third-party script, font or image at runtime. | Add a star-count call or any network request the privacy page denies. |
| Keep primary controls at least 44 px high. | Hide focus rings or animate keyboard actions. |
| Keep hover behind a hover-capable pointer check. | Let touch taps stick in hover states. |
