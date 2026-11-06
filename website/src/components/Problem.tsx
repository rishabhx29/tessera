import React from 'react';
import { Image as ImageIcon } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { FAILURES } from '../data/site';

const CAPABILITIES = [
  ['Text inside screenshots', 'Indexed locally', 'No', 'Sometimes, paid', 'Copilot+ PCs only'],
  ['Search images by description', 'SigLIP, local', 'No', 'Uploads the image', 'Copilot+ PCs only'],
  ['One query across formats', 'Screenshot, PDF, Word, text', 'File names', 'Usually one format', 'Partial'],
  ['Works offline', 'Yes, at query time', 'Yes', 'No', 'Partly'],
  ['Cost', 'Free, MIT', 'Bundled', '$15 to $19 a month', 'New hardware'],
] as const;

export const Problem: React.FC = () => (
  <Section id="problem" labelledBy="problem-title">
    <div data-reveal>
      <SectionHead
        id="problem-title"
        size="large"
        title={
          <>
            Your PC can find a file by its name.
            <br />
            It cannot find the receipt inside it.
          </>
        }
        lede="Windows indexes names and dates. The words inside a screenshot, the page of a PDF and the paragraph in a Word file stay invisible. On a Copilot+ PC you get part of this, if you buy the right laptop."
      />
    </div>

    <div className="mt-14 grid gap-5 md:mt-20 lg:grid-cols-2" data-reveal-group>
      <div className="min-w-0 rounded-lg border border-line bg-paper-card p-5 sm:p-6">
        <p className="kicker">What the file system sees</p>
        <div className="mt-5 flex items-start gap-4 border-b border-line pb-5">
          <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-line bg-paper text-ink-muted">
            <ImageIcon className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="truncate font-mono text-[12px] text-ink">
              Screenshot 2026-10-11 142743.png
            </p>
            <p className="mt-1 text-[12px] text-ink-muted">
              Pictures / Screenshots · 240 KB · modified 11 Oct 2026, 14:32
            </p>
          </div>
        </div>
        <dl className="mt-5 space-y-3 font-mono text-[11px]">
          {[
            ['Name', 'Screenshot 2026-10-11 142743.png'],
            ['Date', '11 October 2026'],
            ['Text inside', 'Not indexed'],
          ].map(([term, value]) => (
            <div key={term} className="flex items-baseline justify-between gap-4">
              <dt className="text-ink-muted">{term}</dt>
              <dd className={term === 'Text inside' ? 'text-ink-muted' : 'text-ink-soft'}>
                {value}
              </dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="min-w-0 rounded-lg border border-line-strong bg-accent-soft/40 p-5 sm:p-6">
        <p className="kicker text-accent-deep">What Tessera sees</p>
        <div className="mt-5 flex items-start gap-4 border-b border-accent/20 pb-5">
          <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-md border border-accent/30 bg-paper-card text-accent">
            <ImageIcon className="h-5 w-5" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <p className="truncate font-mono text-[12px] text-ink">
              Screenshot 2026-10-11 142743.png
            </p>
            <p className="mt-1 text-[12px] text-ink-muted">Pictures / Screenshots</p>
          </div>
        </div>
        <p className="mt-5 text-[15px] leading-relaxed text-ink">
          Invoice <mark className="bg-accent/15 text-accent-deep">QF-3391</mark>, airline
          travel booking confirmed. Paid on Visa ending 4012.
        </p>
        <div className="mt-4 flex flex-wrap gap-1.5">
          {['OCR text', 'keyword match', 'image arm'].map((tag) => (
            <span
              key={tag}
              className="rounded-sm border border-accent/25 bg-paper-card/70 px-1.5 py-0.5 font-mono text-[10px] text-accent-deep"
            >
              {tag}
            </span>
          ))}
        </div>
      </div>
    </div>

    <div className="mt-16 grid gap-8 border-t border-line pt-10 md:grid-cols-3 md:gap-10">
      {FAILURES.map((failure) => (
        <div key={failure.n} data-reveal>
          <p className="font-mono text-[11px] text-accent">{failure.n}</p>
          <h3 className="mt-3 text-[1.0625rem] font-medium leading-snug text-ink">
            {failure.title}
          </h3>
          <p className="mt-2.5 text-[14px] leading-relaxed text-ink-soft">{failure.body}</p>
          <p className="mt-4 border-t border-line pt-3 text-[13px] font-medium text-ink">
            {failure.fix}
          </p>
        </div>
      ))}
    </div>

    <div className="mt-16" data-reveal>
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h3 className="font-display text-[1.7rem] text-ink">What changes with Tessera</h3>
        <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted">
          General comparison, not a benchmark
        </p>
      </div>
      <div className="mt-5 space-y-6 md:hidden">
        {CAPABILITIES.map(([capability, tessera, explorer, cloud, copilot]) => (
          <div key={capability}>
            <p className="text-[13px] font-medium text-ink">{capability}</p>
            <p className="mt-2 rounded-sm bg-accent-soft/40 px-2.5 py-1.5 text-[12.5px] font-medium text-accent-deep">
              <span className="mr-2 font-mono text-[9px] uppercase tracking-[0.1em]">Tessera</span>
              {tessera}
            </p>
            <dl className="mt-1.5 space-y-1 px-2.5">
              {[
                ['Explorer', explorer],
                ['Cloud tools', cloud],
                ['Copilot+', copilot],
              ].map(([label, value]) => (
                <div key={label} className="flex items-baseline justify-between gap-4">
                  <dt className="shrink-0 font-mono text-[9px] uppercase tracking-[0.08em] text-ink-muted">
                    {label}
                  </dt>
                  <dd className="text-right text-[12px] text-ink-soft">{value}</dd>
                </div>
              ))}
            </dl>
          </div>
        ))}
      </div>
      <div className="mt-5 hidden overflow-x-auto md:block">
        <table className="w-full min-w-[760px] border-collapse text-left text-[13px]">
          <caption className="sr-only">
            Capability comparison between Tessera, Windows Explorer, cloud search tools
            and Copilot+ search.
          </caption>
          <thead>
            <tr className="border-y border-line-strong">
              <th scope="col" className="py-3 pr-4 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-ink-muted">
                Capability
              </th>
              <th scope="col" className="px-4 py-3 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-accent-deep">
                Tessera
              </th>
              <th scope="col" className="px-4 py-3 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-ink-muted">
                Explorer
              </th>
              <th scope="col" className="px-4 py-3 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-ink-muted">
                Cloud tools
              </th>
              <th scope="col" className="px-4 py-3 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-ink-muted">
                Copilot+
              </th>
            </tr>
          </thead>
          <tbody>
            {CAPABILITIES.map(([capability, tessera, explorer, cloud, copilot]) => (
              <tr key={capability} className="border-b border-line align-top">
                <th scope="row" className="py-4 pr-4 font-medium text-ink">
                  {capability}
                </th>
                <td className="bg-accent-soft/40 px-4 py-4 font-medium text-accent-deep">
                  {tessera}
                </td>
                <td className="px-4 py-4 text-ink-soft">{explorer}</td>
                <td className="px-4 py-4 text-ink-soft">{cloud}</td>
                <td className="px-4 py-4 text-ink-soft">{copilot}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="mt-4 text-[12px] leading-relaxed text-ink-muted">
        Prices are the commercial tools we could find. Copilot+ search needs an NPU PC.
        Tessera needs nothing beyond the machine you own.
      </p>
    </div>
  </Section>
);
