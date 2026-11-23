import React, { useEffect, useMemo, useRef, useState } from 'react';
import { FileText, Search, X } from 'lucide-react';
import { Section, SectionHead } from './ui';

type Lens = 'pictures' | 'words' | 'documents';
type DocType = 'pdf' | 'docx' | 'txt';

interface Picture {
  kind: 'picture';
  file: string;
  src: string;
  width: number;
  height: number;
  text?: string;
  reasons: string;
  keywords: string;
}

interface Doc {
  kind: 'document';
  file: string;
  type: DocType;
  page: string;
  folder: string;
  snippet: string;
  reasons: string;
  keywords: string;
}

type Item = Picture | Doc;

const PICTURES: readonly Picture[] = [
  {
    kind: 'picture',
    file: '12-the-brown-dog-waiting-at-the-gate.jpg',
    src: '/media/corpus/dog.jpg',
    width: 480,
    height: 480,
    reasons: 'looks like this',
    keywords: 'dog grass garden pet animal',
  },
  {
    kind: 'picture',
    file: '15-a-white-van-parked-outside-a-bakery.jpg',
    src: '/media/corpus/van.jpg',
    width: 480,
    height: 480,
    reasons: 'looks like this',
    keywords: 'van vehicle white street',
  },
  {
    kind: 'picture',
    file: '07-an-orange-cat-asleep-on-a-radiator.jpg',
    src: '/media/corpus/cat.jpg',
    width: 480,
    height: 480,
    reasons: 'looks like this',
    keywords: 'cat orange pet radiator',
  },
  {
    kind: 'picture',
    file: '01-a-red-bicycle-leaning-on-a-brick-wall.jpg',
    src: '/media/corpus/bicycle.jpg',
    width: 480,
    height: 480,
    reasons: 'looks like this',
    keywords: 'bicycle red bike wall',
  },
  {
    kind: 'picture',
    file: '05-a-silver-train-crossing-a-stone-bridge.jpg',
    src: '/media/corpus/train.jpg',
    width: 480,
    height: 480,
    reasons: 'looks like this',
    keywords: 'train silver bridge railway',
  },
  {
    kind: 'picture',
    file: '1904-dog-licence-stamp-13320792.jpg',
    src: '/media/corpus/licence.jpg',
    width: 800,
    height: 672,
    text: 'LICENSE TWO SHILLINGS',
    reasons: 'mentions this',
    keywords: 'licence license two shillings stamp dog tax 1904',
  },
  {
    kind: 'picture',
    file: 'brown-dog-11-march-1910-2-43312784.jpg',
    src: '/media/corpus/brown-dog.jpg',
    width: 800,
    height: 546,
    text: 'EXIT THE “BROWN DOG.”',
    reasons: 'mentions this',
    keywords: 'brown dog exit banner memorial london',
  },
  {
    kind: 'picture',
    file: '2024-11-16-ho-chi-minh-city-at-night-4-156974857.jpg',
    src: '/media/corpus/chill.jpg',
    width: 800,
    height: 534,
    text: 'CHILL • WHERE',
    reasons: 'mentions this',
    keywords: 'neon chill saigon night city sign',
  },
  {
    kind: 'picture',
    file: 'banks-street-mid-city-new-orleans-7-march-2020-k-101163602.jpg',
    src: '/media/corpus/banks-street.jpg',
    width: 800,
    height: 600,
    text: '504-482-5771',
    reasons: 'mentions this',
    keywords: 'banks street new orleans live music zitos phone',
  },
];

const DOCUMENTS: readonly Doc[] = [
  {
    kind: 'document',
    file: 'Travel_Expense_Policy_2026.pdf',
    type: 'pdf',
    page: 'page 3 of 18',
    folder: 'Policy',
    snippet: 'Attach the original airline invoice and boarding receipts within 14 days.',
    reasons: 'mentions this',
    keywords: 'invoice airline travel policy expenses receipts',
  },
  {
    kind: 'document',
    file: 'October_Expense_Claim_Final.docx',
    type: 'docx',
    page: 'paragraph 4',
    folder: 'Claims',
    snippet: 'Airline invoice attached as a digital voucher. Flights to the summit.',
    reasons: 'name matches',
    keywords: 'invoice airline claim expenses flight',
  },
  {
    kind: 'document',
    file: 'Board_Meeting_Deck_Q3.pdf',
    type: 'pdf',
    page: 'page 12 of 32',
    folder: 'Board',
    snippet: 'Consolidated quarterly revenue with red deficit callouts on operating cash flow.',
    reasons: 'mentions this',
    keywords: 'q3 quarterly revenue red chart board deck deficit',
  },
  {
    kind: 'document',
    file: 'Q3_Financial_Analysis_Memo.docx',
    type: 'docx',
    page: 'paragraph 2',
    folder: 'Finance',
    snippet: 'Q3 budget update: the red waterfall chart shows unexpected hardware amortisation.',
    reasons: 'mentions this',
    keywords: 'q3 budget red waterfall chart finance',
  },
  {
    kind: 'document',
    file: 'lease-agreement.txt',
    type: 'txt',
    page: '2 KB',
    folder: 'Home',
    snippet: 'Rent is due on the first of each month. The deposit is held in escrow.',
    reasons: 'name matches',
    keywords: 'lease rent deposit apartment',
  },
];

