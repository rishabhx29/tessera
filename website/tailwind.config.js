import plugin from 'tailwindcss/plugin';

/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        paper: {
          DEFAULT: '#F6F2EA',
          deep: '#EDE6D8',
          card: '#FCFAF6',
        },
        ink: {
          DEFAULT: '#16140F',
          soft: '#3E392F',
          muted: '#6B6557',
        },
        line: {
          DEFAULT: '#DFD7C8',
          strong: '#C6BDAB',
        },
        accent: {
          DEFAULT: '#C2410C',
          deep: '#8E2D06',
          soft: '#F7E6D8',
        },
        night: {
          DEFAULT: '#12110E',
          soft: '#1B1915',
          line: '#33302A',
          ink: '#F2EDE3',
          muted: '#A69E8D',
        },
        sage: {
          DEFAULT: '#5E7358',
          deep: '#4F6249',
        },
        slate: '#4C5A66',
      },
      fontFamily: {
        display: ['"Instrument Serif"', 'Charter', 'Georgia', 'serif'],
        sans: [
          '"Inter Tight Variable"',
          '"Inter Tight"',
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'Roboto',
          'sans-serif',
        ],
        mono: [
          '"JetBrains Mono Variable"',
          'ui-monospace',
          '"Cascadia Code"',
          'Consolas',
          'monospace',
        ],
      },
      boxShadow: {
        window: '0 40px 80px -30px rgba(22, 20, 15, 0.45), 0 18px 40px -20px rgba(22, 20, 15, 0.30)',
        lift: '0 20px 40px -24px rgba(22, 20, 15, 0.35)',
      },
      letterSpacing: {
        display: '-0.02em',
      },
      transitionTimingFunction: {
        out: 'cubic-bezier(0.16, 1, 0.3, 1)',
        'in-out': 'cubic-bezier(0.65, 0, 0.35, 1)',
      },
    },
  },
  plugins: [
    // Hover styles only apply where a real pointer can hover. Touch devices
    // otherwise fire hover on tap and leave the state stuck.
    plugin(({ addVariant }) => {
      addVariant('hover', '@media (hover: hover) and (pointer: fine) { &:hover }');
    }),
  ],
}
