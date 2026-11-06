import React, { useEffect, useState } from 'react';
import { ArrowDown } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { ILLUSTRATIONS } from './illustrations';
import { STAGES } from '../data/site';

export const ProductStory: React.FC = () => {
  const [active, setActive] = useState(STAGES[0].id);

  useEffect(() => {
    const chapters = STAGES.map((stage) => document.getElementById(`stage-${stage.id}`)).filter(
      (element): element is HTMLElement => element !== null,
    );
    if (!chapters.length) return;

    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries
          .filter((entry) => entry.isIntersecting)
          .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
        if (visible?.target.id) {
          setActive(visible.target.id.replace('stage-', ''));
        }
      },
      { rootMargin: '-38% 0px -52% 0px', threshold: 0 },
    );

    chapters.forEach((chapter) => observer.observe(chapter));
    return () => observer.disconnect();
  }, []);

  return (
    <Section id="product" tone="card" labelledBy="product-title">
      <div data-reveal>
        <SectionHead
          id="product-title"
          size="large"
          title={
            <>
              Five stages, one index,
              <br />
              from disk to result.
            </>
          }
          lede="Every file walks the same path. Watch, read, embed, fuse, answer. Nothing is uploaded, nothing is approximated, and every stage runs on the CPU of the machine in front of you."
          aside={
            <a href="#stage-watch" className="link-arrow">
              Walk through the pipeline
              <ArrowDown className="h-3.5 w-3.5" aria-hidden="true" />
            </a>
          }
        />
      </div>

      <div className="mt-16 grid gap-12 md:mt-24 lg:grid-cols-[15rem_minmax(0,1fr)] lg:gap-16">
        <nav aria-label="Pipeline stages" className="hidden lg:block">
          <ol className="sticky top-28 space-y-1">
            {STAGES.map((stage) => {
              const isActive = stage.id === active;
              return (
                <li key={stage.id}>
                  <a
                    href={`#stage-${stage.id}`}
                    aria-current={isActive ? 'true' : undefined}
                    className={`group flex items-baseline gap-3 rounded-sm border-l py-2.5 pl-4 transition-colors duration-200 ${
                      isActive
                        ? 'border-accent text-ink'
                        : 'border-line text-ink-muted hover:border-line-strong hover:text-ink-soft'
                    }`}
                  >
                    <span
                      className={`font-mono text-[10px] ${
                        isActive ? 'text-accent' : 'text-ink-muted'
                      }`}
                    >
                      {stage.n}
                    </span>
                    <span className="text-[15px] font-medium leading-none">{stage.title}</span>
                  </a>
                  <p className="mb-1 pl-[3.4rem] text-[12px] leading-snug text-ink-muted">
                    {isActive ? stage.summary : ''}
                  </p>
                </li>
              );
            })}
          </ol>
        </nav>

        <div className="min-w-0">
          {STAGES.map((stage, index) => {
            const Illustration = ILLUSTRATIONS[stage.id];
            const flip = index % 2 === 1;
            return (
              <article
                key={stage.id}
                id={`stage-${stage.id}`}
                aria-labelledby={`stage-${stage.id}-title`}
                className="scroll-mt-28 border-t border-line pb-14 pt-9 first:border-t-0 first:pt-0 md:pb-20 md:pt-12 md:first:pt-0"
              >
                <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
                  <span className="font-mono text-[11px] text-accent">{stage.n}</span>
                  <h3
                    id={`stage-${stage.id}-title`}
                    className="font-display text-[clamp(2.1rem,3.6vw,3.2rem)] leading-none tracking-display text-ink"
                  >
                    {stage.title}
                  </h3>
                  <span className="font-mono text-[11px] uppercase tracking-[0.1em] text-ink-muted">
                    {stage.summary}
                  </span>
                </div>

                <div
                  className={`mt-7 grid gap-7 md:mt-9 md:grid-cols-2 md:items-start md:gap-10 ${
                    flip ? 'md:[&>*:first-child]:order-2' : ''
                  }`}
                >
                  <div className="min-w-0">
                    <p className="font-display text-[1.45rem] leading-[1.25] text-ink sm:text-[1.65rem]">
                      {stage.heading}
                    </p>
                    <p className="mt-4 text-[15px] leading-[1.7] text-ink-soft">{stage.body}</p>
                    <ul className="mt-5 space-y-2.5 border-t border-line pt-5">
                      {stage.bullets.map((bullet) => (
                        <li
                          key={bullet}
                          className="grid grid-cols-[auto_minmax(0,1fr)] gap-2.5 text-[13.5px] leading-relaxed text-ink-soft"
                        >
                          <span
                            aria-hidden="true"
                            className="mt-[0.55em] h-1 w-1 rounded-full bg-accent"
                          />
                          {bullet}
                        </li>
                      ))}
                    </ul>
                  </div>

                  <div className="min-w-0">
                    <Illustration />
                    <dl className="mt-4 divide-y divide-line border-t border-line text-[12px]">
                      {stage.spec.map(([term, value]) => (
                        <div key={term} className="flex items-baseline justify-between gap-4 py-2">
                          <dt className="font-mono text-[10px] uppercase tracking-[0.08em] text-ink-muted">
                            {term}
                          </dt>
                          <dd className="text-right text-ink-soft">{value}</dd>
                        </div>
                      ))}
                    </dl>
                  </div>
                </div>
              </article>
            );
          })}
        </div>
      </div>
    </Section>
  );
};
