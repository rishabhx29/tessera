import React from 'react';
import { ShieldCheck, Database, Lock, FileCode, Radio } from 'lucide-react';

export const PrivacySection: React.FC = () => {
  return (
    <section id="architecture" className="py-20 bg-canvas border-b border-line">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="max-w-2xl mb-12">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            Local-First Architecture
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary">
            Your files never leave your machine. Not even for search.
          </h2>
          <p className="mt-3 text-base sm:text-lg text-ink-secondary leading-relaxed">
            Privacy in Tessera is not a promise in an unreadable policy document; it is an architectural property enforced by code.
          </p>
        </div>

        {/* 4 Pillars Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6 mb-12">
          {/* Pillar 1 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile">
            <div className="w-10 h-10 rounded-lg bg-subtle border border-line flex items-center justify-center text-accent mb-4">
              <Radio className="w-5 h-5 text-accent" />
            </div>
            <h3 className="font-serif text-xl font-bold text-ink-primary mb-2">
              Zero network calls at query time
            </h3>
            <p className="text-sm text-ink-secondary leading-relaxed mb-3">
              Pressing Alt+Space and searching executes entirely in-process against your local SQLite database and memory-mapped ONNX session.
              You can disconnect Wi-Fi entirely; search behaves identically.
            </p>
            <div className="text-xs font-mono text-ink-muted">
              Invariant: 0 inbound or outbound network sockets opened during search.
            </div>
          </div>

          {/* Pillar 2 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile">
            <div className="w-10 h-10 rounded-lg bg-subtle border border-line flex items-center justify-center text-accent mb-4">
              <Lock className="w-5 h-5 text-accent" />
            </div>
            <h3 className="font-serif text-xl font-bold text-ink-primary mb-2">
              No accounts, no telemetry, no tracking
            </h3>
            <p className="text-sm text-ink-secondary leading-relaxed mb-3">
              There is no signup form, no email collection, no usage analytics, and no background heartbeat.
              Tessera has no servers to collect your personal habits or document queries.
            </p>
            <div className="text-xs font-mono text-ink-muted">
              Invariant: No user identity layer. No telemetry events compiled into the binary.
            </div>
          </div>

          {/* Pillar 3 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile">
            <div className="w-10 h-10 rounded-lg bg-subtle border border-line flex items-center justify-center text-accent mb-4">
              <Database className="w-5 h-5 text-accent" />
            </div>
            <h3 className="font-serif text-xl font-bold text-ink-primary mb-2">
              The index is a standard SQLite file
            </h3>
            <p className="text-sm text-ink-secondary leading-relaxed mb-3">
              All OCR text, metadata, document tiles, and vector embeddings reside in a standard SQLite file (<code className="text-xs font-mono text-ink-primary">tessera.db</code>).
              You can open it in DB Browser, back it up, move it to an external drive, or delete it completely.
            </p>
            <div className="text-xs font-mono text-ink-muted">
              Invariant: Portable standard format. No proprietary locked databases.
            </div>
          </div>

          {/* Pillar 4 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile">
            <div className="w-10 h-10 rounded-lg bg-subtle border border-line flex items-center justify-center text-accent mb-4">
              <ShieldCheck className="w-5 h-5 text-accent" />
            </div>
            <h3 className="font-serif text-xl font-bold text-ink-primary mb-2">
              Checksum-pinned model weights
            </h3>
            <p className="text-sm text-ink-secondary leading-relaxed mb-3">
              Small quantized models download once during initial setup and are verified against cryptographic SHA-256 hashes compiled into the binary.
              Any altered or tampered weight file is instantly rejected.
            </p>
            <div className="text-xs font-mono text-ink-muted">
              Invariant: Strict ADR-0009 model integrity pinning in code.
            </div>
          </div>
        </div>

        {/* Agent MCP callout box (Phase 3 roadmap context) */}
        <div className="p-6 rounded-xl bg-subtle border border-line-strong">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
            <div className="space-y-1">
              <div className="inline-flex items-center gap-1.5 font-mono text-xs text-accent font-semibold uppercase">
                <FileCode className="w-4 h-4" />
                <span>Phase 3 Roadmap: Model Context Protocol (MCP)</span>
              </div>
              <h4 className="font-serif text-lg font-bold text-ink-primary">
                Expose your local index to AI assistants on your terms
              </h4>
              <p className="text-sm text-ink-secondary max-w-2xl">
                The exact same offline index will be accessible to local coding assistants and desktop agents via an MCP server in v0.3.
                Agents query your local index through structured tools without needing cloud uploads.
              </p>
            </div>
            <div className="shrink-0 font-mono text-xs px-3 py-1.5 rounded bg-surface border border-line text-ink-muted">
              Status: Planned for v0.3
            </div>
          </div>
        </div>
      </div>
    </section>
  );
};
