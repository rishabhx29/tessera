import React from 'react';
import { Database, Lock, ShieldCheck, WifiOff } from 'lucide-react';
import { Section, SectionHeading } from './Section';

const PILLARS = [
  {
    icon: WifiOff,
    title: 'No network at query time',
    body: 'Search runs in the app process against the local index. Turn off Wi-Fi and results are the same. Product Definition v1.0 puts it plainly: no network calls at query time.',
    note: 'No accounts. No update pings. No heartbeat.',
  },
  {
    icon: Lock,
    title: 'No telemetry to collect',
    body: 'There is no signup, no analytics, and no error reporting service compiled into the binary. Tessera has no server that could receive your queries, even if it wanted to.',
    note: 'Nothing to opt out of, because nothing is sent.',
  },
  {
    icon: Database,
    title: 'One file you can inspect',
    body: 'Text, metadata, and embeddings live in a single SQLite index. Open it, back it up, move it, or delete it. It stays yours.',
    note: 'Measured: 113.7 MB at 5,000 files, 686 MB at 50,000.',
  },
  {
    icon: ShieldCheck,
    title: 'Models checked on arrival',
    body: 'The models download once, on first run, and each file is checked against a SHA-256 hash pinned in the binary. A single changed byte is rejected before loading.',
    note: 'About 1.3 GB of models. The installer itself is 10.3 MB.',
  },
];

export const PrivacySection: React.FC = () => (
  <Section id="privacy" labelledBy="privacy-title">
    <SectionHeading
      id="privacy-title"
      title="Your files stay on your machine."
      lede="Privacy here is a property of the build, not a promise in a policy page. The search path has no network step, and the index is an ordinary file on your disk."
      aside={
        <a
          href="https://github.com/rishabhx29/tessera/blob/main/SECURITY.md"
          target="_blank"
          rel="noopener noreferrer"
          className="hairline-link"
        >
          Read the security policy
        </a>
      }
    />

    <div className="mt-12 grid border-t border-line md:grid-cols-2">
      {PILLARS.map(({ icon: Icon, title, body, note }, index) => (
        <article
          key={title}
          className={`border-b border-line py-8 md:py-10 ${
            index % 2 === 0 ? 'md:border-r md:pr-10' : 'md:pl-10'
          }`}
        >
          <Icon
            className="h-5 w-5 text-accent"
            strokeWidth={1.6}
            aria-hidden="true"
          />
          <h3 className="mt-4 font-serif text-[21px] font-bold leading-snug tracking-tight text-ink-primary">
            {title}
          </h3>
          <p className="mt-2 max-w-[46ch] text-[14px] leading-relaxed text-ink-secondary">
            {body}
          </p>
          <p className="mt-4 font-mono text-[11px] leading-relaxed text-ink-faint">
            {note}
          </p>
        </article>
      ))}
    </div>

    <div className="flex flex-col gap-2 py-6 sm:flex-row sm:items-baseline sm:justify-between">
      <p className="max-w-[62ch] text-[13px] leading-relaxed text-ink-secondary">
        <span className="font-medium text-ink-primary">Planned, not built:</span>{' '}
        an MCP server in v0.3 that would let a local assistant query the same
        index. It is on the roadmap, and it is not in this build.
      </p>
      <span className="w-fit shrink-0 rounded-full border border-line bg-subtle px-2 py-0.5 font-mono text-[10px] uppercase tracking-[0.04em] text-ink-muted">
        v0.3 plan
      </span>
    </div>
  </Section>
);
