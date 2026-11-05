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
  '.svg': 'image/svg+xml',
};

const server = http.createServer((req, res) => {
  let filePath = path.join(DIST_DIR, req.url === '/' ? 'index.html' : req.url.split('?')[0]);
  if (!fs.existsSync(filePath)) {
    filePath = path.join(DIST_DIR, 'index.html');
  }

  const ext = path.extname(filePath);
  const contentType = MIME_TYPES[ext] || 'application/octet-stream';

  fs.readFile(filePath, (err, content) => {
    if (err) {
      res.writeHead(500);
      res.end('Server error: ' + err.code);
    } else {
      res.writeHead(200, { 'Content-Type': contentType });
      res.end(content, 'utf-8');
    }
  });
});

async function run() {
  const PORT = 4188;
  await new Promise((resolve) => server.listen(PORT, resolve));
  console.log(`Preview server running at http://localhost:${PORT}`);

  const browser = await chromium.launch();

  // 1. Desktop 1440x900
  console.log('Taking Desktop screenshot (1440x900)...');
  const desktopContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1,
  });
  const desktopPage = await desktopContext.newPage();
  await desktopPage.goto(`http://localhost:${PORT}`, { waitUntil: 'networkidle' });
  await desktopPage.waitForTimeout(500);

  // Measure performance timing
  const perfMetrics = await desktopPage.evaluate(() => {
    const nav = performance.getEntriesByType('navigation')[0];
    const paint = performance.getEntriesByType('paint');
    const fcp = paint.find((p) => p.name === 'first-contentful-paint');
    return {
      domContentLoaded: nav ? Math.round(nav.domContentLoadedEventEnd - nav.startTime) : null,
      loadTime: nav ? Math.round(nav.loadEventEnd - nav.startTime) : null,
      fcp: fcp ? Math.round(fcp.startTime) : null,
      domInteractive: nav ? Math.round(nav.domInteractive - nav.startTime) : null,
    };
  });
  console.log('Performance Metrics (Desktop):', JSON.stringify(perfMetrics, null, 2));

  // Save desktop screenshots
  const desktopViewportPath = path.join(PREVIEW_DIR, 'desktop-1440x900-hero.png');
  const desktopFullPath = path.join(PREVIEW_DIR, 'desktop-1440x900-full.png');
  await desktopPage.screenshot({ path: desktopViewportPath, fullPage: false });
  await desktopPage.screenshot({ path: desktopFullPath, fullPage: true });
  console.log(`Desktop screenshots saved: ${desktopViewportPath}, ${desktopFullPath}`);

  // Test interactive click on pdf filter tab
  const pdfButton = desktopPage.locator('button:has-text("pdf")');
  if (await pdfButton.isVisible()) {
    await pdfButton.click();
    await desktopPage.waitForTimeout(300);
    const pdfTabPath = path.join(PREVIEW_DIR, 'desktop-interactive-pdf-tab.png');
    await desktopPage.screenshot({ path: pdfTabPath });
    console.log(`Interactive screenshot saved: ${pdfTabPath}`);
  }

  await desktopContext.close();

  // 2. Mobile 390x844
  console.log('Taking Mobile screenshot (390x844)...');
  const mobileContext = await browser.newContext({
    viewport: { width: 390, height: 844 },
    deviceScaleFactor: 2,
    isMobile: true,
  });
  const mobilePage = await mobileContext.newPage();
  await mobilePage.goto(`http://localhost:${PORT}`, { waitUntil: 'networkidle' });
  await mobilePage.waitForTimeout(500);

  const mobileViewportPath = path.join(PREVIEW_DIR, 'mobile-390x844-hero.png');
  const mobileFullPath = path.join(PREVIEW_DIR, 'mobile-390x844-full.png');
  await mobilePage.screenshot({ path: mobileViewportPath, fullPage: false });
  await mobilePage.screenshot({ path: mobileFullPath, fullPage: true });
  console.log(`Mobile screenshots saved: ${mobileViewportPath}, ${mobileFullPath}`);

  await mobileContext.close();

  // Close browser and server
  await browser.close();
  server.close();
  console.log('Screenshot capture completed successfully.');

  fs.writeFileSync(
    path.join(PREVIEW_DIR, 'metrics.json'),
    JSON.stringify(perfMetrics, null, 2),
    'utf-8'
  );
}

run().catch((err) => {
  console.error('Error during screenshot run:', err);
  server.close();
  process.exit(1);
});
