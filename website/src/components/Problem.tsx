import React from 'react';
import { Section, SectionHead } from './ui';

const CAPABILITIES = [
  ['Text inside screenshots', 'Indexed locally', 'No', 'Sometimes, paid', 'Copilot+ PCs only'],
  ['Search images by description', 'SigLIP, local', 'No', 'Uploads the image', 'Copilot+ PCs only'],
  ['One query across formats', 'Screenshot, PDF, Word, text', 'File names', 'Usually one format', 'Partial'],
  ['Works offline', 'Yes, at query time', 'Yes', 'No', 'Partly'],
  ['Cost', 'Free, MIT', 'Bundled', '$15 to $19 a month', 'New hardware'],
] as const;

const FileThumb: React.FC = () => (
  <img
    src="/media/invoice-receipt.webp"
    alt=""
    width={48}
    height={48}
    loading="lazy"
    decoding="async"
    className="h-12 w-12 shrink-0 rounded-[5px] border border-line-strong/70 bg-white object-cover object-top"
  />
);

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
        lede="Windows indexes names and dates. The words inside a screenshot, a PDF or a Word file stay invisible — Copilot+ search only covers part of that, on a laptop you have to buy."
      />
    </div>

    <div className="mt-12 grid items-start gap-5 md:mt-16 lg:grid-cols-2" data-reveal-group>
      <div className="min-w-0 rounded-lg border border-line bg-paper-card p-5 sm:p-6">
        <p className="kicker">What the file system sees</p>
        <div className="mt-4 flex items-start gap-4 border-b border-line pb-5">
          <FileThumb />
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
            ['Date', '11 October 2026'],
            ['Dimensions', '1920 × 1080'],
            ['Size', '240 KB'],
            ['Type', 'PNG image'],
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
        <div className="mt-4 overflow-hidden rounded-md border border-line-strong/60 bg-white">
          <img
            src="/media/invoice-receipt-crop.webp"
            alt="Top of the invoice screenshot: Skyline Air booking confirmation, invoice SK-3391, paid on Visa ending 4012."
            width={880}
            height={369}
            loading="lazy"
            decoding="async"
            className="aspect-[880/369] w-full object-cover object-left-top"
          />
        </div>
        <p className="mt-4 text-[15px] leading-relaxed text-ink">
          Invoice <mark className="bg-accent/15 text-accent-deep">SK-3391</mark>, airline
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

    <div className="mt-14 md:mt-16" data-reveal>
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h3 className="font-display text-[1.7rem] text-ink">What changes with Tessera</h3>
        <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted">
          General comparison, not a benchmark
        </p>
      </div>
      <div className="mt-5 space-y-5 md:hidden">
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
                <th scope="row" className="py-3.5 pr-4 font-medium text-ink">
                  {capability}
                </th>
                <td className="bg-accent-soft/40 px-4 py-3.5 font-medium text-accent-deep">
                  {tessera}
                </td>
                <td className="px-4 py-3.5 text-ink-soft">{explorer}</td>
                <td className="px-4 py-3.5 text-ink-soft">{cloud}</td>
                <td className="px-4 py-3.5 text-ink-soft">{copilot}</td>
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
