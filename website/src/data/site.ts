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

export const FAQ = [
  {
    q: 'When can I download it?',
    a: 'There is no public release yet. The Windows installer is built and has run on a clean Windows 11 machine. Publishing it to GitHub Releases is the next step, with no date promised. macOS and Linux remain source only.',
  },
  {
    q: 'What can it search?',
    a: 'Images and screenshots through OCR, PDFs with a text layer, Word documents, and plain text. Video is planned for v0.2, XLSX and PPTX for v0.4. Scanned PDFs without a text layer are counted, not silently dropped.',
  },
  {
    q: 'Does it send anything anywhere?',
    a: 'No. Search is local with no network step, and the build has no telemetry or account layer. The only request is the first-run model download, checked against SHA-256 hashes pinned in the binary.',
  },
  {
    q: 'Is the 100 ms target met?',
    a: 'At 5,000 files, yes: 65 ms p50 and 80 ms p95. At 50,000 files, no: 457 ms p50 and 523 ms p95, mostly in the meaning arm. That row is reported as a miss, and the method is in the performance log.',
  },
  {
    q: 'What does it cost?',
    a: 'Free, under the MIT license. No subscription, no account, no paid tier.',
  },
] as const;

export const FORMAT_RAIL = ['PNG', 'JPG', 'WEBP', 'BMP', 'GIF', 'TIFF', 'PDF', 'DOCX', 'TXT', 'MD', 'CSV', 'LOG'] as const;
