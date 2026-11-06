import React from 'react';
import { ArrowUpRight } from 'lucide-react';
import { Section, SectionHeading } from './Section';

const PERFORMANCE_LOG =
  'https://github.com/rishabhx29/tessera/blob/main/docs/performance.md';

const Badge: React.FC<{
  children: React.ReactNode;
  tone?: 'pass' | 'miss' | 'neutral';
}> = ({ children, tone = 'neutral' }) => {
  const styles =
    tone === 'miss'
      ? 'border-accent/40 bg-accent-subtle text-accent'
      : tone === 'pass'
        ? 'border-line-strong bg-subtle text-ink-secondary'
        : 'border-line bg-surface text-ink-muted';

  return (
    <span
      className={`inline-flex w-fit items-center rounded-full border px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.04em] ${styles}`}
    >
      {children}
    </span>
  );
};

const Bar: React.FC<{
  label: string;
  value: string;
  percent: number;
  tone: 'pass' | 'miss';
}> = ({ label, value, percent, tone }) => (
  <div>
    <div className="flex items-baseline justify-between gap-3 font-mono text-[11px]">
      <span className="text-ink-secondary">{label}</span>
      <span className="text-ink-primary">{value}</span>
    </div>
    <div className="relative mt-1.5 h-2 overflow-hidden rounded-sm bg-inset">
      <div
        className={`h-full ${tone === 'pass' ? 'bg-accent' : 'bg-ink-primary'}`}
        style={{ width: `${percent}%` }}
      />
      <span
        className="absolute inset-y-[-2px] left-[20%] border-l border-dashed border-accent"
        aria-hidden="true"
      />
    </div>
  </div>
);

const StatRow: React.FC<{
  label: string;
  badge: React.ReactNode;
  value: string;
  secondary: string;
  context: React.ReactNode;
  badgeTone?: 'pass' | 'miss' | 'neutral';
}> = ({ label, badge, value, secondary, context, badgeTone }) => (
  <article className="grid gap-4 border-b border-line py-6 md:grid-cols-[11.5rem_minmax(0,0.85fr)_minmax(0,1fr)] md:gap-8 md:py-7">
    <div className="flex flex-col items-start gap-2">
      <Badge tone={badgeTone}>{badge}</Badge>
      <h3 className="text-[13px] font-medium leading-snug text-ink-primary">
        {label}
      </h3>
    </div>
    <div>
      <p className="font-mono text-[22px] font-semibold leading-none tracking-tight text-ink-primary">
        {value}
      </p>
      <p className="mt-1.5 font-mono text-[11px] text-ink-muted">{secondary}</p>
    </div>
    <p className="max-w-[54ch] text-[13px] leading-relaxed text-ink-secondary">
      {context}
    </p>
  </article>
);

