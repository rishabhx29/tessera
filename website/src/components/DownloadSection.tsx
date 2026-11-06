import React, { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, Check, Copy } from 'lucide-react';
import { Section, SectionHeading } from './Section';

const REPO_URL = 'https://github.com/rishabhx29/tessera';
const RELEASES_URL = 'https://github.com/rishabhx29/tessera/releases';
const CHANGELOG_URL =
  'https://github.com/rishabhx29/tessera/blob/main/CHANGELOG.md';

const BUILD_COMMAND = `git clone https://github.com/rishabhx29/tessera.git
cd tessera
cargo run -p tessera-app --release`;

const PLATFORMS = [
  {
    name: 'Windows 10 and 11',
    meta: 'x64',
    status: 'Publishing next',
    tone: 'next' as const,
    body: 'The NSIS installer is built and tested locally. It is 10.3 MB and leaves the models to a first-run download. Publishing it to GitHub Releases is the next step.',
  },
  {
    name: 'macOS',
    meta: 'Apple Silicon and Intel',
    status: 'Source only',
    tone: 'quiet' as const,
    body: 'The Vision OCR adapter compiles and passes a smoke test on a macOS CI runner. It has not been measured on Apple hardware, so no speed is claimed. There is no build to download.',
  },
  {
    name: 'Linux',
    meta: 'x86_64',
    status: 'Source only',
    tone: 'quiet' as const,
    body: 'The cross-platform ONNX fallback is the OCR path here. It is not packaged and has no installer. Build it from source if you want to try it.',
  },
];

export const DownloadSection: React.FC = () => {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copyBuildCommand = async () => {
    try {
      await navigator.clipboard.writeText(BUILD_COMMAND);
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  return (
    <Section id="download" tone="surface" labelledBy="download-title">
      <SectionHeading
        id="download-title"
        title="The first release is being prepared."
        lede="There is no public download yet. The Windows installer exists and has been tested on a real machine. The next step is publishing it. Until then, the source is open and builds."
        aside={
          <a
            href={CHANGELOG_URL}
            target="_blank"
            rel="noopener noreferrer"
            className="hairline-link"
          >
            Read the changelog
          </a>
        }
      />

      <div className="mt-10 grid gap-8 border-y border-line py-8 md:grid-cols-[minmax(0,1fr)_minmax(0,0.85fr)] md:items-center md:gap-12">
        <div>
          <span className="inline-flex items-center gap-2 rounded-full border border-accent/35 bg-accent-subtle px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.06em] text-accent">
            <span
              className="h-1.5 w-1.5 rounded-full bg-accent"
              aria-hidden="true"
            />
            Not published yet
          </span>
          <p className="mt-4 max-w-[44ch] font-serif text-[24px] font-bold leading-snug tracking-tight text-ink-primary">
            Follow GitHub Releases. The installer will land there first.
          </p>
          <div className="mt-5 flex flex-wrap gap-3">
            <a
              href={RELEASES_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-primary"
            >
              Follow releases
              <ArrowUpRight className="h-4 w-4" aria-hidden="true" />
            </a>
            <a
              href={REPO_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-secondary"
            >
              Browse the source
            </a>
          </div>
        </div>

        <dl className="grid grid-cols-2 gap-x-6 gap-y-4 border-line text-[12px] leading-relaxed md:border-l md:pl-10">
          <div>
            <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
              Installer size
            </dt>
            <dd className="mt-1 text-ink-primary">10.3 MB</dd>
          </div>
          <div>
            <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
              Models, first run
            </dt>
            <dd className="mt-1 text-ink-primary">About 1.3 GB</dd>
          </div>
          <div>
            <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
              License
            </dt>
            <dd className="mt-1 text-ink-primary">MIT</dd>
          </div>
          <div>
            <dt className="font-mono text-[10px] uppercase tracking-[0.06em] text-ink-faint">
              Price
            </dt>
            <dd className="mt-1 text-ink-primary">Free</dd>
          </div>
        </dl>
      </div>

      <div className="mt-2">
        {PLATFORMS.map((platform) => (
          <article
            key={platform.name}
            className="grid gap-3 border-b border-line py-6 md:grid-cols-[13rem_minmax(0,1fr)_auto] md:items-start md:gap-8"
          >
            <div>
              <h3 className="text-[15px] font-medium text-ink-primary">
                {platform.name}
              </h3>
              <p className="mt-0.5 font-mono text-[11px] text-ink-muted">
                {platform.meta}
              </p>
            </div>
            <p className="max-w-[62ch] text-[13px] leading-relaxed text-ink-secondary">
              {platform.body}
            </p>
            <span
              className={`w-fit rounded-full border px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.04em] ${
                platform.tone === 'next'
                  ? 'border-line-strong bg-subtle text-ink-primary'
                  : 'border-line bg-surface text-ink-muted'
              }`}
            >
              {platform.status}
            </span>
          </article>
        ))}
      </div>

      <div className="mt-10 border border-line bg-canvas">
        <div className="flex flex-col gap-3 border-b border-line px-5 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <h3 className="font-serif text-[18px] font-bold tracking-tight text-ink-primary">
              Build from source
            </h3>
            <p className="mt-0.5 text-[12px] text-ink-secondary">
              Needs Rust stable and the Tauri 2 prerequisites for your OS.
            </p>
          </div>
          <button
            type="button"
            onClick={copyBuildCommand}
            className="btn btn-secondary w-fit text-[13px]"
          >
            {copied ? (
              <>
                <Check className="h-4 w-4 text-accent" aria-hidden="true" />
                Copied
              </>
            ) : (
              <>
                <Copy className="h-4 w-4" aria-hidden="true" />
                Copy commands
              </>
            )}
            <span className="sr-only" role="status" aria-live="polite">
              {copied ? 'Build commands copied to clipboard' : ''}
            </span>
          </button>
        </div>
        <pre className="overflow-x-auto whitespace-pre-wrap break-all px-5 py-4 font-mono text-[11px] leading-relaxed text-ink-primary sm:whitespace-pre sm:break-normal sm:text-[12px]">
          <code>{BUILD_COMMAND}</code>
        </pre>
      </div>
    </Section>
  );
};
