import React, { useEffect, useMemo, useRef, useState } from 'react';
import { FileText, Image as ImageIcon, Search, X } from 'lucide-react';
import { Section, SectionHead } from './ui';

type RowType = 'image' | 'pdf' | 'docx' | 'txt';

interface Row {
  file: string;
  type: RowType;
  thumb?: string;
  detail: string;
  snippet: string;
  arms: readonly string[];
  keywords: string;
}

const ROWS: readonly Row[] = [
  {
    file: '12-the-brown-dog-waiting-at-the-gate.jpg',
    type: 'image',
    thumb: '/media/corpus/dog.jpg',
    detail: 'Pictures / Photos · 412 KB',
    snippet: 'the brown dog waiting at the gate',
    arms: ['image', 'keyword'],
    keywords: 'dog grass garden pet animal',
  },
  {
    file: '15-a-white-van-parked-outside-a-bakery.jpg',
    type: 'image',
    thumb: '/media/corpus/van.jpg',
    detail: 'Pictures / Photos · 388 KB',
    snippet: 'a white van parked outside a bakery',
    arms: ['image', 'keyword'],
    keywords: 'van vehicle white street',
  },
  {
    file: '07-an-orange-cat-asleep-on-a-radiator.jpg',
    type: 'image',
    thumb: '/media/corpus/cat.jpg',
    detail: 'Pictures / Photos · 356 KB',
    snippet: 'an orange cat asleep on a radiator',
    arms: ['image', 'keyword'],
    keywords: 'cat orange pet radiator',
  },
  {
    file: '01-a-red-bicycle-leaning-on-a-brick-wall.jpg',
    type: 'image',
    thumb: '/media/corpus/bicycle.jpg',
    detail: 'Pictures / Photos · 430 KB',
    snippet: 'a red bicycle leaning on a brick wall',
    arms: ['image', 'keyword'],
    keywords: 'bicycle red bike wall',
  },
  {
    file: '05-a-silver-train-crossing-a-stone-bridge.jpg',
    type: 'image',
    thumb: '/media/corpus/train.jpg',
    detail: 'Pictures / Photos · 447 KB',
    snippet: 'a silver train crossing a stone bridge',
    arms: ['image', 'keyword'],
    keywords: 'train silver bridge railway',
  },
  {
    file: 'Travel_Expense_Policy_2026.pdf',
    type: 'pdf',
    detail: 'Documents / Policy · page 3 of 18',
    snippet: 'Attach the original airline invoice and boarding receipts within 14 days.',
    arms: ['meaning', 'keyword'],
    keywords: 'invoice airline travel policy expenses receipts',
  },
  {
    file: 'October_Expense_Claim_Final.docx',
    type: 'docx',
    detail: 'Documents / Claims · paragraph 4',
    snippet: 'Airline invoice attached as a digital voucher. Flights to the summit.',
    arms: ['keyword'],
    keywords: 'invoice airline claim expenses flight',
  },
  {
    file: 'Board_Meeting_Deck_Q3.pdf',
    type: 'pdf',
    detail: 'Documents / Board · page 12 of 32',
    snippet: 'Consolidated quarterly revenue with red deficit callouts on operating cash flow.',
    arms: ['meaning', 'image'],
    keywords: 'q3 quarterly revenue red chart board deck deficit',
  },
  {
    file: 'Q3_Financial_Analysis_Memo.docx',
    type: 'docx',
    detail: 'Documents / Finance · paragraph 2',
    snippet: 'Q3 budget update: the red waterfall chart shows unexpected hardware amortisation.',
    arms: ['keyword', 'meaning'],
    keywords: 'q3 budget red waterfall chart finance',
  },
  {
    file: 'lease-agreement.txt',
    type: 'txt',
    detail: 'Text · 2 KB',
    snippet: 'Rent is due on the first of each month. The deposit is held in escrow.',
    arms: ['keyword'],
    keywords: 'lease rent deposit apartment',
  },
];

const QUERIES = [
  'a dog on grass',
  'the white van',
  'invoice from the airline',
  'Q3 budget',
] as const;

const STOPWORDS = new Set(['the', 'a', 'an', 'on', 'from', 'of', 'in', 'at', 'and', 'to']);

const tokensFor = (query: string): string[] =>
  query
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length > 1 && !STOPWORDS.has(token));

const typeLabel: Record<RowType, string> = {
  image: 'Photo',
  pdf: 'PDF',
  docx: 'Word',
  txt: 'Text',
};

