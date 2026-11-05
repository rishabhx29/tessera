import React, { useState } from 'react';
import { Eye, FileSearch, Binary, GitMerge, Zap, Check } from 'lucide-react';

interface Stage {
  id: string;
  stepNumber: string;
  title: string;
  shortSummary: string;
  icon: React.ComponentType<{ className?: string }>;
  details: {
    heading: string;
    description: string;
    bullets: string[];
    technicalBadge: string;
    specQuote: string;
  };
}

const STAGES: Stage[] = [
  {
    id: 'watch',
    stepNumber: '01',
    title: 'Watch',
    shortSummary: 'Non-intrusive background file watcher',
    icon: Eye,
    details: {
      heading: 'Background filesystem events at idle OS priority',
      description:
        'Tessera listens to directory changes using native OS hooks. It indexes your files when the PC is idle and automatically steps aside when you need full machine performance.',
      bullets: [
        'Newest-first queue: a freshly captured screenshot is indexed within seconds, even with a large backlog.',
        'Idle-only priority: automatically pauses on battery power and runs threads at background IO priority.',
        'OneDrive safe: detects cloud placeholder files and strictly skips them without triggering large file downloads.',
        'Hard caps: files >50 MB are skipped with reasons recorded on the local diagnostics screen.',
      ],
      technicalBadge: 'ReadDirectoryChangesW · Newest-first Priority Queue · Battery-aware',
      specQuote: 'Models load per backlog drain and unload when idle — keeping background RAM under 250 MB.',
    },
  },
  {
    id: 'extract',
    stepNumber: '02',
    title: 'Extract',
    shortSummary: 'Fast format-specific text extraction',
    icon: FileSearch,
    details: {
      heading: 'Format-specialized text extraction pipelines',
      description:
        'Instead of treating all files with a heavy generic hammer, Tessera routes each file extension through an optimized pipeline.',
      bullets: [
        'Screenshots & Images: Windows.Media.Ocr native engine runs in 34.4 ms p50, extracting 188 mean characters per image.',
        'Cross-platform fallback: PP-OCRv6 Det-tiny + Rec-small available on Linux/macOS.',
        'PDF Documents: page text layer extraction into individual page-numbered tiles.',
        'DOCX & Text: raw XML document text extraction with structured chunking.',
      ],
      technicalBadge: 'Windows.Media.Ocr (34.4 ms) · Page-Aware Chunker · Fallback Engine',
      specQuote: 'Tile abstraction preserves 1-based page numbers so search results jump straight to the exact page.',
    },
  },
  {
    id: 'embed',
    stepNumber: '03',
    title: 'Embed',
    shortSummary: 'Local vision & language embeddings',
    icon: Binary,
    details: {
      heading: 'Dual-tower representation in pinned vector spaces',
      description:
        'Local quantized neural networks generate high-dimensional vectors to allow search by meaning and visual description without cloud APIs.',
      bullets: [
        'Vision arm: SigLIP-base int8 embedding space for matching visual scenes and descriptions.',
        'Document arm: MiniLM-L6-v2 transformer for document semantics and concept search.',
        'Checksum pinning: model weights are compiled with pinned SHA-256 digests in lock.json.',
        'Zero cloud leakage: all tensor inferences execute locally via ONNX Runtime CPU provider.',
      ],
      technicalBadge: 'SigLIP-base int8 (768-dim) · MiniLM-L6-v2 · SHA-256 Pinned',
      specQuote: 'COCO retrieval benchmark measures 0.7015 R@1 and 0.9454 R@5 through the shipped SiglipEmbedder.',
    },
  },
  {
    id: 'fuse',
    stepNumber: '04',
    title: 'Fuse',
    shortSummary: 'Evidence-weighted reciprocal rank fusion',
    icon: GitMerge,
    details: {
      heading: 'Rank-based merging across dissimilar channels',
      description:
        'BM25 lexical scores and vector cosine similarities are not mathematically comparable. Tessera merges rankings using weighted Reciprocal Rank Fusion (RRF).',
      bullets: [
        'Evidence weighting: text-bearing files weight the lexical arm at 1.0 and image arm at 0.05 to prevent noise from diluting exact matches.',
        'Pure photos (no OCR text) let the SigLIP vision arm vote with full 1.0 weight.',
        'Result transparency: every search hit details exactly which arm surfaced it and why.',
        'Typo lookalike expansion: query-side folding (0/o, 1/l) catches common OCR character confusions.',
      ],
      technicalBadge: 'Weighted RRF · Evidence-aware Weight Sweep · Lookalike Folding',
      specQuote: 'Measured sweep proved 0.05 image weight preserves 0.964 top-1 screenshot lexical retrieval without dilution.',
    },
  },
  {
    id: 'answer',
    stepNumber: '05',
    title: 'Answer',
    shortSummary: 'Instant query response in milliseconds',
    icon: Zap,
    details: {
      heading: 'Sub-millisecond lexical filtering + vector ranking',
      description:
        'When you summon the search bar with Alt+Space, query execution runs across parallel local SQLite tables.',
      bullets: [
        'SQLite FTS5 Trigram tokenizer answers keyword keystrokes in 0.4 ms p50.',
        'sqlite-vec vector extension scans KNN distances locally.',
        'Results grouped into one clean unified grid with instant keyboard navigation (Arrows, Enter, Esc).',
        'One-click actions: reveal in Explorer, copy snippet, open on specific page.',
      ],
      technicalBadge: 'SQLite 3.53 + APSW · sqlite-vec · FTS5 Trigram · Alt+Space Summon',
      specQuote: 'Query end-to-end executes in ~50 ms parallel execution on standard consumer desktop hardware.',
    },
  },
];

