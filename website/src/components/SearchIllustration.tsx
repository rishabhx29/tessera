import React, { useState } from 'react';
import { FileText, Image as ImageIcon, Search } from 'lucide-react';

type ResultType = 'screenshot' | 'pdf' | 'word';

interface DemoResult {
  type: ResultType;
  arm: string;
  file: string;
  page?: number;
  snippet: React.ReactNode;
  preview: string;
}

interface Scenario {
  id: string;
  label: string;
  query: string;
  results: DemoResult[];
}

const SCENARIOS: Scenario[] = [
  {
    id: 'airline-invoice',
    label: 'the invoice from the airline',
    query: 'the invoice from the airline',
    results: [
      {
        type: 'screenshot',
        arm: 'OCR text · keyword match',
        file: 'Screenshot 2026-10-11 142743.png',
        snippet: (
          <>
            Invoice{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              QF-3391
            </mark>
            , airline travel booking confirmed
          </>
        ),
        preview:
          'Qantas Airways. Total AUD 1,248.50, paid on Visa ending 4012.',
      },
      {
        type: 'pdf',
        arm: 'Page 3 · meaning match',
        file: 'Travel_Expense_Policy_2026.pdf',
        page: 3,
        snippet: (
          <>
            Attach the original{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              airline invoice
            </mark>{' '}
            and boarding receipts within 14 days
          </>
        ),
        preview:
          'Section 4.2, travel expenses. Page text was extracted, chunked and embedded.',
      },
      {
        type: 'word',
        arm: 'Paragraph 4 · keyword match',
        file: 'October_Expense_Claim_Final.docx',
        snippet: (
          <>
            Overseas flights for the summit. Airline{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              invoice
            </mark>{' '}
            attached as a digital voucher
          </>
        ),
        preview: 'Item 04, approved by the manager on 24 October.',
      },
    ],
  },
  {
    id: 'red-chart',
    label: 'the red chart from the board deck',
    query: 'the red chart from the board deck',
    results: [
      {
        type: 'screenshot',
        arm: 'SigLIP description',
        file: 'Screenshot 2026-09-30 180211.png',
        snippet: (
          <>
            Q3 financial review, bar chart with the operating deficit marked in{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              red
            </mark>
          </>
        ),
        preview: 'Matched by what the image looks like, with no text needed.',
      },
      {
        type: 'pdf',
        arm: 'Page 12 · meaning match',
        file: 'Board_Meeting_Deck_Q3.pdf',
        page: 12,
        snippet: (
          <>
            Consolidated{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              quarterly revenue
            </mark>{' '}
            with red deficit callouts
          </>
        ),
        preview: 'Slide 12, operating cash flow. Text layer from the PDF.',
      },
      {
        type: 'word',
        arm: 'Paragraph 2 · keyword match',
        file: 'Q3_Financial_Analysis_Memo.docx',
        snippet: (
          <>
            Figure 3, the red{' '}
            <mark className="bg-transparent font-medium text-ink-primary underline decoration-accent/50 decoration-2 underline-offset-2">
              waterfall chart
            </mark>{' '}
            shows unexpected hardware amortisation
          </>
        ),
        preview: 'Word paragraph extracted from document.xml.',
      },
    ],
  },
];

const TYPE_LABEL: Record<ResultType, string> = {
  screenshot: 'Screenshot',
  pdf: 'PDF',
  word: 'Word',
};

const TYPE_STYLE: Record<ResultType, string> = {
  screenshot: 'border-accent/30 bg-accent-subtle text-accent',
  pdf: 'border-line-strong bg-subtle text-ink-primary',
  word: 'border-line bg-surface text-ink-secondary',
};

const FILTERS: Array<{ id: 'all' | ResultType; label: string }> = [
  { id: 'all', label: 'All' },
  { id: 'screenshot', label: 'Screenshot' },
  { id: 'pdf', label: 'PDF' },
  { id: 'word', label: 'Word' },
];

export const SearchIllustration: React.FC = () => {
  const [scenario, setScenario] = useState(SCENARIOS[0]);
  const [query, setQuery] = useState(SCENARIOS[0].query);
  const [filter, setFilter] = useState<'all' | ResultType>('all');

  const selectScenario = (next: Scenario) => {
    setScenario(next);
    setQuery(next.query);
    setFilter('all');
  };

  const results =
    filter === 'all'
      ? scenario.results
      : scenario.results.filter((result) => result.type === filter);

  return (
    <div className="overflow-hidden rounded-xl border border-line-strong bg-surface shadow-tile">
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line bg-subtle px-4 py-2.5">
        <span className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
          Interactive illustration
        </span>
        <span className="inline-flex items-center gap-1.5 rounded-full border border-line bg-surface px-2 py-0.5 font-mono text-[10px] text-ink-muted">
          <span className="h-1 w-1 rounded-full bg-accent" aria-hidden="true" />
          Not a screenshot
        </span>
      </div>

      <div className="border-b border-line bg-canvas p-4 sm:p-5">
        <label htmlFor="illustration-query" className="sr-only">
          Search query for the illustration
        </label>
        <div className="relative">
          <Search
            className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-muted"
            aria-hidden="true"
          />
          <input
            id="illustration-query"
            type="text"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="field pl-10 pr-4 sm:pr-28"
            placeholder="Describe a file or type the words you remember"
            autoComplete="off"
            spellCheck={false}
          />
          <span className="absolute right-3 top-1/2 hidden -translate-y-1/2 items-center gap-1 sm:flex">
            <kbd className="kbd">Ctrl</kbd>
            <kbd className="kbd">Alt</kbd>
            <kbd className="kbd">Space</kbd>
          </span>
        </div>

        <div className="mt-3 flex flex-wrap items-center gap-2">
          <span className="font-mono text-[11px] text-ink-muted">Try:</span>
          {SCENARIOS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => selectScenario(item)}
              aria-pressed={scenario.id === item.id}
              className={`flex min-h-9 items-center rounded-md border px-2.5 text-left text-[11px] transition-colors duration-150 ${
                scenario.id === item.id
                  ? 'border-ink-primary bg-ink-primary text-canvas'
                  : 'border-line bg-surface text-ink-secondary hover:border-line-strong hover:text-ink-primary'
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line bg-subtle/60 px-4 py-2">
        <div className="flex items-center gap-1" aria-label="Filter by format">
          {FILTERS.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setFilter(item.id)}
              aria-pressed={filter === item.id}
              className={`flex min-h-8 items-center rounded px-2 font-mono text-[10px] transition-colors duration-150 ${
                filter === item.id
                  ? 'bg-surface font-semibold text-ink-primary shadow-sm ring-1 ring-line'
                  : 'text-ink-muted hover:text-ink-primary'
              }`}
            >
              {item.label}
            </button>
          ))}
        </div>
        <span className="hidden font-mono text-[10px] text-ink-muted sm:inline">
          Ranked by reciprocal rank fusion
        </span>
      </div>

      <div className="p-4 sm:p-5">
        <p className="sr-only" role="status">
          {results.length} {results.length === 1 ? 'illustration result' : 'illustration results'} for {query}
        </p>
        <ul
          key={`${scenario.id}-${filter}`}
          className="swap-in divide-y divide-line border-y border-line"
        >
          {results.map((result) => {
            const Icon = result.type === 'screenshot' ? ImageIcon : FileText;
            return (
              <li
                key={`${result.file}-${result.page ?? 0}`}
                className="grid grid-cols-[auto_minmax(0,1fr)] gap-3 py-3.5 first:pt-0 last:pb-0"
              >
                <span className="mt-0.5 flex h-7 w-7 items-center justify-center rounded-md border border-line bg-subtle text-ink-secondary">
                  <Icon className="h-3.5 w-3.5" aria-hidden="true" />
                </span>
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span
                      className={`rounded border px-1.5 py-0.5 font-mono text-[10px] ${TYPE_STYLE[result.type]}`}
                    >
                      {TYPE_LABEL[result.type]}
                      {result.page ? ` · page ${result.page}` : ''}
                    </span>
                    <span className="font-mono text-[10px] text-ink-muted">
                      {result.arm}
                    </span>
                  </div>
                  <div className="mt-1.5 truncate font-mono text-[11px] text-ink-secondary">
                    {result.file}
                  </div>
                  <p className="mt-1.5 text-[13px] leading-relaxed text-ink-primary">
                    {result.snippet}
                  </p>
                  <p className="mt-1 text-[12px] leading-relaxed text-ink-muted">
                    {result.preview}
                  </p>
                </div>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="border-t border-line bg-canvas px-4 py-2.5 text-[11px] leading-relaxed text-ink-muted">
        The query bar is editable. The results are fixed for this illustration,
        so nothing here is a live index or a recording.
      </div>
    </div>
  );
};
