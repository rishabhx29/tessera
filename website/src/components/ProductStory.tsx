import React, { useState } from 'react';
import { ArrowDown } from 'lucide-react';
import { Section, SectionHead } from './ui';

const LENSES = [
  {
    id: 'pictures',
    label: 'Pictures',
    title: 'What it looks like',
    body: 'Describe a photo loosely. The wall answers with the pictures that fit.',
    src: '/media/lens-pictures.webp',
    alt: 'The app in its pictures lens after searching for “hot air balloons in the sky”: a wall of sixty photos, balloons first, then skies, a landscape and a screenshot.',
    query: 'hot air balloons in the sky',
    meta: '60 shown · 52 ms',
  },
  {
    id: 'words',
    label: 'Words',
    title: 'The text inside',
    body: 'Search a word you saw in a screenshot. The matched line is marked.',
    src: '/media/lens-words.webp',
    alt: 'The app in its words lens after searching for “invoice”: the Skyline Air invoice first, with the matched line called out beneath it, then photos whose printed text mentions invoices.',
    query: 'invoice',
    meta: '60 shown · 30 ms',
  },
  {
    id: 'documents',
    label: 'Documents',
    title: 'The page it’s on',
    body: 'Ask inside PDFs and Word files. Every hit names the page it came from.',
    src: '/media/lens-documents.webp',
    alt: 'The app in its documents lens after searching for “sourdough”: a recipe, a menu, a report, an invoice and an itinerary, each row with the matched snippet and the page for the PDFs.',
    query: 'sourdough',
    meta: '5 results · 18 ms',
  },
] as const;

export const ProductStory: React.FC = () => {
  const [active, setActive] = useState<(typeof LENSES)[number]['id']>(LENSES[0].id);
  const lens = LENSES.find((item) => item.id === active) ?? LENSES[0];

  return (
    <Section id="product" tone="card" labelledBy="product-title">
      <div data-reveal>
        <SectionHead
          id="product-title"
          size="large"
          title={
            <>
              Ask the way
              <br />
              you remember it.
            </>
          }
          lede="A description, a few words, a number off a page. The same box takes all three, and every result says why it is there."
          aside={
            <a href="#demo" className="link-arrow">
              Try the sample search
              <ArrowDown className="h-3.5 w-3.5" aria-hidden="true" />
            </a>
          }
        />
      </div>

      <div className="mt-14 grid items-start gap-8 md:mt-20 lg:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)] lg:gap-14">
        <div data-reveal>
          <div role="group" aria-label="Choose a search lens" className="border-t border-line">
            {LENSES.map((item, index) => {
              const isActive = item.id === active;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActive(item.id)}
                  aria-pressed={isActive}
                  className="group flex w-full items-start gap-4 border-b border-line py-5 text-left"
                >
                  <span
                    className={`mt-1.5 font-mono text-[10px] tabular-nums ${
                      isActive ? 'text-accent' : 'text-ink-muted'
                    }`}
                  >
                    {String(index + 1).padStart(2, '0')}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-baseline justify-between gap-3">
                      <span
                        className={`font-display text-[1.55rem] leading-none tracking-display transition-colors duration-200 sm:text-[1.8rem] ${
                          isActive ? 'text-ink' : 'text-ink-soft group-hover:text-ink'
                        }`}
                      >
                        {item.title}
                      </span>
                      <span
                        aria-hidden="true"
                        className={`h-1.5 w-1.5 shrink-0 rounded-full transition-colors duration-200 ${
                          isActive ? 'bg-accent' : 'bg-line-strong/60'
                        }`}
                      />
                    </span>
                    <span
                      className={`mt-2 block text-[13.5px] leading-relaxed transition-colors duration-200 ${
                        isActive ? 'text-ink-soft' : 'text-ink-muted'
                      }`}
                    >
                      {item.body}
                    </span>
                  </span>
                </button>
              );
            })}
          </div>
        </div>

        <figure data-reveal className="min-w-0">
          <div className="overflow-hidden rounded-xl border border-line-strong bg-paper shadow-lift">
            <img
              key={lens.id}
              src={lens.src}
              alt={lens.alt}
              width={1054}
              height={768}
              decoding="async"
              className="swap-in w-full"
            />
          </div>
          <figcaption className="mt-3 flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
            <span>
              Actual app window · <span className="normal-case tracking-normal text-ink-soft">“{lens.query}”</span>
            </span>
            <span>{lens.meta}</span>
          </figcaption>
        </figure>
      </div>
    </Section>
  );
};