export const HowItWorks: React.FC = () => {
  const [activeStageId, setActiveStageId] = useState<string>('watch');
  const activeStage = STAGES.find((s) => s.id === activeStageId) || STAGES[0];

  return (
    <section id="how-it-works" className="py-20 bg-canvas border-b border-line">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="max-w-2xl mb-12">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            The Technical Architecture
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary">
            How it works: from saved file to instant answer.
          </h2>
          <p className="mt-3 text-base sm:text-lg text-ink-secondary leading-relaxed">
            A technically honest five-stage pipeline designed for personal scale, background efficiency, and zero network dependency.
          </p>
        </div>

        {/* Stepper Navigation Pills */}
        <div className="grid grid-cols-2 sm:grid-cols-5 gap-2 sm:gap-3 mb-10">
          {STAGES.map((stage) => {
            const IconComponent = stage.icon;
            const isSelected = stage.id === activeStageId;
            return (
              <button
                key={stage.id}
                onClick={() => setActiveStageId(stage.id)}
                className={`p-3 rounded-lg text-left transition-all border ${
                  isSelected
                    ? 'bg-surface border-accent shadow-tile ring-1 ring-accent'
                    : 'bg-subtle/70 border-line hover:border-line-strong hover:bg-subtle'
                }`}
              >
                <div className="flex items-center justify-between mb-2">
                  <span className="font-mono text-xs font-bold text-accent">
                    {stage.stepNumber}
                  </span>
                  <IconComponent
                    className={`w-4 h-4 ${
                      isSelected ? 'text-accent' : 'text-ink-muted'
                    }`}
                  />
                </div>
                <div className="font-serif font-bold text-sm text-ink-primary">
                  {stage.title}
                </div>
                <div className="text-[11px] text-ink-muted truncate mt-0.5">
                  {stage.shortSummary}
                </div>
              </button>
            );
          })}
        </div>

        {/* Active Stage Detailed Panel */}
        <div className="rounded-xl border border-line-strong bg-surface shadow-tile overflow-hidden">
          <div className="p-6 sm:p-8">
            <div className="flex flex-col lg:flex-row lg:items-start justify-between gap-8">
              {/* Left Column: Heading and Details */}
              <div className="space-y-4 flex-1">
                <div className="inline-flex items-center gap-2 px-2.5 py-1 rounded bg-subtle border border-line text-xs font-mono text-accent font-semibold">
                  <span>STAGE {activeStage.stepNumber} · {activeStage.title.toUpperCase()}</span>
                </div>

                <h3 className="font-serif text-2xl font-bold text-ink-primary">
                  {activeStage.details.heading}
                </h3>

                <p className="text-base text-ink-secondary leading-relaxed">
                  {activeStage.details.description}
                </p>

                {/* Bullets */}
                <div className="pt-2 space-y-2.5">
                  {activeStage.details.bullets.map((bullet, idx) => (
                    <div key={idx} className="flex items-start gap-2.5 text-sm text-ink-secondary">
                      <div className="w-5 h-5 rounded-full bg-accent-subtle flex items-center justify-center shrink-0 mt-0.5">
                        <Check className="w-3 h-3 text-accent" />
                      </div>
                      <span>{bullet}</span>
                    </div>
                  ))}
                </div>
              </div>

              {/* Right Column: Technical Spec Box */}
              <div className="w-full lg:w-80 shrink-0 space-y-4">
                <div className="p-4 rounded-lg bg-canvas border border-line font-mono text-xs space-y-3">
                  <div className="text-ink-muted uppercase tracking-wider text-[10px] font-bold">
                    Engine Implementation
                  </div>
                  <div className="p-2.5 rounded bg-surface border border-line text-ink-primary break-all">
                    {activeStage.details.technicalBadge}
                  </div>
                  <div className="text-[11px] text-ink-secondary italic leading-relaxed pt-2 border-t border-line">
                    "{activeStage.details.specQuote}"
                  </div>
                </div>

                <div className="p-3.5 rounded-lg bg-subtle border border-line text-xs text-ink-secondary">
                  <div className="font-semibold text-ink-primary mb-1">
                    Guaranteed Invariant
                  </div>
                  <div>
                    Zero telemetry emitted. Index stored as a single standard SQLite file in your local user directory.
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Stepper Footer Controls */}
          <div className="px-6 py-3.5 bg-subtle border-t border-line flex items-center justify-between text-xs">
            <span className="font-mono text-ink-muted">
              Pipeline Step {activeStage.stepNumber} of 05
            </span>
            <div className="flex items-center gap-1">
              {STAGES.map((s) => (
                <button
                  key={s.id}
                  onClick={() => setActiveStageId(s.id)}
                  className="p-2 flex items-center justify-center rounded focus-visible:ring-2 focus-visible:ring-accent"
                  aria-label={`Jump to step ${s.stepNumber}`}
                >
                  <span
                    className={`block h-2 rounded-full transition-all ${
                      s.id === activeStageId ? 'w-6 bg-accent' : 'w-2 bg-line-strong'
                    }`}
                  />
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>
    </section>
  );
};
