const http = require('http');
const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const DIST_DIR = path.join(__dirname, 'dist');
const PREVIEW_DIR = path.join(__dirname, '.preview');

if (!fs.existsSync(PREVIEW_DIR)) {
  fs.mkdirSync(PREVIEW_DIR, { recursive: true });
}

const MIME_TYPES = {
  '.html': 'text/html',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.mp4': 'video/mp4',
  '.webm': 'video/webm',
  '.vtt': 'text/vtt',
};

const server = http.createServer((req, res) => {
  let filePath = path.join(
    DIST_DIR,
    req.url === '/' ? 'index.html' : decodeURIComponent(req.url.split('?')[0])
  );
  if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    filePath = path.join(DIST_DIR, 'index.html');
  }

  const ext = path.extname(filePath);
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';

  fs.readFile(filePath, (err, content) => {
    if (err) {
      res.writeHead(500);
      res.end('Server error: ' + err.code);
    } else {
      res.writeHead(200, {
        'Content-Type': contentType,
        'Accept-Ranges': 'bytes',
        'Cache-Control': 'no-store',
      });
      res.end(content);
    }
  });
});

const checks = {
  desktop: {},
  mobile: {},
  reducedMotion: {},
};

function watchPage(page, bucket) {
  bucket.consoleErrors = [];
  bucket.pageErrors = [];
  page.addInitScript(() => {
    window.__perf = { lcp: null, cls: 0 };
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) window.__perf.lcp = entry.startTime;
    }).observe({ type: 'largest-contentful-paint', buffered: true });
    new PerformanceObserver((list) => {
      for (const entry of list.getEntries()) {
        if (!entry.hadRecentInput) window.__perf.cls += entry.value;
      }
    }).observe({ type: 'layout-shift', buffered: true });
  });
  page.on('console', (message) => {
    if (message.type() === 'error') {
      bucket.consoleErrors.push(message.text());
    }
  });
  page.on('pageerror', (error) => {
    bucket.pageErrors.push(String(error));
  });
}

// Fixed UI (sticky header, skip link) appears at odd offsets in element
// screenshots. Hide it only while a section is captured, then restore it.
async function shootSection(page, selector, file) {
  const section = page.locator(selector);
  await section.scrollIntoViewIfNeeded();
  // Lenis animates the scroll, so reveals start late. Wait them out so the
  // capture is never mid-stagger.
  await page.waitForTimeout(1800);
  const style = await page.addStyleTag({
    content:
      'header{position:static!important}.skip-link{display:none!important}',
  });
  try {
    await section.screenshot({ path: file });
  } finally {
    await style.evaluate((node) => node.remove());
  }
}

async function inspect(page) {  return page.evaluate(() => {
    const video = document.querySelector('video');
    const buttons = Array.from(document.querySelectorAll('button'));
    const unnamedButtons = buttons
      .filter((button) => {
        const label =
          button.getAttribute('aria-label') ||
          button.textContent.replace(/\s+/g, ' ').trim();
        return !label;
      })
      .map((button) => button.outerHTML.slice(0, 120));

    const missingAlt = Array.from(document.querySelectorAll('img'))
      .filter((img) => !img.hasAttribute('alt'))
      .map((img) => img.getAttribute('src'));

    const targets = Array.from(
      document.querySelectorAll('a[href], button, input, summary')
    ).filter((el) => {
      const rect = el.getBoundingClientRect();
      return rect.width > 0 && rect.height > 0;
    });

    const smallTargets = targets
      .filter((el) => {
        const rect = el.getBoundingClientRect();
        return rect.height < 24 || rect.width < 24;
      })
      .slice(0, 8)
      .map((el) => ({
        tag: el.tagName,
        text: el.textContent.replace(/\s+/g, ' ').trim().slice(0, 40),
        width: Math.round(el.getBoundingClientRect().width),
        height: Math.round(el.getBoundingClientRect().height),
      }));

    return {
      title: document.title,
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      horizontalOverflow:
        document.documentElement.scrollWidth >
        document.documentElement.clientWidth + 1,
      video: video
        ? {
            src: video.currentSrc,
            poster: video.getAttribute('poster'),
            duration: Number.isFinite(video.duration)
              ? Math.round(video.duration * 100) / 100
              : null,
            readyState: video.readyState,
            controls: video.controls,
            autoplay: video.autoplay,
            tracks: video.querySelectorAll('track').length,
            captionsSrc: video.querySelector('track')?.getAttribute('src'),
          }
        : null,
      unnamedButtons,
      missingAlt,
      smallTargets,
      reducedMotion: window.matchMedia('(prefers-reduced-motion: reduce)')
        .matches,
      canHover: window.matchMedia('(hover: hover) and (pointer: fine)').matches,
    };
  });
}

