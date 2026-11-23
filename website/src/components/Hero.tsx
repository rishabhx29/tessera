import React, { useEffect, useRef } from 'react';
import { ArrowDown, ArrowUpRight } from 'lucide-react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import { HeroStage } from './HeroStage';
import { HeroTiles } from './HeroTiles';
import { FORMAT_RAIL, RELEASES } from '../data/site';
import { prefersReducedMotion } from '../lib/motion';

export const Hero: React.FC = () => {
  const sectionRef = useRef<HTMLElement>(null);

  useEffect(() => {
    if (prefersReducedMotion()) return;

    gsap.registerPlugin(ScrollTrigger);
    const ctx = gsap.context(() => {
      // The intro is CSS so it starts at first paint; this only adds the
      // parallax once the page is hydrated.
      gsap.to('.hero-window', {
        y: -44,
        ease: 'none',
        scrollTrigger: {
          trigger: sectionRef.current,
          start: 'top top',
          end: 'bottom top',
          scrub: true,
        },
      });
    }, sectionRef);

    return () => ctx.revert();
  }, []);

  return (
    <section
      id="top"
      ref={sectionRef}
      aria-labelledby="hero-title"
      className="relative overflow-hidden border-b border-line"
    >
      <HeroTiles />

      <div className="shell relative">
        <div className="mx-auto flex max-w-3xl flex-col items-center pb-14 pt-20 text-center sm:pt-28 md:pb-20">
          <h1
            id="hero-title"
            className="font-display text-[clamp(2.9rem,7.2vw,6rem)] leading-[0.95] tracking-display text-ink"
          >
            <span className="block overflow-hidden pb-1">
              <span className="hero-line block" style={{ animationDelay: '0.1s' }}>
                A thousand photos.
              </span>
            </span>
            <span className="block overflow-hidden pb-2">
              <span
                className="hero-line hero-mark block italic text-accent"
                style={{ animationDelay: '0.19s' }}
              >
                One search bar.
              </span>
            </span>
          </h1>

          <p
            className="hero-fade mt-7 max-w-[36rem] text-[1.0625rem] leading-[1.7] text-ink-soft sm:text-[1.1875rem]"
            style={{ animationDelay: '0.57s' }}
          >
            Describe the picture — the Bali trip from last year, the dog in a blanket — and
            Tessera finds it while your chat stays open. It reads the text inside screenshots,
            PDFs and Word too. Local, offline, and fast on the machine you already own.
          </p>

          <div
            className="hero-fade mt-9 flex flex-col items-center gap-3 sm:flex-row"
            style={{ animationDelay: '0.64s' }}
          >
            <a href="#demo" className="btn btn-primary w-full sm:w-auto">
              See it work
              <ArrowDown className="h-4 w-4" aria-hidden="true" />
            </a>
            <a
              href={RELEASES}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-ghost w-full sm:w-auto"
            >
              Releases on GitHub
              <ArrowUpRight className="h-4 w-4" aria-hidden="true" />
            </a>
          </div>

          <p
            className="hero-fade mt-6 flex flex-wrap items-center justify-center gap-x-2 gap-y-1.5 text-[13px] text-ink-muted"
            style={{ animationDelay: '0.71s' }}
          >
            <span>Summon it anywhere with</span>
            <span className="inline-flex items-center gap-1">
              <kbd className="kbd">Ctrl</kbd>
              <kbd className="kbd">Alt</kbd>
              <kbd className="kbd">T</kbd>
            </span>
          </p>
        </div>

        <HeroStage />
      </div>

      <div className="mt-16 border-y border-line bg-paper-card/60">
        <div className="shell flex flex-wrap items-center justify-center gap-x-3 gap-y-2 py-3 font-mono text-[10px] uppercase tracking-[0.14em] text-ink-muted">
          {FORMAT_RAIL.map((format, index) => (
            <React.Fragment key={format}>
              {index > 0 && (
                <span aria-hidden="true" className="text-line-strong">
                  /
                </span>
              )}
              <span>{format}</span>
            </React.Fragment>
          ))}
        </div>
      </div>
    </section>
  );
};
