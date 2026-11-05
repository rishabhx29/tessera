import React, { useState } from 'react';
import { ChevronDown } from 'lucide-react';

interface FAQItem {
  id: string;
  question: string;
  answer: string;
  details?: string;
}

const FAQ_ITEMS: FAQItem[] = [
  {
    id: 'formats',
    question: 'What file formats does Tessera index in v0.1?',
    answer:
      'In v0.1, Tessera indexes screenshots and images (PNG, JPG, JPEG, WEBP, BMP, GIF, TIFF) via local OCR and SigLIP vision embeddings. For documents, it indexes PDFs with text layers, DOCX Word files, and plain text formats (TXT, MD, CSV, LOG) using chunked MiniLM semantic embeddings.',
    details:
      'Video files (Whisper + keyframes) arrive in v0.2, and XLSX/PPTX in v0.4. Scanned PDFs lacking a text layer are counted on the local diagnostics page rather than silently discarded.',
  },
  {
    id: 'telemetry',
    question: 'Does Tessera upload anything to the cloud or phone home?',
    answer:
      'No. Tessera has zero network calls at query time and zero telemetry compiled into the binary. No usage analytics, no error tracking servers, and no account synchronization. Everything happens strictly within your local machine.',
    details:
      'You can verify this yourself by inspecting network traffic with Wireshark or running Tessera in an isolated offline environment.',
  },
  {
    id: 'battery-performance',
    question: 'Will background indexing slow down my PC or drain my laptop battery?',
    answer:
      'No. The background watcher runs at idle CPU and IO priority. It automatically pauses execution when your laptop is unplugged and running on battery power.',
    details:
      'Machine learning models load into memory only during an active backlog drain and unload immediately once the queue is empty, preserving an idle RAM ceiling of ~250 MB.',
  },
  {
    id: 'index-size',
    question: 'How large is the search index on disk?',
    answer:
      'The index consumes roughly ~25 MB of disk space per 1,000 files. This includes trigram text tokens, document chunks, and vector embeddings.',
    details:
      'The entire index is stored in a single standard SQLite file (tessera.db) in your local application directory, which you can inspect or delete at any time.',
  },
  {
    id: 'platforms',
    question: 'What is the status of Windows vs. macOS and Linux support?',
    answer:
      'Windows 10/11 x64 is the primary launch target with native Windows.Media.Ocr acceleration (34.4 ms p50). The macOS Vision OCR adapter is implemented and passes CI tests, but its physical benchmark timings are unmeasured. Linux is supported through the cross-platform ONNX fallback.',
    details:
      'We refuse to market "fast on all platforms" until physical hardware benchmarks are completed and published for macOS.',
  },
  {
    id: 'models',
    question: 'How are neural network models downloaded and verified?',
    answer:
      'Required quantized models download once upon initial setup. Every model file must match a cryptographic SHA-256 checksum hardcoded into the compiled binary (ADR-0009).',
    details:
      'If any file differs by a single byte, the engine rejects the download and halts loading to guarantee software supply-chain integrity.',
  },
  {
    id: 'license',
    question: 'What is the license and is Tessera free for commercial use?',
    answer:
      'Tessera is 100% free and released under the permissive MIT License. You are completely free to use it for personal projects, enterprise work, and commercial development without restriction or subscription fees.',
  },
];

export const FAQSection: React.FC = () => {
  const [openIds, setOpenIds] = useState<Record<string, boolean>>({ formats: true });

  const toggleItem = (id: string) => {
    setOpenIds((prev) => ({
      ...prev,
      [id]: !prev[id],
    }));
  };

  return (
    <section id="faq" className="py-20 bg-canvas border-b border-line">
      <div className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8">
        {/* Section Header */}
        <div className="text-center max-w-2xl mx-auto mb-14">
          <div className="text-xs font-mono uppercase tracking-wider text-accent font-semibold mb-2">
            Clear Answers
          </div>
          <h2 className="font-serif text-3xl sm:text-4xl font-bold tracking-tight text-ink-primary">
            Frequently Asked Questions
          </h2>
          <p className="mt-3 text-base text-ink-secondary">
            Honest facts about formats, performance, privacy, and roadmap status.
          </p>
        </div>

        {/* Accordion List */}
        <div className="space-y-3">
          {FAQ_ITEMS.map((item) => {
            const isOpen = !!openIds[item.id];
            return (
              <div
                key={item.id}
                className="rounded-lg border border-line bg-surface transition-all overflow-hidden"
              >
                <button
                  type="button"
                  onClick={() => toggleItem(item.id)}
                  className="w-full px-5 py-4 text-left flex items-center justify-between gap-4 hover:bg-subtle/50 transition-colors focus-visible:ring-2 focus-visible:ring-accent"
                  aria-expanded={isOpen}
                >
                  <span className="font-serif text-base sm:text-lg font-bold text-ink-primary">
                    {item.question}
                  </span>
                  <ChevronDown
                    className={`w-4 h-4 text-ink-muted shrink-0 transition-transform duration-200 ${
                      isOpen ? 'rotate-180 text-accent' : ''
                    }`}
                  />
                </button>

                {isOpen && (
                  <div className="px-5 pb-5 pt-1 text-sm text-ink-secondary border-t border-line/60 bg-canvas/30 space-y-2 leading-relaxed">
                    <p>{item.answer}</p>
                    {item.details && (
                      <p className="text-xs text-ink-muted font-sans border-l-2 border-line-strong pl-3 mt-2">
                        {item.details}
                      </p>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
};
