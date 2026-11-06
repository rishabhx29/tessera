import React from 'react';
import { ArrowUp } from 'lucide-react';
import { GithubIcon } from './Icons';
import { Mark } from './Mark';
import { ISSUES, RELEASES, REPO } from '../data/site';

const COLUMNS = [
  {
    title: 'Product',
    links: [
      { label: 'How it works', href: '#product' },
      { label: 'The sample search', href: '#demo' },
      { label: 'Measured numbers', href: '#numbers' },
      { label: 'Privacy', href: '#privacy' },
      { label: 'Install', href: '#install' },
    ],
  },
  {
    title: 'Project',
    links: [
      { label: 'Source', href: REPO },
      { label: 'Releases', href: RELEASES },
      { label: 'Issues', href: ISSUES },
      { label: 'Changelog', href: `${REPO}/blob/main/CHANGELOG.md` },
    ],
  },
  {
    title: 'Documents',
    links: [
      { label: 'Product definition', href: `${REPO}/blob/main/docs/product-definition.md` },
      { label: 'Architecture', href: `${REPO}/blob/main/docs/architecture.md` },
      { label: 'Performance log', href: `${REPO}/blob/main/docs/performance.md` },
      { label: 'Security', href: `${REPO}/blob/main/SECURITY.md` },
    ],
  },
] as const;

const isExternal = (href: string) => href.startsWith('http');

export const Footer: React.FC = () => (
  <footer className="bg-paper-card text-[13px] text-ink-soft">
    <div className="shell border-x border-line">
      <div className="grid gap-10 border-b border-line py-14 sm:grid-cols-2 md:grid-cols-[minmax(0,1.3fr)_repeat(3,minmax(0,0.7fr))] md:gap-8">
        <div className="max-w-[34ch]">
          <a
            href="#top"
            className="inline-flex items-center gap-2.5 rounded-sm"
            aria-label="Tessera, back to top"
          >
            <Mark />
            <span className="font-display text-[1.5rem] leading-none tracking-display text-ink">
              Tessera
            </span>
          </a>
          <p className="mt-4 text-[13px] leading-relaxed text-ink-muted">
            One search bar for screenshots, PDFs and documents. Local, offline and
            open source. Windows first, with the first public release still being
            prepared.
          </p>
          <a
            href={REPO}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-5 inline-flex min-h-9 items-center gap-2 font-mono text-[11px] text-ink-muted transition-colors duration-150 hover:text-ink"
          >
            <GithubIcon className="h-4 w-4" />
            rishabhx29/tessera
          </a>
        </div>

        {COLUMNS.map((column) => (
          <nav key={column.title} aria-labelledby={`footer-${column.title.toLowerCase()}`}>
            <h2
              id={`footer-${column.title.toLowerCase()}`}
              className="font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted"
            >
              {column.title}
            </h2>
            <ul className="mt-4 space-y-2.5">
              {column.links.map((link) => (
                <li key={link.label}>
                  <a
                    href={link.href}
                    {...(isExternal(link.href)
                      ? { target: '_blank', rel: 'noopener noreferrer' }
                      : {})}
                    className="inline-flex min-h-6 items-center text-[13px] text-ink-soft transition-colors duration-150 hover:text-ink"
                  >
                    {link.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        ))}
      </div>

      <div className="flex flex-col gap-5 py-8 sm:flex-row sm:items-end sm:justify-between">
        <p className="font-display text-[clamp(2.6rem,7vw,5rem)] leading-none tracking-display text-ink">
          One box.
          <span className="italic text-accent"> Every file.</span>
        </p>
        <a
          href="#top"
          className="inline-flex min-h-9 w-fit items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-ink-muted transition-colors duration-150 hover:text-ink"
        >
          Back to top
          <ArrowUp className="h-3.5 w-3.5" aria-hidden="true" />
        </a>
      </div>

      <div className="flex flex-col gap-3 border-t border-line py-5 font-mono text-[10px] text-ink-muted sm:flex-row sm:items-center sm:justify-between">
        <span>© 2026 Tessera contributors · MIT</span>
        <span className="flex flex-wrap gap-x-4 gap-y-1">
          <span>No cookies</span>
          <span>No telemetry</span>
          <span>No accounts</span>
          <span>Windows 10 and 11 x64</span>
        </span>
      </div>
    </div>
  </footer>
);