async function run() {
  const PORT = 4188;
  await new Promise((resolve) => server.listen(PORT, resolve));
  console.log(`Preview server running at http://localhost:${PORT}`);

  const browser = await chromium.launch();

  // 1. Desktop 1440x900
  console.log('Desktop 1440x900...');
  const desktopContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
  });
  const desktopPage = await desktopContext.newPage();
  watchPage(desktopPage, checks.desktop);
  await desktopPage.goto(`http://localhost:${PORT}`, {
    waitUntil: 'networkidle',
  });
  await desktopPage.waitForTimeout(500);

  checks.desktop.performance = await desktopPage.evaluate(() => {
    const nav = performance.getEntriesByType('navigation')[0];
    const paint = performance.getEntriesByType('paint');
    const fcp = paint.find((p) => p.name === 'first-contentful-paint');
    return {
      lcp: window.__perf.lcp ? Math.round(window.__perf.lcp) : null,
      cls: Math.round(window.__perf.cls * 1000) / 1000,
      hydrated: document.querySelectorAll('#root > *').length > 0,
      headPreloads: Array.from(
        document.head.querySelectorAll('link[rel="preload"]')
      ).map((link) => link.getAttribute('href')),
      rootPreloads: Array.from(
        document.querySelectorAll('#root link[rel="preload"]')
      ).length,
      domContentLoaded: nav
        ? Math.round(nav.domContentLoadedEventEnd - nav.startTime)
        : null,
      loadTime: nav ? Math.round(nav.loadEventEnd - nav.startTime) : null,
      fcp: fcp ? Math.round(fcp.startTime) : null,
      domInteractive: nav ? Math.round(nav.domInteractive - nav.startTime) : null,
    };
  });

  await desktopPage.screenshot({
    path: path.join(PREVIEW_DIR, 'desktop-1440x900-hero.png'),
  });
  await desktopPage.screenshot({
    path: path.join(PREVIEW_DIR, 'desktop-1440x900-full.png'),
    fullPage: true,
  });

  for (const [name, selector] of [
    ['problem', '#problem'],
    ['product', '#product'],
    ['demo', '#demo'],
    ['numbers', '#numbers'],
    ['privacy', '#privacy'],
    ['install', '#install'],
    ['faq', '#faq'],
  ]) {
    const section = desktopPage.locator(selector);
    if (await section.count()) {
      await shootSection(
        desktopPage,
        selector,
        path.join(PREVIEW_DIR, `desktop-section-${name}.png`)
      );
    }
  }

  // Keyboard: first Tab should reveal the skip link, then land on the wordmark.
  await desktopPage.keyboard.press('Tab');
  checks.desktop.firstTab = await desktopPage.evaluate(() => {
    const el = document.activeElement;
    const styles = getComputedStyle(el);
    return {
      tag: el.tagName,
      text: el.textContent.replace(/\s+/g, ' ').trim(),
      transform: styles.transform,
      visible: el.getBoundingClientRect().top >= 0,
    };
  });
  await desktopPage.screenshot({
    path: path.join(PREVIEW_DIR, 'desktop-focus-skip-link.png'),
  });

  await desktopPage.keyboard.press('Tab');
  checks.desktop.secondTab = await desktopPage.evaluate(() => ({
    tag: document.activeElement.tagName,
    text: document.activeElement.textContent.replace(/\s+/g, ' ').trim(),
  }));

  // Focus ring on a primary action.
  await desktopPage.locator('a.btn-primary').first().focus();
  checks.desktop.focusRing = await desktopPage.evaluate(() => {
    const el = document.activeElement;
    const styles = getComputedStyle(el);
    return {
      text: el.textContent.replace(/\s+/g, ' ').trim(),
      outlineWidth: styles.outlineWidth,
      outlineColor: styles.outlineColor,
      outlineOffset: styles.outlineOffset,
    };
  });

  // Exercise the sample search and read the live result count.
  const demoInput = desktopPage.locator('#demo-query');
  if (await demoInput.count()) {
    await demoInput.fill('invoice');
    await desktopPage.waitForTimeout(250);
    checks.desktop.demoSearch = await demoInput.evaluate((input) => {
      const status = input
        .closest('section')
        ?.querySelector('[aria-live="polite"]');
      return {
        value: input.value,
        announced: status ? status.textContent.trim() : null,
      };
    });
    await desktopPage.screenshot({
      path: path.join(PREVIEW_DIR, 'desktop-interactive-search.png'),
    });
    await demoInput.fill('');
  }

  checks.desktop.dom = await inspect(desktopPage);
  await desktopContext.close();

  // 2. Mobile 390x844 (touch, no hover)
  console.log('Mobile 390x844...');
  const mobileContext = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
    hasTouch: true,
  });
  const mobilePage = await mobileContext.newPage();
  watchPage(mobilePage, checks.mobile);
  await mobilePage.goto(`http://localhost:${PORT}`, {
    waitUntil: 'networkidle',
  });
  await mobilePage.waitForTimeout(500);

  await mobilePage.screenshot({
    path: path.join(PREVIEW_DIR, 'mobile-390x844-hero.png'),
  });
  await mobilePage.screenshot({
    path: path.join(PREVIEW_DIR, 'mobile-390x844-full.png'),
    fullPage: true,
  });

  for (const [name, selector] of [
    ['demo', '#demo'],
    ['numbers', '#numbers'],
    ['privacy', '#privacy'],
    ['install', '#install'],
  ]) {
    const section = mobilePage.locator(selector);
    if (await section.count()) {
      await shootSection(
        mobilePage,
        selector,
        path.join(PREVIEW_DIR, `mobile-section-${name}.png`)
      );
    }
  }

  const menuButton = mobilePage.locator('button[aria-controls="mobile-nav"]');
  checks.mobile.menuButtonVisible = await menuButton.isVisible();
  if (checks.mobile.menuButtonVisible) {
    await menuButton.click();
    await mobilePage.waitForTimeout(250);
    checks.mobile.menuExpanded = await menuButton.getAttribute('aria-expanded');
    await mobilePage.screenshot({
      path: path.join(PREVIEW_DIR, 'mobile-390x844-menu.png'),
    });
    await mobilePage.keyboard.press('Escape');
    checks.mobile.menuExpandedAfterEscape =
      await menuButton.getAttribute('aria-expanded');
  }

  checks.mobile.dom = await inspect(mobilePage);
  await mobileContext.close();

  // 3. Reduced motion
  console.log('Reduced motion...');
  const reducedContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: 'reduce',
  });
  const reducedPage = await reducedContext.newPage();
  watchPage(reducedPage, checks.reducedMotion);
  await reducedPage.goto(`http://localhost:${PORT}`, {
    waitUntil: 'networkidle',
  });
  await reducedPage.waitForTimeout(400);
  checks.reducedMotion.styles = await reducedPage.evaluate(() => {
    const button = document.querySelector('.btn');
    const html = getComputedStyle(document.documentElement);
    return {
      mediaMatches: window.matchMedia('(prefers-reduced-motion: reduce)')
        .matches,
      scrollBehavior: html.scrollBehavior,
      buttonTransitionDuration: button
        ? getComputedStyle(button).transitionDuration
        : null,
    };
  });
  await reducedPage.screenshot({
    path: path.join(PREVIEW_DIR, 'desktop-reduced-motion-hero.png'),
  });
  await shootSection(
    reducedPage,
    '#demo',
    path.join(PREVIEW_DIR, 'desktop-reduced-motion-demo.png')
  );
  checks.reducedMotion.dom = await inspect(reducedPage);
  await reducedContext.close();

  // 4. No JavaScript: the build prerenders, so the page must still read.
  const noJsContext = await browser.newContext({
    javaScriptEnabled: false,
    viewport: { width: 1280, height: 900 },
  });
  const noJsPage = await noJsContext.newPage();
  await noJsPage.goto(`http://localhost:${PORT}`, { waitUntil: 'load' });
  checks.noJs = await noJsPage.evaluate(() => ({
    h1: document.querySelector('h1')?.textContent.replace(/\s+/g, ' ').trim(),
    sections: document.querySelectorAll('main section').length,
    rootChildren: document.getElementById('root')?.children.length ?? 0,
    faqTriggers: document.querySelectorAll('[aria-expanded]').length,
  }));
  await noJsPage.screenshot({
    path: path.join(PREVIEW_DIR, 'desktop-no-js-hero.png'),
  });
  await noJsContext.close();

  await browser.close();
  server.close();

  fs.writeFileSync(
    path.join(PREVIEW_DIR, 'checks.json'),
    JSON.stringify(checks, null, 2),
    'utf-8'
  );
  fs.writeFileSync(
    path.join(PREVIEW_DIR, 'metrics.json'),
    JSON.stringify(checks.desktop.performance, null, 2),
    'utf-8'
  );

  console.log(JSON.stringify(checks, null, 2));
  console.log('Screenshot capture completed successfully.');
}

run().catch((err) => {
  console.error('Error during screenshot run:', err);
  server.close();
  process.exit(1);
});
