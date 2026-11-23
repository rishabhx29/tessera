import React from 'react';
import { ArrowUpRight } from 'lucide-react';
import { GithubIcon } from './Icons';
import { RELEASES, REPO } from '../data/site';

const FACTS = [
  ['Runtime', 'WebView2, already on Windows'],
  ['Models', 'About 1.3 GB, once'],
  ['Account', 'None'],
  ['License', 'MIT'],
] as const;

export const Install: React.FC = () => (
  <section
    id="install"
    aria-labelledby="install-title"
    className="border-b border-night-line bg-night text-night-ink"
  >
    <div className="shell border-x border-night-line py-20 sm:py-24 md:py-28">
      <div className="grid items-start gap-12 lg:grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)] lg:gap-20">
        <div data-reveal className="min-w-0">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-night-muted">
            Install
          </p>
          <h2
            id="install-title"
            className="mt-5 font-display text-[clamp(2.6rem,5.6vw,4.6rem)] leading-[0.98] tracking-display"
          >
            Windows first.
            <br />
            <span className="italic text-[#E8A87C]">The installer is built.</span>
          </h2>
          <p className="mt-6 max-w-[44ch] text-[1.0625rem] leading-[1.7] text-night-muted">
            It has run on a clean Windows 11 machine. Publishing it to GitHub
            Releases is the next step, and no date is being promised. Releases is
            the only place it will appear.
          </p>

          <dl className="mt-10 grid max-w-md grid-cols-2 gap-x-6 gap-y-5 border-t border-night-line pt-6">
            {FACTS.map(([term, value]) => (
              <div key={term}>
                <dt className="font-mono text-[9px] uppercase tracking-[0.1em] text-night-muted">
                  {term}
                </dt>
                <dd className="mt-1 text-[13px] text-night-ink/90">{value}</dd>
              </div>
            ))}
          </dl>

          <p className="mt-6 font-mono text-[10px] uppercase tracking-[0.1em] text-night-muted">
            macOS and Linux remain source only
          </p>
        </div>

        <div
          data-reveal
          className="min-w-0 rounded-xl border border-night-line bg-night-soft p-6 sm:p-8"
        >
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <p className="font-display text-[1.9rem] leading-none tracking-display text-night-ink">
                Tessera for Windows
              </p>
              <p className="mt-2 font-mono text-[10px] uppercase tracking-[0.1em] text-night-muted">
                Windows 10 and 11 · x64
              </p>
            </div>
            <span className="w-fit rounded-sm border border-[#E8A87C]/50 px-2 py-1 font-mono text-[9px] uppercase tracking-[0.1em] text-[#E8A87C]">
              Built
            </span>
          </div>

          <div className="mt-7 flex items-start gap-2.5 rounded-lg border border-night-line bg-night px-4 py-3.5">
            <span
              aria-hidden="true"
              className="mt-[0.45rem] h-1.5 w-1.5 shrink-0 rounded-full bg-[#E8A87C]"
            />
            <p className="text-[12px] leading-relaxed text-night-muted">
              Next step: publishing the signed installer to Releases. Nothing is
              hosted on this page.
            </p>
          </div>

          <a
            href={RELEASES}
            target="_blank"
            rel="noopener noreferrer"
            className="btn btn-night mt-5 w-full"
          >
            Open Releases
            <ArrowUpRight className="h-4 w-4" aria-hidden="true" />
          </a>

          <a
            href={REPO}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-4 flex min-h-9 w-fit items-center gap-2 font-mono text-[11px] text-night-muted transition-colors duration-150 hover:text-night-ink"
          >
            <GithubIcon className="h-4 w-4" />
            Source on GitHub
          </a>

          <div className="mt-7 border-t border-night-line pt-5">
            <p className="text-[12.5px] leading-relaxed text-night-muted">
              Point it at the folders you want. It downloads its models once,
              verifies the hashes, then stays offline. Ctrl+Alt+T opens the box
              from anywhere.
            </p>
          </div>
        </div>
      </div>
    </div>
  </section>
);
