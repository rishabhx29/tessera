import React from 'react';
import { ArrowUpRight } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { REPO, STATS } from '../data/site';

const MISS = STATS[0];
const OTHERS = STATS.slice(1);

const RANKING = [
  { label: '300 photos', value: '0.7015', width: 70.15, captions: '1,501 captions' },
  { label: '5,000 photos', value: '0.3552', width: 35.52, captions: '25,010 captions' },
] as const;

export const Numbers: React.FC = () => (
  <Section id="numbers" tone="card" labelledBy="numbers-title">
      <div data-reveal>
        <SectionHead
          id="numbers-title"
          size="large"
          title={
            <>
              The numbers,
              <br />
              including the one that misses.
            </>
          }
          lede="Measured on one laptop, with the method written down. No target is written here as though it were a result, and the row that fails the 100 ms target is the first one you see."
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

      <div className="mt-14 grid gap-4 md:mt-20 lg:grid-cols-[minmax(0,1.18fr)_minmax(0,0.82fr)]">
        <div
          data-reveal
          className="rounded-xl border border-accent/30 bg-accent-soft/55 p-6 sm:p-8"
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <span className="rounded-sm bg-accent px-2 py-1 font-mono text-[10px] uppercase tracking-[0.1em] text-paper-card">
              {MISS.badge}
            </span>
            <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
              {MISS.label}
            </span>
          </div>

          <p className="mt-6 flex flex-wrap items-baseline gap-x-3 gap-y-1">
            <span className="font-display text-[clamp(3.6rem,8vw,6rem)] leading-[0.9] tracking-display text-accent-deep">
              {MISS.value}
            </span>
            <span className="font-mono text-[12px] text-ink-muted">{MISS.unit}</span>
          </p>
          <p className="mt-2 text-[14px] text-ink-soft">{MISS.secondary}</p>

          <div className="mt-7 space-y-4">
            {[
              { label: 'p50', value: 457, width: 83.1 },
              { label: 'p95', value: 523, width: 95.1 },
            ].map((bar) => (
              <div key={bar.label}>
                <div className="flex items-baseline justify-between font-mono text-[10px] text-ink-muted">
                  <span>{bar.label}</span>
                  <span>{bar.value} ms</span>
                </div>
                <div className="relative mt-1.5 h-2.5 rounded-sm bg-paper-card">
                  <span
                    className="stat-bar-fill absolute inset-y-0 left-0 rounded-sm bg-accent"
                    style={{ width: `${bar.width}%` }}
                  />
                  <span
                    aria-hidden="true"
                    className="absolute inset-y-[-4px] left-[18.2%] w-px bg-ink"
                  />
                </div>
              </div>
            ))}
            <p className="flex items-center gap-2 font-mono text-[10px] text-ink-muted">
              <span aria-hidden="true" className="h-2 w-px bg-ink" />
              Target: under 100 ms, at 550 ms scale
            </p>
          </div>

          <p className="mt-6 border-t border-accent/20 pt-4 text-[13.5px] leading-relaxed text-ink-soft">
            {MISS.body}
          </p>
        </div>

        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-1" data-reveal-group>
          {OTHERS.slice(0, 2).map((stat) => (
            <div key={stat.label} className="rounded-xl border border-line bg-paper p-5 sm:p-6">
              <div className="flex items-baseline justify-between gap-3">
                <span className="font-mono text-[9px] uppercase tracking-[0.1em] text-ink-muted">
                  {stat.badge}
                </span>
                <span className="font-mono text-[9px] text-ink-muted">{stat.unit}</span>
              </div>
              <p className="mt-4 font-display text-[2.8rem] leading-none tracking-display text-ink">
                {stat.value}
              </p>
              <p className="mt-2 text-[13px] font-medium text-ink">{stat.label}</p>
              <p className="mt-1 font-mono text-[10px] text-ink-muted">{stat.secondary}</p>
              <p className="mt-4 border-t border-line pt-3 text-[12.5px] leading-relaxed text-ink-soft">
                {stat.body}
              </p>
            </div>
          ))}
        </div>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2" data-reveal-group>
        {OTHERS.slice(2).map((stat) => (
          <div key={stat.label} className="rounded-xl border border-line bg-paper p-6">
            <div className="flex items-baseline justify-between gap-3">
              <span className="font-mono text-[9px] uppercase tracking-[0.1em] text-ink-muted">
                {stat.badge}
              </span>
              <span className="font-mono text-[9px] text-ink-muted">{stat.unit}</span>
            </div>
            <p className="mt-3 font-display text-[2.6rem] leading-none tracking-display text-ink">
              {stat.value}
            </p>
            <p className="mt-2 text-[13px] font-medium text-ink">{stat.label}</p>
            <p className="mt-1 font-mono text-[10px] text-ink-muted">{stat.secondary}</p>

            {stat.label.includes('Description search, 5,000') ? (
              <div className="mt-5 rounded-md border border-line bg-paper-card p-3.5">
                {RANKING.map((row) => (
                  <div key={row.label} className="mb-3 last:mb-0">
                    <div className="flex items-baseline justify-between font-mono text-[10px] text-ink-muted">
                      <span>{row.label}</span>
                      <span className="text-ink-soft">{row.value}</span>
                    </div>
                    <div className="mt-1 h-2 rounded-sm bg-paper-deep">
                      <span
                        className="block h-full rounded-sm bg-slate"
                        style={{ width: `${row.width}%` }}
                      />
                    </div>
                    <p className="mt-0.5 text-right font-mono text-[9px] text-ink-muted">
                      {row.captions}
                    </p>
                  </div>
                ))}
                <p className="border-t border-line pt-2 font-mono text-[9px] uppercase tracking-[0.08em] text-ink-muted">
                  R@1, bar drawn to value
                </p>
              </div>
            ) : null}

            <p className="mt-4 border-t border-line pt-3 text-[12.5px] leading-relaxed text-ink-soft">
              {stat.body}
            </p>
          </div>
        ))}
      </div>

      <p className="mt-8 border-t border-line pt-5 text-[12px] leading-relaxed text-ink-muted">
        Measured on an AMD Ryzen 3 5300U, 8 threads, 7.3 GB RAM, Windows 11 build
        26300. The performance log records the method, the corpus shape and the
        caveats behind each row.
      </p>
    </Section>
);
