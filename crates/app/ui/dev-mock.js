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
    "border-radius:999px;background:#59554E;color:#FAF9F5;font:600 10px/1.6" +
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
      kind: "png",
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
      path: "C:\\Users\\you\\Documents\\Trips\\lisbon\\booking-confirmation.docx",
      name: "booking-confirmation.docx",
      folder: "Documents\\Trips\\lisbon",
      kind: "docx",
      page: null,
      snippet: [miss("Your e-ticket for the "), hit("Lisbon"), miss(" flight is attached. Check-in opens 36 hours before…")],
      arms: ["keyword", "meaning"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Trips\\lisbon\\packing.txt",
      name: "packing.txt",
      folder: "Documents\\Trips\\lisbon",
      kind: "txt",
      page: null,
      snippet: [miss("passport, adapters, the travel "), hit("insurance"), miss(" card, and printouts of every booking…")],
      arms: ["meaning"],
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
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\IMG_4471.jpg",
      name: "IMG_4471.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "jpg",
      page: null,
      snippet: [],
      arms: ["image"],
    },
    {
      path: "C:\\Users\\you\\Documents\\notes\\ocr-notes.md",
      name: "ocr-notes.md",
      folder: "Documents\\notes",
      kind: "md",
      page: null,
      snippet: [miss("The recogniser emits an [batch, steps, 18710] tensor, so cost tracks total batched "), hit("width"), miss("…")],
      arms: ["keyword"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\Screenshot 2026-11-04 171033.png",
      name: "Screenshot 2026-11-04 171033.png",
      folder: "Pictures\\Screenshots",
      kind: "png",
      page: null,
      snippet: [miss("Tessera — searched "), hit("“invoice airline”"), miss(" in 66 ms — keyword 17 · image 36 · meaning 44")],
      arms: ["keyword"],
    },
  ];

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

  const WATCH_LOG = [
    "[watch] watching 2 roots",
    "[watch] indexed 1 — Screenshot 2026-11-05 094218.png (32.9 ms OCR, 1 chunk embedded)",
    "[watch] indexed 1 — quarterly-report.docx (2.4 ms extract, 3 chunks embedded)",
    "[watch] unchanged 214 on the 60 s rescan",
    "[watch] queue drained — sessions released, idle 43 MB",
    "[watch] rescan owed after store outage; retrying in 2 s",
    "[watch] retrying broken-export.docx (attempt 2, next in 10 m)",
    "[watch] queue drained",
  ];

  function previewFor(path, query) {
    const row = ROWS.find((candidate) => candidate.path === path);
    const isImage = row && /\.(png|jpe?g)$/i.test(row.kind === "jpg" ? "x.jpg" : row.name);
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

  // A canvas-drawn stand-in screenshot, so the preview pane has real pixels
  // to lay out during review.
  function fakeImageBytes(path) {
    if (imageCache.has(path)) return Promise.resolve(imageCache.get(path));
    const canvas = document.createElement("canvas");
    canvas.width = 880;
    canvas.height = 560;
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#f6f4ef";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "#e5e2d9";
    ctx.fillRect(0, 0, canvas.width, 36);
    ctx.fillStyle = "#c2410c";
    ctx.beginPath();
    ctx.arc(18, 18, 6, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#181715";
    ctx.font = "600 22px system-ui, sans-serif";
    ctx.fillText("Booking confirmation — QF-3391", 24, 84);
    ctx.fillStyle = "#45423d";
    ctx.font = "15px system-ui, sans-serif";
    const lines = [
      "Passenger  ·  R. T.",
      "Flight     ·  QF-3391  LIS → OPO",
      "Total due  ·  €214.60 by Nov 18",
      "Seat       ·  14A          Gate 22",
    ];
    lines.forEach((line, index) => ctx.fillText(line, 24, 130 + index * 30));
    ctx.fillStyle = "#fff7ed";
    ctx.fillRect(24, 270, 480, 130);
    ctx.strokeStyle = "#c2410c";
    ctx.strokeRect(24, 270, 480, 130);
    ctx.fillStyle = "#9a3412";
    ctx.font = "600 14px ui-monospace, Consolas, monospace";
    ctx.fillText("PAYMENT RECEIPT — 2026-11-05", 40, 300);
    ctx.fillStyle = "#59554e";
    ctx.fillText("airline · invoice · €214.60", 40, 326);
    ctx.fillStyle = "#d1cdc0";
    ctx.fillRect(24, 440, 832, 1);
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

  // ---- the mock IPC ----------------------------------------------------------

  const handlers = {
    search({ query }) {
      const q = String(query || "").toLowerCase();
      const rows = q.includes("nothing") ? [] : ROWS;
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
    status() {
      return Promise.resolve(STATUS);
    },
    watch_status() {
      return Promise.resolve({
        running: true,
        paused: false,
        log: WATCH_LOG,
        summary: null,
      });
    },
    set_paused() {
      return Promise.resolve();
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
