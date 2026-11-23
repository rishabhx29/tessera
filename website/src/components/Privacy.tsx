import React from 'react';
import { ArrowUpRight } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { REPO } from '../data/site';

const PROMISES = [
  {
    title: 'No network at query time',
    body: 'Search runs against the local SQLite index. Pull the cable out and every result still comes back.',
  },
  {
    title: 'No telemetry, no account',
    body: 'The build has no analytics client and no sign-in layer. There is no server that could receive a query.',
  },
  {
    title: 'Reads in place, never moves',
    body: 'The watcher reads filesystem events. It does not rename, move, modify or upload a single file.',
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
        lede="Privacy here is the shape of the build, not a policy paragraph. Nothing leaves the machine, because there is nowhere for it to go."
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

    <div
      className="mt-14 grid gap-10 border-t border-line pt-10 md:mt-20 md:grid-cols-3 md:gap-12"
      data-reveal-group
    >
      {PROMISES.map((promise, index) => (
        <div key={promise.title}>
          <p className="font-mono text-[10px] text-accent">
            {String(index + 1).padStart(2, '0')}
          </p>
          <p className="mt-4 font-display text-[1.6rem] leading-[1.1] tracking-display text-ink">
            {promise.title}
          </p>
          <p className="mt-2.5 max-w-[34ch] text-[13.5px] leading-relaxed text-ink-soft">
            {promise.body}
          </p>
        </div>
      ))}
    </div>

    <p className="mt-12 flex flex-wrap items-baseline gap-x-3 gap-y-1 border-t border-line pt-5 text-[12.5px] leading-relaxed text-ink-muted">
      <span className="text-ink-soft">The index lives in one folder:</span>
      <code className="font-mono text-[11.5px] text-ink-soft">
        %LOCALAPPDATA%\TesseraData
      </code>
      <span>Delete it and everything is gone. No uninstaller ritual.</span>
    </p>
  </Section>
);
