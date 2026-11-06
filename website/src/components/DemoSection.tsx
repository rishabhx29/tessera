import React from 'react';
import { ArrowDown, Info } from 'lucide-react';
import { Section, SectionHeading } from './Section';
import { SearchIllustration } from './SearchIllustration';
import { DEMO_IS_PLACEHOLDER, DEMO_VIDEO } from '../demo';

const RECORDING_BEATS = [
  {
    number: '01',
    title: 'Summon it anywhere',
    body: 'Ctrl+Alt+Space opens the search window over whatever you are doing.',
  },
  {
    number: '02',
    title: 'Ask in your own words',
    body: 'One phrase returns a screenshot, a PDF page and a Word paragraph in the same grid.',
  },
  {
    number: '03',
    title: 'Save, then find',
    body: 'A file saved during the demo becomes searchable while the index catches up.',
  },
];

export const DemoSection: React.FC = () => (
  <Section id="demo" tone="surface" labelledBy="demo-title">
    <SectionHeading
      id="demo-title"
      title={<>One query. A screenshot, a PDF, a Word document.</>}
      lede={
        <>
          Press Ctrl+Alt+Space, type what you remember, and read the answer in
          one grid. Each hit shows which part of the engine found it.
        </>
      }
      aside={
        <a href="#measured" className="hairline-link">
          Skip to the measured numbers
          <ArrowDown className="h-3.5 w-3.5" aria-hidden="true" />
        </a>
      }
    />

    <div className="mt-10 grid gap-8 lg:grid-cols-[minmax(0,1fr)_17rem] lg:items-start lg:gap-10">
      <figure>
        <div className="relative overflow-hidden rounded-xl border border-line-strong bg-inset">
          <video
            className="aspect-video w-full"
            controls
            playsInline
            preload="metadata"
            width={1280}
            height={720}
            poster={DEMO_VIDEO.poster}
            aria-describedby="demo-caption"
            aria-label={
              DEMO_IS_PLACEHOLDER
                ? 'Placeholder slate for the Tessera demo recording'
                : 'Tessera demo recording'
            }
          >
            <source src={DEMO_VIDEO.src} type="video/mp4" />
            <track
              kind="captions"
              src={DEMO_VIDEO.captions}
              srcLang="en"
              label={DEMO_VIDEO.captionsLabel}
              default
            />
            Your browser cannot play this video. The recording will be published
            with the first release.
          </video>

          {DEMO_IS_PLACEHOLDER && (
            <span className="pointer-events-none absolute right-3 top-3 rounded-full border border-line-strong bg-canvas/95 px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.06em] text-ink-muted">
              Placeholder slate
            </span>
          )}
        </div>

        <figcaption
          id="demo-caption"
          className="mt-3 flex items-start gap-2 text-[12px] leading-relaxed text-ink-muted"
        >
          <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          <span>
            {DEMO_IS_PLACEHOLDER
              ? 'There is no app footage yet. The player holds an honest slate until the first recording is made; the file is replaced in place, at the same path.'
              : 'Recorded on the reference Windows machine. Captions are on by default and can be switched off in the player.'}
          </span>
        </figcaption>
      </figure>

      <aside aria-label="What the recording will show">
        <h3 className="font-serif text-[1.15rem] font-bold text-ink-primary">
          What the recording will show
        </h3>
        <ol className="mt-4 border-t border-line">
          {RECORDING_BEATS.map((beat) => (
            <li
              key={beat.number}
              className="grid grid-cols-[1.75rem_minmax(0,1fr)] gap-2 border-b border-line py-3.5"
            >
              <span className="font-mono text-[10px] text-ink-faint">
                {beat.number}
              </span>
              <div>
                <h4 className="text-[13px] font-semibold text-ink-primary">
                  {beat.title}
                </h4>
                <p className="mt-1 text-[12px] leading-relaxed text-ink-secondary">
                  {beat.body}
                </p>
              </div>
            </li>
          ))}
        </ol>
        <p className="mt-3 text-[11px] leading-relaxed text-ink-muted">
          These are the beats of the first recording. They are not a claim that
          the footage exists today.
        </p>
      </aside>
    </div>

    <div className="mt-16 border-t border-line pt-10">
      <div className="grid gap-8 md:grid-cols-[minmax(0,0.78fr)_minmax(0,1.22fr)] md:items-start md:gap-10">
        <div>
          <h3 className="font-serif text-[clamp(1.4rem,1.1rem+1vw,1.8rem)] font-bold leading-tight tracking-tight text-ink-primary">
            The same query, as an illustration.
          </h3>
          <p className="mt-3 max-w-[40ch] text-[14px] leading-relaxed text-ink-secondary">
            This is a working illustration of the result view, built to be poked
            at. Switch the query, filter by format, and read the match reason on
            each hit.
          </p>
          <p className="mt-4 max-w-[40ch] border-l border-line-strong pl-3 text-[12px] leading-relaxed text-ink-muted">
            It is not a screenshot and it is not a recording. The query bar is
            editable; the results stay fixed so the illustration cannot pretend
            to be live.
          </p>
        </div>
        <SearchIllustration />
      </div>
    </div>
  </Section>
);
