import React from 'react';
import { ArrowUpRight, FolderSearch, ShieldCheck, Trash2, WifiOff } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { REPO } from '../data/site';

const DATA_ROWS = [
  ['Files you index', 'Read in place. Text and vectors stay in the local index.', 'Nothing'],
  ['Search queries', 'Matched inside the app process. No request is made.', 'Nothing'],
  ['Usage and crashes', 'No telemetry, no analytics, no crash upload in the build.', 'Nothing'],
  ['Accounts', 'There are none to sign into.', 'Nothing'],
  ['The index', '%LOCALAPPDATA%\\TesseraData\\index.sqlite', 'Yours'],
  ['Model download', 'About 1.3 GB, once, checked against pinned SHA-256 hashes.', 'One request'],
] as const;

const PROMISES = [
  {
    icon: WifiOff,
    title: 'No network at query time',
    body: 'Search runs against the local SQLite index. Pull the cable out and every result still comes back.',
  },
  {
    icon: ShieldCheck,
    title: 'No telemetry, no account',
    body: 'The build has no analytics client and no sign-in layer. There is no server that could receive a query.',
  },
  {
    icon: FolderSearch,
    title: 'Reads in place, never moves',
    body: 'The watcher reads filesystem events. It does not rename, move, modify or upload a single file.',
  },
  {
    icon: Trash2,
    title: 'Delete one folder, remove everything',
    body: 'The index and models live in one directory you own, separate from the installer folder.',
  },
] as const;

export const Privacy: React.FC = () => (
  <Section id="privacy" labelledBy="privacy-title">
    <div data-reveal>
      <SectionHead
        id="privacy-title"
        size="large"
        title={
          <>
            Your files stay
            <br />
            where they already are.
          </>
        }
        lede="Privacy here is not a policy paragraph. It is the shape of the build: no network step at query time, no telemetry client, no account layer. The only request the app ever makes is the first-run model download."
        aside={
          <a
            href={`${REPO}/blob/main/SECURITY.md`}
            target="_blank"
            rel="noopener noreferrer"
            className="link-arrow"
          >
            Read SECURITY.md
            <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
          </a>
        }
      />
    </div>

    <div className="mt-14 grid gap-10 md:mt-20 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)] lg:gap-14">
      <div data-reveal className="min-w-0">
        <dl className="md:hidden">
          {DATA_ROWS.map(([data, where, seen]) => (
            <div
              key={data}
              className="grid grid-cols-[minmax(0,1fr)_auto] items-baseline gap-x-4 border-t border-line py-4 first:border-t-0 first:pt-0"
            >
              <dt className="text-[13.5px] font-medium text-ink">{data}</dt>
              <dd
                className={`shrink-0 justify-self-end font-mono text-[11px] ${
                  seen === 'Nothing' ? 'text-sage' : 'text-ink-muted'
                }`}
              >
                {seen}
              </dd>
              <dd className="col-span-2 mt-1.5 break-words text-[13px] leading-relaxed text-ink-soft">
                {where}
              </dd>
            </div>
          ))}
        </dl>
        <div className="hidden overflow-x-auto md:block">
          <table className="w-full min-w-[540px] border-collapse text-left">
            <caption className="sr-only">
              What leaves the machine, where each item goes, and what the project can see.
            </caption>
            <thead>
              <tr className="border-y border-line-strong">
                {['Data', 'Where it goes', 'What we see'].map((heading) => (
                  <th
                    key={heading}
                    scope="col"
                    className="py-3 pr-4 font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-ink-muted last:pr-0 last:text-right"
                  >
                    {heading}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {DATA_ROWS.map(([data, where, seen]) => (
                <tr key={data} className="border-b border-line align-top">
                  <th scope="row" className="w-[30%] py-4 pr-4 text-[13.5px] font-medium text-ink">
                    {data}
                  </th>
                  <td className="py-4 pr-4 text-[13px] leading-relaxed text-ink-soft">{where}</td>
                  <td
                    className={`py-4 text-right font-mono text-[11px] ${
                      seen === 'Nothing' ? 'text-sage' : 'text-ink-muted'
                    }`}
                  >
                    {seen}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="mt-4 text-[12px] leading-relaxed text-ink-muted">
          The path is the default on Windows. Set TESSERA_DATA_DIR to put the index
          on another drive. Uninstall removes the app, not your index, until you
          delete that folder yourself.
        </p>
      </div>

      <ul className="min-w-0 divide-y divide-line border-y border-line" data-reveal-group>
        {PROMISES.map(({ icon: Icon, title, body }) => (
          <li key={title} className="grid grid-cols-[auto_minmax(0,1fr)] gap-4 py-5 first:pt-0 last:pb-0">
            <span className="flex h-9 w-9 items-center justify-center rounded-md border border-line bg-paper-card text-accent">
              <Icon className="h-4 w-4" aria-hidden="true" />
            </span>
            <div>
              <p className="text-[14.5px] font-medium text-ink">{title}</p>
              <p className="mt-1.5 text-[13px] leading-relaxed text-ink-soft">{body}</p>
            </div>
          </li>
        ))}
      </ul>
    </div>
  </Section>
);
