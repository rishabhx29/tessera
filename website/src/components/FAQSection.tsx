import React, { useState } from 'react';
import { Plus } from 'lucide-react';
import { Section } from './Section';

interface FAQItem {
  id: string;
  question: string;
  answer: React.ReactNode;
}

const FAQ_ITEMS: FAQItem[] = [
  {
    id: 'download',
    question: 'When can I download it?',
    answer: (
      <>
        There is no public release yet. The Windows installer is built and has
        been tested locally. Publishing it to GitHub Releases is the next step,
        and there is no date to promise. Watch the{' '}
        <a
          href="https://github.com/rishabhx29/tessera/releases"
          target="_blank"
          rel="noopener noreferrer"
          className="link"
        >
          Releases page
        </a>{' '}
        to see it land.
      </>
    ),
  },
  {
    id: 'formats',
    question: 'What can it search?',
    answer: (
      <>
        In v0.1: screenshots and images (PNG, JPG, WEBP, BMP, GIF, TIFF) through
        OCR, PDFs with a text layer, Word documents, and plain text (TXT, MD,
        CSV, LOG). Video is planned for v0.2; XLSX and PPTX for v0.4. Scanned
        PDFs without a text layer are flagged and counted, not silently dropped.
      </>
    ),
  },
  {
    id: 'privacy',
    question: 'Does it send anything anywhere?',
    answer: (
      <>
        No. Search runs locally with no network step, and there is no telemetry
        or account layer in the build. The only download is the model set on
        first run, checked against SHA-256 hashes pinned in the binary.
      </>
    ),
  },
  {
    id: 'battery',
    question: 'What does it cost in battery and memory?',
    answer: (
      <>
        Indexing runs at idle priority and pauses on battery under the default
        schedule. Models load when there is work and are released when the queue
        drains. The 5,000-file drain peaked at 452 MB. A separate watcher run
        measured 43.2 MB working set once idle; the 5,000-file run's idle figure
        was not re-measured. The product target is under 250 MB at idle, and the
        engine queues work rather than holding models to meet it.
      </>
    ),
  },
  {
    id: 'index-size',
    question: 'How big is the index?',
    answer: (
      <>
        113.7 MB for 5,000 files, or 22.7 MB per 1,000. At 50,000 files it was
        686 MB, or 13.7 MB per 1,000, because the mix of file types changes. It
        is one SQLite file you can inspect, back up, move, or delete.
      </>
    ),
  },
  {
    id: 'speed',
    question: 'Is the 100 ms search target met?',
    answer: (
      <>
        At 5,000 files, yes: 65 ms p50 and 80 ms p95. At 50,000 files, no: 457
        ms p50 and 523 ms p95. The 50,000-file row is a miss, mostly in the
        meaning arm, and it is reported as one. The full method and caveats are
        in the{' '}
        <a
          href="https://github.com/rishabhx29/tessera/blob/main/docs/performance.md"
          target="_blank"
          rel="noopener noreferrer"
          className="link"
        >
          performance log
        </a>
        .
      </>
    ),
  },
  {
    id: 'platforms',
    question: 'What about macOS and Linux?',
    answer: (
      <>
        Windows 10 and 11 x64 is first. The macOS Vision adapter compiles and
        passes a smoke test on a CI runner, but it has not been measured on
        Apple hardware. Linux uses the ONNX fallback and is source only. Neither
        has a downloadable build.
      </>
    ),
  },
  {
    id: 'license',
    question: 'What does it cost, and under what license?',
    answer: (
      <>
        Free, under the MIT license. No subscription, no account, no paid tier.
      </>
    ),
  },
];

export const FAQSection: React.FC = () => {
  const [openId, setOpenId] = useState<string | null>('download');

  return (
    <Section id="faq" labelledBy="faq-title">
      <div className="grid gap-10 md:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] md:gap-16">
        <div className="md:sticky md:top-24 md:self-start">
          <h2
            id="faq-title"
            className="font-serif text-[clamp(1.85rem,1.15rem+2.15vw,3rem)] font-bold leading-[1.08] tracking-tighter text-ink-primary"
          >
            Questions worth asking.
          </h2>
          <p className="mt-4 max-w-[38ch] text-[15px] leading-relaxed text-ink-secondary">
            Short answers, including the ones that are not flattering. If
            something is missing, open an issue.
          </p>
          <a
            href="https://github.com/rishabhx29/tessera/issues"
            target="_blank"
            rel="noopener noreferrer"
            className="hairline-link mt-6"
          >
            Ask on GitHub
          </a>
        </div>

        <div className="border-t border-line">
          {FAQ_ITEMS.map((item) => {
            const isOpen = openId === item.id;
            const panelId = `faq-panel-${item.id}`;
            const buttonId = `faq-button-${item.id}`;

            return (
              <div key={item.id} className="border-b border-line">
                <h3>
                  <button
                    type="button"
                    id={buttonId}
                    aria-expanded={isOpen}
                    aria-controls={panelId}
                    onClick={() => setOpenId(isOpen ? null : item.id)}
                    className="group flex w-full items-center justify-between gap-6 py-5 text-left"
                  >
                    <span
                      className={`text-[15px] font-medium leading-snug transition-colors duration-150 ${
                        isOpen ? 'text-ink-primary' : 'text-ink-secondary'
                      }`}
                    >
                      {item.question}
                    </span>
                    <Plus
                      className={`h-4 w-4 shrink-0 text-ink-muted transition-transform duration-150 ${
                        isOpen ? 'rotate-45 text-accent' : ''
                      }`}
                      aria-hidden="true"
                    />
                  </button>
                </h3>
                <div
                  id={panelId}
                  role="region"
                  aria-labelledby={buttonId}
                  hidden={!isOpen}
                  className="pb-5 pr-8"
                >
                  <p className="max-w-[62ch] text-[14px] leading-relaxed text-ink-secondary">
                    {item.answer}
                  </p>
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </Section>
  );
};
