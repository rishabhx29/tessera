/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        canvas: '#FAF9F5',
        surface: '#FFFFFF',
        subtle: '#F3F1EB',
        inset: '#EBE8E0',
        line: '#E5E2D9',
        'line-strong': '#D1CDC0',
        ink: {
          primary: '#181715',
          secondary: '#45423D',
          muted: '#59554E',
          faint: '#6B665E',
        },
        accent: {
          DEFAULT: '#C2410C',
          hover: '#9A3412',
          subtle: '#FFF7ED',
          border: '#FDBA74',
        },
      },
      fontFamily: {
        serif: [
          'Charter',
          'Bitstream Charter',
          'Sitka Text',
          'Cambria',
          'Georgia',
          'serif',
        ],
        sans: [
          '-apple-system',
          'BlinkMacSystemFont',
          '"Segoe UI"',
          'Roboto',
          '"Helvetica Neue"',
          'Arial',
          'sans-serif',
        ],
        mono: [
          'ui-monospace',
          '"Cascadia Code"',
          '"Source Code Pro"',
          'Menlo',
          'Consolas',
          'monospace',
        ],
      },
      boxShadow: {
        'tile': '0 1px 3px rgba(24, 23, 21, 0.05), 0 1px 2px rgba(24, 23, 21, 0.03)',
        'elevated': '0 10px 30px -10px rgba(24, 23, 21, 0.08), 0 4px 6px -2px rgba(24, 23, 21, 0.03)',
        'modal': '0 25px 50px -12px rgba(24, 23, 21, 0.14)',
      },
      letterSpacing: {
        'tightest': '-0.03em',
        'tighter': '-0.02em',
        'tight': '-0.01em',
      },
    },
  },
  plugins: [],
}
