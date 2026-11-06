import React, { useRef, useState } from 'react';
import { ArrowLeft, ArrowRight } from 'lucide-react';
import { Section, SectionHeading } from './Section';

interface Stage {
  id: string;
  number: string;
  title: string;
  summary: string;
  heading: string;
  description: string;
  bullets: string[];
  spec: Array<[string, string]>;
}

const STAGES: Stage[] = [
  {
    id: 'watch',
    number: '01',
    title: 'Watch',
    summary: 'Filesystem events, newest first',
    heading: 'New files first, while the machine is idle.',
    description:
      'Tessera watches the folders you choose. A screenshot you just saved jumps the queue, so it does not wait behind a backlog. The watcher runs at idle priority and pauses on battery.',
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
    id: 'extract',
    number: '02',
    title: 'Extract',
    summary: 'Read the text inside each format',
    heading: 'Read the words inside each format.',
    description:
      'Images go through the platform OCR path. PDFs are read page by page. Word and plain text are pulled from their own bytes. A tile keeps the page number, so a result can open at the right place.',
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
    number: '03',
    title: 'Embed',
    summary: 'Two small models, one vector space',
    heading: 'Two small models, pinned to one vector space.',
    description:
      'SigLIP base embeds images for description search. MiniLM-L6 embeds document chunks for meaning search. Model versions are recorded in the index. Changing a model forces a re-index instead of mixing vector spaces.',
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
    number: '04',
    title: 'Fuse',
    summary: 'Rank-based merging across arms',
    heading: 'Rank-based fusion, because scores are not comparable.',
    description:
      'BM25 and cosine similarity live on different scales. Tessera merges the arms by rank with Reciprocal Rank Fusion, and weights the image arm down for files that already have text.',
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
    number: '05',
    title: 'Answer',
    summary: 'One grid, keyboard first',
    heading: 'One grid, in tens of milliseconds at 5,000 files.',
    description:
      'A fused query took 65 ms p50 at 5,000 files on the reference machine, against a 100 ms target. At 50,000 files the same query took 457 ms p50 and missed that target. The measured section shows both.',
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

export const HowItWorks: React.FC = () => {
  const [activeIndex, setActiveIndex] = useState(0);
  const tabRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const stage = STAGES[activeIndex];

  const onTabKeyDown = (
    event: React.KeyboardEvent<HTMLButtonElement>,
    index: number,
  ) => {
    const last = STAGES.length - 1;
    let next = index;

    if (event.key === 'ArrowRight') next = index === last ? 0 : index + 1;
    else if (event.key === 'ArrowLeft') next = index === 0 ? last : index - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;
    else return;

    event.preventDefault();
    setActiveIndex(next);
    tabRefs.current[next]?.focus();
  };

  const go = (direction: -1 | 1) => {
    const next = (activeIndex + direction + STAGES.length) % STAGES.length;
    setActiveIndex(next);
    tabRefs.current[next]?.focus();
  };

  return (
    <Section id="how-it-works" labelledBy="how-title">
      <SectionHeading
        id="how-title"
        title={<>From a saved file to a ranked answer.</>}
        lede={
          <>
            Five stages, all local. Models load in batches, the queue goes newest
            first, and the index is one SQLite file you can inspect.
          </>
        }
      />

      <div
        role="tablist"
        aria-label="Pipeline stages"
        className="mt-10 grid grid-cols-5 gap-px overflow-hidden rounded-lg border border-line bg-line"
      >
        {STAGES.map((item, index) => {
          const selected = index === activeIndex;
          return (
            <button
              key={item.id}
              ref={(element) => {
                tabRefs.current[index] = element;
              }}
              type="button"
              role="tab"
              id={`stage-tab-${item.id}`}
              aria-selected={selected}
              aria-controls={`stage-panel-${item.id}`}
              tabIndex={selected ? 0 : -1}
              onClick={() => setActiveIndex(index)}
              onKeyDown={(event) => onTabKeyDown(event, index)}
              className={`flex min-h-[4.5rem] flex-col items-start justify-between p-2.5 text-left transition-colors duration-150 sm:min-h-[5.5rem] sm:p-4 ${
                selected
                  ? 'bg-surface shadow-[inset_0_2px_0_#C2410C]'
                  : 'bg-canvas hover:bg-subtle'
              }`}
            >
              <span
                className={`font-mono text-[10px] ${
                  selected ? 'text-accent' : 'text-ink-faint'
                }`}
              >
                {item.number}
              </span>
              <span className="mt-2 block">
                <span className="block font-serif text-[13px] font-bold text-ink-primary sm:text-[15px]">
                  {item.title}
                </span>
                <span className="mt-0.5 hidden text-[11px] leading-snug text-ink-muted sm:block">
                  {item.summary}
                </span>
              </span>
            </button>
          );
        })}
      </div>

      <div
        role="tabpanel"
        id={`stage-panel-${stage.id}`}
        aria-labelledby={`stage-tab-${stage.id}`}
        tabIndex={0}
        key={stage.id}
        className="swap-in border-x border-b border-line bg-surface"
      >
        <div className="grid gap-8 p-5 sm:p-8 lg:grid-cols-[minmax(0,1fr)_19rem] lg:gap-10">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
              Stage {stage.number} of 05
            </p>
            <h3 className="mt-2 font-serif text-[clamp(1.4rem,1.15rem+1vw,1.85rem)] font-bold leading-tight tracking-tight text-ink-primary">
              {stage.heading}
            </h3>
            <p className="mt-3 max-w-[58ch] text-[14px] leading-relaxed text-ink-secondary sm:text-[15px]">
              {stage.description}
            </p>
            <ul className="mt-5 space-y-2.5">
              {stage.bullets.map((bullet) => (
                <li key={bullet} className="flex items-start gap-2.5 text-[13px] leading-relaxed text-ink-secondary">
                  <span
                    className="mt-[0.5rem] h-1 w-1 shrink-0 bg-accent"
                    aria-hidden="true"
                  />
                  <span>{bullet}</span>
                </li>
              ))}
            </ul>
          </div>

          <div className="border border-line bg-canvas">
            <p className="border-b border-line px-4 py-2.5 font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
              Engine
            </p>
            <dl className="divide-y divide-line">
              {stage.spec.map(([term, value]) => (
                <div
                  key={term}
                  className="grid grid-cols-[5.25rem_minmax(0,1fr)] gap-3 px-4 py-3"
                >
                  <dt className="font-mono text-[10px] uppercase leading-relaxed text-ink-faint">
                    {term}
                  </dt>
                  <dd className="text-[12px] leading-relaxed text-ink-secondary">
                    {value}
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        </div>

        <div className="flex items-center justify-between border-t border-line bg-canvas px-3 py-2 sm:px-5">
          <span className="font-mono text-[10px] text-ink-muted">
            {stage.title}
          </span>
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => go(-1)}
              className="flex h-10 w-10 items-center justify-center rounded-md text-ink-secondary transition-colors duration-150 hover:bg-subtle hover:text-ink-primary"
              aria-label="Previous stage"
            >
              <ArrowLeft className="h-4 w-4" aria-hidden="true" />
            </button>
            <button
              type="button"
              onClick={() => go(1)}
              className="flex h-10 w-10 items-center justify-center rounded-md text-ink-secondary transition-colors duration-150 hover:bg-subtle hover:text-ink-primary"
              aria-label="Next stage"
            >
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
        </div>
      </div>
    </Section>
  );
};