const Highlighted: React.FC<{ text: string; tokens: string[] }> = ({ text, tokens }) => {
  if (!tokens.length) return <>{text}</>;
  const pattern = new RegExp(`(${tokens.map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|')})`, 'gi');
  const parts = text.split(pattern);
  return (
    <>
      {parts.map((part, index) =>
        tokens.includes(part.toLowerCase()) ? (
          <mark key={index} className="bg-accent/15 text-accent-deep">
            {part}
          </mark>
        ) : (
          <React.Fragment key={index}>{part}</React.Fragment>
        ),
      )}
    </>
  );
};

export const Demo: React.FC = () => {
  const [query, setQuery] = useState<string>('');
  const [hasMore, setHasMore] = useState(false);
  const listRef = useRef<HTMLUListElement>(null);

  const tokens = useMemo(() => tokensFor(query), [query]);
  const rows = useMemo(
    () =>
      tokens.length === 0
        ? ROWS
        : ROWS.filter((row) => {
            const haystack = `${row.file} ${row.snippet} ${row.keywords}`.toLowerCase();
            return tokens.every((token) => haystack.includes(token));
          }),
    [tokens],
  );

  // The list is capped in height, so say so with a fade instead of ending on a
  // half row. It only shows when there is actually more below.
  useEffect(() => {
    const el = listRef.current;
    if (!el) return;
    const update = () => {
      setHasMore(el.scrollHeight - el.scrollTop - el.clientHeight > 4);
    };
    update();
    el.addEventListener('scroll', update, { passive: true });
    const observer = new ResizeObserver(update);
    observer.observe(el);
    return () => {
      el.removeEventListener('scroll', update);
      observer.disconnect();
    };
  }, [rows.length]);

  return (
    <Section id="demo" labelledBy="demo-title">
      <div data-reveal>
        <SectionHead
          id="demo-title"
          size="large"
          title={
            <>
              Type the words
              <br />
              you actually remember.
            </>
          }
          lede="A colour. An object. A line from the middle of a page. Tessera searches the text it read and the picture it saw, then merges the two ranked lists into one."
          aside={
            <p className="font-mono text-[11px] uppercase tracking-[0.1em] text-ink-muted">
              A fixed sample, filtered in your browser
            </p>
          }
        />
      </div>

      <div className="mt-14 grid gap-10 md:mt-20 lg:grid-cols-[minmax(0,0.72fr)_minmax(0,1.28fr)] lg:gap-14">
        <div className="space-y-8" data-reveal>
          <p className="font-display text-[1.6rem] leading-[1.25] text-ink sm:text-[1.85rem]">
            The search bar is the whole product. Everything else is there to make
            one box trustworthy.
          </p>

          <ul className="space-y-4">
            {[
              ['Search by what you saw', 'Describe the picture, or type the words that were inside it. Both arms run.'],
              ['Summon it anywhere', 'The app binds Ctrl+Alt+Space globally, so the box arrives over whatever you are doing.'],
              ['Keyboard before mouse', 'Arrows move, Enter opens, Escape leaves. Every row says which arm found it.'],
            ].map(([title, body]) => (
              <li key={title} className="border-t border-line pt-4">
                <p className="text-[15px] font-medium text-ink">{title}</p>
                <p className="mt-1.5 text-[14px] leading-relaxed text-ink-soft">{body}</p>
              </li>
            ))}
          </ul>

          <div className="rounded-lg border border-line bg-paper-card p-5 text-[12px]">
            <p className="kicker">This demo, next to the app</p>
            <dl className="mt-4 space-y-2.5">
              {[
                ['Corpus', 'Ten sample files', 'Your own folders'],
                ['Runs', 'In this page', 'Local engine, no network'],
                ['Speed', 'Not measured', '65 ms p50 at 5,000 files'],
                ['Opens files', 'No', 'Yes, or reveals in folder'],
              ].map(([label, demo, app]) => (
                <div key={label} className="grid grid-cols-[4.5rem_minmax(0,1fr)_minmax(0,1fr)] gap-2">
                  <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-muted">
                    {label}
                  </dt>
                  <dd className="text-ink-muted">{demo}</dd>
                  <dd className="text-ink">{app}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>

        <div className="flex flex-col overflow-hidden rounded-xl border border-line-strong bg-paper-card shadow-lift">
          <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-3">
            <span className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted">
              <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
              Sample search
            </span>
            <span className="font-mono text-[10px] text-ink-muted">10 files</span>
          </div>

          <div className="border-b border-line bg-paper px-4 py-4">
            <label htmlFor="demo-query" className="sr-only">
              Sample search query
            </label>
            <div className="relative">
              <Search
                className="pointer-events-none absolute left-3.5 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-muted"
                aria-hidden="true"
              />
              <input
                id="demo-query"
                type="text"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Describe a file, or the words inside it"
                autoComplete="off"
                spellCheck={false}
                className="field pl-10 pr-10"
              />
              {query && (
                <button
                  type="button"
                  onClick={() => setQuery('')}
                  aria-label="Clear the sample query"
                  className="absolute right-2 top-1/2 flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded text-ink-muted transition-colors duration-150 hover:text-ink"
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                </button>
              )}
            </div>

            <div className="mt-3 flex flex-wrap gap-1.5">
              {QUERIES.map((sample) => (
                <button
                  key={sample}
                  type="button"
                  onClick={() => setQuery(sample)}
                  aria-pressed={query === sample}
                  className={`flex min-h-8 items-center rounded-sm border px-2 py-1 text-left text-[11px] transition-colors duration-150 ${
                    query === sample
                      ? 'border-ink bg-ink text-paper-card'
                      : 'border-line bg-paper-card text-ink-soft hover:border-line-strong hover:text-ink'
                  }`}
                >
                  {sample}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center justify-between border-b border-line px-4 py-2 font-mono text-[10px] text-ink-muted">
            <span aria-live="polite">
              {rows.length} {rows.length === 1 ? 'result' : 'results'}
            </span>
            <span>ranked by weighted RRF</span>
          </div>

          <div className="relative flex min-h-0 flex-1 flex-col">
          <ul
            ref={listRef}
            className="max-h-[30rem] flex-1 overflow-y-auto"
            data-lenis-prevent
          >
            {rows.map((row) => (
              <li key={row.file} className="border-b border-line px-4 py-4 last:border-b-0">
                <div className="grid grid-cols-[3.5rem_minmax(0,1fr)] gap-3.5 sm:grid-cols-[4rem_minmax(0,1fr)]">
                  {row.thumb ? (
                    <img
                      src={row.thumb}
                      alt=""
                      width={64}
                      height={64}
                      loading="lazy"
                      className="h-14 w-14 rounded border border-line object-cover sm:h-16 sm:w-16"
                    />
                  ) : (
                    <span className="flex h-14 w-14 items-center justify-center rounded border border-line bg-paper text-ink-muted sm:h-16 sm:w-16">
                      <FileText className="h-5 w-5" aria-hidden="true" />
                    </span>
                  )}

                  <div className="min-w-0">
                    <div className="flex items-start justify-between gap-3">
                      <p className="min-w-0 truncate font-mono text-[11.5px] text-ink">{row.file}</p>
                      <span className="hidden shrink-0 gap-1 sm:flex">
                        {row.arms.map((arm) => (
                          <span
                            key={arm}
                            className="rounded-sm border border-line bg-paper px-1.5 py-0.5 font-mono text-[9px] text-ink-muted"
                          >
                            {arm}
                          </span>
                        ))}
                      </span>
                    </div>
                    <p className="mt-1 flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.06em] text-ink-muted">
                      {row.type === 'image' ? (
                        <ImageIcon className="h-3 w-3" aria-hidden="true" />
                      ) : (
                        <FileText className="h-3 w-3" aria-hidden="true" />
                      )}
                      {typeLabel[row.type]} · {row.detail}
                    </p>
                    <p className="mt-1.5 text-[13.5px] leading-relaxed text-ink-soft">
                      <Highlighted text={row.snippet} tokens={tokens} />
                    </p>
                    <p className="mt-1 flex gap-1 sm:hidden">
                      {row.arms.map((arm) => (
                        <span
                          key={arm}
                          className="rounded-sm border border-line px-1.5 py-0.5 font-mono text-[9px] text-ink-muted"
                        >
                          {arm}
                        </span>
                      ))}
                    </p>
                  </div>
                </div>
              </li>
            ))}

            {rows.length === 0 && (
              <li className="px-5 py-12 text-center">
                <p className="font-display text-[1.5rem] text-ink">Nothing in this sample.</p>
                <p className="mx-auto mt-2 max-w-[34ch] text-[13px] leading-relaxed text-ink-muted">
                  The real app answers from your folders, and says when it has no
                  match instead of inventing one.
                </p>
              </li>
            )}
          </ul>
          <div
            aria-hidden="true"
            className={`pointer-events-none absolute inset-x-0 bottom-0 h-10 bg-gradient-to-t from-paper-card to-transparent transition-opacity duration-200 ${
              hasMore ? 'opacity-100' : 'opacity-0'
            }`}
          />
          </div>

          <p className="border-t border-line bg-paper px-4 py-3 text-[11px] leading-relaxed text-ink-muted">
            Ten fixed sample files: five photos and five documents. Type to
            filter them, or pick a query. No engine, no network, and no timings
            are simulated here.
          </p>
        </div>
      </div>
    </Section>
  );
};
