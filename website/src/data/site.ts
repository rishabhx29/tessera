export const REPO = 'https://github.com/rishabhx29/tessera';
export const RELEASES = `${REPO}/releases`;
export const ISSUES = `${REPO}/issues`;

export const NAV_LINKS = [
  { href: '#product', label: 'Product' },
  { href: '#demo', label: 'Demo' },
  { href: '#numbers', label: 'Numbers' },
  { href: '#privacy', label: 'Privacy' },
  { href: '#install', label: 'Install' },
] as const;

export const FAILURES = [
  {
    n: '01',
    title: 'Screenshots are named after the clock.',
    body: 'Screenshot 2026-10-11 at 14.32.11.png says nothing about the airline invoice inside it. Windows can see the date. It cannot read the words.',
    fix: 'Tessera reads the words inside the image.',
  },
  {
    n: '02',
    title: 'Semantic search wants new hardware.',
    body: 'Copilot+ search needs an NPU laptop. Most people are going to keep the machine they already own.',
    fix: 'Tessera runs on the CPU in that machine.',
  },
  {
    n: '03',
    title: 'One tool sees one format.',
    body: 'Screenshot tools, photo tools and document tools each hold a slice. You end up guessing which one to open.',
    fix: 'One index covers screenshots, PDFs, Word and text.',
  },
] as const;

export type Stage = {
  id: string;
  n: string;
  title: string;
  summary: string;
  heading: string;
  body: string;
  bullets: readonly string[];
  spec: readonly (readonly [string, string])[];
};

export const STAGES: readonly Stage[] = [
  {
    id: 'watch',
    n: '01',
    title: 'Watch',
    summary: 'New files jump the queue',
    heading: 'A screenshot you just saved does not wait behind a backlog.',
    body: 'Tessera watches the folders you choose. Filesystem events go into a newest-first queue, the watcher runs at idle priority, and it pauses on battery. It never moves or modifies a file.',
    bullets: [
      'Newest-first queue with up to four workers.',
      'OneDrive placeholders are skipped, never hydrated.',
      'Files over 50 MB are skipped and counted on the diagnostics screen.',
    ],
    spec: [
      ['Trigger', 'Directory changes, plus mtime, size and SHA-1'],
      ['Schedule', 'Idle-only, always, or paused'],
      ['Safety', 'No file is moved or modified'],
    ],
  },
  {
    id: 'read',
    n: '02',
    title: 'Read',
    summary: 'The words inside every format',
    heading: 'Read the words inside each format, and keep the page number.',
    body: 'Images go through the platform OCR path. PDFs are read page by page. Word and plain text are pulled from their own bytes. Every tile keeps its page or paragraph, so a result opens at the right place.',
    bullets: [
      'Windows uses Windows.Media.Ocr, measured at 34.4 ms p50 over 40 screenshots.',
      'Other platforms use the PP-OCRv6 small models through ONNX Runtime.',
      'PDFs without a text layer are counted for later OCR, never dropped silently.',
    ],
    spec: [
      ['Images', 'PNG, JPG, WEBP, BMP, GIF, TIFF'],
      ['Documents', 'PDF text layer, DOCX, TXT, MD, CSV, LOG'],
      ['Limit', 'Files over 50 MB, with a recorded reason'],
    ],
  },
  {
    id: 'embed',
    n: '03',
    title: 'Embed',
    summary: 'Two small models, one vector space',
    heading: 'Two small models, pinned to one vector space.',
    body: 'SigLIP base embeds images for description search. MiniLM-L6 embeds document chunks for meaning search. Model versions are recorded in the index, so changing a model forces a re-index instead of mixing vector spaces.',
    bullets: [
      'Image arm: SigLIP base, int8, 768 dimensions.',
      'Text arm: MiniLM-L6, int8, 384 dimensions.',
      'Weights are verified against SHA-256 hashes pinned in the binary.',
    ],
    spec: [
      ['Download', 'About 1.3 GB, once, checksum-pinned'],
      ['Inference', 'ONNX Runtime, CPU'],
      ['Versioning', 'Recorded in the index meta table'],
    ],
  },
  {
    id: 'fuse',
    n: '04',
    title: 'Fuse',
    summary: 'Rank-based merging across arms',
    heading: 'Rank-based fusion, because the scores are not comparable.',
    body: 'BM25 and cosine similarity live on different scales. Tessera merges the arms by rank with Reciprocal Rank Fusion, and weights the image arm down for files that already have text.',
    bullets: [
      'SQLite FTS5 trigram search handles partial words and lookalike typos.',
      'Text-bearing files weight the image arm at 0.05, so it cannot drown exact matches.',
      'Every hit states which arm surfaced it and why.',
    ],
    spec: [
      ['Lexical', 'FTS5 trigram, BM25'],
      ['Fusion', 'Weighted reciprocal rank fusion'],
      ['Transparency', 'Match arm shown on every result'],
    ],
  },
  {
    id: 'answer',
    n: '05',
    title: 'Answer',
    summary: 'One grid, keyboard first',
    heading: 'One grid, in tens of milliseconds at 5,000 files.',
    body: 'A fused query took 65 ms p50 at 5,000 files on the reference machine, against a 100 ms target. At 50,000 files the same query took 457 ms p50 and missed that target. Both numbers are on this page.',
    bullets: [
      'Keyboard first: arrows, Enter, Escape.',
      'Preview shows OCR text, a page number, or the paragraph.',
      'Open the file or reveal it in its folder.',
    ],
    spec: [
      ['At 5,000 files', '65 ms p50 / 80 ms p95, target met'],
      ['At 50,000 files', '457 ms p50 / 523 ms p95, target missed'],
      ['Index', 'One SQLite file'],
    ],
  },
];

