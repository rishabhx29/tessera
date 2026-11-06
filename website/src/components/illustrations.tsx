import React from 'react';
import { FileText, Image as ImageIcon } from 'lucide-react';

const IllusFrame: React.FC<{
  label: string;
  children: React.ReactNode;
  footer?: string;
  dark?: boolean;
}> = ({ label, children, footer, dark = false }) => (
  <div
    className={`overflow-hidden rounded-lg border ${
      dark ? 'border-night-line bg-night' : 'border-line bg-paper-card'
    } shadow-lift`}
  >
    <div
      className={`flex items-center justify-between border-b px-4 py-2.5 ${
        dark ? 'border-night-line' : 'border-line'
      }`}
    >
      <span
        className={`font-mono text-[10px] uppercase tracking-[0.12em] ${
          dark ? 'text-night-muted' : 'text-ink-muted'
        }`}
      >
        {label}
      </span>
      <span className="flex gap-1" aria-hidden="true">
        <span className={`h-1.5 w-1.5 rounded-full ${dark ? 'bg-night-line' : 'bg-line-strong'}`} />
        <span className={`h-1.5 w-1.5 rounded-full ${dark ? 'bg-night-line' : 'bg-line-strong'}`} />
        <span className={`h-1.5 w-1.5 rounded-full ${dark ? 'bg-night-line' : 'bg-line-strong'}`} />
      </span>
    </div>
    <div className="p-4 sm:p-5">{children}</div>
    {footer && (
      <div
        className={`border-t px-4 py-2.5 text-[11px] ${
          dark ? 'border-night-line text-night-muted' : 'border-line text-ink-muted'
        }`}
      >
        {footer}
      </div>
    )}
  </div>
);

export const WatchIllus: React.FC = () => {
  const rows = [
    { name: 'Screenshot 2026-11-05 094218.png', state: 'queued', hot: true },
    { name: 'Invoice_March.pdf', state: 'reading', hot: false },
    { name: 'meeting-notes.md', state: 'indexed', hot: false },
    { name: 'travel-policy.docx', state: 'indexed', hot: false },
  ];

  return (
    <IllusFrame label="Pictures / Screenshots" footer="Newest first. Up to four workers, idle priority.">
      <div className="space-y-2">
        {rows.map((row) => {
          const Icon = /\.(png|jpe?g|webp|bmp|gif|tiff?)$/i.test(row.name) ? ImageIcon : FileText;
          return (
            <div
              key={row.name}
              className={`flex items-center justify-between gap-3 rounded-md border px-3 py-2.5 ${
                row.hot ? 'border-accent/40 bg-accent-soft/60' : 'border-line bg-paper'
              }`}
            >
              <span className="flex min-w-0 items-center gap-2.5">
                <Icon
                  className={`h-4 w-4 shrink-0 ${row.hot ? 'text-accent' : 'text-ink-muted'}`}
                  aria-hidden="true"
                />
                <span className="truncate font-mono text-[11px] text-ink-soft">{row.name}</span>
              </span>
              <span
                className={`shrink-0 rounded-sm px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.08em] ${
                  row.state === 'queued'
                    ? 'bg-accent text-paper-card'
                    : row.state === 'reading'
                      ? 'border border-line-strong text-ink-soft'
                      : 'text-ink-muted'
                }`}
              >
                {row.state}
              </span>
            </div>
          );
        })}
      </div>
      <div className="mt-4 flex items-center justify-between border-t border-line pt-3 font-mono text-[10px] text-ink-muted">
        <span>4 in queue</span>
        <span>paused: no</span>
      </div>
    </IllusFrame>
  );
};

