import React, { useState } from 'react';
import {
  Search,
  ArrowRight,
  Clock,
  Info
} from 'lucide-react';

interface SearchDemoScenario {
  id: string;
  label: string;
  query: string;
  intent: string;
  results: Array<{
    type: 'screenshot' | 'pdf' | 'docx';
    title: string;
    path: string;
    page?: number;
    matchArm: string;
    latency: string;
    matchedText: string;
    highlightRange: [number, number]; // index in matchedText
    snippetPreview: string;
    badgeColor: string;
  }>;
}

const DEMO_SCENARIOS: SearchDemoScenario[] = [
  {
    id: 'airline-invoice',
    label: 'v0.1 Core Demo: "airline invoice"',
    query: 'the invoice from the airline',
    intent: 'Mixed-format retrieval across OCR text, PDF pages, and Word documents in one query',
    results: [
      {
        type: 'screenshot',
        title: 'Screenshot 2026-10-11 142743.png',
        path: 'C:\\Users\\User\\Pictures\\Screenshots\\',
        matchArm: 'Lexical FTS5 + Windows.Media.Ocr (34.4 ms)',
        latency: '34.4 ms',
        matchedText: 'INVOICE QF-3391 · Airline Travel Booking Confirmed',
        highlightRange: [0, 15],
        snippetPreview: 'TAX INVOICE · Qantas Airways · Booking Ref: QF-3391 · Total AUD 1,248.50 · Paid Visa ending 4012',
        badgeColor: 'border-amber-600/30 bg-amber-50 text-amber-900',
      },
      {
        type: 'pdf',
        title: 'Travel_Expense_Policy_2026.pdf',
        path: 'C:\\Users\\User\\Documents\\Policies\\',
        page: 3,
        matchArm: 'Text Arm (MiniLM-L6) + Trigram BM25',
        latency: '35.0 ms',
        matchedText: 'corporate airline invoice submission guideline',
        highlightRange: [10, 25],
        snippetPreview: 'Section 4.2: Employees must attach the original airline invoice and boarding receipts within 14 calendar days...',
        badgeColor: 'border-red-600/30 bg-red-50 text-red-900',
      },
      {
        type: 'docx',
        title: 'October_Expense_Claim_Final.docx',
        path: 'C:\\Users\\User\\Documents\\Expenses\\',
        matchArm: 'Full-evidence Lexical trigram',
        latency: '0.4 ms',
        matchedText: 'itemised invoice for airline transit',
        highlightRange: [9, 24],
        snippetPreview: 'Item 04: Overseas flights for summit. Airline invoice attached as digital voucher. Approved by manager.',
        badgeColor: 'border-blue-600/30 bg-blue-50 text-blue-900',
      },
    ],
  },
  {
    id: 'red-chart',
    label: 'Visual Description: "red chart"',
    query: 'quarterly revenue red chart',
    intent: 'SigLIP description search matching visual content without requiring explicit text labels',
    results: [
      {
        type: 'screenshot',
        title: 'Screenshot 2026-09-30 180211.png',
        path: 'C:\\Users\\User\\Pictures\\Screenshots\\',
        matchArm: 'SigLIP-base int8 (Vision-Text cosine 0.74)',
        latency: '62.0 ms',
        matchedText: 'Q3 Financial Review · EBITDA margins by quarter',
        highlightRange: [0, 19],
        snippetPreview: '[Visual match: Bar chart with distinct red negative margin column highlighting quarterly operating deficit]',
        badgeColor: 'border-amber-600/30 bg-amber-50 text-amber-900',
      },
      {
        type: 'pdf',
        title: 'Board_Meeting_Deck_Q3.pdf',
        path: 'C:\\Users\\User\\Documents\\Presentations\\',
        page: 12,
        matchArm: 'MiniLM semantic vector match',
        latency: '35.0 ms',
        matchedText: 'quarterly revenue summary and variance indicators',
        highlightRange: [0, 17],
        snippetPreview: 'Slide 12: Consolidated quarterly revenue with color-coded red deficit callouts in operating cash flow...',
        badgeColor: 'border-red-600/30 bg-red-50 text-red-900',
      },
      {
        type: 'docx',
        title: 'Q3_Financial_Analysis_Memo.docx',
        path: 'C:\\Users\\User\\Documents\\Financials\\',
        matchArm: 'FTS5 trigram token match',
        latency: '0.4 ms',
        matchedText: 'revenue waterfall chart commentary',
        highlightRange: [0, 7],
        snippetPreview: 'Refer to figure 3 for the red waterfall bars indicating unexpected hardware amortization expenses.',
        badgeColor: 'border-blue-600/30 bg-blue-50 text-blue-900',
      },
    ],
  },
];

