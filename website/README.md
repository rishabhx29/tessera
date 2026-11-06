# Tessera marketing website

The public site for Tessera, a local search bar for screenshots, PDFs and
documents. Warm paper, ink, one terracotta accent, and hairline structure.

## Quick start

```bash
npm ci
npm run dev
```

Production build and lint:

```bash
npm run build
npm run lint
```

## Demo video: where the real recording goes

The site ships with an honest placeholder slate, not app footage. The player is
already wired for a real recording. Replace three files and flip one flag.

| Asset | Path | Format |
| --- | --- | --- |
| Video | `public/media/demo.mp4` | MP4, H.264, `yuv420p`, 1280x720 or 1920x1080, no audio needed, under ~30 MB |
| Poster | `public/media/demo-poster.png` | PNG or JPG still from the recording, 1280x720 or 1920x1080 |
| Captions | `public/media/demo.en.vtt` | WebVTT, English, cues timed to the new video |

Replace the files in place and keep the names. The player reads the paths from
`src/demo.ts`:

```ts
export const DEMO_IS_PLACEHOLDER = true;
export const DEMO_VIDEO = {
  src: '/media/demo.mp4',
  poster: '/media/demo-poster.png',
  captions: '/media/demo.en.vtt',
  captionsLabel: 'English',
} as const;
```

When the recording is real, set `DEMO_IS_PLACEHOLDER = false`. That removes the
placeholder badge and switches the caption note and the player's accessible
label. No layout change is needed.

Regenerate the poster after replacing the video:

```bash
ffmpeg -i public/media/demo.mp4 -ss 00:00:08 -frames:v 1 -y public/media/demo-poster.png
```

The recording should show three beats, in this order: summon with Alt+Space, one
query returning a screenshot, a PDF page and a Word paragraph, and a freshly
saved file becoming searchable. Do not substitute a mockup or a screen recording
of an illustration. If the footage is not real, keep the slate.

Captions mirror what is said and shown. They are on by default. Do not remove
the track: the player relies on it for viewers who cannot hear the recording.

## Verification

```bash
npm run build
node take-screenshots.cjs
node run-lighthouse.cjs
```

`take-screenshots.cjs` serves the production build, then captures:

- `desktop-1440x900-hero.png` and `desktop-1440x900-full.png`
- `desktop-section-{problem,demo,how-it-works,measured,privacy,download,faq}.png`
- `desktop-focus-skip-link.png` and `desktop-interactive-pdf-tab.png`
- `mobile-390x844-hero.png`, `mobile-390x844-full.png`, `mobile-390x844-menu.png`
- `mobile-section-{demo,measured,download}.png`
- `desktop-reduced-motion-hero.png` and `desktop-reduced-motion-demo.png`

It also writes `.preview/checks.json` with console and page errors, horizontal
overflow, keyboard focus order, the skip link, focus ring styles, accessible
button names, video and caption state, and reduced-motion behavior. Everything
is written to `website/.preview/`, which is gitignored.

## Current measured numbers

From the latest local production build and audit (headless Chromium, this
machine):

- JavaScript: 288.51 kB, 86.97 kB gzipped
- CSS: 24.61 kB, 6.08 kB gzipped
- HTML: 1.84 kB, 0.83 kB gzipped
- Lighthouse: performance 94, accessibility 100, best practices 100, SEO 100
- FCP 2.3 s, LCP 2.6 s, CLS 0, TBT 20 ms

These are local numbers, not a guarantee. Re-run the scripts before quoting
them.

## Rules for this directory

1. No runtime network requests. System font stacks only, no analytics, no
   third-party scripts.
2. No fabricated app screenshots or footage. Illustrations are labelled as
   illustrations.
3. Every performance number comes from `docs/performance.md`, with the machine
   and sample size that make it honest. The 50,000-file query row is a miss and
   is presented as one.
4. There is no public release yet. Copy points at GitHub Releases and says the
   first release is being prepared.

See [`DESIGN.md`](./DESIGN.md) for the palette, type scale, motion rules and
component inventory.
