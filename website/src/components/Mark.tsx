import React from 'react';

/**
 * Four tessera tiles. One terracotta, one ink, one muted, one outlined.
 * The same mark is the favicon and the app icon.
 */
export const Mark: React.FC<{ className?: string }> = ({ className = 'h-8 w-8' }) => (
  <span
    aria-hidden="true"
    className={`grid grid-cols-2 gap-[3px] rounded-[5px] border border-line-strong bg-paper-card p-[3px] ${className}`}
  >
    <span className="rounded-[1px] bg-accent" />
    <span className="rounded-[1px] bg-ink" />
    <span className="rounded-[1px] bg-ink-muted" />
    <span className="rounded-[1px] border border-accent" />
  </span>
);
