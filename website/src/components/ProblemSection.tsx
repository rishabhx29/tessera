import React from 'react';
import { ArrowRight } from 'lucide-react';
import { Section, SectionHeading } from './Section';

const FAILURES = [
  {
    number: '01',
    title: 'Screenshots are named after the clock.',
    body: 'Screenshot 2026-10-11 at 14.32.11.png says nothing about the airline invoice inside it. Windows can see the date, not the words.',
    answer: 'Tessera reads the words inside the image.',
  },
  {
    number: '02',
    title: 'Semantic search wants new hardware.',
    body: 'Copilot+ search needs an NPU PC. Most people are going to keep the machine they already own.',
    answer: 'Tessera runs on the CPU in that machine.',
  },
  {
    number: '03',
    title: 'One tool sees one format.',
    body: 'Screenshot tools, photo tools and document tools each hold a slice. You end up guessing which one to open.',
    answer: 'One index covers screenshots, PDFs, Word and text.',
  },
];

const COMPARISON = [
  {
    capability: 'Text inside screenshots',
    tessera: 'Indexed locally',
    explorer: 'No',
    cloud: 'Sometimes, paid',
    copilot: 'Copilot+ PCs only',
  },
  {
    capability: 'Search images by description',
    tessera: 'SigLIP, local',
    explorer: 'No',
    cloud: 'Uploads the image',
    copilot: 'Copilot+ PCs only',
  },
  {
    capability: 'One query across formats',
    tessera: 'Screenshot, PDF, Word, text',
    explorer: 'File names',
    cloud: 'Usually one format',
    copilot: 'Partial',
  },
  {
    capability: 'Works offline',
    tessera: 'Yes, at query time',
    explorer: 'Yes',
    cloud: 'No',
    copilot: 'Partly',
  },
  {
    capability: 'Cost',
    tessera: 'Free, MIT',
    explorer: 'Bundled',
    cloud: 'About $15 to $19 a month',
    copilot: 'New hardware',
  },
];

export const ProblemSection: React.FC = () => (
  <Section id="problem" labelledBy="problem-title">
    <SectionHeading
      id="problem-title"
      title={
        <>
          Your PC can find a file by its name. It cannot find the receipt inside
          it.
        </>
      }
      lede={
        <>
          Windows indexes names and dates. The words inside a screenshot, the page
          of a PDF and the paragraph in a Word file stay invisible. On a Copilot+
          PC you get part of this, if you buy the right laptop.
        </>
      }
    />

    <ol className="mt-12 border-t border-line">
      {FAILURES.map((failure) => (
        <li
          key={failure.number}
          className="grid gap-3 border-b border-line py-6 md:grid-cols-[2.5rem_minmax(0,1.25fr)_minmax(0,0.75fr)] md:gap-8 md:py-7"
        >
          <span className="font-mono text-[11px] text-ink-faint">
            {failure.number}
          </span>
          <div>
            <h3 className="font-serif text-[19px] font-bold leading-snug text-ink-primary">
              {failure.title}
            </h3>
            <p className="mt-1.5 max-w-[58ch] text-sm leading-relaxed text-ink-secondary">
              {failure.body}
            </p>
          </div>
          <p className="flex items-start gap-2 text-sm leading-relaxed text-ink-primary md:justify-self-end">
            <ArrowRight
              className="mt-1 h-3.5 w-3.5 shrink-0 text-accent"
              aria-hidden="true"
            />
            <span>{failure.answer}</span>
          </p>
        </li>
      ))}
    </ol>

    <div className="mt-14">
      <div className="flex flex-col gap-2 sm:flex-row sm:items-baseline sm:justify-between">
        <h3 className="font-serif text-[22px] font-bold tracking-tight text-ink-primary">
          What changes with Tessera
        </h3>
        <p className="text-[12px] text-ink-muted">
          General comparison, not a benchmark. Prices are the commercial tools we
          could find.
        </p>
      </div>

      <div className="mt-5 hidden overflow-hidden rounded-lg border border-line bg-surface md:block">
        <table className="w-full border-collapse text-left text-[12px] lg:text-[13px]">
          <caption className="sr-only">
            Capability comparison between Tessera, Windows Explorer, cloud search
            tools and Copilot+ search
          </caption>
          <thead>
            <tr className="border-b border-line bg-subtle/70">
              <th scope="col" className="w-[30%] px-4 py-3 font-medium text-ink-muted">
                Capability
              </th>
              <th scope="col" className="px-4 py-3 font-semibold text-accent">
                Tessera
              </th>
              <th scope="col" className="px-4 py-3 font-medium text-ink-muted">
                Explorer
              </th>
              <th scope="col" className="px-4 py-3 font-medium text-ink-muted">
                Cloud search tools
              </th>
              <th scope="col" className="px-4 py-3 font-medium text-ink-muted">
                Copilot+
              </th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {COMPARISON.map((row) => (
              <tr key={row.capability}>
                <th
                  scope="row"
                  className="px-4 py-3 font-medium text-ink-primary"
                >
                  {row.capability}
                </th>
                <td className="bg-accent-subtle/40 px-4 py-3 font-medium text-ink-primary">
                  {row.tessera}
                </td>
                <td className="px-4 py-3 text-ink-secondary">{row.explorer}</td>
                <td className="px-4 py-3 text-ink-secondary">{row.cloud}</td>
                <td className="px-4 py-3 text-ink-secondary">{row.copilot}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <div className="mt-5 divide-y divide-line border-y border-line md:hidden">
        {COMPARISON.map((row) => (
          <div key={row.capability} className="py-4">
            <h4 className="font-serif text-[16px] font-bold text-ink-primary">
              {row.capability}
            </h4>
            <dl className="mt-2.5 grid grid-cols-[5.75rem_minmax(0,1fr)] gap-x-3 gap-y-1.5 text-[12px] leading-snug">
              <dt className="text-ink-muted">Tessera</dt>
              <dd className="font-medium text-accent">{row.tessera}</dd>
              <dt className="text-ink-muted">Explorer</dt>
              <dd className="text-ink-secondary">{row.explorer}</dd>
              <dt className="text-ink-muted">Cloud tools</dt>
              <dd className="text-ink-secondary">{row.cloud}</dd>
              <dt className="text-ink-muted">Copilot+</dt>
              <dd className="text-ink-secondary">{row.copilot}</dd>
            </dl>
          </div>
        ))}
      </div>
    </div>
  </Section>
);
