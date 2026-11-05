# Tessera Website — Design System & Editorial Direction (v2 Light Theme)

## 1. Design Direction & Philosophy

The previous dark "deep obsidian" design was discarded in favor of an **editorial, light-theme aesthetic** engineered for trust, clarity, and readability.

Tessera is an offline, local-first semantic search engine for personal files. Its design reflects physical materials:
- **Canvas**: Warm off-white paper (`#FAF9F5`), reducing eye fatigue and evoking high-grade archival paper.
- **Ink**: Deep charcoal ink (`#181715` for headlines, `#45423D` for body text), delivering print-grade contrast without the stark harshness of pure `#000000`.
- **Accent**: One confident accent — **Terracotta / Sienna** (`#C2410C`), symbolizing a *tessera* (the fired ceramic or stone tile of a mosaic).
- **Subtlety & Craft**: 1px hairline borders (`#E5E2D9`), hairline division grids, and tactile physical feedback rather than neon gradients, card-in-card nesting, or generic AI slop.
- **Absolute Honesty**: No fabricated app screenshots. The search vignette is explicitly labeled as an interactive illustration demonstrating the 3-format search experience.

---

## 2. Reference Sites Studied & Recorded

As specified in the research brief, eight real award-winning reference sites and curated design benchmarks were studied, and key principles were extracted into this design system:

| Reference Site | URL | Key Elements Taken |
| :--- | :--- | :--- |
| **Daylight Computer** *(Awwwards SOTD)* | `https://daylightcomputer.com` | **Palette & Warmth**: Warm ivory/paper-like canvas (`#FAF9F5`), charcoal ink (`#181715`), tactile contrast without ocular glare. |
| **Stripe Press** | `https://press.stripe.com` | **Editorial Typography**: Classical transitional serif display typography paired with clean technical sans copy and generous letter-spacing on subheadings. |
| **Linear** | `https://linear.app` | **Structure & Density**: 1px precision hairline borders (`#E5E2D9`), keyboard-shortcut badges (`Alt + Space`), and crisp status pill indicators. |
| **Raycast** | `https://raycast.com` | **Interactive Search Vignette**: Interactive live search simulation with instant keyboard navigation, multi-format results, and sub-millisecond query feel. |
| **Shift 5** *(by Non-Linear Studio, Awwwards SOTD)* | `https://shift5.net` | **Data & Proof Presentation**: Clean technical proof blocks, honest metric callouts with exact unit tags, test scripts, and explicit caveats. |
| **Reflect Notes** | `https://reflect.app` | **Tactile Privacy Callout**: Off-white card surfaces, reassuring architectural clarity about local storage, and zero cloud network calls. |
| **Resend** | `https://resend.com` | **Developer Ergonomics**: Minimalist copy-to-clipboard terminal snippet, clean platform matrix with secondary GitHub star count. |
| **Magic UI / 21st.dev** | `https://magicui.design` / `https://21st.dev` | **Micro-interactions**: Subtle interactive 5-stage pipeline stepper (Watch → Extract → Embed → Fuse → Answer) and CSS focus rings. |

---

## 3. Color Palette & Contrast Verification

All text meets or exceeds **WCAG AA** standards, with primary and secondary typography exceeding **WCAG AAA**:

