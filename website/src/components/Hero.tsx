import React from 'react';
import {
  ArrowUpRight,
  FileText,
  Image as ImageIcon,
  Play,
  Search,
} from 'lucide-react';

const RELEASES_URL = 'https://github.com/rishabhx29/tessera/releases';

const RESULTS = [
  {
    kind: 'Screenshot',
    arm: 'OCR text',
    file: 'Screenshot 2026-10-11 142743.png',
    snippet: (
      <>
        Invoice <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">QF-3391</mark>
        {' '}· Airline travel booking confirmed
      </>
    ),
    icon: ImageIcon,
  },
  {
    kind: 'PDF',
    arm: 'Page 3',
    file: 'Travel_Expense_Policy_2026.pdf',
    snippet: (
      <>
        Employees must attach the original <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">airline invoice</mark>
        {' '}within 14 days
      </>
    ),
    icon: FileText,
  },
  {
    kind: 'Word',
    arm: 'Paragraph 4',
    file: 'October_Expense_Claim_Final.docx',
    snippet: (
      <>
        Airline <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">invoice</mark>
        {' '}attached as a digital voucher
      </>
    ),
    icon: FileText,
  },
];

const SearchSpecimen: React.FC = () => (
  <figure className="relative">
    <div className="overflow-hidden rounded-xl border border-line-strong bg-surface shadow-elevated">
      <div className="flex items-center justify-between gap-3 border-b border-line bg-subtle px-4 py-2.5">
        <span className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
          Result grid
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface px-2 py-0.5 font-mono text-[10px] text-ink-muted">
          <span className="h-1 w-1 rounded-full bg-accent" aria-hidden="true" />
          Illustration, not a screenshot
        </span>
      </div>

      <div className="p-4 sm:p-5">
        <div className="flex items-center gap-3 rounded-lg border border-line-strong bg-canvas px-3.5 py-3">
          <Search className="h-4 w-4 shrink-0 text-ink-muted" aria-hidden="true" />
          <span className="min-w-0 flex-1 truncate text-[13px] text-ink-primary sm:text-sm">
            the invoice from the airline
          </span>
          <span className="hidden items-center gap-1 sm:flex">
            <kbd className="kbd">Ctrl</kbd>
            <kbd className="kbd">Alt</kbd>
            <kbd className="kbd">Space</kbd>
          </span>
        </div>

        <ul className="mt-4 divide-y divide-line border-y border-line">
          {RESULTS.map((result) => {
            const Icon = result.icon;
            return (
              <li key={result.file} className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 py-3">
                <span className="mt-0.5 flex h-7 w-7 items-center justify-center rounded-md border border-line bg-subtle text-ink-secondary">
                  <Icon className="h-3.5 w-3.5" aria-hidden="true" />
                </span>
                <div className="min-w-0">
                  <div className="flex items-baseline justify-between gap-3">
                    <span className="truncate font-mono text-[11px] text-ink-secondary">
                      {result.file}
                    </span>
                    <span className="shrink-0 font-mono text-[10px] text-ink-faint">
                      {result.kind} · {result.arm}
                    </span>
                  </div>
                  <p className="mt-1 text-[13px] leading-snug text-ink-secondary">
                    {result.snippet}
                  </p>
                </div>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="border-t border-line bg-canvas px-4 py-2.5 text-[11px] leading-relaxed text-ink-muted">
        One query, three formats, one ranking. This grid is an illustration of the
        result view, not app footage.
      </div>
    </div>
    <figcaption className="sr-only">
      Illustration of a Tessera result grid. One query for an airline invoice
      returns a screenshot, a PDF page and a Word paragraph in the same list.
    </figcaption>
  </figure>
);

export const Hero: React.FC = () => (
  <section id="top" className="border-b border-line">
    <div className="mx-auto w-full max-w-6xl border-x border-line px-5 sm:px-8 lg:px-12">
      <div className="grid gap-12 py-14 md:py-20 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] lg:items-center lg:gap-14">
        <div>
          <p className="flex items-center gap-2 font-mono text-[11px] text-ink-muted">
            <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
            Windows first. The first public release is being prepared.
          </p>

          <h1 className="mt-5 font-serif text-[clamp(2.5rem,1.4rem+3.9vw,4.35rem)] font-bold leading-[1.03] tracking-tightest text-ink-primary">
            One search bar for everything your PC{' '}
            <span className="italic">can&apos;t search</span>.
          </h1>

          <p className="mt-6 max-w-[40ch] text-[16px] leading-relaxed text-ink-secondary sm:text-[17px]">
            Screenshots, images, PDFs and Word documents in one result grid.
            Tessera runs on your machine, keeps its index on your disk, and makes
            no network calls when you search.
          </p>

          <div className="mt-7 flex flex-col gap-2.5 sm:flex-row">
            <a href="#demo" className="btn btn-primary">
              <Play className="h-3.5 w-3.5 fill-current" aria-hidden="true" />
              Watch the demo
            </a>
            <a
              href={RELEASES_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-secondary"
            >
              Releases on GitHub
              <ArrowUpRight className="h-4 w-4 text-ink-muted" aria-hidden="true" />
            </a>
          </div>

          <p className="mt-5 max-w-[42ch] text-[12px] leading-relaxed text-ink-muted">
            No public release yet. The Windows installer is built and tested
            locally; publishing to GitHub Releases is next. MIT licensed.
          </p>
        </div>

        <SearchSpecimen />
      </div>
    </div>
  </section>
);
