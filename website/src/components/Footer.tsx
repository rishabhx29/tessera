import React from 'react';
import { GithubIcon } from './Icons';

export const Footer: React.FC = () => {
  return (
    <footer className="bg-canvas border-t border-line text-xs text-ink-secondary py-14">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-4 gap-8 mb-12">
          {/* Brand Col */}
          <div className="space-y-3 sm:col-span-2">
            <div className="flex items-center gap-2.5">
              <div className="w-6 h-6 rounded bg-subtle border border-line-strong p-0.5 grid grid-cols-2 gap-0.5">
                <div className="bg-accent rounded-[1px]"></div>
                <div className="bg-ink-primary rounded-[1px]"></div>
                <div className="bg-ink-muted rounded-[1px]"></div>
                <div className="border border-accent rounded-[1px]"></div>
              </div>
              <span className="font-serif text-lg font-bold text-ink-primary">
                Tessera
              </span>
            </div>

            <p className="text-xs text-ink-secondary max-w-sm leading-relaxed">
              A local-first semantic search engine for your own files — images, screenshots, PDFs, and documents — that runs entirely on your machine, answers in milliseconds, and never uploads anything.
            </p>

            <div className="font-mono text-[11px] text-ink-muted pt-1">
              Windows 10/11 x64 · macOS adapter on CI · MIT License
            </div>
          </div>

          {/* Docs Links */}
          <div className="space-y-2.5 font-sans">
            <div className="font-serif text-sm font-bold text-ink-primary">
              Documentation
            </div>
            <ul className="space-y-2 text-xs">
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/blob/main/docs/product-definition.md"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Product Definition v1.0
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/blob/main/docs/performance.md"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Performance Benchmarks
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/blob/main/docs/architecture.md"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  System Architecture
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/blob/main/SECURITY.md"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Security & Zero-Telemetry
                </a>
              </li>
            </ul>
          </div>

          {/* Project & Community */}
          <div className="space-y-2.5 font-sans">
            <div className="font-serif text-sm font-bold text-ink-primary">
              Project & Contact
            </div>
            <ul className="space-y-2 text-xs">
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors inline-flex items-center gap-1.5"
                >
                  <GithubIcon className="w-3.5 h-3.5" />
                  <span>GitHub Repository</span>
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/releases"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Releases
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29/tessera/issues"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Issue Tracker
                </a>
              </li>
              <li>
                <a
                  href="https://github.com/rishabhx29"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="hover:text-ink-primary hover:underline transition-colors"
                >
                  Maintainer: @rishabhx29
                </a>
              </li>
            </ul>
          </div>
        </div>

        {/* Bottom bar */}
        <div className="pt-8 border-t border-line flex flex-col sm:flex-row items-center justify-between gap-4 font-mono text-[11px] text-ink-muted">
          <div>
            © 2026 Tessera Contributors. Open source under the MIT License.
          </div>
          <div className="flex items-center gap-4">
            <span>No cookies</span>
            <span>·</span>
            <span>No telemetry</span>
            <span>·</span>
            <span>No cloud accounts</span>
          </div>
        </div>
      </div>
    </footer>
  );
};