const ITEMS: readonly Item[] = [...PICTURES, ...DOCUMENTS];

const LENSES: readonly Lens[] = ['pictures', 'words', 'documents'];

const QUERIES = [
  'a dog on grass',
  'two shillings',
  'invoice from the airline',
  'Q3 budget',
] as const;

const STOPWORDS = new Set(['the', 'a', 'an', 'on', 'from', 'of', 'in', 'at', 'and', 'to']);

const tokensFor = (query: string): string[] =>
  query
    .toLowerCase()
    .split(/[^a-z0-9]+/)
    .filter((token) => token.length > 1 && !STOPWORDS.has(token));

const typeLabel: Record<DocType, string> = {
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

const Reason: React.FC<{ text: string }> = ({ text }) => (
  <span className="flex min-w-0 items-center gap-1.5 font-mono text-[9.5px] leading-snug text-accent-deep">
    <span className="h-1 w-1 shrink-0 bg-accent" aria-hidden="true" />
    <span className="truncate">{text}</span>
  </span>
);

const inLens = (item: Item, lens: Lens): boolean => {
  if (lens === 'pictures') return item.kind === 'picture';
  if (lens === 'words') return item.kind === 'picture' && Boolean(item.text);
  return item.kind === 'document';
};

const haystackFor = (item: Item): string =>
  `${item.file} ${item.kind === 'picture' ? item.text ?? '' : item.snippet} ${item.keywords}`.toLowerCase();

export const Demo: React.FC = () => {
  const [query, setQuery] = useState<string>('');
  const [lens, setLens] = useState<Lens>('pictures');
  const [hasMore, setHasMore] = useState(false);
  const listRef = useRef<HTMLDivElement>(null);

  const tokens = useMemo(() => tokensFor(query), [query]);
  const results = useMemo(
    () =>
      ITEMS.filter(
        (item) => inLens(item, lens) && tokens.every((token) => haystackFor(item).includes(token)),
      ),
    [lens, tokens],
  );
  const pictures = results.filter((item): item is Picture => item.kind === 'picture');
  const documents = results.filter((item): item is Doc => item.kind === 'document');
  const showingReasons = tokens.length > 0;

  // The sheet is capped in height, so say so with a fade instead of ending on a
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
  }, [results.length, lens]);

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
          lede="One box, three lenses. Type a few words, switch lens, and the sample answers instantly, with a reason on every result."
          aside={
            <p className="font-mono text-[11px] uppercase tracking-[0.1em] text-ink-muted">
              A fixed sample, filtered in your browser
            </p>
          }
        />
      </div>

      <div className="mt-14 grid gap-10 md:mt-20 lg:grid-cols-[minmax(0,0.58fr)_minmax(0,1.42fr)] lg:gap-16">
        <div className="space-y-8" data-reveal>
          <p className="font-display text-[1.6rem] leading-[1.25] text-ink sm:text-[1.85rem]">
            The search bar is the whole product. Everything else is there to make
            one box trustworthy.
          </p>

          <ul className="space-y-4">
            {[
              ['Summon it anywhere', 'Ctrl+Alt+T brings the box over whatever is on screen.'],
              ['Keyboard before mouse', 'Arrows move, Enter opens, Escape leaves. Every hit says why it is there.'],
            ].map(([title, body]) => (
              <li key={title} className="border-t border-line pt-4">
                <p className="text-[14.5px] font-medium text-ink">{title}</p>
                <p className="mt-1 text-[13.5px] leading-relaxed text-ink-soft">{body}</p>
              </li>
            ))}
          </ul>

          <p className="text-[12px] leading-relaxed text-ink-muted">
            Fourteen fixed sample files, filtered in your browser. The real app
            runs the same lenses on your own folders.
          </p>
        </div>

        <div className="flex flex-col overflow-hidden rounded-xl border border-line-strong bg-paper-card shadow-lift">
          <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3">
            <span className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted">
              <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
              Sample search
            </span>
            <div
              role="group"
              aria-label="Search scope"
              className="inline-flex items-center gap-0.5 rounded-full border border-line bg-paper p-0.5"
            >
              {LENSES.map((value) => (
                <button
                  key={value}
                  type="button"
                  onClick={() => setLens(value)}
                  aria-pressed={lens === value}
                  className={`flex h-6 items-center rounded-full px-2.5 text-[11px] transition-colors duration-150 ${
                    lens === value ? 'bg-ink text-paper-card' : 'text-ink-soft hover:text-ink'
                  }`}
                >
                  {value}
                </button>
              ))}
            </div>
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
            <span aria-live="polite">{results.length} shown</span>
            <span>scoped to {lens}</span>
          </div>

          <div className="relative flex min-h-0 flex-1 flex-col">
            <div
              ref={listRef}
              className="max-h-[30rem] flex-1 overflow-y-auto"
              data-lenis-prevent
            >
              {results.length === 0 && (
                <div className="px-5 py-12 text-center">
                  <p className="font-display text-[1.5rem] text-ink">
                    Nothing in this sample{query ? ` matches “${query}”` : ''}.
                  </p>
                  <p className="mx-auto mt-2 max-w-[36ch] text-[13px] leading-relaxed text-ink-muted">
                    The real app answers from your folders, and names the cause
                    when nothing matches instead of inventing a result.
                  </p>
                  <div className="mt-5 flex flex-wrap items-center justify-center gap-1.5">
                    {LENSES.filter((value) => value !== lens).map((value) => (
                      <button
                        key={value}
                        type="button"
                        onClick={() => setLens(value)}
                        className="flex min-h-8 items-center rounded-sm border border-line px-2.5 text-[11px] text-ink-soft transition-colors duration-150 hover:border-line-strong hover:text-ink"
                      >
                        try {value}
                      </button>
                    ))}
                  </div>
                </div>
              )}

              {pictures.length > 0 && lens !== 'words' && (
                <ul className="flex flex-wrap gap-1.5 px-4 pb-4 pt-4">
                  {pictures.map((picture) => (
                    <li key={picture.file} className="min-w-0 max-w-full">
                      <div
                        className="h-[96px] overflow-hidden rounded-[5px] border border-line bg-paper sm:h-[118px]"
                        style={{ aspectRatio: `${picture.width} / ${picture.height}` }}
                      >
                        <img
                          src={picture.src}
                          alt={picture.file}
                          width={picture.width}
                          height={picture.height}
                          loading="lazy"
                          className="h-full w-full object-cover"
                        />
                      </div>
                      {showingReasons && (
                        <div className="mt-1.5 max-w-full">
                          <Reason text={picture.reasons} />
                        </div>
                      )}
                    </li>
                  ))}
                </ul>
              )}

              {pictures.length > 0 && lens === 'words' && (
                <ul className="grid gap-4 px-4 pb-4 pt-4 sm:grid-cols-2">
                  {pictures.map((picture) => (
                    <li key={picture.file}>
                      <div className="aspect-[3/2] overflow-hidden rounded-[6px] border border-line bg-paper">
                        <img
                          src={picture.src}
                          alt={picture.file}
                          width={picture.width}
                          height={picture.height}
                          loading="lazy"
                          className="h-full w-full object-cover"
                        />
                      </div>
                      <div className="mt-2">
                        <p className="truncate text-[12.5px] font-medium text-ink">{picture.file}</p>
                        <p className="mt-0.5 font-mono text-[9px] uppercase tracking-[0.06em] text-ink-muted">
                          words in the picture
                        </p>
                        <p className="mt-1 font-mono text-[10.5px] leading-snug text-ink-soft">
                          <Highlighted text={picture.text ?? ''} tokens={tokens} />
                        </p>
                        {showingReasons && (
                          <div className="mt-1.5">
                            <Reason text={picture.reasons} />
                          </div>
                        )}
                      </div>
                    </li>
                  ))}
                </ul>
              )}

              {documents.length > 0 && (
                <ul className={pictures.length > 0 || lens === 'documents' ? 'border-t border-line' : ''}>
                  {documents.map((doc) => (
                    <li key={doc.file} className="border-b border-line px-4 py-3.5 last:border-b-0">
                      <div className="grid grid-cols-[20px_minmax(0,1fr)] gap-3">
                        <FileText className="mt-0.5 h-5 w-5 text-ink-muted" aria-hidden="true" />
                        <div className="min-w-0">
                          <div className="flex items-baseline gap-2">
                            <p className="min-w-0 truncate text-[13px] font-medium text-ink">{doc.file}</p>
                            <span className="shrink-0 rounded border border-line px-1 font-mono text-[9px] leading-4 text-ink-muted">
                              {doc.page}
                            </span>
                          </div>
                          <p className="mt-1 text-[12.5px] leading-relaxed text-ink-soft">
                            <Highlighted text={doc.snippet} tokens={tokens} />
                          </p>
                          <div className="mt-1.5 flex items-center gap-1.5 font-mono text-[9.5px] text-ink-muted">
                            <span>{typeLabel[doc.type]}</span>
                            <span aria-hidden="true">·</span>
                            <span className="truncate">{doc.folder}</span>
                            {showingReasons && (
                              <>
                                <span aria-hidden="true">·</span>
                                <Reason text={doc.reasons} />
                              </>
                            )}
                          </div>
                        </div>
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div
              aria-hidden="true"
              className={`pointer-events-none absolute inset-x-0 bottom-0 h-10 bg-gradient-to-t from-paper-card to-transparent transition-opacity duration-200 ${
                hasMore ? 'opacity-100' : 'opacity-0'
              }`}
            />
          </div>

          <p className="border-t border-line bg-paper px-4 py-3 text-[11px] leading-relaxed text-ink-muted">
            The lens scopes the sample the way it scopes the app; the query
            filters it. No engine, no network, no simulated timings.
          </p>
        </div>
      </div>
    </Section>
  );
};
