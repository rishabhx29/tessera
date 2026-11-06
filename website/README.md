# Tessera marketing website

The public site for Tessera, a local search bar for screenshots, PDFs and
documents. Warm paper, ink, one terracotta accent, Instrument Serif headings,
and a tile field behind the hero.

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

The build prerenders the page. `vite build` writes the client bundle, a second
`vite build --ssr` pass compiles `src/entry-server.tsx`, and
`scripts/prerender.mjs` renders the markup into `dist/index.html` and adds
preloads for the three fonts used above the fold. `src/main.tsx` hydrates that
markup; `npm run dev` has no prerender and mounts the app normally.

## What is on the page

`Nav`, `Hero`, `Problem`, `ProductStory`, `Demo`, `Numbers`, `Privacy`,
`Install`, `Faq`, `Footer`. Page-wide smooth scroll and reveals live in
`src/lib/motion.ts`. The hero tile field is `src/components/HeroTiles.tsx`.
The hero intro is a CSS animation in `src/index.css` so it starts at first
paint and does not wait for hydration.

## The sample demo

The demo is a fixed set of ten rows in `src/components/Demo.tsx`. Five are
sample photos in `public/media/corpus/`, five are documents. Typing filters
file names, snippets and keyword lists in the page.

There is no engine, no network request and no simulated timing. The panel says
so, and the comparison table beside it separates this sample from the app. Keep
both notes in place.

To edit the sample, change `ROWS`. Thumbnails use `alt=""` because the file name
sits right next to the image.

## Verification

```bash
npm run build
node take-screenshots.cjs
node run-lighthouse.cjs
```

`take-screenshots.cjs` serves the production build on port 4188, then captures:

- `desktop-1440x900-hero.png` and `desktop-1440x900-full.png`
- `desktop-section-{problem,product,demo,numbers,privacy,install,faq}.png`
- `desktop-focus-skip-link.png` and `desktop-interactive-search.png`
- `mobile-390x844-hero.png`, `mobile-390x844-full.png`, `mobile-390x844-menu.png`
- `mobile-section-{demo,numbers,privacy,install}.png`
- `desktop-reduced-motion-hero.png` and `desktop-reduced-motion-demo.png`

It also writes `.preview/checks.json` with console and page errors, horizontal
overflow, first Tab, focus ring styles, the live demo result count, the mobile
menu state, accessible button names, missing image alt text, small hit targets,
reduced-motion behavior and a no-JavaScript load. Output goes to
`website/.preview/`, which is gitignored.

`run-lighthouse.cjs` sends brotli or gzip when the browser asks for it, the way
a static host does. Without that the audit charges every byte at full size and
reports a slower site than anyone downloads.

## Current build numbers

From a local production build on 2026-11-07:

- HTML after prerender: 131.24 kB, 19.76 kB gzipped
- Entry JS: 440.43 kB, 143.31 kB gzipped
- Three.js chunk: 742.80 kB, 187.69 kB gzipped, loaded on desktop only, after idle
- CSS: 41.18 kB, 10.84 kB gzipped
- Largest self-hosted font file: 89.80 kB

These are local numbers. Re-run the build before quoting them.

## Rules for this directory

1. No runtime network requests. Fonts are self-hosted woff2 files from
   `@fontsource`, not a CDN. No analytics and no third-party scripts.
2. No fabricated app stills or footage. Illustrations are labelled as
   illustrations, and the demo is labelled as a fixed sample.
3. Every performance number comes from `docs/performance.md`, with the machine
   and sample size that make it honest. The 50,000-file query row is a miss and
   is shown first.
4. There is no public release yet. Copy points at GitHub Releases and says the
   first release is being prepared.

See [`DESIGN.md`](./DESIGN.md) for the palette, type scale, motion rules and
component inventory.
