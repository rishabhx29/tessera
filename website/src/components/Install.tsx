import React, { useEffect, useRef, useState } from 'react';
import { ArrowUpRight, Check, Copy, Terminal } from 'lucide-react';
import { GithubIcon } from './Icons';
import { RELEASES, REPO } from '../data/site';

const COMMANDS = [
  'git clone https://github.com/rishabhx29/tessera.git',
  'cd tessera',
  'cargo run -p tessera-app',
] as const;

const PLATFORMS = [
  ['Windows 10 and 11, x64', 'Installer built, tested locally. Publishing is the next step.', 'First'],
  ['macOS', 'The Vision adapter compiles and passes a smoke test. Not measured on Apple hardware.', 'Source only'],
  ['Linux', 'The ONNX fallback path builds from source.', 'Source only'],
] as const;

export const Install: React.FC = () => {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(timer.current), []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(COMMANDS.join('\n'));
      setCopied(true);
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => setCopied(false), 2200);
    } catch {
      setCopied(false);
    }
  };

  return (
    <section
      id="install"
      aria-labelledby="install-title"
      className="border-b border-night-line bg-night text-night-ink"
    >
      <div className="shell border-x border-night-line py-20 sm:py-24 md:py-28">
        <div className="grid gap-12 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] lg:gap-16">
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
              Releases is the next step, and no date is being promised. Until
              then, the source builds in three commands.
            </p>

            <div className="mt-8 flex flex-col gap-3 sm:flex-row">
              <a
                href={RELEASES}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-night w-full sm:w-auto"
              >
                Watch Releases
                <ArrowUpRight className="h-4 w-4" aria-hidden="true" />
              </a>
              <a
                href={REPO}
                target="_blank"
                rel="noopener noreferrer"
                className="btn btn-night-ghost w-full sm:w-auto"
              >
                <GithubIcon className="h-4 w-4" />
                Source on GitHub
              </a>
            </div>

            <dl className="mt-10 grid grid-cols-2 gap-x-6 gap-y-4 border-t border-night-line pt-6 text-[12px] sm:grid-cols-3">
              {[
                ['Runtime', 'WebView2, already present'],
                ['Models', 'About 1.3 GB, once'],
                ['Hardware', 'CPU only, no NPU'],
                ['Account', 'None'],
                ['License', 'MIT'],
                ['Data', 'One folder, yours'],
              ].map(([term, value]) => (
                <div key={term}>
                  <dt className="font-mono text-[9px] uppercase tracking-[0.1em] text-night-muted">
                    {term}
                  </dt>
                  <dd className="mt-1 text-night-ink/90">{value}</dd>
                </div>
              ))}
            </dl>
          </div>

          <div data-reveal className="min-w-0 lg:pt-3">
            <div className="overflow-hidden rounded-xl border border-night-line bg-[#0B0A08]">
              <div className="flex items-center justify-between gap-3 border-b border-night-line px-4 py-2.5">
                <span className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.1em] text-night-muted">
                  <Terminal className="h-3.5 w-3.5" aria-hidden="true" />
                  Build from source
                </span>
                <button
                  type="button"
                  onClick={copy}
                  className="inline-flex min-h-8 items-center gap-1.5 rounded border border-night-line px-2 py-1 font-mono text-[10px] text-night-muted transition-colors duration-150 hover:border-night-muted hover:text-night-ink"
                >
                  {copied ? (
                    <>
                      <Check className="h-3 w-3 text-[#9DB68F]" aria-hidden="true" />
                      Copied
                    </>
                  ) : (
                    <>
                      <Copy className="h-3 w-3" aria-hidden="true" />
                      Copy
                    </>
                  )}
                </button>
              </div>

              <pre className="overflow-x-auto p-4 font-mono text-[11.5px] leading-[2] text-night-muted sm:p-5">
                <code>
                  {COMMANDS.map((command) => (
                    <span key={command} className="block break-all whitespace-pre-wrap sm:break-normal sm:whitespace-pre">
                      <span className="select-none text-[#E8A87C]" aria-hidden="true">
                        {'> '}
                      </span>
                      {command}
                    </span>
                  ))}
                </code>
              </pre>

              <p className="border-t border-night-line px-4 py-2.5 text-[11px] leading-relaxed text-night-muted sm:px-5">
                Needs a Rust toolchain. The app fetches and verifies its models on
                first run, then makes no further requests.
              </p>
            </div>

            <p aria-live="polite" className="sr-only">
              {copied ? 'Build commands copied to the clipboard.' : ''}
            </p>

            <div className="mt-4 rounded-lg border border-night-line bg-night-soft px-4 py-3.5">
              <p className="flex items-start gap-2 text-[12px] leading-relaxed text-night-muted">
                <span
                  className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-[#E8A87C]"
                  aria-hidden="true"
                />
                No installer is hosted on this page. The Releases page is the only
                place it will appear.
              </p>
            </div>

            <div className="mt-4 rounded-lg border border-night-line p-5">
              <p className="font-mono text-[10px] uppercase tracking-[0.12em] text-night-muted">
                First run
              </p>
              <ol className="mt-3 space-y-2.5">
                {[
                  'Point it at the folders you want watched.',
                  'It downloads about 1.3 GB of models, once, and checks the hashes.',
                  'It indexes at idle priority, newest file first.',
                  'Ctrl+Alt+T opens the box from anywhere.',
                ].map((step, index) => (
                  <li key={step} className="flex gap-3 text-[12.5px] leading-relaxed text-night-muted">
                    <span className="font-mono text-[10px] text-[#E8A87C]">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                    {step}
                  </li>
                ))}
              </ol>
            </div>
          </div>
        </div>

        <div className="mt-16 border-t border-night-line pt-2" data-reveal>
          {PLATFORMS.map(([name, note, status]) => (
            <div
              key={name}
              className="grid gap-2 border-b border-night-line py-5 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.4fr)_auto] md:items-center md:gap-8"
            >
              <p className="text-[15px] font-medium text-night-ink">{name}</p>
              <p className="text-[13px] leading-relaxed text-night-muted">{note}</p>
              <span
                className={`w-fit rounded-sm border px-2 py-1 font-mono text-[9px] uppercase tracking-[0.1em] ${
                  status === 'First'
                    ? 'border-[#E8A87C]/50 text-[#E8A87C]'
                    : 'border-night-line text-night-muted'
                }`}
              >
                {status}
              </span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
};
