import { readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const root = process.cwd();
const templatePath = path.join(root, 'dist', 'index.html');
const ssrEntry = path.join(root, 'dist-ssr', 'entry-server.js');
const marker = '<div id="root"></div>';

const template = await readFile(templatePath, 'utf8');
if (!template.includes(marker)) {
  throw new Error(`Prerender: ${marker} not found in ${templatePath}`);
}

const { render } = await import(pathToFileURL(ssrEntry).href);
const markup = render();

let page = template.replace(marker, `<div id="root">${markup}</div>`);

// The headline and lede swap fonts late, and Lighthouse counts the swap as the
// largest paint. Start the three files the first screen actually uses while
// the CSS is still parsing.
const cssHref = page.match(/<link rel="stylesheet"[^>]*href="([^"]+\.css)"/)?.[1];
if (cssHref) {
  const cssPath = path.join(root, 'dist', cssHref.replace(/^\//, ''));
  const css = await readFile(cssPath, 'utf8');
  const wanted = [
    /inter-tight-latin-wght-normal-[^")]+\.woff2/,
    /instrument-serif-latin-400-normal-[^")]+\.woff2/,
    /instrument-serif-latin-400-italic-[^")]+\.woff2/,
  ];
  const links = wanted
    .map((pattern) => css.match(pattern)?.[0])
    .filter(Boolean)
    .map(
      (file) =>
        `    <link rel="preload" as="font" type="font/woff2" crossorigin href="/assets/${file}" />`
    )
    .join('\n');

  if (links) page = page.replace('</head>', `${links}\n  </head>`);
}

await writeFile(templatePath, page, 'utf8');
console.log(`Prerendered ${markup.length} characters into dist/index.html`);