export const ReadIllus: React.FC = () => (
  <IllusFrame label="OCR pass" footer="Windows.Media.Ocr · 34.4 ms p50 · page kept">
    <div className="relative overflow-hidden rounded-md border border-line bg-paper p-4">
      <span
        aria-hidden="true"
        className="scan-line pointer-events-none absolute inset-x-0 top-0 h-10 bg-gradient-to-b from-transparent via-accent/15 to-transparent"
      />
      <div className="flex items-center justify-between border-b border-line pb-3">
        <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
          Airline receipt
        </span>
        <span className="font-mono text-[10px] text-ink-muted">14:32</span>
      </div>
      <div className="mt-3 space-y-2 text-[13px] leading-relaxed text-ink-soft">
        <p>Invoice QF-3391, airline travel booking confirmed.</p>
        <p className="rounded-sm bg-accent/12 px-1 text-ink">
          Passenger: A. Rao · Fare: AUD 1,248.50
        </p>
        <p>Paid on Visa ending 4012. Seat 22A.</p>
      </div>
      <div className="mt-4 flex items-center justify-between border-t border-line pt-3">
        <span className="rounded-sm border border-sage/30 bg-sage/10 px-1.5 py-0.5 font-mono text-[9px] uppercase tracking-[0.08em] text-sage-deep">
          text extracted
        </span>
        <span className="font-mono text-[10px] text-ink-muted">188 chars</span>
      </div>
    </div>
  </IllusFrame>
);

const BAR_HEIGHTS = [10, 18, 26, 16, 34, 22, 12, 30, 20, 14, 28, 18];

export const EmbedIllus: React.FC = () => (
  <IllusFrame label="Vector space" footer="Model versions pinned in the index meta table">
    <div className="grid gap-4 sm:grid-cols-2">
      <div className="rounded-md border border-line bg-paper p-3">
        <div className="flex items-baseline justify-between">
          <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
            SigLIP base
          </span>
          <span className="font-mono text-[11px] text-sage">768-d</span>
        </div>
        <div className="mt-3 flex h-12 items-end gap-1" aria-hidden="true">
          {BAR_HEIGHTS.map((height, index) => (
            <span
              key={index}
              className="w-full rounded-[1px] bg-sage/70"
              style={{ height: `${height + 8}px` }}
            />
          ))}
        </div>
      </div>
      <div className="rounded-md border border-line bg-paper p-3">
        <div className="flex items-baseline justify-between">
          <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
            MiniLM-L6
          </span>
          <span className="font-mono text-[11px] text-slate">384-d</span>
        </div>
        <div className="mt-3 flex h-12 items-end gap-1" aria-hidden="true">
          {[...BAR_HEIGHTS].reverse().map((height, index) => (
            <span
              key={index}
              className="w-full rounded-[1px] bg-slate/60"
              style={{ height: `${height + 8}px` }}
            />
          ))}
        </div>
      </div>
    </div>
    <div className="mt-4 flex items-center gap-3 border-t border-line pt-3">
      <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
        one index
      </span>
      <span className="h-px flex-1 bg-line" aria-hidden="true" />
      <span className="font-mono text-[10px] text-ink">index.sqlite</span>
    </div>
  </IllusFrame>
);

const RANKS = [
  ['Keyword', 'invoice', 'screenshot'],
  ['Image', 'dog', 'IMG_4471'],
  ['Meaning', 'receipt', 'policy.pdf'],
] as const;

export const FuseIllus: React.FC = () => (
  <IllusFrame label="Reciprocal rank fusion" footer="Weighted by arm. Every hit states why it surfaced.">
    <div className="space-y-2">
      {RANKS.map(([arm, query, file], index) => (
        <div key={arm} className="flex items-center gap-3 text-[11px]">
          <span
            className={`w-16 shrink-0 font-mono text-[10px] uppercase tracking-[0.08em] ${
              index === 0 ? 'text-accent' : index === 1 ? 'text-sage' : 'text-slate'
            }`}
          >
            {arm}
          </span>
          <span className="flex-1 truncate rounded-sm border border-line bg-paper px-2 py-1.5 font-mono text-ink-soft">
            {query}
          </span>
          <span className="hidden w-28 truncate font-mono text-[10px] text-ink-muted sm:block">
            {file}
          </span>
        </div>
      ))}
    </div>
    <div className="mt-4 flex items-center gap-2 border-t border-line pt-3">
      <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">merge</span>
      <span className="h-px flex-1 bg-line" aria-hidden="true" />
      <span className="font-mono text-[10px] text-accent">RRF k=60</span>
    </div>
    <div className="mt-3 rounded-md border border-accent/30 bg-accent-soft/50 px-3 py-2.5">
      <div className="flex items-center justify-between gap-3">
        <span className="truncate font-mono text-[11px] text-ink">Screenshot 2026-10-11 142743.png</span>
        <span className="shrink-0 font-mono text-[10px] text-accent-deep">0.0328</span>
      </div>
      <p className="mt-1 text-[11px] text-ink-soft">keyword + image · invoice</p>
    </div>
  </IllusFrame>
);

