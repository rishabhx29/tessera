import React, { useState } from 'react';
import { Menu, X } from 'lucide-react';
import { GithubIcon } from './Icons';

export const Navbar: React.FC = () => {
  const [mobileMenuOpen, setMobileMenuOpen] = useState(false);

  return (
    <header className="sticky top-0 z-50 bg-canvas/95 backdrop-blur-md border-b border-line">
      <div className="max-w-6xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="flex items-center justify-between h-16">
          {/* Logo & Brand */}
          <a href="#" className="flex items-center gap-3 group">
            {/* Tessera Mosaic Icon */}
            <div className="w-8 h-8 rounded-md bg-subtle border border-line-strong p-1 grid grid-cols-2 gap-1 group-hover:border-accent transition-colors">
              <div className="bg-accent rounded-sm"></div>
              <div className="bg-ink-primary rounded-sm"></div>
              <div className="bg-ink-muted rounded-sm"></div>
              <div className="border border-accent rounded-sm"></div>
            </div>
            <div className="flex items-baseline gap-2">
              <span className="font-serif text-xl font-bold tracking-tight text-ink-primary">
                Tessera
              </span>
              <span className="hidden sm:inline-block text-xs font-mono px-2 py-0.5 rounded-full bg-subtle text-ink-muted border border-line">
                v0.1 pre-alpha
              </span>
            </div>
          </a>

          {/* Desktop Navigation */}
          <nav className="hidden md:flex items-center gap-8 text-sm font-medium text-ink-secondary">
            <a
              href="#how-it-works"
              className="hover:text-ink-primary transition-colors focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
            >
              How it works
            </a>
            <a
              href="#proof"
              className="hover:text-ink-primary transition-colors focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
            >
              Measured proof
            </a>
            <a
              href="#architecture"
              className="hover:text-ink-primary transition-colors focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
            >
              Local-first
            </a>
            <a
              href="#platforms"
              className="hover:text-ink-primary transition-colors focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
            >
              Platforms
            </a>
            <a
              href="#faq"
              className="hover:text-ink-primary transition-colors focus-visible:ring-2 focus-visible:ring-accent rounded-sm"
            >
              FAQ
            </a>
          </nav>

          {/* Right Action */}
          <div className="hidden sm:flex items-center gap-3">
            <a
              href="https://github.com/rishabhx29/tessera"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-2 px-3.5 py-1.5 text-xs font-medium rounded-md bg-surface text-ink-primary border border-line hover:border-ink-muted hover:shadow-sm transition-all focus-visible:ring-2 focus-visible:ring-accent"
              aria-label="Star Tessera on GitHub"
            >
              <GithubIcon className="w-4 h-4 text-ink-primary" />
              <span>Star on GitHub</span>
              <span className="px-1.5 py-0.2 bg-subtle text-ink-muted font-mono rounded text-[11px] border border-line">
                MIT
              </span>
            </a>
          </div>

          {/* Mobile menu button */}
          <div className="flex md:hidden">
            <button
              type="button"
              onClick={() => setMobileMenuOpen(!mobileMenuOpen)}
              className="p-2 rounded-md text-ink-secondary hover:text-ink-primary hover:bg-subtle border border-transparent hover:border-line focus-visible:ring-2 focus-visible:ring-accent"
              aria-expanded={mobileMenuOpen}
              aria-label="Toggle navigation menu"
            >
              {mobileMenuOpen ? <X className="w-5 h-5" /> : <Menu className="w-5 h-5" />}
            </button>
          </div>
        </div>
      </div>

      {/* Mobile Menu Dropdown */}
      {mobileMenuOpen && (
        <div className="md:hidden border-b border-line bg-canvas px-4 pt-2 pb-4 space-y-3">
          <nav className="flex flex-col space-y-2 text-sm font-medium text-ink-secondary">
            <a
              href="#how-it-works"
              onClick={() => setMobileMenuOpen(false)}
              className="px-3 py-2 rounded-md hover:bg-subtle hover:text-ink-primary"
            >
              How it works
            </a>
            <a
              href="#proof"
              onClick={() => setMobileMenuOpen(false)}
              className="px-3 py-2 rounded-md hover:bg-subtle hover:text-ink-primary"
            >
              Measured proof
            </a>
            <a
              href="#architecture"
              onClick={() => setMobileMenuOpen(false)}
              className="px-3 py-2 rounded-md hover:bg-subtle hover:text-ink-primary"
            >
              Local-first architecture
            </a>
            <a
              href="#platforms"
              onClick={() => setMobileMenuOpen(false)}
              className="px-3 py-2 rounded-md hover:bg-subtle hover:text-ink-primary"
            >
              Platforms & Downloads
            </a>
            <a
              href="#faq"
              onClick={() => setMobileMenuOpen(false)}
              className="px-3 py-2 rounded-md hover:bg-subtle hover:text-ink-primary"
            >
              FAQ
            </a>
          </nav>
          <div className="pt-2 border-t border-line">
            <a
              href="https://github.com/rishabhx29/tessera"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center justify-center gap-2 w-full py-2 px-4 rounded-md bg-surface text-ink-primary border border-line text-xs font-medium"
            >
              <GithubIcon className="w-4 h-4" />
              <span>Star on GitHub (MIT)</span>
            </a>
          </div>
        </div>
      )}
    </header>
  );
};
