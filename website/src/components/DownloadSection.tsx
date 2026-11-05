import React, { useState } from 'react';
import {
  Check,
  Copy,
  ExternalLink,
} from 'lucide-react';

export const DownloadSection: React.FC = () => {
  const [copied, setCopied] = useState(false);
  const buildCommand = 'git clone https://github.com/rishabhx29/tessera.git\ncd tessera\ncargo build --release';

  const handleCopy = () => {
    navigator.clipboard.writeText(buildCommand);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <section id="platforms" className="py-20 bg-surface border-b border-line">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="max-w-3xl mb-12">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            Availability & Releases
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary">
            Download Tessera for your operating system.
          </h2>
          <p className="mt-3 text-base sm:text-lg text-ink-secondary leading-relaxed">
            Tessera is completely free and open source. Windows 10/11 is our primary launch target for v0.1, with macOS and Linux adapters engineered alongside.
          </p>
        </div>

        {/* Platform Matrix */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-12">
          {/* Windows Platform Card (Primary) */}
          <div className="p-6 rounded-xl bg-canvas border-2 border-accent/40 shadow-tile flex flex-col justify-between relative overflow-hidden">
            <div className="absolute top-3 right-3">
              <span className="px-2 py-0.5 rounded-full bg-accent-subtle text-accent text-[11px] font-mono font-semibold border border-accent/30">
                Primary Target
              </span>
            </div>

            <div>
              <div className="w-10 h-10 rounded-lg bg-surface border border-line flex items-center justify-center text-ink-primary mb-4 font-mono font-bold">
                🪟
              </div>
              <h3 className="font-serif text-xl font-bold text-ink-primary mb-1">
                Windows 10 / 11
              </h3>
              <div className="text-xs font-mono text-ink-muted mb-4">
                x64 architecture
              </div>

              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Hardware-accelerated OCR using native <code className="text-xs font-mono bg-subtle px-1 py-0.5 rounded">Windows.Media.Ocr</code>.
                Tested on builds up to Windows 11 build 26300.
              </p>

              <div className="space-y-1.5 text-xs text-ink-secondary font-mono">
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-accent" />
                  <span>34.4 ms p50 OCR extraction</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-accent" />
                  <span>Background idle scheduling</span>
                </div>
              </div>
            </div>

            <div className="mt-6 pt-4 border-t border-line">
              <a
                href="https://github.com/rishabhx29/tessera/releases"
                target="_blank"
                rel="noopener noreferrer"
                className="w-full inline-flex items-center justify-center gap-2 py-2.5 px-4 rounded-md bg-accent text-white font-medium text-xs hover:bg-accent-hover transition-colors shadow-sm"
              >
                <span>Windows build — coming with v0.1</span>
                <ExternalLink className="w-3.5 h-3.5" />
              </a>
              <div className="text-[11px] text-center font-mono text-ink-muted mt-2">
                Release binary will land on GitHub Releases
              </div>
            </div>
          </div>

          {/* macOS Platform Card */}
          <div className="p-6 rounded-xl bg-canvas border border-line shadow-tile flex flex-col justify-between">
            <div>
              <div className="w-10 h-10 rounded-lg bg-surface border border-line flex items-center justify-center text-ink-primary mb-4 font-mono font-bold">
                🍎
              </div>
              <h3 className="font-serif text-xl font-bold text-ink-primary mb-1">
                macOS
              </h3>
              <div className="text-xs font-mono text-ink-muted mb-4">
                Apple Silicon (M1/M2/M3/M4) & Intel
              </div>

              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Native Apple Vision OCR framework adapter (<code className="text-xs font-mono bg-subtle px-1 py-0.5 rounded">vision.rs</code>).
                Built and verified on macOS-14 GitHub Actions runner.
              </p>

              <div className="space-y-1.5 text-xs text-ink-secondary font-mono">
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-amber-600" />
                  <span>CI-verified on macOS runners</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-ink-muted" />
                  <span>Latency unmeasured on physical Mac</span>
                </div>
              </div>
            </div>

            <div className="mt-6 pt-4 border-t border-line">
              <a
                href="https://github.com/rishabhx29/tessera"
                target="_blank"
                rel="noopener noreferrer"
                className="w-full inline-flex items-center justify-center gap-2 py-2.5 px-4 rounded-md bg-surface text-ink-primary border border-line font-medium text-xs hover:bg-subtle transition-colors"
              >
                <span>Source available on GitHub</span>
              </a>
              <div className="text-[11px] text-center font-mono text-ink-muted mt-2">
                Targeting experimental v0.1.x
              </div>
            </div>
          </div>

          {/* Linux Platform Card */}
          <div className="p-6 rounded-xl bg-canvas border border-line shadow-tile flex flex-col justify-between">
            <div>
              <div className="w-10 h-10 rounded-lg bg-surface border border-line flex items-center justify-center text-ink-primary mb-4 font-mono font-bold">
                🐧
              </div>
              <h3 className="font-serif text-xl font-bold text-ink-primary mb-1">
                Linux
              </h3>
              <div className="text-xs font-mono text-ink-muted mb-4">
                x86_64 / aarch64
              </div>

              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Powered by the cross-platform ONNX fallback engine (PP-OCRv6 det-tiny + rec-small).
                Supports open desktop environments.
              </p>

              <div className="space-y-1.5 text-xs text-ink-secondary font-mono">
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-accent" />
                  <span>ADR-0007 fallback design</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="w-1.5 h-1.5 rounded-full bg-accent" />
                  <span>Headless CLI search support</span>
                </div>
              </div>
            </div>

            <div className="mt-6 pt-4 border-t border-line">
              <a
                href="https://github.com/rishabhx29/tessera"
                target="_blank"
                rel="noopener noreferrer"
                className="w-full inline-flex items-center justify-center gap-2 py-2.5 px-4 rounded-md bg-surface text-ink-primary border border-line font-medium text-xs hover:bg-subtle transition-colors"
              >
                <span>Build via Cargo</span>
              </a>
              <div className="text-[11px] text-center font-mono text-ink-muted mt-2">
                Pure Rust core + ONNX Runtime
              </div>
            </div>
          </div>
        </div>

        {/* Build From Source Snippet (Resend / Developer Craft) */}
        <div className="p-6 rounded-xl bg-canvas border border-line-strong shadow-tile">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
            <div>
              <div className="font-serif text-lg font-bold text-ink-primary">
                Build from source today
              </div>
              <div className="text-xs text-ink-secondary">
                Requires standard Rust 1.80+ and Cargo toolchain.
              </div>
            </div>

            <button
              onClick={handleCopy}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-surface border border-line text-xs font-mono text-ink-primary hover:border-ink-muted transition-colors self-start sm:self-auto"
            >
              {copied ? (
                <>
                  <Check className="w-3.5 h-3.5 text-green-600" />
                  <span className="text-green-700 font-semibold">Copied command</span>
                </>
              ) : (
                <>
                  <Copy className="w-3.5 h-3.5 text-ink-muted" />
                  <span>Copy shell snippet</span>
                </>
              )}
            </button>
          </div>

          <div className="bg-subtle p-4 rounded-lg border border-line font-mono text-xs text-ink-primary overflow-x-auto">
            <pre className="text-xs leading-relaxed">{buildCommand}</pre>
          </div>
        </div>
      </div>
    </section>
  );
};
