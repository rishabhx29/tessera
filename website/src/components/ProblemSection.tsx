import React from 'react';

export const ProblemSection: React.FC = () => {
  return (
    <section className="py-20 bg-subtle/50 border-b border-line">
      <div className="max-w-5xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="max-w-3xl mb-14">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            The Reality of Desktop Search
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary leading-tight">
            Windows treats images as opaque pictures, and a folder of ten thousand files as a dead-end maze.
          </h2>
          <p className="mt-4 text-base sm:text-lg text-ink-secondary leading-relaxed">
            Every day you capture screenshots, save PDFs, and download receipts.
            The moment they land on your hard drive, traditional file search forgets what is inside them.
          </p>
        </div>

        {/* 3 Problem Cards with Editorial Contrast */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-12">
          {/* Card 1 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile flex flex-col justify-between">
            <div>
              <div className="w-10 h-10 rounded-lg bg-red-50 border border-red-200 flex items-center justify-center text-red-700 mb-4 font-mono font-bold text-sm">
                01
              </div>
              <h3 className="font-serif text-lg font-bold text-ink-primary mb-2">
                Screenshots have useless names
              </h3>
              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Filenames like <code className="text-xs font-mono bg-subtle px-1.5 py-0.5 rounded text-ink-primary">Screenshot 2026-10-11 at 14.32.11.png</code> contain
                zero semantic signal. You remember the airline confirmation code, but Windows search only knows the creation timestamp.
              </p>
            </div>
            <div className="text-xs font-mono text-ink-muted pt-3 border-t border-line">
              Tessera runs native OCR to index every word automatically.
            </div>
          </div>

          {/* Card 2 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile flex flex-col justify-between">
            <div>
              <div className="w-10 h-10 rounded-lg bg-amber-50 border border-amber-200 flex items-center justify-center text-amber-800 mb-4 font-mono font-bold text-sm">
                02
              </div>
              <h3 className="font-serif text-lg font-bold text-ink-primary mb-2">
                Copilot+ requires $1,500 new laptops
              </h3>
              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Microsoft provides semantic search, but restricts it exclusively to Copilot+ certified PCs with 40+ TOPS NPU silicon.
                Your existing 8-core desktop or lightweight laptop is left behind.
              </p>
            </div>
            <div className="text-xs font-mono text-ink-muted pt-3 border-t border-line">
              Tessera runs on standard CPU threads in under 250 MB idle RAM.
            </div>
          </div>

          {/* Card 3 */}
          <div className="p-6 rounded-xl bg-surface border border-line shadow-tile flex flex-col justify-between">
            <div>
              <div className="w-10 h-10 rounded-lg bg-blue-50 border border-blue-200 flex items-center justify-center text-blue-800 mb-4 font-mono font-bold text-sm">
                03
              </div>
              <h3 className="font-serif text-lg font-bold text-ink-primary mb-2">
                Single-format tools fragment your search
              </h3>
              <p className="text-sm text-ink-secondary leading-relaxed mb-4">
                Existing tools only index one format: screenshot tools only see screenshots, photo tools only see camera rolls, document tools ignore images.
                You are forced to guess which tool to open.
              </p>
            </div>
            <div className="text-xs font-mono text-ink-muted pt-3 border-t border-line">
              One search bar fuses screenshots, PDFs, and documents together.
            </div>
          </div>
        </div>

        {/* Comparison Table: The Honest Positioning */}
        <div className="rounded-xl border border-line-strong bg-surface overflow-hidden shadow-tile">
          <div className="px-6 py-4 bg-canvas border-b border-line flex items-center justify-between">
            <h3 className="font-serif text-base font-bold text-ink-primary">
              Positioning vs Other Approaches
            </h3>
            <span className="text-xs font-mono text-ink-muted">Transparent comparison</span>
          </div>

          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="bg-subtle text-ink-secondary font-mono text-xs border-b border-line">
                <tr>
                  <th className="py-3 px-4 sm:px-6 font-medium">Capability</th>
                  <th className="py-3 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/50">
                    Tessera (v0.1)
                  </th>
                  <th className="py-3 px-4 sm:px-6 font-medium">Windows Explorer</th>
                  <th className="py-3 px-4 sm:px-6 font-medium">Commercial SaaS</th>
                  <th className="py-3 px-4 sm:px-6 font-medium">Copilot+ Search</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line font-sans text-ink-secondary text-xs sm:text-sm">
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Search screenshot text (OCR)
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Yes (34.4 ms p50)
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 text-ink-muted">No</td>
                  <td className="py-3.5 px-4 sm:px-6">Varies ($15-$19/mo)</td>
                  <td className="py-3.5 px-4 sm:px-6">Yes (Copilot+ only)</td>
                </tr>
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Search images by description
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Yes (SigLIP local)
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 text-ink-muted">No</td>
                  <td className="py-3.5 px-4 sm:px-6">Requires cloud upload</td>
                  <td className="py-3.5 px-4 sm:px-6">Yes (Copilot+ only)</td>
                </tr>
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Mixed-format single query
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Images + PDFs + DOCX
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 text-ink-muted">Filename only</td>
                  <td className="py-3.5 px-4 sm:px-6 text-ink-muted">Single format only</td>
                  <td className="py-3.5 px-4 sm:px-6">Partial</td>
                </tr>
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Hardware requirement
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Any standard PC (CPU)
                  </td>
                  <td className="py-3.5 px-4 sm:px-6">Any PC</td>
                  <td className="py-3.5 px-4 sm:px-6">Any PC</td>
                  <td className="py-3.5 px-4 sm:px-6 text-red-700 font-medium">New NPU PC only</td>
                </tr>
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Privacy & Telemetry
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Zero network calls
                  </td>
                  <td className="py-3.5 px-4 sm:px-6">Bing sync telemetry</td>
                  <td className="py-3.5 px-4 sm:px-6 text-red-700">Cloud servers</td>
                  <td className="py-3.5 px-4 sm:px-6">Microsoft account sync</td>
                </tr>
                <tr>
                  <td className="py-3.5 px-4 sm:px-6 font-medium text-ink-primary">
                    Price & License
                  </td>
                  <td className="py-3.5 px-4 sm:px-6 font-semibold text-accent bg-accent-subtle/20 font-mono">
                    Free · Open Source (MIT)
                  </td>
                  <td className="py-3.5 px-4 sm:px-6">Bundled in OS</td>
                  <td className="py-3.5 px-4 sm:px-6">$15–$19 / month</td>
                  <td className="py-3.5 px-4 sm:px-6">Hardware bundled</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </section>
  );
};