| Token Name | Hex Code | Semantic Role | Contrast vs `#FAF9F5` | Standard Met |
| :--- | :--- | :--- | :--- | :--- |
| `canvas` | `#FAF9F5` | Primary page background (warm paper) | Baseline (1.0:1) | Canvas |
| `surface` | `#FFFFFF` | Elevated card & panel surface | 1.05:1 | Background |
| `subtle` | `#F3F1EB` | Secondary surface / badge background | 1.08:1 | Background |
| `inset` | `#EBE8E0` | Deep subtle panel & code container | 1.15:1 | Background |
| `line` | `#E5E2D9` | 1px precision hairline borders | 1.22:1 | Non-text UI |
| `line-strong` | `#D1CDC0` | Focus / active border indicators | 1.51:1 | Non-text UI |
| `ink.primary` | `#181715` | Headings, titles, high-emphasis text | **15.4:1** | **WCAG AAA** (≥ 7:1) |
| `ink.secondary` | `#45423D` | Body copy, section descriptions | **8.8:1** | **WCAG AAA** (≥ 7:1) |
| `ink.muted` | `#59554E` | Metadata, captions, secondary specs | **6.6:1** | **WCAG AA** (≥ 4.5:1) |
| `ink.faint` | `#6B665E` | Subdued annotations, code labels | **5.0:1** | **WCAG AA** (≥ 4.5:1) |
| `accent.DEFAULT` | `#C2410C` | Terracotta primary CTA, active states | **5.1:1** | **WCAG AA** (≥ 4.5:1) |
| `accent.hover` | `#9A3412` | CTA hover state, dark terracotta | **7.3:1** | **WCAG AAA** (≥ 7:1) |
| `accent.subtle` | `#FFF7ED` | Pill highlights, active stage background | 1.05:1 | Background |

---

## 4. Typography Hierarchy & System Stacks

To guarantee **zero runtime network requests**, **zero font-loading FOIT/CLS**, and instant rendering, the site utilizes native system font stacks:

1. **Editorial Serif (Headings & Display)**:
   - Stack: `Charter, "Bitstream Charter", "Sitka Text", Cambria, Georgia, serif`
   - Purpose: Evokes the craft of fine print and authoritative editorial publications.
2. **Engineered Sans (Interface & Body)**:
   - Stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif`
   - Purpose: Clean, modern Swiss grotesk legibility across mobile and desktop.
3. **Monospace (Data, Code & Units)**:
   - Stack: `ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, monospace`
   - Purpose: Precise tabular alignment for benchmark measurements, file paths, and terminal commands.

### Typographic Scale

| Level | Size / Line Height | Tracking | Font Family | Usage |
| :--- | :--- | :--- | :--- | :--- |
| **Hero Title** | 3.75rem (60px) / 1.12 | `-0.025em` | Serif (Bold) | Hero value proposition |
| **Section Title** | 2.25rem (36px) / 1.2 | `-0.02em` | Serif (Bold) | Section headlines |
| **Card Heading** | 1.25rem (20px) / 1.3 | `-0.01em` | Serif (Bold) | Feature & proof card titles |
| **Body Large** | 1.125rem (18px) / 1.6 | `normal` | Sans (Regular) | Hero sub-lines |
| **Body Standard** | 0.875rem (14px) / 1.55 | `normal` | Sans (Regular) | Primary descriptive paragraphs |
| **Eyebrow / Badge**| 0.75rem (12px) / 1.4 | `+0.05em` | Monospace (SemiBold) | Category tags, step numbers |
| **Code / Metadata** | 0.6875rem (11px) / 1.4 | `normal` | Monospace (Regular) | File paths, benchmark units |

---

## 5. Spacing, Elevation & Layout Scale

- **Max Container Width**: `max-w-5xl` (1024px) for content; `max-w-6xl` (1152px) for header bar.
- **Section Padding**: `py-20` (80px) on desktop, `py-12` (48px) on mobile for generous breathing space.
- **Elevation System**:
  - `shadow-tile`: `0 1px 3px rgba(24, 23, 21, 0.05), 0 1px 2px rgba(24, 23, 21, 0.03)` (subtle tactile surface).
  - `shadow-elevated`: `0 10px 30px -10px rgba(24, 23, 21, 0.08)` (search vignette).
- **Hairline Dividers**: `1px solid #E5E2D9` between sections and grid cells to create structural discipline.

---

## 6. Motion & Accessibility Invariants

1. **Reduced Motion Respect**:
   - Implemented via `@media (prefers-reduced-motion: reduce)` in `src/index.css`. All CSS transitions and animations collapse to `0.01ms` when reduced motion is preferred.
2. **Keyboard Navigability**:
   - Explicit `:focus-visible` ring in terracotta (`outline: 2px solid #C2410C; outline-offset: 2px;`).
   - All interactive scenarios, tabs, accordions, and platform cards are fully focusable with Enter/Space support.
3. **Touch Targets**:
   - All interactive controls have touch target sizes exceeding 32×32px (and 48×48px on primary actions), eliminating touch-target warnings in Lighthouse.

