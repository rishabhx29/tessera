import React from 'react';

type Tone = 'canvas' | 'surface';

const TONE: Record<Tone, string> = {
  canvas: 'bg-canvas',
  surface: 'bg-surface',
};

interface SectionProps {
  id?: string;
  tone?: Tone;
  className?: string;
  innerClassName?: string;
  labelledBy?: string;
  children: React.ReactNode;
}

/**
 * Page section with the site's hairline frame: a full-width divider, then a
 * max-w-6xl column with visible vertical rules. The rules are the structure;
 * sections do not need cards to separate themselves.
 */
export const Section: React.FC<SectionProps> = ({
  id,
  tone = 'canvas',
  className = '',
  innerClassName = 'py-16 md:py-24',
  labelledBy,
  children,
}) => (
  <section
    id={id}
    aria-labelledby={labelledBy}
    className={`border-b border-line ${TONE[tone]} ${className}`}
  >
    <div
      className={`mx-auto w-full max-w-6xl border-x border-line px-5 sm:px-8 lg:px-12 ${innerClassName}`}
    >
      {children}
    </div>
  </section>
);

interface SectionHeadingProps {
  id: string;
  title: React.ReactNode;
  lede?: React.ReactNode;
  aside?: React.ReactNode;
}

/**
 * Heading on the left, lede on the right. The split is the section rhythm;
 * no uppercase kicker needed above every heading.
 */
export const SectionHeading: React.FC<SectionHeadingProps> = ({
  id,
  title,
  lede,
  aside,
}) => (
  <div className="grid gap-6 md:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)] md:items-end md:gap-12">
    <h2
      id={id}
      className="font-serif text-[clamp(1.85rem,1.15rem+2.15vw,3rem)] font-bold leading-[1.08] tracking-tighter text-ink-primary"
    >
      {title}
    </h2>
    {(lede || aside) && (
      <div className="max-w-[46ch] space-y-4 md:justify-self-end">
        {lede && (
          <p className="text-[15px] leading-relaxed text-ink-secondary sm:text-base">
            {lede}
          </p>
        )}
        {aside}
      </div>
    )}
  </div>
);
