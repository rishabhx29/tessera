import React, { useEffect, useState } from 'react';
import { ArrowUpRight, Menu, X } from 'lucide-react';
import { GithubIcon } from './Icons';

const LINKS = [
  { href: '#demo', label: 'Demo' },
  { href: '#how-it-works', label: 'How it works' },
  { href: '#measured', label: 'Measured' },
  { href: '#privacy', label: 'Privacy' },
  { href: '#faq', label: 'FAQ' },
];

const RELEASES_URL = 'https://github.com/rishabhx29/tessera/releases';
const REPO_URL = 'https://github.com/rishabhx29/tessera';

export const Navbar: React.FC = () => {
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setOpen(false);
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [open]);

  return (
    <header className="sticky top-0 z-50 border-b border-line bg-canvas/95 backdrop-blur-sm">
      <div className="mx-auto w-full max-w-6xl border-x border-line px-5 sm:px-8 lg:px-12">
        <div className="flex h-16 items-center justify-between gap-4">
          <a
            href="#top"
            className="flex min-h-11 items-center gap-2.5 rounded-md"
            aria-label="Tessera, back to top"
          >
            <span
              aria-hidden="true"
              className="grid h-7 w-7 grid-cols-2 gap-[3px] rounded-[6px] border border-line-strong bg-subtle p-[3px]"
            >
              <span className="rounded-[1px] bg-accent" />
              <span className="rounded-[1px] bg-ink-primary" />
              <span className="rounded-[1px] bg-ink-muted" />
              <span className="rounded-[1px] border border-accent" />
            </span>
            <span className="font-serif text-[19px] font-bold tracking-tight text-ink-primary">
              Tessera
            </span>
            <span className="hidden items-center gap-1.5 rounded-full border border-line bg-surface px-2.5 py-1 font-mono text-[10px] text-ink-muted sm:inline-flex">
              <span className="h-1.5 w-1.5 rounded-full bg-accent" aria-hidden="true" />
              First release soon
            </span>
          </a>

          <nav
            aria-label="Primary"
            className="hidden items-center gap-1 md:flex"
          >
            {LINKS.map((link) => (
              <a
                key={link.href}
                href={link.href}
                className="flex min-h-11 items-center rounded-md px-3 text-[13px] text-ink-secondary transition-colors duration-150 hover:text-ink-primary"
              >
                {link.label}
              </a>
            ))}
          </nav>

          <div className="flex items-center gap-1">
            <a
              href={REPO_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="hidden h-11 w-11 items-center justify-center rounded-md text-ink-secondary transition-colors duration-150 hover:bg-subtle hover:text-ink-primary sm:flex"
              aria-label="Tessera on GitHub"
            >
              <GithubIcon className="h-4 w-4" />
            </a>
            <a
              href={RELEASES_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-primary hidden !min-h-9 px-3.5 text-xs sm:inline-flex"
            >
              Releases
              <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
            </a>
            <button
              type="button"
              onClick={() => setOpen((value) => !value)}
              className="flex h-11 w-11 items-center justify-center rounded-md text-ink-secondary transition-colors duration-150 hover:bg-subtle hover:text-ink-primary md:hidden"
              aria-expanded={open}
              aria-controls="mobile-nav"
              aria-label={open ? 'Close navigation menu' : 'Open navigation menu'}
            >
              {open ? <X className="h-5 w-5" /> : <Menu className="h-5 w-5" />}
            </button>
          </div>
        </div>
      </div>

      {open && (
        <div
          id="mobile-nav"
          className="swap-in border-t border-line bg-canvas md:hidden"
        >
          <nav
            aria-label="Mobile"
            className="mx-auto flex w-full max-w-6xl flex-col border-x border-line px-5 pb-5 pt-2"
          >
            {LINKS.map((link) => (
              <a
                key={link.href}
                href={link.href}
                onClick={() => setOpen(false)}
                className="flex min-h-12 items-center border-b border-line text-[15px] text-ink-primary last:border-b-0"
              >
                {link.label}
              </a>
            ))}
            <div className="mt-4 flex gap-2">
              <a
                href={RELEASES_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-primary flex-1"
              >
                Releases
                <ArrowUpRight className="h-4 w-4" aria-hidden="true" />
              </a>
              <a
                href={REPO_URL}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-secondary flex-1"
              >
                <GithubIcon className="h-4 w-4" />
                Source
              </a>
            </div>
          </nav>
        </div>
      )}
    </header>
  );
};
