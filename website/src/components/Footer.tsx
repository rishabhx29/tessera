import React from 'react';
import { GithubIcon } from './Icons';

const REPO = 'https://github.com/rishabhx29/tessera';

const DOC_LINKS = [
  ['Product definition', `${REPO}/blob/main/docs/product-definition.md`],
  ['Performance log', `${REPO}/blob/main/docs/performance.md`],
  ['Architecture', `${REPO}/blob/main/docs/architecture.md`],
  ['Security', `${REPO}/blob/main/SECURITY.md`],
] as const;

const PROJECT_LINKS = [
  ['Source', REPO],
  ['Releases', `${REPO}/releases`],
  ['Issues', `${REPO}/issues`],
  ['Changelog', `${REPO}/blob/main/CHANGELOG.md`],
] as const;

const Mark: React.FC = () => (
  <span className="grid h-6 w-6 grid-cols-2 gap-0.5 rounded border border-line-strong bg-subtle p-0.5">
    <span className="rounded-[1px] bg-accent" />
    <span className="rounded-[1px] bg-ink-primary" />
    <span className="rounded-[1px] bg-ink-muted" />
    <span className="rounded-[1px] border border-accent" />
  </span>
);

export const Footer: React.FC = () => (
  <footer className="bg-canvas text-[12px] text-ink-secondary">
    <div className="mx-auto w-full max-w-6xl border-x border-line px-5 py-12 sm:px-8 lg:px-12">
      <div className="grid gap-10 sm:grid-cols-2 md:grid-cols-[minmax(0,1.4fr)_minmax(0,0.8fr)_minmax(0,0.8fr)]">
        <div>
          <a
            href="#top"
            className="inline-flex items-center gap-2.5 rounded"
            aria-label="Tessera, back to top"
          >
            <Mark />
            <span className="font-serif text-lg font-bold text-ink-primary">
              Tessera
            </span>
          </a>
          <p className="mt-4 max-w-[38ch] leading-relaxed">
            One search bar for screenshots, PDFs, and documents. Local, offline,
            and open source. Windows first, with the first public release still
            being prepared.
          </p>
          <p className="mt-4 font-mono text-[11px] text-ink-faint">
            Windows 10 and 11 x64. MIT license.
          </p>
        </div>

        <nav aria-labelledby="footer-docs">
          <h2
            id="footer-docs"
            className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-faint"
          >
            Documentation
          </h2>
          <ul className="mt-3 space-y-2">
            {DOC_LINKS.map(([label, href]) => (
              <li key={label}>
                <a
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex min-h-6 items-center transition-colors duration-150 hover:text-ink-primary"
                >
                  {label}
                </a>
              </li>
            ))}
          </ul>
        </nav>

        <nav aria-labelledby="footer-project">
          <h2
            id="footer-project"
            className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-faint"
          >
            Project
          </h2>
          <ul className="mt-3 space-y-2">
            {PROJECT_LINKS.map(([label, href]) => (
              <li key={label}>
                <a
                  href={href}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="inline-flex min-h-6 items-center transition-colors duration-150 hover:text-ink-primary"
                >
                  {label}
                </a>
              </li>
            ))}
            <li>
              <a
                href="https://github.com/rishabhx29"
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex min-h-6 items-center gap-1.5 transition-colors duration-150 hover:text-ink-primary"
              >
                <GithubIcon className="h-3.5 w-3.5" />
                @rishabhx29
              </a>
            </li>
          </ul>
        </nav>
      </div>

      <div className="mt-10 flex flex-col gap-3 border-t border-line pt-6 font-mono text-[10px] text-ink-faint sm:flex-row sm:items-center sm:justify-between">
        <span>© 2026 Tessera contributors</span>
        <span className="flex flex-wrap gap-x-4 gap-y-1">
          <span>No cookies</span>
          <span>No telemetry</span>
          <span>No accounts</span>
          <span>MIT</span>
        </span>
      </div>
    </div>
  </footer>
);
