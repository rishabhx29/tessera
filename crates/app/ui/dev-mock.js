// A stand-in for the Tauri IPC that exists *only* in a plain browser.
//
// The packaged app loads this file too, but the first check makes it inert:
// when the real `window.__TAURI__` is present, the mock never installs. Its
// purpose is visual review of this window without building the Rust shell —
// every payload mirrors the shapes in `crates/app/src/engine.rs` (SearchPayload,
// Preview, StatusPayload) and `main.rs` (WatchStatus), not invented ones.
//
// The corner badge it paints says "mock": a screenshot with it must never be
// presented as the real window.

(function () {
  "use strict";

  if (window.__TAURI__ || window.__TAURI_INTERNALS__) return;

  const badge = document.createElement("div");
  badge.textContent = "mock";
  badge.style.cssText =
    "position:fixed;right:8px;bottom:8px;z-index:9999;padding:2px 8px;" +
    "border-radius:999px;background:#3B424E;color:#ECF0F5;font:600 10px/1.6" +
    " ui-monospace,Consolas,monospace;letter-spacing:0.08em;opacity:0.75;";
  document.addEventListener("DOMContentLoaded", () => document.body.append(badge));

  const wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  // ---- fixtures -------------------------------------------------------------

  const hit = (text) => ({ text, hit: true });
  const miss = (text) => ({ text, hit: false });

  const ROWS = [
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-11-05 094218.png",
      name: "Screenshot 2026-11-05 094218.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      page: null,
      snippet: [miss("…the invoice from the airline, total due "), hit("€214.60"), miss(" by Nov 18. Booking reference QF-3391…")],
      arms: ["keyword", "image", "meaning"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Finance\\refunds\\airline-refund-policy.pdf",
      name: "airline-refund-policy.pdf",
      folder: "Documents\\Finance\\refunds",
      kind: "pdf",
      page: 12,
      snippet: [miss("Refunds are processed to the original payment method within "), hit("14 business days"), miss(" of…")],
      arms: ["keyword"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-11-04 171033.png",
      name: "Screenshot 2026-11-04 171033.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      page: null,
      snippet: [miss("Tessera — searched "), hit("“invoice airline”"), miss(" in 66 ms — keyword 17 · image 36 · meaning 44")],
      arms: ["keyword", "image"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Trips\\lisbon\\booking-confirmation.docx",
      name: "booking-confirmation.docx",
      folder: "Documents\\Trips\\lisbon",
      kind: "docx",
      page: null,
      snippet: [miss("Your e-ticket for the "), hit("Lisbon"), miss(" flight is attached. Check-in opens 36 hours before…")],
      arms: ["keyword", "meaning"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\IMG_4471.jpg",
      name: "IMG_4471.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      page: null,
      snippet: [],
      arms: ["image"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Trips\\lisbon\\packing.txt",
      name: "packing.txt",
      folder: "Documents\\Trips\\lisbon",
      kind: "text",
      page: null,
      snippet: [miss("passport, adapters, the travel "), hit("insurance"), miss(" card, and printouts of every booking…")],
      arms: ["meaning"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-11-03 140122.png",
      name: "Screenshot 2026-11-03 140122.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      page: null,
      snippet: [miss("Quarterly spend by category — travel "), hit("€1,284"), miss(", software €640, meals €310…")],
      arms: ["keyword", "image"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Finance\\expenses\\2026-q3.pdf",
      name: "2026-q3.pdf",
      folder: "Documents\\Finance\\expenses",
      kind: "pdf",
      page: 3,
      snippet: [miss("Airline and rail travel reimbursed at cost; see appendix for the "), hit("receipt"), miss(" list…")],
      arms: ["meaning"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\IMG_4402.jpg",
      name: "IMG_4402.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      page: null,
      snippet: [],
      arms: ["image"],
    },
    {
      path: "C:\\Users\\you\\Documents\\notes\\ocr-notes.md",
      name: "ocr-notes.md",
      folder: "Documents\\notes",
      kind: "text",
      page: null,
      snippet: [miss("The recogniser emits an [batch, steps, 18710] tensor, so cost tracks total batched "), hit("width"), miss("…")],
      arms: ["keyword"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-10-30 082210.png",
      name: "Screenshot 2026-10-30 082210.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      page: null,
      snippet: [miss("Could not read this one — the row still shows its name, folder and the words that matched.")],
      arms: ["keyword"],
    },
  ];

  // One fixture is deliberately undecodable so the glyph fallback can be
  // reviewed, exactly as the real command refuses a corrupt file. It is a
  // behaviour of the mock, not a field the engine payloads carry.
  const BROKEN_THUMBS = new Set([
    "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-10-30 082210.png",
  ]);


  const SEARCH_ARMS = ["keyword", "image", "meaning"];

  const STATUS = {
    index_path: "C:\\Users\\you\\AppData\\Local\\Tessera\\index.sqlite",
    index_mb: 113.7,
    folders: ["C:\\Users\\you\\Documents", "C:\\Users\\you\\Pictures"],
    counts: [
      ["indexed", 4998],
      ["awaiting_ocr", 0],
      ["failed", 2],
    ],
    failures: [
      {
        path: "C:\\Users\\you\\Downloads\\real-pdf-0006.pdf",
        error: "the file is 0 bytes — invalid file header",
      },
      {
        path: "C:\\Users\\you\\Downloads\\broken-export.docx",
        error: "the zip could not be opened (encrypted OOXML is not supported)",
      },
    ],
    skip_reasons: [
      ["cloud placeholder", 31],
      ["over 50 MB", 4],
    ],
    models: [
      { name: "siglip", complete: true, missing: [] },
      { name: "minilm", complete: true, missing: [] },
      { name: "ppocr", complete: true, missing: [] },
    ],
    image_vectors: 1800,
    text_vectors: 17154,
  };

  function previewFor(path, query) {
    const row = ROWS.find((candidate) => candidate.path === path);
    const isImage = row?.kind === "image";
    const terms = String(query || "")
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean);
    const segment = (text) =>
      terms.some((term) => term.length > 1 && text.toLowerCase().includes(term)) ? hit(text) : miss(text);
    const text = isImage
      ? [
          segment("BOOKING CONFIRMATION"),
          miss("\nQF-3391 · LIS → LISBON\n"),
          segment("Total due €214.60 by Nov 18"),
          miss("\nSeat 14A · Gate 22"),
        ]
      : [
          segment("Refunds are processed to the original payment method"),
          miss(" within fourteen business days of approval. Partial refunds for "),
          segment("cancellations within 24 hours"),
          miss(" carry no fee."),
        ];
    return {
      path: path,
      name: row ? row.name : "unknown file",
      kind: row ? row.kind : "txt",
      page: row ? row.page : null,
      text: text,
      image_path: isImage ? path : null,
    };
  }

  const imageCache = new Map();

  // A canvas-drawn stand-in image, so the preview pane has real pixels to lay
  // out during review. Photos get the scene the thumbnails use; screenshots
  // get the document mock.
  function fakeImageBytes(path) {
    if (imageCache.has(path)) return Promise.resolve(imageCache.get(path));
    const canvas = document.createElement("canvas");
    canvas.width = 880;
    canvas.height = 560;
    const ctx = canvas.getContext("2d");
    if (/IMG_/i.test(path)) {
      const sky = ctx.createLinearGradient(0, 0, 0, canvas.height);
      sky.addColorStop(0, "#cfe3f0");
      sky.addColorStop(0.55, "#e9dcc0");
      sky.addColorStop(1, "#7d8f5a");
      ctx.fillStyle = sky;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#f3e2b8";
      ctx.beginPath();
      ctx.arc(canvas.width * 0.76, canvas.height * 0.28, 58, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "rgba(70, 80, 45, 0.85)";
      ctx.beginPath();
      ctx.ellipse(canvas.width * 0.4, canvas.height * 0.72, 128, 66, 0.1, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.arc(canvas.width * 0.25, canvas.height * 0.62, 48, 0, Math.PI * 2);
      ctx.fill();
    } else {
      ctx.fillStyle = "#f8f9fb";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#e6e9ee";
      ctx.fillRect(0, 0, canvas.width, 36);
      ctx.fillStyle = "#2f5fe3";
      ctx.beginPath();
      ctx.arc(18, 18, 6, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "#101318";
      ctx.font = "600 22px system-ui, sans-serif";
      ctx.fillText("Booking confirmation — QF-3391", 24, 84);
      ctx.fillStyle = "#3b424e";
      ctx.font = "15px system-ui, sans-serif";
      const lines = [
        "Passenger  ·  R. T.",
        "Flight     ·  QF-3391  LIS → OPO",
        "Total due  ·  €214.60 by Nov 18",
        "Seat       ·  14A          Gate 22",
      ];
      lines.forEach((line, index) => ctx.fillText(line, 24, 130 + index * 30));
      ctx.fillStyle = "#edf2fe";
      ctx.fillRect(24, 270, 480, 130);
      ctx.strokeStyle = "#2f5fe3";
      ctx.strokeRect(24, 270, 480, 130);
      ctx.fillStyle = "#1e4bc8";
      ctx.font = "600 14px ui-monospace, Consolas, monospace";
      ctx.fillText("PAYMENT RECEIPT — 2026-11-05", 40, 300);
      ctx.fillStyle = "#5a6270";
      ctx.fillText("airline · invoice · €214.60", 40, 326);
      ctx.fillStyle = "#cbd1da";
      ctx.fillRect(24, 440, 832, 1);
    }
    return new Promise((resolve) => {
      canvas.toBlob((blob) => {
        blob.arrayBuffer().then((buffer) => {
          const bytes = new Uint8Array(buffer);
          imageCache.set(path, bytes);
          resolve(bytes);
        });
      }, "image/png");
    });
  }

  // ---- result thumbnails -----------------------------------------------------
  //
  // The real command returns a 320 px JPEG per image (see
  // `AppService::thumbnail`). The mock draws one so the list can be reviewed
  // with real pixels, and refuses the one fixture that stands for an
  // undecodable file.
  const thumbCache = new Map();

  function drawFakeThumb(ctx, path, width, height) {
    if (/IMG_/i.test(path)) {
      // A photograph: sky, sun, a field and a dark shape reading as a subject.
      const sky = ctx.createLinearGradient(0, 0, 0, height);
      sky.addColorStop(0, "#cfe3f0");
      sky.addColorStop(0.55, "#e9dcc0");
      sky.addColorStop(1, "#7d8f5a");
      ctx.fillStyle = sky;
      ctx.fillRect(0, 0, width, height);
      ctx.fillStyle = "#f3e2b8";
      ctx.beginPath();
      ctx.arc(width * 0.76, height * 0.28, 22, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = "rgba(70, 80, 45, 0.85)";
      ctx.beginPath();
      ctx.ellipse(width * 0.4, height * 0.72, 48, 26, 0.1, 0, Math.PI * 2);
      ctx.fill();
      ctx.beginPath();
      ctx.arc(width * 0.25, height * 0.62, 18, 0, Math.PI * 2);
      ctx.fill();
      return;
    }
    // A screenshot: window chrome, a title, and a small chart, so thumbnail
    // rows read as pictures at 44 px rather than as generic documents.
    const palettes = [
      ["#2f5fe3", "#e8eefc"],
      ["#3f6f8a", "#e1edf3"],
      ["#4d6b52", "#e6ede6"],
    ];
    let hash = 0;
    for (const ch of path) hash = (hash * 31 + ch.charCodeAt(0)) >>> 0;
    const [ink, wash] = palettes[hash % palettes.length];
    ctx.fillStyle = "#f8f9fb";
    ctx.fillRect(0, 0, width, height);
    ctx.fillStyle = "#e6e9ee";
    ctx.fillRect(0, 0, width, 26);
    ctx.fillStyle = ink;
    ctx.beginPath();
    ctx.arc(14, 13, 4, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#101318";
    ctx.font = "600 16px system-ui, sans-serif";
    ctx.fillText("Booking confirmation — QF-3391", 18, 56);
    ctx.fillStyle = "#3b424e";
    ctx.font = "11px system-ui, sans-serif";
    for (let index = 0; index < 3; index += 1) {
      const y = 76 + index * 16;
      ctx.fillRect(18, y, index === 1 ? width * 0.42 : width * 0.62, 6);
    }
    ctx.fillStyle = wash;
    ctx.fillRect(18, 124, width - 36, height - 150);
    ctx.fillStyle = ink;
    const bars = [0.32, 0.58, 0.44, 0.76, 0.5];
    bars.forEach((fraction, index) => {
      const barWidth = (width - 60) / bars.length - 8;
      const x = 26 + index * (barWidth + 8);
      const barHeight = (height - 166) * fraction;
      ctx.fillRect(x, height - 26 - barHeight, barWidth, barHeight);
    });
    ctx.strokeStyle = ink;
    ctx.strokeRect(18, height - 46, width * 0.3, 24);
  }

  function fakeThumbBytes(path) {
    if (thumbCache.has(path)) return Promise.resolve(thumbCache.get(path));
    return new Promise((resolve, reject) => {
      if (BROKEN_THUMBS.has(path)) {
        reject(new Error("mock: that image could not be decoded"));
        return;
      }
      const canvas = document.createElement("canvas");
      canvas.width = 320;
      canvas.height = 213;
      drawFakeThumb(canvas.getContext("2d"), path, canvas.width, canvas.height);
      canvas.toBlob((blob) => {
        blob.arrayBuffer().then((buffer) => {
          const bytes = new Uint8Array(buffer);
          thumbCache.set(path, bytes);
          resolve(bytes);
        });
      }, "image/jpeg", 0.82);
    });
  }

  // ---- the mock IPC ----------------------------------------------------------

  const handlers = {
    search({ query }) {
      const q = String(query || "").toLowerCase();
      let rows = ROWS;
      if (q.includes("nothing")) rows = [];
      // "a dog on grass" is the picture-arm example: show the image-heavy set.
      if (q.includes("dog")) rows = ROWS.filter((row) => row.kind === "image");
      const notes = [];
      if (q.includes("noimg")) notes.push("the image arm has no vectors to rank yet");
      return Promise.resolve({
        rows: rows,
        notes: notes,
        millis: 1240,
        engine_millis: 66,
        arms: SEARCH_ARMS,
      });
    },
    preview({ path, query }) {
      return wait(30).then(() => previewFor(path, query));
    },
    preview_image({ path }) {
      return wait(40).then(() => fakeImageBytes(path));
    },
    thumbnail({ path }) {
      // A little latency, so the skeleton is a real state rather than a
      // below-the-fold myth.
      return wait(70).then(() => fakeThumbBytes(path));
    },
    status() {
      return Promise.resolve(STATUS);
    },
    // The home state's newest-files list. The fixtures become the six newest
    // rows, spread from a minute ago to over a week, so the relative labels
    // exercise every form the real window can print. Counts match STATUS.
    recent({ limit }) {
      const now = Date.now() / 1000;
      const ages = [40, 22 * 60, 3.2 * 3600, 29 * 3600, 4 * 86400, 11 * 86400, 16 * 86400, 23 * 86400];
      const rows = ROWS.slice(0, limit ?? 6).map((row, index) => ({
        path: row.path,
        name: row.name,
        folder: row.folder,
        kind: row.kind,
        mtime: (now - ages[index % ages.length]) * 1e9,
      }));
      return wait(20).then(() => ({ rows, indexed: 4998, folders: 2 }));
    },
    index_folder({ path }) {
      return wait(900).then(
        () => `Indexed 128 files from ${path} (3 skipped, 1 failed, 0 awaiting OCR)`,
      );
    },
    rebuild_index() {
      return wait(400).then(() => STATUS.folders);
    },
    confirm_rebuild() {
      return Promise.resolve(window.confirm("Rebuild the index? (mock)"));
    },
    install_models({ sets }) {
      return wait(1500).then(() => sets.map((set) => `${set}: downloaded and verified (mock)`));
    },
    pick_folder() {
      return Promise.resolve("C:\\Users\\you\\Pictures\\Screenshots");
    },
    autostart_enabled() {
      return Promise.resolve(false);
    },
    set_autostart() {
      return Promise.resolve();
    },
    reveal() {
      return Promise.resolve();
    },
  };

  window.__TAURI__ = {
    core: {
      invoke(command, args = {}) {
        const handler = handlers[command];
        if (!handler) return Promise.reject(new Error(`mock: no handler for ${command}`));
        return handler(args);
      },
    },
  };
})();
