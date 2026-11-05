import React from 'react';
import { Cpu, AlertTriangle } from 'lucide-react';

interface MetricFact {
  id: string;
  metric: string;
  value: string;
  unit: string;
  secondaryStat: string;
  title: string;
  description: string;
  script: string;
  caveat: string;
}

const MEASURED_FACTS: MetricFact[] = [
  {
    id: 'ocr-native',
    metric: 'OCR Latency (Windows Native)',
    value: '34.4 ms',
    unit: 'p50 per image',
    secondaryStat: '188 mean characters extracted',
    title: 'Hardware-native OCR on real screenshots',
    description:
      'Timed using cargo run --release --example ocr_bench over 40 real user screenshots. Windows.Media.Ocr processes an image in 34.4 ms p50 (p95: 139.9 ms), 28× faster than unoptimized models.',
    script: 'crates/core/examples/ocr_bench.rs',
    caveat:
      'Single-threaded, one image at a time, cold file cache. Ambient machine load creates run-to-run variation (repeat pass measured 45.7 ms p50). 1 of 40 screenshots had zero text.',
  },
  {
    id: 'save-searchable',
    metric: 'Image Save → Searchable',
    value: '~260–350 ms',
    unit: 'projected end-to-end',
    secondaryStat: 'Target was < 1,500 ms',
    title: 'Fresh screenshot ready in a fraction of a second',
    description:
      'Components measured individually: image decode (5–100 ms) + Windows native OCR (50 ms p50) + SigLIP-base int8 vision embedding (200 ms p50).',
    script: 'spike/swap_test2.py & ocr_bench',
    caveat:
      'Sum of verified component timings. Multi-worker queue throughput will scale across up to 4 threads; document extraction (~1,000 ms) is slower and queued in background.',
  },
  {
    id: 'coco-retrieval',
    metric: 'Description Search Recall',
    value: '0.7015 R@1',
    unit: '0.9454 at R@5 (top 5)',
    secondaryStat: '300 images · 1,501 human queries',
    title: 'Visual description retrieval on standard benchmark',
    description:
      'Measured on COCO 2014 validation split through the shipped SiglipEmbedder and query normalizer. 94.5% of queries find the matching photograph within the top 5 results.',
    script: 'crates/core/tests/coco_retrieval.rs',
    caveat:
      'Rank-1 accuracy decays by ~8.5 points per doubling of library size (0.3552 at 5,000 photos). Text-bearing files are retrieved at 96% by OCR, so visual search is specifically for content photos.',
  },
  {
    id: 'lexical-fts',
    metric: 'Lexical Keystroke Query',
    value: '0.4 ms',
    unit: 'p50 query latency',
    secondaryStat: '2.6 ms p95 over 406 tiles',
    title: 'Sub-millisecond instant search on every keystroke',
    description:
      'SQLite FTS5 trigram tokenizer matches partial words ("invoic" → "invoice") and typos ("inv0ice" via lookalike expansion) virtually instantly.',
    script: 'spike/spike.py',
    caveat:
      'Measured over 406 index tiles. In full hybrid mode, vector scanning across 50k items adds ~140 ms without quantization; lexical alone remains sub-millisecond.',
  },
];

export const ProofSection: React.FC = () => {
  return (
    <section id="proof" className="py-20 bg-surface border-b border-line">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="max-w-3xl mb-12">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            Verifiable Benchmarks
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary">
            Measured numbers, never targets presented as results.
          </h2>
          <p className="mt-3 text-base sm:text-lg text-ink-secondary leading-relaxed">
            Every figure below was measured with committed test scripts on real files and standard consumer laptop hardware.
            No simulated synthetic cloud instances.
          </p>
        </div>

        {/* Hardware Context Callout Box */}
        <div className="p-4 sm:p-5 rounded-lg bg-canvas border border-line-strong mb-12 flex flex-col sm:flex-row sm:items-center justify-between gap-4 font-mono text-xs">
          <div className="flex items-center gap-3">
            <div className="w-9 h-9 rounded-md bg-subtle border border-line flex items-center justify-center shrink-0">
              <Cpu className="w-5 h-5 text-accent" />
            </div>
            <div>
              <div className="font-bold text-ink-primary">Benchmark Hardware Rig</div>
              <div className="text-ink-secondary text-[11px]">
                AMD Ryzen 3 5300U (8 threads) · 7.3 GB RAM · Windows 11 build 26300 · Release build · ONNX Runtime 1.31 CPU
              </div>
            </div>
          </div>
          <div className="text-[11px] text-ink-muted sm:text-right shrink-0">
            Source: <code className="text-ink-primary">docs/performance.md</code>
          </div>
        </div>

        {/* 4 Measured Fact Cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-12">
          {MEASURED_FACTS.map((fact) => (
            <div
              key={fact.id}
              className="p-6 rounded-xl bg-canvas border border-line shadow-tile flex flex-col justify-between"
            >
              <div>
                {/* Metric Header */}
                <div className="flex items-baseline justify-between gap-2 border-b border-line pb-4 mb-4">
                  <div>
                    <div className="text-xs font-mono text-ink-muted uppercase">
                      {fact.metric}
                    </div>
                    <div className="font-mono text-3xl font-bold text-ink-primary tracking-tight mt-1">
                      {fact.value}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="text-xs font-mono font-medium text-accent">
                      {fact.unit}
                    </div>
                    <div className="text-[11px] font-mono text-ink-muted mt-0.5">
                      {fact.secondaryStat}
                    </div>
                  </div>
                </div>

                {/* Title & Description */}
                <h3 className="font-serif text-lg font-bold text-ink-primary mb-2">
                  {fact.title}
                </h3>
                <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                  {fact.description}
                </p>
              </div>

              {/* Caveat & Provenance */}
              <div className="pt-3 border-t border-line space-y-2">
                <div className="flex items-start gap-2 text-xs text-ink-muted">
                  <AlertTriangle className="w-3.5 h-3.5 text-amber-700 shrink-0 mt-0.5" />
                  <span className="leading-snug">
                    <strong className="text-ink-secondary">Honest caveat:</strong> {fact.caveat}
                  </span>
                </div>
                <div className="text-[11px] font-mono text-ink-faint">
                  Reproduce: <code className="text-ink-muted">{fact.script}</code>
                </div>
              </div>
            </div>
          ))}
        </div>

        {/* Summary Banner on Engineering Discipline */}
        <div className="p-6 rounded-xl bg-subtle border border-line flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div className="space-y-1">
            <h4 className="font-serif text-base font-bold text-ink-primary">
              The Tessera Benchmark Rule
            </h4>
            <p className="text-sm text-ink-secondary">
              If an OCR engine misses characters or a model fails to reproduce on real files, it is recorded as a failure rather than swept into marketing copy.
            </p>
          </div>
          <a
            href="https://github.com/rishabhx29/tessera/blob/main/docs/performance.md"
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-2 px-4 py-2 rounded-md bg-surface text-ink-primary border border-line text-xs font-mono hover:border-ink-muted transition-colors shrink-0"
          >
            <span>Read full performance log</span>
          </a>
        </div>
      </div>
    </section>
  );
};