export const ProofSection: React.FC = () => (
  <Section id="measured" tone="surface" labelledBy="measured-title">
    <SectionHeading
      id="measured-title"
      title={<>The numbers we have, including the one that misses.</>}
      lede={
        <>
          Every figure below comes from docs/performance.md, measured on one
          machine with committed scripts. Targets are marked as met or missed.
          The 50,000-file row is a miss, and it is shown as one.
        </>
      }
      aside={
        <a
          href={PERFORMANCE_LOG}
          target="_blank"
          rel="noopener noreferrer"
          className="hairline-link"
        >
          Read the full performance log
          <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
        </a>
      }
    />

    <div className="mt-10 grid gap-8 border-y border-line py-8 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] md:items-end md:gap-12">
      <div>
        <p className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
          Fused query, 5,000 files
        </p>
        <div className="mt-2 flex flex-wrap items-end gap-x-4 gap-y-2">
          <span className="font-serif text-[clamp(3rem,2rem+4vw,4.75rem)] font-bold leading-none tracking-tightest text-ink-primary">
            65 ms
          </span>
          <span className="mb-1.5 font-mono text-[12px] leading-relaxed text-ink-secondary">
            p50
            <br />
            80 ms p95
          </span>
        </div>
        <p className="mt-3 text-[13px] leading-relaxed text-ink-secondary">
          Target under 100 ms. Gate met.
        </p>
        <div className="mt-5 max-w-sm space-y-3">
          <Bar label="5,000 files" value="65 ms" percent={13} tone="pass" />
          <Bar label="50,000 files" value="457 ms" percent={91.4} tone="miss" />
          <p className="font-mono text-[10px] leading-relaxed text-ink-faint">
            Dashed line: the 100 ms target. Bars share a 0 to 500 ms scale.
          </p>
        </div>
      </div>

      <dl className="grid gap-x-8 gap-y-4 text-[12px] leading-relaxed sm:grid-cols-2">
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
            Machine
          </dt>
          <dd className="mt-1 text-ink-secondary">
            Intel Core Ultra 5 225H, 14 cores, 15 GB RAM
          </dd>
        </div>
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
            System
          </dt>
          <dd className="mt-1 text-ink-secondary">
            Windows 11 build 26200, mains power, no build running
          </dd>
        </div>
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
            Sample
          </dt>
          <dd className="mt-1 text-ink-secondary">
            Four queries, eight repeats, 32 fused samples
          </dd>
        </div>
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
            Arms at 5,000
          </dt>
          <dd className="mt-1 text-ink-secondary">
            Keyword 17 / 58 ms. Image 36 / 46 ms. Meaning 44 / 60 ms. p50 / p95.
          </dd>
        </div>
      </dl>
    </div>

    <div className="mt-2">
      <StatRow
        label="Fused query at 50,000 files"
        badge="Target missed"
        badgeTone="miss"
        value="457 ms p50"
        secondary="523 ms p95 · target under 100 ms"
        context={
          <>
            About 4.6 times over the target. The meaning arm is the largest cost
            at 241 ms p50. This row is not a win, and closing the gap needs a
            different index shape, not a bigger number here.
          </>
        }
      />

      <StatRow
        label="Windows OCR, per image"
        badge="Single sample"
        value="34.4 ms p50"
        secondary="139.9 ms p95 · 188 mean characters"
        context={
          <>
            40 real screenshots, one pass, one image at a time. Measured on an AMD
            Ryzen 3 5300U, 8 threads, 7.3 GB RAM, Windows 11 build 26300. A repeat
            pass moved p50 to 45.7 ms, so read the figure as one sample.
          </>
        }
      />

      <StatRow
        label="Description search, 300 photos"
        badge="COCO, 300 photos"
        value="0.7015 R@1"
        secondary="0.9454 R@5 · 1,501 captions"
        context={
          <>
            Measured through the shipped SigLIP path on COCO 2014 photos. The
            model was asked to reach 0.85 R@1. It does not, at any measured
            library size.
          </>
        }
      />

      <StatRow
        label="Description search, 5,000 photos"
        badge="Rank 1 falls"
        value="0.3552 R@1"
        secondary="0.6033 R@5 · 25,010 captions"
        context={
          <>
            The same path, a larger shelf. Rank 1 loses about 8.5 points per
            doubling of the library, with no flattening in the measured curve.
            Description search is a retrieval aid, not a guarantee.
          </>
        }
      />

      <StatRow
        label="Index size on disk"
        badge="Measured"
        value="22.7 MB / 1,000"
        secondary="13.7 MB / 1,000 at 50,000 files"
        context={
          <>
            113.7 MB for the 5,000-file index; 686 MB for 50,000. The mix of file
            types changes the number, so treat it as the measured shape of these
            corpora, not a constant.
          </>
        }
      />
    </div>

    <div className="mt-12 grid gap-6 border-t border-line pt-8 md:grid-cols-[minmax(0,0.55fr)_minmax(0,1fr)] md:gap-12">
      <h3 className="font-serif text-[22px] font-bold tracking-tight text-ink-primary">
        How to read these numbers
      </h3>
      <ul className="space-y-3 text-[13px] leading-relaxed text-ink-secondary">
        <li>
          <strong className="font-semibold text-ink-primary">
            The corpus is not a personal folder.
          </strong>{' '}
          About 31% of the 5,000 and 50,000-file corpora are real files. Generated
          screenshots are the easy case for OCR, so the real-world number is
          likely worse.
        </li>
        <li>
          <strong className="font-semibold text-ink-primary">
            One machine, one run.
          </strong>{' '}
          The OS file cache was warm across the harness processes. A cold machine
          would be slower.
        </li>
        <li>
          <strong className="font-semibold text-ink-primary">
            p95 is close to the maximum.
          </strong>{' '}
          With 32 samples, the p95 is effectively the near-maximum of those
          samples, not an estimate of the query distribution.
        </li>
        <li>
          <strong className="font-semibold text-ink-primary">
            OCR and queries used different machines.
          </strong>{' '}
          The query rows used the Core Ultra 5 225H. The 34.4 ms OCR row used the
          Ryzen 3 5300U. The hardware is stated on each row.
        </li>
        <li>
          <strong className="font-semibold text-ink-primary">
            COCO captions are not typed queries.
          </strong>{' '}
          The description numbers use human COCO captions, which is a cleaner
          query than a person typing from memory.
        </li>
      </ul>
    </div>
  </Section>
);