export const Hero: React.FC = () => {
  const [activeScenario, setActiveScenario] = useState<SearchDemoScenario>(DEMO_SCENARIOS[0]);
  const [inputValue, setInputValue] = useState(DEMO_SCENARIOS[0].query);
  const [activeTab, setActiveTab] = useState<'all' | 'screenshot' | 'pdf' | 'docx'>('all');

  const handleSelectScenario = (scenario: SearchDemoScenario) => {
    setActiveScenario(scenario);
    setInputValue(scenario.query);
    setActiveTab('all');
  };

  const filteredResults = activeScenario.results.filter((res) => {
    if (activeTab === 'all') return true;
    return res.type === activeTab;
  });

  return (
    <section className="relative pt-12 pb-20 md:pt-20 md:pb-32 overflow-hidden border-b border-line">
      {/* Background Subtle Paper Grid Pattern */}
      <div
        className="absolute inset-0 pointer-events-none opacity-[0.35]"
        style={{
          backgroundImage: `radial-gradient(#D1CDC0 1px, transparent 1px)`,
          backgroundSize: '24px 24px',
        }}
        aria-hidden="true"
      />

      <div className="relative max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Eyebrow badge */}
        <div className="flex items-center justify-center mb-6">
          <div className="inline-flex items-center gap-2 px-3 py-1 rounded-full bg-subtle border border-line-strong text-xs font-mono text-ink-secondary">
            <span className="w-2 h-2 rounded-full bg-accent animate-pulse" />
            <span>Local-first semantic engine · Zero cloud dependencies</span>
          </div>
        </div>

        {/* Headline */}
        <div className="text-center max-w-3xl mx-auto mb-6">
          <h1 className="font-serif text-4xl sm:text-5xl lg:text-6xl font-bold tracking-tight text-ink-primary leading-[1.12]">
            One search bar for everything your PC can't search.
          </h1>
        </div>

        {/* Sub-headline */}
        <div className="text-center max-w-2xl mx-auto mb-10">
          <p className="text-lg sm:text-xl text-ink-secondary leading-relaxed">
            Images, screenshots, PDFs, and documents —{' '}
            <strong className="text-ink-primary font-semibold">runs entirely on your device</strong>,
            answers in milliseconds, and never uploads a single byte.
          </p>
        </div>

        {/* CTA Group */}
        <div className="flex flex-col sm:flex-row items-center justify-center gap-4 mb-14">
          <a
            href="https://github.com/rishabhx29/tessera/releases"
            target="_blank"
            rel="noopener noreferrer"
            className="w-full sm:w-auto inline-flex items-center justify-center gap-2.5 px-6 py-3 rounded-lg bg-accent text-white font-medium text-sm hover:bg-accent-hover shadow-tile transition-all focus-visible:ring-2 focus-visible:ring-accent"
          >
            <span>Windows build — coming with v0.1</span>
            <ArrowRight className="w-4 h-4" />
          </a>

          <a
            href="https://github.com/rishabhx29/tessera"
            target="_blank"
            rel="noopener noreferrer"
            className="w-full sm:w-auto inline-flex items-center justify-center gap-2.5 px-6 py-3 rounded-lg bg-surface text-ink-primary font-medium text-sm border border-line-strong hover:bg-subtle hover:border-ink-muted transition-all focus-visible:ring-2 focus-visible:ring-accent"
          >
            <span>Star on GitHub</span>
            <span className="text-xs font-mono text-ink-muted">· MIT</span>
          </a>
        </div>

        {/* Metric Badges Row */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 max-w-3xl mx-auto mb-16 text-center">
          <div className="p-3 rounded-md bg-surface border border-line">
            <div className="font-mono text-base font-bold text-ink-primary">34.4 ms</div>
            <div className="text-xs text-ink-muted mt-0.5">Windows Native OCR p50</div>
          </div>
          <div className="p-3 rounded-md bg-surface border border-line">
            <div className="font-mono text-base font-bold text-ink-primary">0.4 ms</div>
            <div className="text-xs text-ink-muted mt-0.5">Lexical Trigram Query</div>
          </div>
          <div className="p-3 rounded-md bg-surface border border-line">
            <div className="font-mono text-base font-bold text-ink-primary">0 Bytes</div>
            <div className="text-xs text-ink-muted mt-0.5">Network at query time</div>
          </div>
          <div className="p-3 rounded-md bg-surface border border-line">
            <div className="font-mono text-base font-bold text-ink-primary">1 File</div>
            <div className="text-xs text-ink-muted mt-0.5">Standard SQLite database</div>
          </div>
        </div>

        {/* Abstract Illustrative Search UI Composition */}
        <div className="relative rounded-xl border border-line-strong bg-surface shadow-elevated overflow-hidden">
          {/* Strict Disclaimers & Header Bar */}
          <div className="flex flex-wrap items-center justify-between gap-2 px-4 py-3 bg-subtle border-b border-line text-xs font-mono">
            <div className="flex items-center gap-2 text-ink-secondary">
              <span className="w-2.5 h-2.5 rounded-full bg-line-strong" />
              <span className="w-2.5 h-2.5 rounded-full bg-line-strong" />
              <span className="w-2.5 h-2.5 rounded-full bg-line-strong" />
              <span className="ml-2 font-medium text-ink-primary">Tessera Global Summon</span>
            </div>

            <div className="flex items-center gap-1.5 px-2.5 py-0.5 rounded bg-surface border border-line text-[11px] text-ink-muted">
              <Info className="w-3.5 h-3.5 text-accent" />
              <span>Interactive Illustration · Not a live product screenshot</span>
            </div>
          </div>

          {/* Interactive Search Bar Area */}
          <div className="p-4 sm:p-6 bg-canvas border-b border-line">
            <div className="relative flex items-center">
              <Search className="absolute left-4 w-5 h-5 text-ink-muted" />
              <input
                type="text"
                value={inputValue}
                onChange={(e) => setInputValue(e.target.value)}
                placeholder="Type a phrase, describe a photo, or enter keywords..."
                className="w-full pl-12 pr-28 py-3.5 rounded-lg bg-surface border border-line-strong text-ink-primary placeholder:text-ink-muted font-sans text-base shadow-sm focus:outline-none focus:border-accent focus:ring-1 focus:ring-accent"
                aria-label="Interactive search query demonstration"
              />
              <div className="absolute right-3 hidden sm:flex items-center gap-1 px-2 py-1 rounded bg-subtle border border-line text-xs font-mono text-ink-muted">
                <kbd className="font-semibold text-ink-primary">Alt</kbd>
                <span>+</span>
                <kbd className="font-semibold text-ink-primary">Space</kbd>
              </div>
            </div>

            {/* Scenario Quick Selector Pills */}
            <div className="flex flex-wrap items-center gap-2 mt-3 text-xs">
              <span className="text-ink-muted font-mono">Sample queries:</span>
              {DEMO_SCENARIOS.map((scenario) => (
                <button
                  key={scenario.id}
                  onClick={() => handleSelectScenario(scenario)}
                  className={`px-3 py-1 rounded-md font-sans transition-all text-xs border ${
                    activeScenario.id === scenario.id
                      ? 'bg-ink-primary text-white border-ink-primary'
                      : 'bg-surface text-ink-secondary border-line hover:border-ink-muted'
                  }`}
                >
                  {scenario.label}
                </button>
              ))}
            </div>
          </div>

          {/* Filter Bar & Evidence Honesty Header */}
          <div className="flex flex-wrap items-center justify-between gap-3 px-4 sm:px-6 py-2.5 bg-subtle border-b border-line text-xs">
            <div className="flex items-center gap-2">
              <span className="text-ink-muted font-mono">Format filters:</span>
              <div className="flex items-center gap-1">
                {(['all', 'screenshot', 'pdf', 'docx'] as const).map((tab) => (
                  <button
                    key={tab}
                    onClick={() => setActiveTab(tab)}
                    className={`px-2.5 py-0.5 rounded text-xs transition-colors capitalize ${
                      activeTab === tab
                        ? 'bg-surface font-semibold text-ink-primary border border-line shadow-sm'
                        : 'text-ink-muted hover:text-ink-primary'
                    }`}
                  >
                    {tab === 'all' ? `All (${activeScenario.results.length})` : tab}
                  </button>
                ))}
              </div>
            </div>

            <div className="text-[11px] font-mono text-ink-muted">
              Fused rank: Reciprocal Rank Fusion (RRF) · Full evidence
            </div>
          </div>

          {/* Result Grid Items */}
          <div className="p-4 sm:p-6 bg-surface divide-y divide-line">
            {filteredResults.map((item, idx) => (
              <div
                key={item.title}
                className="py-4 first:pt-0 last:pb-0 hover:bg-canvas/50 px-3 rounded-lg transition-colors group"
              >
                <div className="flex flex-col sm:flex-row sm:items-start justify-between gap-3">
                  <div className="space-y-1.5 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-xs px-2 py-0.5 rounded font-semibold bg-subtle text-ink-primary border border-line">
                        Hit #{idx + 1}
                      </span>
                      <span
                        className={`font-mono text-[11px] px-2 py-0.5 rounded border ${item.badgeColor}`}
                      >
                        {item.type.toUpperCase()}
                        {item.page ? ` · Page ${item.page}` : ''}
                      </span>
                      <h2 className="text-sm font-semibold text-ink-primary font-mono group-hover:text-accent transition-colors">
                        {item.title}
                      </h2>
                    </div>

                    <div className="text-xs font-mono text-ink-muted">
                      {item.path}
                    </div>

                    {/* Matched Snippet */}
                    <div className="mt-2 text-sm text-ink-secondary bg-subtle/80 p-2.5 rounded-md border border-line font-sans leading-relaxed">
                      <div className="text-xs font-mono text-accent font-medium mb-1">
                        Matched OCR / Text segment:
                      </div>
                      <span className="bg-amber-100 text-ink-primary font-semibold px-1 py-0.5 rounded border border-amber-200">
                        {item.matchedText}
                      </span>
                      <p className="mt-1 text-xs text-ink-secondary">
                        {item.snippetPreview}
                      </p>
                    </div>
                  </div>

                  {/* Engine Arm & Latency Badge */}
                  <div className="sm:text-right shrink-0">
                    <div className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded bg-canvas border border-line text-xs font-mono text-ink-primary">
                      <Clock className="w-3.5 h-3.5 text-accent" />
                      <span>{item.latency}</span>
                    </div>
                    <div className="text-[11px] font-mono text-ink-muted mt-1">
                      {item.matchArm}
                    </div>
                  </div>
                </div>
              </div>
            ))}
          </div>

          {/* Illustration Footer Note */}
          <div className="px-4 py-3 bg-subtle border-t border-line text-center text-xs text-ink-muted">
            Three formats returned in under 40 milliseconds from one query. No cloud round-trip, no index sync lag.
          </div>
        </div>
      </div>
    </section>
  );
};
