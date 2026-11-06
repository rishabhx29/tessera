import { useEffect } from 'react';
import gsap from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';
import Lenis from 'lenis';

export const prefersReducedMotion = (): boolean =>
  typeof window !== 'undefined' &&
  window.matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * Page-wide motion: Lenis for the scroll, GSAP for everything that reacts to
 * it. Without a preference for reduced motion this smooths the wheel and
 * reveals section intros once. With the preference set, it does nothing and
 * the page scrolls natively.
 */
export function usePageMotion(): void {
  useEffect(() => {
    if (prefersReducedMotion()) return;

    gsap.registerPlugin(ScrollTrigger);

    const lenis = new Lenis({
      lerp: 0.09,
      wheelMultiplier: 0.9,
      touchMultiplier: 1.15,
      anchors: { offset: -76 },
    });

    const tick = (time: number) => lenis.raf(time * 1000);
    lenis.on('scroll', ScrollTrigger.update);
    gsap.ticker.add(tick);
    gsap.ticker.lagSmoothing(0);

    if (import.meta.env.DEV) {
      (window as unknown as { __lenis?: Lenis }).__lenis = lenis;
    }

    const ctx = gsap.context(() => {
      // From-state is applied only when the trigger fires, so content stays
      // visible by default and a missed trigger can never hide a section.
      // Anything already on screen keeps its prerendered state: animating it
      // now would blank content the reader is looking at.
      const inView = (el: Element) =>
        el.getBoundingClientRect().top < window.innerHeight * 0.88;

      gsap.utils.toArray<HTMLElement>('[data-reveal]').forEach((el) => {
        if (inView(el)) return;
        gsap.fromTo(
          el,
          { y: 26, autoAlpha: 0 },
          {
            y: 0,
            autoAlpha: 1,
            duration: 0.95,
            ease: 'expo.out',
            immediateRender: false,
            scrollTrigger: { trigger: el, start: 'top 88%', once: true },
          },
        );
      });

      gsap.utils.toArray<HTMLElement>('[data-reveal-group]').forEach((group) => {
        if (inView(group)) return;
        gsap.fromTo(
          Array.from(group.children),
          { y: 22, autoAlpha: 0 },
          {
            y: 0,
            autoAlpha: 1,
            duration: 0.85,
            ease: 'expo.out',
            stagger: 0.07,
            immediateRender: false,
            scrollTrigger: { trigger: group, start: 'top 86%', once: true },
          },
        );
      });
    });

    return () => {
      ctx.revert();
      gsap.ticker.remove(tick);
      lenis.destroy();
      ScrollTrigger.getAll().forEach((trigger) => trigger.kill());
    };
  }, []);
}
