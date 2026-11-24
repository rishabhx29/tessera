import React, { useEffect, useRef, useState } from 'react';
import { Download, Menu, Star, X } from 'lucide-react';
import { Mark } from './Mark';
import { DOWNLOAD, NAV_LINKS, REPO } from '../data/site';

export const Nav: React.FC = () => {
  const [open, setOpen] = useState(false);
  const [scrolled, setScrolled] = useState(false);
  const barRef = useRef<HTMLSpanElement>(null);
  const toggleRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let frame = 0;

    const update = () => {
      frame = 0;
      const max = document.documentElement.scrollHeight - window.innerHeight;
      const progress = max > 0 ? Math.min(1, window.scrollY / max) : 0;
      if (barRef.current) barRef.current.style.transform = `scaleX(${progress})`;
      setScrolled(window.scrollY > 16);
    };

    const onScroll = () => {
      if (frame) return;
      frame = window.requestAnimationFrame(update);
    };

    update();
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.removeEventListener('scroll', onScroll);
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      setOpen(false);
      // The menu unmounts, so put focus back on the control that owns it.
      toggleRef.current?.focus();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open]);

  return (
    <header
      className={`sticky top-0 z-50 border-b transition-colors duration-300 ${
        scrolled
          ? 'border-line bg-paper/95 backdrop-blur-md supports-[backdrop-filter]:bg-paper/92'
          : 'border-transparent bg-paper'
      }`}
    >
      <div className="shell">
        <div className="flex h-[4.5rem] items-center justify-between gap-6">
          <a
            href="#top"
            className="flex min-h-11 items-center gap-3 rounded-sm"
            aria-label="Tessera, back to top"
          >
            <Mark />
            <span className="font-display text-[1.65rem] leading-none tracking-display text-ink">
              Tessera
            </span>
          </a>

          <nav aria-label="Primary" className="hidden items-center gap-7 md:flex">
            {NAV_LINKS.map((link) => (
              <a
                key={link.href}
                href={link.href}
                className="flex min-h-11 items-center text-[0.9375rem] text-ink-soft transition-colors duration-150 hover:text-ink"
              >
                {link.label}
              </a>
            ))}
          </nav>

          <div className="flex items-center gap-1.5">
            <a
              href={REPO}
              target="_blank"
              rel="noopener noreferrer"
              className="hidden h-11 items-center gap-1.5 rounded-md px-2.5 text-ink-soft transition-colors duration-150 hover:text-ink sm:flex"
              aria-label="Star Tessera on GitHub"
            >
              <Star className="h-[17px] w-[17px]" aria-hidden="true" />
              <span className="hidden text-[0.9375rem] lg:inline">Star</span>
            </a>
            <a
              href={DOWNLOAD}
              className="btn btn-primary hidden !min-h-10 px-4 text-[0.875rem] sm:inline-flex"
            >
              <Download className="h-4 w-4" aria-hidden="true" />
              Download
            </a>
            <button
              ref={toggleRef}
              type="button"
              onClick={() => setOpen((value) => !value)}
              className="flex h-11 w-11 items-center justify-center rounded-md text-ink-soft transition-colors duration-150 hover:text-ink md:hidden"
              aria-expanded={open}
              aria-controls="mobile-nav"
              aria-label={open ? 'Close navigation menu' : 'Open navigation menu'}
            >
              {open ? <X className="h-5 w-5" /> : <Menu className="h-5 w-5" />}
            </button>
          </div>
        </div>
      </div>

      <span
        ref={barRef}
        aria-hidden="true"
        className="absolute inset-x-0 bottom-[-1px] h-[2px] origin-left scale-x-0 bg-accent"
      />

      {open && (
        <div
          id="mobile-nav"
          className="menu-in border-t border-line bg-paper md:hidden"
        >
          <nav aria-label="Mobile" className="shell flex flex-col pb-6 pt-2">
            {NAV_LINKS.map((link) => (
              <a
                key={link.href}
                href={link.href}
                onClick={() => setOpen(false)}
                className="flex min-h-14 items-center border-b border-line font-display text-[1.75rem] leading-none text-ink last:border-b-0"
              >
                {link.label}
              </a>
            ))}
            <div className="mt-5 flex gap-2">
              <a
                href={DOWNLOAD}
                className="btn btn-primary flex-1"
              >
                <Download className="h-4 w-4" aria-hidden="true" />
                Download
              </a>
              <a
                href={REPO}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-ghost flex-1"
              >
                <Star className="h-4 w-4" aria-hidden="true" />
                Star
              </a>
            </div>
          </nav>
        </div>
      )}
    </header>
  );
};