export const AnswerIllus: React.FC = () => (
  <IllusFrame dark label="Tessera · results" footer="Keyboard first: arrows, Enter, Escape.">
    <div className="flex items-center gap-2 rounded-md border border-accent/40 bg-night-soft px-3 py-2.5">
      <ImageIcon className="h-3.5 w-3.5 text-night-muted" aria-hidden="true" />
      <span className="flex-1 truncate text-[12px] text-night-ink">the invoice from the airline</span>
      <span className="rounded-sm border border-night-line px-1 py-0.5 font-mono text-[9px] text-night-muted">
        Ctrl K
      </span>
    </div>
    <p className="mt-2 px-1 font-mono text-[10px] text-night-muted">
      3 results · 66 ms engine · keyword + image + meaning
    </p>
    <div className="mt-3 space-y-1.5">
      <div className="flex items-center gap-3 rounded-md border border-night-line bg-night-soft px-3 py-2.5">
        <span className="h-8 w-8 shrink-0 rounded-sm border border-night-line bg-gradient-to-br from-[#3b3a32] to-[#22211c]" aria-hidden="true" />
        <span className="min-w-0 flex-1">
          <span className="block truncate font-mono text-[10px] text-night-ink">
            Screenshot 2026-10-11 142743.png
          </span>
          <span className="block truncate text-[11px] text-night-muted">
            …the invoice from the airline, total due AUD 1,248.50…
          </span>
        </span>
        <span className="hidden gap-1 sm:flex">
          <span className="rounded-sm border border-night-line px-1 py-0.5 font-mono text-[8px] text-night-muted">keyword</span>
          <span className="rounded-sm border border-night-line px-1 py-0.5 font-mono text-[8px] text-night-muted">image</span>
        </span>
      </div>
      <div className="flex items-center gap-3 rounded-md px-3 py-2.5">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm border border-night-line text-night-muted">
          <FileText className="h-3.5 w-3.5" aria-hidden="true" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate font-mono text-[10px] text-night-ink">
            Travel_Expense_Policy_2026.pdf
          </span>
          <span className="block truncate text-[11px] text-night-muted">
            Page 3 · attach the original airline invoice within 14 days
          </span>
        </span>
        <span className="hidden rounded-sm border border-night-line px-1 py-0.5 font-mono text-[8px] text-night-muted sm:inline">
          meaning
        </span>
      </div>
      <div className="flex items-center gap-3 rounded-md px-3 py-2.5">
        <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-sm border border-night-line text-night-muted">
          <FileText className="h-3.5 w-3.5" aria-hidden="true" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate font-mono text-[10px] text-night-ink">
            October_Expense_Claim_Final.docx
          </span>
          <span className="block truncate text-[11px] text-night-muted">
            Paragraph 4 · airline invoice attached as a digital voucher
          </span>
        </span>
        <span className="hidden rounded-sm border border-night-line px-1 py-0.5 font-mono text-[8px] text-night-muted sm:inline">
          keyword
        </span>
      </div>
    </div>
  </IllusFrame>
);

export const ILLUSTRATIONS: Record<string, React.FC> = {
  watch: WatchIllus,
  read: ReadIllus,
  embed: EmbedIllus,
  fuse: FuseIllus,
  answer: AnswerIllus,
};
