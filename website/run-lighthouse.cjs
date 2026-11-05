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
  const PORT = 4195;
  await new Promise((resolve) => server.listen(PORT, resolve));
  console.log(`Lighthouse target server running on http://127.0.0.1:${PORT}`);

  const DEBUG_PORT = 9222;
  console.log('Launching Chromium on remote debugging port', DEBUG_PORT);
  const browser = await chromium.launch({
    args: [`--remote-debugging-port=${DEBUG_PORT}`],
  });

  try {
    const lighthouse = require('lighthouse').default || require('lighthouse');

    console.log('Running programmatic Lighthouse audit...');
    const result = await lighthouse(`http://127.0.0.1:${PORT}`, {
      port: DEBUG_PORT,
      output: 'json',
      logLevel: 'error',
      onlyCategories: ['performance', 'accessibility', 'best-practices', 'seo'],
    });

    const lhr = result.lhr;
    const scores = {
      performance: Math.round((lhr.categories.performance?.score || 0) * 100),
      accessibility: Math.round((lhr.categories.accessibility?.score || 0) * 100),
      bestPractices: Math.round((lhr.categories['best-practices']?.score || 0) * 100),
      seo: Math.round((lhr.categories.seo?.score || 0) * 100),
      fcp: lhr.audits['first-contentful-paint']?.displayValue,
      lcp: lhr.audits['largest-contentful-paint']?.displayValue,
      cls: lhr.audits['cumulative-layout-shift']?.displayValue,
      tbt: lhr.audits['total-blocking-time']?.displayValue,
    };

    console.log('Lighthouse Audit Results:');
    console.log(JSON.stringify(scores, null, 2));

    fs.writeFileSync(
      path.join(PREVIEW_DIR, 'lighthouse-summary.json'),
      JSON.stringify(scores, null, 2),
      'utf8'
    );

    fs.writeFileSync(
      path.join(PREVIEW_DIR, 'lighthouse-report.json'),
      JSON.stringify(lhr, null, 2),
      'utf8'
    );
  } catch (err) {
    console.error('Lighthouse audit error:', err);
  } finally {
    await browser.close();
    server.close();
  }
}

run();
