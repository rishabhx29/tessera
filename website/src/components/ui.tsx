import React from 'react';

type Tone = 'paper' | 'card';

interface SectionProps {
  id: string;
  tone?: Tone;
  className?: string;
  children: React.ReactNode;
  labelledBy?: string;
}

export const Section: React.FC<SectionProps> = ({
  id,
  tone = 'paper',
  className = '',
  children,
  labelledBy,
}) => (
  <section
    id={id}
    aria-labelledby={labelledBy}
    className={`border-b border-line ${tone === 'card' ? 'bg-paper-card' : 'bg-paper'} ${className}`}
  >
    <div className="shell frame py-20 sm:py-24 md:py-32">{children}</div>
  </section>
);

interface SectionHeadProps {
  id: string;
  title: React.ReactNode;
  lede?: React.ReactNode;
  aside?: React.ReactNode;
  size?: 'default' | 'large';
}

/** Big serif statement on the left, the explanation on the right. */
export const SectionHead: React.FC<SectionHeadProps> = ({
  id,
  title,
  lede,
  aside,
  size = 'default',
}) => (
  <div className="grid gap-8 md:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)] md:items-end md:gap-14">
    <h2
      id={id}
      className={`font-display leading-[1.02] tracking-display text-ink ${
        size === 'large'
          ? 'text-[clamp(2.6rem,5.4vw,4.9rem)]'
          : 'text-[clamp(2.2rem,4.1vw,3.7rem)]'
      }`}
    >
      {title}
    </h2>
    {(lede || aside) && (
      <div className="max-w-[46ch] space-y-4 md:justify-self-end">
        {lede && <p className="text-[1.0625rem] leading-[1.7] text-ink-soft">{lede}</p>}
        {aside}
      </div>
    )}
  </div>
);
