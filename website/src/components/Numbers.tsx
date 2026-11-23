import React from 'react';
import { ArrowUpRight } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { REPO } from '../data/site';

const FIGURES = [
  {
    badge: '5,000 files',
    value: '65',
    unit: 'ms p50',
    title: 'Fused search, target met',
    detail: '80 ms p95. Both under the 100 ms target on the reference laptop.',
    tone: 'plain' as const,
  },
  {
    badge: '50,000 files',
    value: '457',
    unit: 'ms p50',
    title: 'Fused search, target missed',
    detail: '523 ms p95, about 4.6 times over. The meaning arm is most of the cost, and this row is reported as a miss.',
    tone: 'miss' as const,
  },
  {
    badge: 'COCO · 300 photos',
    value: '0.70',
    unit: 'R@1',
    title: 'Description search, rank 1',
    detail: '0.945 R@5. A retrieval aid, not a guarantee, and it is labelled as one.',
    tone: 'plain' as const,
  },
] as const;

export const Numbers: React.FC = () => (
  <Section id="numbers" tone="card" labelledBy="numbers-title">
    <div data-reveal>
      <SectionHead
        id="numbers-title"
        size="large"
        title={
          <>
            Three numbers.
            <br />
            One of them is a miss.
          </>
        }
        lede="Measured on one laptop, with the method written down. No target is dressed up as a result."
        aside={
          <a
            href={`${REPO}/blob/main/docs/performance.md`}
            target="_blank"
            rel="noopener noreferrer"
            className="link-arrow"
          >
            Read the performance log
            <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
          </a>
        }
      />
    </div>

    <div className="mt-14 grid gap-4 md:mt-20 md:grid-cols-3" data-reveal-group>
      {FIGURES.map((figure) => (
        <div
          key={figure.badge}
          className={`flex flex-col rounded-xl border p-6 sm:p-8 ${
            figure.tone === 'miss'
              ? 'border-accent/30 bg-accent-soft/55'
              : 'border-line bg-paper'
          }`}
        >
          <p className="kicker">{figure.badge}</p>
          <p className="mt-8 flex flex-wrap items-baseline gap-x-2">
            <span
              className={`font-display text-[clamp(3rem,6vw,4.4rem)] leading-[0.9] tracking-display ${
                figure.tone === 'miss' ? 'text-accent-deep' : 'text-ink'
              }`}
            >
              {figure.value}
            </span>
            <span className="font-mono text-[11px] text-ink-muted">{figure.unit}</span>
          </p>
          <p className="mt-5 text-[14.5px] font-medium text-ink">{figure.title}</p>
          <p className="mt-1.5 text-[13px] leading-relaxed text-ink-soft">{figure.detail}</p>
        </div>
      ))}
    </div>

    <p className="mt-8 border-t border-line pt-5 text-[12px] leading-relaxed text-ink-muted">
      AMD Ryzen 3 5300U, 8 threads, 7.3 GB RAM, Windows 11 build 26300. The{' '}
      <a
        href={`${REPO}/blob/main/docs/performance.md`}
        target="_blank"
        rel="noopener noreferrer"
        className="underline decoration-line-strong underline-offset-2 transition-colors duration-150 hover:text-ink"
      >
        performance log
      </a>{' '}
      records the corpus shape and the caveats behind each row.
    </p>
  </Section>
);
