import React, { useState } from 'react';
import { ArrowUpRight, Plus } from 'lucide-react';
import { Section, SectionHead } from './ui';
import { FAQ, ISSUES } from '../data/site';

export const Faq: React.FC = () => {
  const [open, setOpen] = useState<string | null>(FAQ[0].q);

  return (
    <Section id="faq" labelledBy="faq-title">
      <div data-reveal>
        <SectionHead
          id="faq-title"
          title={
            <>
              Questions,
              <br />
              answered plainly.
            </>
          }
          lede="The short version of what ships, what does not, and what the measurements actually say."
          aside={
            <a
              href={ISSUES}
              target="_blank"
              rel="noopener noreferrer"
              className="link-arrow"
            >
              Ask in the issue tracker
              <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
            </a>
          }
        />
      </div>

      <div className="mt-12 border-t border-line md:mt-16" data-reveal>
        {FAQ.map((item, index) => {
          const isOpen = open === item.q;
          const triggerId = `faq-trigger-${index}`;
          const panelId = `faq-panel-${index}`;
          return (
            <div key={item.q} className="border-b border-line">
              <h3>
                <button
                  type="button"
                  id={triggerId}
                  aria-expanded={isOpen}
                  aria-controls={panelId}
                  onClick={() => setOpen(isOpen ? null : item.q)}
                  className="group flex w-full items-start justify-between gap-6 py-5 text-left sm:py-6"
                >
                  <span className="flex items-baseline gap-4">
                    <span className="hidden font-mono text-[10px] text-ink-muted sm:inline">
                      {String(index + 1).padStart(2, '0')}
                    </span>
                    <span className="text-[15.5px] font-medium leading-snug text-ink sm:text-[17px]">
                      {item.q}
                    </span>
                  </span>
                  <span
                    aria-hidden="true"
                    className={`mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full border transition-colors duration-200 ${
                      isOpen
                        ? 'border-accent bg-accent text-paper-card'
                        : 'border-line text-ink-muted group-hover:border-line-strong group-hover:text-ink'
                    }`}
                  >
                    <Plus
                      className={`h-3.5 w-3.5 transition-transform duration-300 ${
                        isOpen ? 'rotate-45' : ''
                      }`}
                    />
                  </span>
                </button>
              </h3>
              <div
                id={panelId}
                role="region"
                aria-labelledby={triggerId}
                inert={!isOpen}
                className={`grid transition-[grid-template-rows] duration-300 ease-out ${
                  isOpen ? 'grid-rows-[1fr]' : 'grid-rows-[0fr]'
                }`}
              >
                <div className="overflow-hidden">
                  <p className="max-w-[68ch] pb-6 pl-0 text-[14px] leading-[1.75] text-ink-soft sm:pl-9">
                    {item.a}
                  </p>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </Section>
  );
};