---

## 7. Component Inventory

1. **`Navbar`**:
   - Editorial wordmark with mosaic tessera SVG icon.
   - Jump links to all major content sections.
   - Primary GitHub star button with MIT badge.
   - Responsive mobile navigation toggle.
2. **`Hero`**:
   - Plain-language promise: "One search bar for everything your PC can't search."
   - Honest sub-line: "runs entirely on your device, answers in milliseconds, and never uploads anything."
   - Primary CTA: "Windows build — coming with v0.1" (honest release link).
   - Abstract interactive search vignette:
     - Clear disclaimer: *Interactive Illustration · Not a live product screenshot*.
     - Interactive scenario switcher (`"the invoice from the airline"`, `"quarterly revenue red chart"`).
     - Live 3-format result breakdown: Screenshot (OCR), PDF (page snippet), and DOCX (paragraph match).
     - Live latency badges and engine arm transparency.
3. **`ProblemSection`**:
   - The failure modes of Windows desktop search.
   - Comprehensive positioning matrix comparing Tessera against Windows Explorer, Cloud SaaS, and Copilot+.
4. **`HowItWorks`**:
   - 5-step interactive pipeline stepper:
     1. **Watch**: Idle OS priority, newest-first queue, battery pause, OneDrive placeholder skip.
     2. **Extract**: `Windows.Media.Ocr` (34.4 ms p50), PDF text chunks, DOCX XML parser.
     3. **Embed**: SigLIP-base int8 vision model + MiniLM-L6 language model.
     4. **Fuse**: Evidence-weighted Reciprocal Rank Fusion (lexical 1.0, image 0.05 for text-bearing files).
     5. **Answer**: SQLite FTS5 trigram + sqlite-vec vector distance search in tens of milliseconds.
5. **`ProofSection`**:
   - 4 strictly measured facts from `docs/performance.md`:
     - 34.4 ms p50 Windows Native OCR.
     - ~260–350 ms projected save-to-searchable.
     - 0.7015 R@1 / 0.9454 R@5 COCO text-to-image retrieval over 300 photos and 1,501 human queries.
     - 0.4 ms p50 Lexical trigram query latency.
   - Hardware context callout card (AMD Ryzen 3 5300U, 8 threads, 7.3 GB RAM, Windows 11).
   - Honest caveats attached to every single number.
6. **`PrivacySection`**:
   - The 4 pillars of local-first engineering: 100% Offline, No Accounts / No Telemetry, Single SQLite Index File, SHA-256 Pinned Weights.
   - Model Context Protocol (MCP) phase 3 architectural callout.
7. **`DownloadSection`**:
   - Honest platform matrix: Windows 10/11 x64 (primary v0.1 launch), macOS (CI-verified Vision adapter, latency unmeasured), Linux (ONNX fallback).
   - Interactive copy-to-clipboard build-from-source command box.
8. **`FAQSection`**:
   - 7 detailed, accessible accordion items answering real questions about formats, telemetry, battery, index size, platforms, model downloads, and MIT licensing.
9. **`Footer`**:
   - Complete documentation links, GitHub repository, issues tracker, maintainer credit, and copyright notice.

---

## 8. Strict Design Rules: Do's and Don'ts

| Do | Don't |
| :--- | :--- |
| **Do** use warm off-white canvas (`#FAF9F5`) and deep charcoal ink (`#181715`). | **Don't** use dark mode or stark `#000000` on `#FFFFFF`. |
| **Do** use one confident terracotta accent (`#C2410C`). | **Don't** use multi-color neon gradients or rainbow border glows. |
| **Do** clearly label illustrative mockups as interactive illustrations. | **Don't** fabricate or present mockups as real app screenshots. |
| **Do** quote real measured benchmarks from `docs/performance.md` with caveats. | **Don't** invent unverified speed claims or present targets as results. |
| **Do** keep runtime network requests strictly at zero. | **Don't** import Google Fonts or third-party tracking scripts. |
| **Do** provide keyboard accessibility and touch targets for all interactive elements. | **Don't** create click targets smaller than 32px or hide focus rings. |
