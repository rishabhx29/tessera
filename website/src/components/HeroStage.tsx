import React, { useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { ArrowUp, Check, Minus, Pause, Play, Search, Square, X } from 'lucide-react';

const ASK = 'hey, did you get those pics from the bali trip last year?';
const REPLY = 'on it — one sec';
const QUERY = 'bali trip';

const PHOTOS: readonly string[] = Array.from(
  { length: 12 },
  (_, index) => `/media/hero/bali/b${String(index + 1).padStart(2, '0')}.jpg`,
);

interface Phase {
  id: 'ask' | 'reply' | 'launch' | 'type' | 'results' | 'select' | 'send' | 'sent' | 'reset';
  duration: number;
}

const PHASES: readonly Phase[] = [
  { id: 'ask', duration: 3400 },
  { id: 'reply', duration: 2600 },
  { id: 'launch', duration: 2000 },
  { id: 'type', duration: 1900 },
  { id: 'results', duration: 2400 },
  { id: 'select', duration: 2500 },
  { id: 'send', duration: 2100 },
  { id: 'sent', duration: 4800 },
  { id: 'reset', duration: 950 },
];

const SENT_PHASE = PHASES.findIndex((phase) => phase.id === 'sent');

const REDUCE_QUERY = '(prefers-reduced-motion: reduce)';

const subscribeReduced = (onChange: () => void) => {
  const media = window.matchMedia(REDUCE_QUERY);
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
};

const getReduced = (): boolean => window.matchMedia(REDUCE_QUERY).matches;

const LENSES = ['all', 'pictures', 'words', 'documents'] as const;

export const HeroStage: React.FC = () => {
  const figureRef = useRef<HTMLElement>(null);
  const [phase, setPhase] = useState(0);
  const [paused, setPaused] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [inView, setInView] = useState(true);
  const [pageVisible, setPageVisible] = useState(true);
  const [asked, setAsked] = useState(0);
  const [typed, setTyped] = useState(0);

  const reduced = useSyncExternalStore(subscribeReduced, getReduced, () => false);
  const current = reduced ? SENT_PHASE : phase;
  const id = PHASES[current].id;
  const playing = !paused && !hovered && inView && pageVisible && !reduced;

  useEffect(() => {
    if (!playing) return;
    const timer = window.setTimeout(() => {
      const next = (phase + 1) % PHASES.length;
      if (next === 0) {
        setAsked(0);
        setTyped(0);
      }
      setPhase(next);
    }, PHASES[phase].duration);
    return () => window.clearTimeout(timer);
  }, [phase, playing]);

  useEffect(() => {
    if (!playing || phase !== 0 || asked >= ASK.length) return;
    const timer = window.setTimeout(
      () => setAsked((count) => Math.min(ASK.length, count + 1)),
      asked === 0 ? 650 : 26,
    );
    return () => window.clearTimeout(timer);
  }, [phase, playing, asked]);

  useEffect(() => {
    if (!playing || phase !== 3 || typed >= QUERY.length) return;
    const timer = window.setTimeout(
      () => setTyped((count) => Math.min(QUERY.length, count + 1)),
      typed === 0 ? 420 : 105,
    );
    return () => window.clearTimeout(timer);
  }, [phase, playing, typed]);

  useEffect(() => {
    const element = figureRef.current;
    if (!element) return;
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), {
      threshold: 0.3,
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const onVisibility = () => setPageVisible(!document.hidden);
    document.addEventListener('visibilitychange', onVisibility);
    return () => document.removeEventListener('visibilitychange', onVisibility);
  }, []);

  const askText = reduced || current > 0 ? ASK : ASK.slice(0, asked);
  const asking = askText.length < ASK.length;
  const queryText = reduced || current > 3 ? QUERY : current === 3 ? QUERY.slice(0, typed) : '';
  const typingQuery = current === 3 && typed < QUERY.length;
  const windowOpen = current >= 2 && current < 7;
  const selecting = current >= 5 && current < 7;
  const leaving = current >= 6;

  return (
    <figure
      ref={figureRef}
      aria-label="A friend asks for last year's Bali photos. The user presses Ctrl, Alt and T, Tessera opens over the chat, the search reads “bali trip”, sixty photos come back, and the album is sent to the chat."
      data-phase={id}
      className="hero-stage relative mx-auto max-w-[68rem] bg-paper"
      onPointerEnter={(event) => {
        if (event.pointerType === 'mouse') setHovered(true);
      }}
      onPointerLeave={(event) => {
        if (event.pointerType === 'mouse') setHovered(false);
      }}
    >
      <div className="relative overflow-hidden rounded-[18px] border border-line-strong/70 bg-paper shadow-[0_36px_70px_-40px_rgba(22,20,15,0.5)] sm:rounded-[22px]">
        <img
          src="/media/hero/bali-wallpaper.webp"
          alt=""
          width={1536}
          height={1024}
          decoding="async"
          className="absolute inset-0 h-full w-full object-cover object-[30%_62%]"
        />
        <div
          aria-hidden="true"
          className="absolute inset-0 bg-gradient-to-b from-night/20 via-night/5 to-night/30"
        />
        <div className="relative p-3 sm:p-10">
          <div className="overflow-hidden rounded-[12px] border border-line-strong/70 bg-paper shadow-[0_26px_60px_-30px_rgba(22,20,15,0.7)]">
            <div aria-hidden="true" className="pointer-events-none relative aspect-[7/10] sm:aspect-[1054/768]">
          {/* The chat stays underneath the whole story. */}
          <div className="absolute inset-0 flex flex-col bg-paper">
            <div className="flex items-center gap-2 border-b border-line px-3 py-2 sm:px-4 sm:py-2.5">
              <span className="flex h-6 w-6 items-center justify-center rounded-full bg-accent-soft text-[10.5px] font-medium text-accent-deep sm:h-7 sm:w-7 sm:text-[11.5px]">
                M
              </span>
              <span className="text-[12px] font-medium text-ink sm:text-[13px]">Maya</span>
              <span className="ml-auto font-mono text-[8.5px] uppercase tracking-[0.12em] text-ink-muted">
                a chat
              </span>
            </div>

            <div className="flex flex-1 flex-col justify-end gap-2 overflow-hidden px-3 pb-3 sm:gap-2.5 sm:px-5 sm:pb-4">
              <div className="flex justify-end opacity-70">
                <p className="max-w-[78%] rounded-2xl rounded-br-sm bg-ink px-3 py-1.5 text-[10.5px] leading-snug text-paper sm:text-[12px]">
                  cleaning out my camera roll — 12,400 photos
                </p>
              </div>
              <div className="flex items-end gap-2 opacity-70">
                <span className="mb-1 hidden h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-soft text-[9px] text-accent-deep sm:flex">
                  M
                </span>
                <p className="max-w-[78%] rounded-2xl rounded-bl-sm border border-line bg-paper.deep px-3 py-1.5 text-[10.5px] leading-snug text-ink sm:text-[12px]">
                  good luck with that
                </p>
              </div>
              <div className="flex items-end gap-2">
                <span className="mb-1 hidden h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-soft text-[9px] text-accent-deep sm:flex">
                  M
                </span>
                <p className="bubble-in max-w-[78%] rounded-2xl rounded-bl-sm border border-line bg-paper.deep px-3 py-2 text-[11.5px] leading-snug text-ink sm:max-w-[70%] sm:text-[13px]">
                  {askText}
                  {asking && playing && (
                    <span className="ml-0.5 inline-block h-[12px] w-[1.5px] translate-y-[2px] animate-pulse bg-ink/60" />
                  )}
                </p>
              </div>

              {current >= 1 && (
                <div className="flex justify-end">
                  <p className="bubble-in max-w-[78%] rounded-2xl rounded-br-sm bg-ink px-3 py-2 text-[11.5px] leading-snug text-paper sm:text-[13px]">
                    {REPLY}
                  </p>
                </div>
              )}

              {current === 1 && (
                <div className="flex flex-wrap items-center justify-center gap-1.5 pt-1">
                  <kbd className="kbd">Ctrl</kbd>
                  <kbd className="kbd">Alt</kbd>
                  <kbd className="kbd">T</kbd>
                  <span className="font-mono text-[8.5px] uppercase tracking-[0.1em] text-ink-muted">
                    summon over the chat
                  </span>
                </div>
              )}

              {current >= SENT_PHASE && (
                <div className="flex justify-end">
                  <div className="bubble-in w-[46%] min-w-[7.5rem] max-w-[15rem] rounded-2xl rounded-br-sm bg-ink p-1">
                    <div className="grid grid-cols-3 gap-0.5">
                      {PHOTOS.slice(0, 9).map((photo, position) => (
                        <span
                          key={photo}
                          className="relative block aspect-square overflow-hidden rounded-[3px]"
                        >
                          <img
                            src={photo}
                            alt=""
                            width={240}
                            height={160}
                            decoding="async"
                            className="h-full w-full object-cover"
                          />
                          {position === 8 && (
                            <span className="absolute inset-0 flex items-center justify-center bg-night/60 font-mono text-[9px] text-paper sm:text-[10px]">
                              +51
                            </span>
                          )}
                        </span>
                      ))}
                    </div>
                    <div className="px-1.5 pb-0.5 pt-1 font-mono text-[8px] uppercase tracking-[0.1em] text-paper/55 sm:text-[8.5px]">
                      60 photos
                    </div>
                  </div>
                </div>
              )}

              {current >= SENT_PHASE && (
                <div className="flex items-end gap-2">
                  <span className="mb-1 hidden h-5 w-5 shrink-0 items-center justify-center rounded-full bg-accent-soft text-[9px] text-accent-deep sm:flex">
                    M
                  </span>
                  <p
                    className="bubble-in max-w-[78%] rounded-2xl rounded-bl-sm border border-line bg-paper.deep px-3 py-2 text-[11.5px] leading-snug text-ink sm:max-w-[70%] sm:text-[13px]"
                    style={{ animationDelay: reduced ? '0ms' : '1400ms' }}
                  >
                    these are perfect, thank you
                  </p>
                </div>
              )}
            </div>

            <div className="flex items-center gap-2 border-t border-line px-3 py-2 sm:px-4 sm:py-2.5">
              <span className="flex h-7 flex-1 items-center rounded-full border border-line bg-paper-card px-3 text-[11px] text-ink-muted sm:h-8 sm:text-[12px]">
                Message
              </span>
              <span className="flex h-7 w-7 items-center justify-center rounded-full bg-accent text-paper-card sm:h-8 sm:w-8">
                <ArrowUp className="h-3.5 w-3.5" aria-hidden="true" />
              </span>
            </div>
          </div>

          <div
            className={`absolute inset-0 z-10 bg-night/20 transition-opacity duration-500 ${
              windowOpen ? 'opacity-100' : 'opacity-0'
            }`}
          />

          {/* The app window, built to match the real one. */}
          <div className="absolute inset-0 z-20 flex items-center justify-center px-2 sm:px-6">
            <div
              className={`w-full max-w-[46rem] overflow-hidden rounded-[10px] border border-line-strong bg-paper-card shadow-[0_40px_90px_-35px_rgba(22,20,15,0.7)] transition-all duration-500 ease-out ${
                current < 2 || current >= 7
                  ? 'translate-y-6 scale-[0.96] opacity-0'
                  : leaving
                    ? 'translate-y-8 scale-[0.98] opacity-0'
                    : 'translate-y-0 scale-100 opacity-100'
              }`}
              style={{ transitionDelay: leaving ? '450ms' : '0ms' }}
            >
              <div className="flex h-7 items-center gap-2 border-b border-night-line bg-night px-2.5 text-paper sm:h-8">
                <span className="grid grid-cols-2 gap-[2px]" aria-hidden="true">
                  <span className="h-[5px] w-[5px] rounded-[1px] bg-accent" />
                  <span className="h-[5px] w-[5px] rounded-[1px] bg-sage" />
                  <span className="h-[5px] w-[5px] rounded-[1px] bg-paper/70" />
                  <span className="h-[5px] w-[5px] rounded-[1px] bg-paper/30" />
                </span>
                <span className="text-[10px] font-medium sm:text-[11px]">Tessera</span>
                <span className="ml-auto flex items-center gap-2.5 text-paper/55">
                  <Minus className="h-2.5 w-2.5" aria-hidden="true" />
                  <Square className="h-2 w-2" aria-hidden="true" />
                  <X className="h-3 w-3" aria-hidden="true" />
                </span>
              </div>

              <div className="flex items-center gap-1 border-b border-line px-2 py-1.5 sm:px-2.5">
                <span className="mr-1 flex items-center gap-[3px]" aria-hidden="true">
                  <span className="h-1.5 w-1.5 rounded-[1px] bg-accent" />
                  <span className="h-1.5 w-1.5 rounded-[1px] bg-ink" />
                  <span className="h-1.5 w-1.5 rounded-[1px] bg-sage" />
                  <span className="h-1.5 w-1.5 rounded-[1px] bg-line-strong" />
                </span>
                {LENSES.map((lens) => (
                  <span
                    key={lens}
                    className={`rounded-full px-2 py-[3px] text-[8.5px] sm:px-2.5 sm:text-[9.5px] ${
                      lens === 'pictures' ? 'bg-ink text-paper-card' : 'text-ink-muted'
                    }`}
                  >
                    {lens}
                  </span>
                ))}
                <span
                  className={`ml-auto hidden whitespace-nowrap font-mono text-[8px] text-ink-muted transition-opacity duration-300 sm:inline sm:text-[9px] ${
                    current >= 4 ? 'opacity-100' : 'opacity-0'
                  }`}
                >
                  60 shown · 52 ms
                </span>
              </div>

              <div className="flex items-center gap-2 border-b border-line px-3 py-1.5 sm:py-2">
                <Search className="h-3 w-3 shrink-0 text-ink-muted sm:h-3.5 sm:w-3.5" aria-hidden="true" />
                <span className="text-[11px] text-ink sm:text-[12.5px]">
                  {queryText || <span className="text-ink-muted">Search by description</span>}
                  {typingQuery && (
                    <span className="ml-0.5 inline-block h-[11px] w-[1.5px] translate-y-[2px] animate-pulse bg-ink/70 sm:h-[13px]" />
                  )}
                </span>
                <X className="ml-auto h-2.5 w-2.5 text-ink-muted sm:h-3 sm:w-3" aria-hidden="true" />
              </div>

              <div className="relative">
                <div className="flex items-center justify-between px-3 pb-1 pt-1.5">
                  <span
                    className={`font-mono text-[8px] uppercase tracking-[0.1em] text-ink-muted transition-opacity duration-300 sm:text-[8.5px] ${
                      current >= 4 ? 'opacity-100' : 'opacity-0'
                    }`}
                  >
                    bali trip
                  </span>
                  <span
                    className={`rounded-full border border-line px-2 py-[2px] text-[8px] text-ink-soft transition-opacity duration-300 sm:text-[9px] ${
                      current >= 4 && current < 6 ? 'opacity-100' : 'opacity-0'
                    }`}
                  >
                    Select all
                  </span>
                </div>

                <div
                  className={`grid grid-cols-4 gap-1 px-2 pb-9 pt-0.5 transition-all duration-500 ease-out sm:gap-1.5 sm:px-2.5 ${
                    leaving
                      ? '-translate-y-4 scale-[0.94] opacity-0'
                      : current < 4
                        ? 'translate-y-2 opacity-0'
                        : ''
                  }`}
                >
                  {PHOTOS.map((photo, position) => (
                    <span
                      key={photo}
                      className={`relative block aspect-[4/3] overflow-hidden rounded-[4px] transition-shadow duration-300 ${
                        selecting ? 'outline outline-2 -outline-offset-2 outline-accent' : ''
                      }`}
                    >
                      <img
                        src={photo}
                        alt=""
                        width={240}
                        height={160}
                        decoding="async"
                        className="h-full w-full object-cover"
                      />
                      <span
                        className={`absolute right-1 top-1 flex h-[13px] w-[13px] items-center justify-center rounded-full bg-accent text-paper-card transition-all duration-300 sm:h-4 sm:w-4 ${
                          selecting ? 'scale-100 opacity-100' : 'scale-50 opacity-0'
                        }`}
                        style={{ transitionDelay: selecting ? `${position * 45}ms` : '0ms' }}
                      >
                        <Check className="h-2 w-2 sm:h-2.5 sm:w-2.5" aria-hidden="true" />
                      </span>
                    </span>
                  ))}
                </div>

                <div
                  className={`absolute inset-x-0 bottom-0 flex items-center justify-between bg-night px-2.5 py-1.5 text-paper transition-opacity duration-300 ${
                    selecting ? 'opacity-100' : 'opacity-0'
                  }`}
                >
                  <span className="font-mono text-[8.5px] uppercase tracking-[0.1em] text-paper/80 sm:text-[9.5px]">
                    60 selected
                  </span>
                  <span
                    className={`flex items-center gap-1 rounded-full bg-accent px-2.5 py-[3px] text-[8.5px] font-medium text-paper-card transition-transform duration-150 sm:text-[9.5px] ${
                      current >= 6 ? 'scale-95' : ''
                    }`}
                  >
                    Send to chat
                    <ArrowUp className="h-2.5 w-2.5" aria-hidden="true" />
                  </span>
                </div>
              </div>
            </div>
          </div>

          <div
            className={`absolute inset-x-0 top-[30%] z-30 flex justify-center px-4 transition-all duration-300 ease-out ${
              current >= 1 && current < 2
                ? 'translate-y-0 opacity-100'
                : 'translate-y-2 opacity-0'
            }`}
          >
            <div className="w-full max-w-[20rem] rounded-xl border border-line-strong/80 bg-paper-card/95 p-3.5 text-center shadow-[0_20px_45px_-20px_rgba(22,20,15,0.6)] sm:max-w-[22rem] sm:p-4">
              <p className="font-mono text-[8.5px] uppercase tracking-[0.14em] text-ink-muted sm:text-[9px]">
                12,400 photos, one question
              </p>
              <p className="mt-1.5 font-display text-[1.15rem] leading-[1.2] text-ink sm:text-[1.35rem]">
                Don't scroll. Launch Tessera and{' '}
                <span className="italic text-accent">describe it</span>.
              </p>
            </div>
          </div>

          <div
            className={`absolute inset-0 z-30 bg-paper transition-opacity duration-700 ${
              id === 'reset' ? 'opacity-100' : 'opacity-0'
            }`}
          />
            </div>
          </div>
        </div>
      </div>

      <div className="mt-3 flex items-center justify-between gap-4">
        <span className="font-mono text-[10px] uppercase tracking-[0.1em] text-ink-muted">
          Painted backdrop
        </span>
        {!reduced && (
          <button
            type="button"
            onClick={() => setPaused((value) => !value)}
            aria-label={paused ? 'Play the story' : 'Pause the story'}
            className="flex h-8 items-center gap-1.5 rounded-full border border-line px-3 text-[11px] text-ink-soft transition-colors duration-150 hover:border-line-strong hover:text-ink"
          >
            {paused ? (
              <Play className="h-3 w-3" aria-hidden="true" />
            ) : (
              <Pause className="h-3 w-3" aria-hidden="true" />
            )}
            {paused ? 'Play' : 'Pause'}
          </button>
        )}
      </div>

      <figcaption className="mt-3 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 border-t border-line pt-3 text-[11px] leading-relaxed text-ink-muted">
        <span className="max-w-[62ch]">
          A friend asks for last year's Bali photos. Ctrl + Alt + T, one search, and the album
          goes back — the chat never closes.
        </span>
        <span className="font-mono text-[10px] uppercase tracking-[0.1em]">
          Photos cropped from the real app
        </span>
      </figcaption>
    </figure>
  );
};