export const STATS = [
  {
    badge: 'Target missed',
    label: 'Fused query at 50,000 files',
    value: '457 ms',
    unit: 'p50',
    secondary: '523 ms p95, target under 100 ms',
    body: 'About 4.6 times over the target. The meaning arm is the largest cost at 241 ms p50. This row is not a win, and closing the gap needs a different index shape, not a bigger number here.',
    tone: 'miss' as const,
  },
  {
    badge: 'Single sample',
    label: 'Windows OCR, per image',
    value: '34.4 ms',
    unit: 'p50',
    secondary: '139.9 ms p95, 188 mean characters',
    body: '40 real screenshots, one pass, one image at a time, on an AMD Ryzen 3 5300U, 8 threads, 7.3 GB RAM, Windows 11 build 26300. A repeat pass moved p50 to 45.7 ms, so read the figure as one sample.',
    tone: 'plain' as const,
  },
  {
    badge: 'COCO, 300 photos',
    label: 'Description search, 300 photos',
    value: '0.7015',
    unit: 'R@1',
    secondary: '0.9454 R@5, 1,501 captions',
    body: 'Measured through the shipped SigLIP path on COCO 2014 photos. The model was asked to reach 0.85 R@1. It does not, at any measured library size.',
    tone: 'plain' as const,
  },
  {
    badge: 'Rank 1 falls',
    label: 'Description search, 5,000 photos',
    value: '0.3552',
    unit: 'R@1',
    secondary: '0.6033 R@5, 25,010 captions',
    body: 'The same path, a larger shelf. Rank 1 loses about 8.4 to 8.9 points per doubling of the library, with no flattening in the measured curve. Description search is a retrieval aid, not a guarantee.',
    tone: 'plain' as const,
  },
  {
    badge: 'Measured',
    label: 'Index size on disk',
    value: '22.7 MB',
    unit: 'per 1,000',
    secondary: '13.7 MB per 1,000 at 50,000 files',
    body: '113.7 MB for the 5,000-file index, 686 MB for 50,000. The mix of file types changes the number, so treat it as the measured shape of these corpora, not a constant.',
    tone: 'plain' as const,
  },
] as const;

export const FAQ = [
  {
    q: 'When can I download it?',
    a: 'There is no public release yet. The Windows installer is built and has been tested locally. Publishing it to GitHub Releases is the next step, and there is no date to promise. Watch the Releases page to see it land.',
  },
  {
    q: 'What can it search?',
    a: 'In v0.1: screenshots and images (PNG, JPG, WEBP, BMP, GIF, TIFF) through OCR, PDFs with a text layer, Word documents, and plain text (TXT, MD, CSV, LOG). Video is planned for v0.2, XLSX and PPTX for v0.4. Scanned PDFs without a text layer are flagged and counted, not silently dropped.',
  },
  {
    q: 'Does it send anything anywhere?',
    a: 'No. Search runs locally with no network step, and there is no telemetry or account layer in the build. The only download is the model set on first run, checked against SHA-256 hashes pinned in the binary.',
  },
  {
    q: 'What does it cost in battery and memory?',
    a: 'Indexing runs at idle priority and pauses on battery under the default schedule. Models load when there is work and are released when the queue drains. The 5,000-file drain peaked at 452 MB. A separate watcher run measured 43.2 MB working set once idle. The product target is under 250 MB at idle, and the engine queues work rather than holding models to meet it.',
  },
  {
    q: 'How big is the index?',
    a: '113.7 MB for 5,000 files, or 22.7 MB per 1,000. At 50,000 files it was 686 MB, or 13.7 MB per 1,000, because the mix of file types changes. It is one SQLite file you can inspect, back up, move, or delete.',
  },
  {
    q: 'Is the 100 ms search target met?',
    a: 'At 5,000 files, yes: 65 ms p50 and 80 ms p95. At 50,000 files, no: 457 ms p50 and 523 ms p95. The 50,000-file row is a miss, mostly in the meaning arm, and it is reported as one. The full method and caveats are in the performance log.',
  },
  {
    q: 'What about macOS and Linux?',
    a: 'Windows 10 and 11 x64 is first. The macOS Vision adapter compiles and passes a smoke test on a CI runner, but it has not been measured on Apple hardware. Linux uses the ONNX fallback and is source only. Neither has a downloadable build.',
  },
  {
    q: 'What does it cost, and under what license?',
    a: 'Free, under the MIT license. No subscription, no account, no paid tier.',
  },
] as const;

export const FORMAT_RAIL = ['PNG', 'JPG', 'WEBP', 'BMP', 'GIF', 'TIFF', 'PDF', 'DOCX', 'TXT', 'MD', 'CSV', 'LOG'] as const;
