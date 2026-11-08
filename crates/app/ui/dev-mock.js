// A stand-in for the Tauri IPC that exists *only* in a plain browser.
//
// The packaged app loads this file too, but the first check makes it inert:
// when the real `window.__TAURI__` is present, the mock never installs. Its
// purpose is visual review of this window without building the Rust shell.
//
// Payloads mirror the shapes in `crates/app/src/engine.rs` (Row, SearchPayload,
// Preview, LibraryPayload, StatusPayload) and the lens rules in
// `crates/core/src/fusion.rs` (SearchMode). The fixtures make the distinction
// visible: `samurai.jpg` is a picture that looks like a samurai,
// `samurai-notes.png` is a screenshot that says the word, and `trip.pdf` is a
// document that discusses one. Each lens answers with only what it may see.
//
// The corner badge says "mock": a screenshot with it must never be presented
// as the real window.

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

  const HOUR = 3600 * 1000;
  const now = Date.now();
  const mtime = (hoursAgo) => (now - hoursAgo * HOUR) * 1e6;

  const PICTURES = [
    {
      path: "C:\\Users\\you\\Pictures\\Trips\\kyoto\\samurai.jpg",
      name: "samurai.jpg",
      folder: "Pictures\\Trips\\kyoto",
      kind: "image",
      width: 1600,
      height: 1067,
      mtime: mtime(3),
      art: "samurai",
      tags: ["samurai", "warrior", "sword", "armor", "japan", "figure", "silhouette"],
      lines: ["temple archive — 2024", "scan 04"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\samurai-notes.png",
      name: "samurai-notes.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      width: 900,
      height: 1400,
      mtime: mtime(5),
      art: "page",
      tags: ["notes", "research", "history", "battle", "study"],
      lines: [
        "samurai — research notes",
        "the samurai served the daimyo",
        "armour, horse, sword",
        "read chapter 4 before friday",
      ],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Trips\\lisbon\\bridge.jpg",
      name: "bridge.jpg",
      folder: "Pictures\\Trips\\lisbon",
      kind: "image",
      width: 1600,
      height: 900,
      mtime: mtime(8),
      art: "bridge",
      tags: ["bridge", "water", "river", "dusk", "sky"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\receipt.png",
      name: "receipt.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      width: 1000,
      height: 1400,
      mtime: mtime(11),
      art: "receipt",
      tags: ["receipt", "shop", "paper", "grocery"],
      lines: ["CITY MARKET", "coffee beans 12.50", "olive oil 9.40", "total 24.90"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\garden.jpg",
      name: "garden.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      width: 1200,
      height: 1600,
      mtime: mtime(16),
      art: "garden",
      tags: ["garden", "plants", "green", "leaves", "flowers"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\sky.jpg",
      name: "sky.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      width: 2400,
      height: 1000,
      mtime: mtime(20),
      art: "sky",
      tags: ["sky", "clouds", "sunset", "horizon"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\invoice.png",
      name: "invoice.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      width: 1000,
      height: 1300,
      mtime: mtime(26),
      art: "receipt",
      tags: ["invoice", "paper", "billing"],
      lines: ["INVOICE 2041", "design work 480.00", "total due 480.00", "pay by 30 november"],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\harbor.jpg",
      name: "harbor.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      width: 1600,
      height: 1200,
      mtime: mtime(30),
      art: "harbor",
      tags: ["harbor", "boat", "water", "sea", "port"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\broken.jpg",
      name: "broken.jpg",
      folder: "Pictures\\Screenshots",
      kind: "image",
      width: 1400,
      height: 1050,
      mtime: mtime(34),
      art: "field",
      broken: true,
      tags: ["field", "grass", "meadow"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Camera Roll\\leaf.jpg",
      name: "leaf.jpg",
      folder: "Pictures\\Camera Roll",
      kind: "image",
      width: 1200,
      height: 1200,
      mtime: mtime(41),
      art: "garden",
      tags: ["leaf", "green", "macro", "plant"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Trips\\lisbon\\street.jpg",
      name: "street.jpg",
      folder: "Pictures\\Trips\\lisbon",
      kind: "image",
      width: 1400,
      height: 1050,
      mtime: mtime(49),
      art: "street",
      tags: ["street", "city", "night", "lights"],
      lines: [],
    },
    {
      path: "C:\\Users\\you\\Pictures\\Screenshots\\laptop.png",
      name: "laptop.png",
      folder: "Pictures\\Screenshots",
      kind: "image",
      width: 1600,
      height: 1000,
      mtime: mtime(58),
      art: "field",
      tags: ["laptop", "desk", "screen", "work"],
      lines: [],
    },
  ];

  const DOCUMENTS = [
    {
      path: "C:\\Users\\you\\Documents\\Trips\\kyoto\\trip.pdf",
      name: "trip.pdf",
      folder: "Documents\\Trips\\kyoto",
      kind: "pdf",
      page: 7,
      mtime: mtime(7),
      tags: ["travel", "japan", "kyoto", "culture"],
      lines: [
        "chapter 4 — kyoto",
        "the samurai class lost its privileges",
        "the trip ends in osaka",
      ],
    },
    {
      path: "C:\\Users\\you\\Documents\\Trips\\lisbon\\packing.txt",
      name: "packing.txt",
      folder: "Documents\\Trips\\lisbon",
      kind: "text",
      page: null,
      mtime: mtime(14),
      tags: ["travel", "packing", "list"],
      lines: ["packing list", "passport, charger, adapter", "walking shoes"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Work\\summary.docx",
      name: "summary.docx",
      folder: "Documents\\Work",
      kind: "docx",
      page: null,
      mtime: mtime(23),
      tags: ["work", "report", "revenue", "quarter"],
      lines: ["quarter summary", "revenue grew 12 percent", "hiring stays flat"],
    },
    {
      path: "C:\\Users\\you\\Documents\\Finance\\refund-policy.pdf",
      name: "refund-policy.pdf",
      folder: "Documents\\Finance",
      kind: "pdf",
      page: 12,
      mtime: mtime(37),
      tags: ["finance", "refund", "policy"],
      lines: [
        "refund policy",
        "refunds are processed within 14 days",
        "keep the original receipt",
      ],
    },
  ];

  let ALL = PICTURES.concat(DOCUMENTS);

  let roots = [
    { path: "C:\\Users\\you\\Pictures", count: PICTURES.length },
    { path: "C:\\Users\\you\\Documents", count: DOCUMENTS.length },
  ];

  let paused = false;
  let autostart = false;

  const models = [
    { name: "text", complete: true, missing: [] },
    { name: "vision", complete: false, missing: ["image_encoder.onnx", "text_encoder.onnx"] },
  ];

  // ---- matching -------------------------------------------------------------

  const terms = (query) =>
    String(query ?? "")
      .toLowerCase()
      .split(/\s+/)
      .filter(Boolean);

  const bodyHits = (file, query) => {
    const words = terms(query);
    if (!words.length) return [];
    return (file.lines ?? []).filter((line) => {
      const text = line.toLowerCase();
      return words.some((word) => text.includes(word));
    });
  };

  const nameHit = (file, query) => {
    const name = file.name.toLowerCase();
    return terms(query).some((word) => name.includes(word));
  };

  const tagHit = (file, query) => {
    const tags = file.tags ?? [];
    return terms(query).some((word) => tags.some((tag) => tag.includes(word)));
  };

  // Same gates as SearchMode: pictures runs only the image arm and no lexical
  // arm; words and documents need text inside the file, so the filename line
  // alone cannot answer them.
  function search(query, mode, limit) {
    const lens = mode || "all";
    const words = terms(query);
    const rows = [];

    for (const file of ALL) {
      const kindOk =
        lens === "all" ||
        ((lens === "pictures" || lens === "words") && file.kind === "image") ||
        (lens === "documents" && file.kind !== "image");
      if (!kindOk) continue;

      const body = bodyHits(file, query);
      const named = nameHit(file, query);
      const tagged = tagHit(file, query);
      const arms = [];

      if (lens === "pictures") {
        if (tagged) arms.push("image");
      } else if (lens === "all") {
        if (tagged) arms.push("image");
        if (body.length || named) arms.push("keyword");
        if (tagged) arms.push("meaning");
      } else if (lens === "words") {
        if (body.length) arms.push("keyword");
        if (tagged) arms.push("meaning");
      } else {
        if (body.length || named) arms.push("keyword");
        if (tagged) arms.push("meaning");
      }

      const needsText = lens === "words" || lens === "documents";
      if (needsText && body.length === 0) arms.length = 0;
      if (!arms.length) continue;

      rows.push({
        path: file.path,
        name: file.name,
        folder: file.folder,
        kind: file.kind,
        page: file.page ?? null,
        snippet: body.length ? highlight(body[0], words) : [],
        arms: [...new Set(arms)],
        width: file.width ?? null,
        height: file.height ?? null,
        marks: file.kind === "image" ? marksFor(file, words) : [],
        name_only: Boolean(named && body.length === 0),
        _rank: arms.length,
        _mtime: file.mtime,
      });
    }

    rows.sort((a, b) => b._rank - a._rank || b._mtime - a._mtime);
    const answer = rows.slice(0, Math.min(limit || 60, 60));
    for (const row of answer) {
      delete row._rank;
      delete row._mtime;
    }
    return {
      rows: answer,
      notes: [],
      millis: 38 + (answer.length % 7) * 3,
      engine_millis: 22 + (answer.length % 5) * 2,
      arms: ["keyword", "image", "meaning"],
      mode: lens,
    };
  }

  function highlight(line, words) {
    const text = String(line);
    const lower = text.toLowerCase();
    let first = -1;
    let length = 0;
    for (const word of words) {
      const at = lower.indexOf(word);
      if (at >= 0 && (first < 0 || at < first)) {
        first = at;
        length = word.length;
      }
    }
    if (first < 0) return [{ text, hit: false }];
    const parts = [];
    if (first > 0) parts.push({ text: text.slice(0, first), hit: false });
    parts.push({ text: text.slice(first, first + length), hit: true });
    if (first + length < text.length) {
      parts.push({ text: text.slice(first + length), hit: false });
    }
    return parts;
  }

  function marksFor(file, words) {
    const marks = [];
    (file.lines ?? []).forEach((line, index) => {
      const lower = line.toLowerCase();
      if (!words.some((word) => lower.includes(word))) return;
      const box = lineBox(file, index);
      marks.push({ x0: box.x0, y0: box.y0, x1: box.x1, y1: box.y1 });
    });
    return marks;
  }

  // The lines' places on the drawn art. The same numbers draw the bars and the
  // amber marks, so a box always covers the line it claims.
  function lineBox(file, index) {
    const page = file.art === "page" || file.art === "receipt";
    const top = page ? 0.2 + index * 0.105 : 0.32 + index * 0.11;
    const widths = [0.58, 0.44, 0.68, 0.36];
    const left = page ? 0.09 : 0.08;
    return {
      x0: left,
      x1: left + widths[index % widths.length],
      y0: top,
      y1: top + 0.055,
    };
  }

  // ---- preview --------------------------------------------------------------

  function preview(path, query) {
    const file = ALL.find((item) => item.path === path);
    if (!file) throw new Error("no file at that path");
    const words = terms(query);
    const text = [];
    // The engine's image text opens with the file's own name line. The sheet
    // drops it; keeping it here proves the drop works.
    if (file.kind === "image") text.push({ text: `${file.name}\n`, hit: false });
    for (const line of file.lines ?? []) {
      const parts = highlight(line, words);
      parts.forEach((part, position) => {
        const last = position === parts.length - 1;
        text.push({ text: last ? `${part.text}\n` : part.text, hit: part.hit });
      });
    }
    return {
      path: file.path,
      name: file.name,
      kind: file.kind,
      page: file.page ?? null,
      text,
      image_path: file.kind === "image" ? file.path : null,
      marks: file.kind === "image" ? marksFor(file, words) : [],
    };
  }

  // ---- library and status ---------------------------------------------------

  function library(kinds, limit) {
    const wanted = Array.isArray(kinds) && kinds.length ? kinds : null;
    const rows = ALL.filter((file) => !wanted || wanted.includes(file.kind))
      .slice()
      .sort((a, b) => b.mtime - a.mtime)
      .slice(0, Math.min(limit || 400, 400))
      .map((file) => ({
        path: file.path,
        name: file.name,
        folder: file.folder,
        kind: file.kind,
        mtime: file.mtime,
        width: file.width ?? null,
        height: file.height ?? null,
      }));
    const pictures = ALL.filter((file) => file.kind === "image").length;
    return {
      rows,
      pictures,
      documents: ALL.length - pictures,
      indexed: ALL.length,
      folders: roots.length,
    };
  }

  function status() {
    const byKind = (kind) => ALL.filter((file) => file.kind === kind).length;
    return {
      index_path: "C:\\Users\\you\\AppData\\Roaming\\tessera\\index",
      index_mb: 84.7,
      folder_rows: roots.map((root) => ({ path: root.path, count: root.count })),
      version: "0.1.0",
      counts: [
        ["indexed", ALL.length],
        ["skipped", 2],
      ],
      counts_by_kind: [
        ["image", byKind("image")],
        ["pdf", byKind("pdf")],
        ["docx", byKind("docx")],
        ["text", byKind("text")],
      ],
      failures: [],
      skip_reasons: [["too large", 2]],
      models,
      image_vectors: byKind("image"),
      text_vectors: ALL.length,
      ocr_engine: "tesseract 5.3",
      highlight_pending: 6,
    };
  }

  // ---- drawn art ------------------------------------------------------------
  //
  // The browser has no files to show. Each fixture gets a small canvas scene at
  // its own aspect ratio, encoded as JPEG bytes like the engine's thumbnails.

  const thumbnailCache = new Map();
  const previewCache = new Map();

  function canvasFor(width, height, longest) {
    const scale = Math.min(1, longest / Math.max(width, height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(2, Math.round(width * scale));
    canvas.height = Math.max(2, Math.round(height * scale));
    return canvas;
  }

  function bytesFromCanvas(canvas) {
    const base64 = canvas.toDataURL("image/jpeg", 0.82).split(",")[1];
    const binary = atob(base64);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return bytes;
  }

  const PALETTES = {
    samurai: ["#1b1410", "#c2571f", "#f0b56a", "#2a1d15"],
    page: ["#f4f1e9", "#d9d4c7", "#2c2a25", "#8d8778"],
    receipt: ["#f7f4ec", "#e3ded1", "#2f2d28", "#8a8477"],
    bridge: ["#101a2b", "#2f5a7a", "#dba766", "#0a1119"],
    garden: ["#15220f", "#3f6b2a", "#9cc26b", "#0b1408"],
    sky: ["#2a3550", "#9a5d74", "#f0c089", "#1a2237"],
    harbor: ["#0c1a20", "#2a6b72", "#d9c9a3", "#081215"],
    street: ["#0b0d16", "#3d3350", "#e0a75e", "#06070d"],
    field: ["#1a1c12", "#6d6a3a", "#c9bd7c", "#0e0f09"],
  };

  function paintArt(file, canvas, detailed) {
    const ctx = canvas.getContext("2d");
    const { width: w, height: h } = canvas;
    const [dark, mid, light, ink] = PALETTES[file.art] ?? PALETTES.field;
    void ink;

    const sky = ctx.createLinearGradient(0, 0, 0, h);
    sky.addColorStop(0, mid);
    sky.addColorStop(0.62, light);
    sky.addColorStop(1, dark);
    ctx.fillStyle = sky;
    ctx.fillRect(0, 0, w, h);

    switch (file.art) {
      case "page":
      case "receipt":
        paintPage(ctx, w, h, file, dark);
        break;
      case "samurai":
        paintSamurai(ctx, w, h, dark, h);
        break;
      case "bridge":
        paintBridge(ctx, w, h, mid, light, dark, h);
        break;
      case "garden":
        paintGarden(ctx, w, h, dark, mid, light, h);
        break;
      case "sky":
        paintSky(ctx, w, h, dark, light, h);
        break;
      case "harbor":
        paintHarbor(ctx, w, h, dark, mid, light, h);
        break;
      case "street":
        paintStreet(ctx, w, h, dark, h);
        break;
      default:
        paintField(ctx, w, h, dark, mid, light, h);
    }

    if (!detailed) {
      const vignette = ctx.createRadialGradient(w / 2, h / 2, h * 0.2, w / 2, h / 2, h);
      vignette.addColorStop(0, "rgba(0,0,0,0)");
      vignette.addColorStop(1, "rgba(0,0,0,0.34)");
      ctx.fillStyle = vignette;
      ctx.fillRect(0, 0, w, h);
    }
  }

  function paintPage(ctx, w, h, file) {
    ctx.fillStyle = "rgba(250,248,242,0.94)";
    ctx.fillRect(0, 0, w, h);
    ctx.fillStyle = "rgba(120,112,96,0.08)";
    for (let y = 0; y < h; y += Math.max(2, Math.round(h / 120))) {
      ctx.fillRect(0, y, w, 1);
    }
    (file.lines ?? []).forEach((line, index) => {
      const box = lineBox(file, index);
      ctx.fillStyle = index === 0 ? "rgba(30,28,24,0.78)" : "rgba(48,45,38,0.52)";
      ctx.fillRect(box.x0 * w, box.y0 * h, (box.x1 - box.x0) * w, 0.018 * h);
      ctx.fillStyle = "rgba(48,45,38,0.22)";
      ctx.fillRect(box.x0 * w, (box.y0 + 0.028) * h, (box.x1 - box.x0) * w * 0.9, 0.012 * h);
      void line;
    });
  }

  function paintSamurai(ctx, w, h, dark, unit) {
    const sun = ctx.createRadialGradient(w * 0.72, h * 0.3, 0, w * 0.72, h * 0.3, h * 0.5);
    sun.addColorStop(0, "rgba(255,226,170,0.95)");
    sun.addColorStop(0.5, "rgba(226,150,70,0.45)");
    sun.addColorStop(1, "rgba(226,150,70,0)");
    ctx.fillStyle = sun;
    ctx.fillRect(0, 0, w, h);

    ctx.fillStyle = dark;
    ctx.beginPath();
    ctx.moveTo(0, h * 0.78);
    ctx.lineTo(w * 0.22, h * 0.62);
    ctx.lineTo(w * 0.46, h * 0.74);
    ctx.lineTo(w * 0.7, h * 0.6);
    ctx.lineTo(w, h * 0.72);
    ctx.lineTo(w, h);
    ctx.lineTo(0, h);
    ctx.closePath();
    ctx.fill();

    const cx = w * 0.42;
    const base = h * 0.82;
    const bodyH = h * 0.3;
    ctx.fillStyle = "#1a120d";
    ctx.beginPath();
    ctx.moveTo(cx - w * 0.055, base);
    ctx.lineTo(cx - w * 0.038, base - bodyH * 0.72);
    ctx.lineTo(cx + w * 0.038, base - bodyH * 0.72);
    ctx.lineTo(cx + w * 0.062, base);
    ctx.closePath();
    ctx.fill();
    ctx.beginPath();
    ctx.arc(cx, base - bodyH * 0.83, unit * 0.052, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.ellipse(cx, base - bodyH * 0.88, unit * 0.085, unit * 0.022, 0, 0, Math.PI * 2);
    ctx.fill();

    ctx.lineCap = "round";
    ctx.strokeStyle = "#1a120d";
    ctx.lineWidth = Math.max(1.5, unit * 0.008);
    ctx.beginPath();
    ctx.moveTo(cx + w * 0.03, base - bodyH * 0.6);
    ctx.lineTo(cx + w * 0.17, base - bodyH * 1.02);
    ctx.stroke();
    ctx.strokeStyle = "rgba(255,230,190,0.55)";
    ctx.lineWidth = Math.max(1, unit * 0.003);
    ctx.beginPath();
    ctx.moveTo(cx + w * 0.031, base - bodyH * 0.6);
    ctx.lineTo(cx + w * 0.171, base - bodyH * 1.02);
    ctx.stroke();
  }

  function paintBridge(ctx, w, h, mid, light, dark) {
    const water = ctx.createLinearGradient(0, h * 0.62, 0, h);
    water.addColorStop(0, light);
    water.addColorStop(1, dark);
    ctx.fillStyle = water;
    ctx.fillRect(0, h * 0.62, w, h * 0.38);

    ctx.lineCap = "round";
    ctx.strokeStyle = "rgba(20,18,16,0.9)";
    ctx.lineWidth = Math.max(2, h * 0.012);
    ctx.beginPath();
    ctx.moveTo(-w * 0.05, h * 0.72);
    ctx.quadraticCurveTo(w * 0.5, h * 0.42, w * 1.05, h * 0.72);
    ctx.stroke();
    ctx.lineWidth = Math.max(1, h * 0.005);
    ctx.beginPath();
    ctx.moveTo(-w * 0.05, h * 0.6);
    ctx.quadraticCurveTo(w * 0.5, h * 0.3, w * 1.05, h * 0.6);
    ctx.stroke();
    ctx.strokeStyle = "rgba(20,18,16,0.7)";
    ctx.lineWidth = Math.max(1, h * 0.004);
    for (let x = 0.04; x < 1; x += 0.06) {
      const y = 0.6 + 0.12 * Math.sin(Math.PI * x);
      ctx.beginPath();
      ctx.moveTo(x * w, (y - 0.11) * h);
      ctx.lineTo(x * w, y * h);
      ctx.stroke();
    }

    const glow = ctx.createRadialGradient(w * 0.5, h * 0.6, 0, w * 0.5, h * 0.6, w * 0.5);
    glow.addColorStop(0, "rgba(255,232,190,0.5)");
    glow.addColorStop(1, "rgba(255,232,190,0)");
    ctx.fillStyle = glow;
    ctx.fillRect(0, h * 0.5, w, h * 0.5);
    void mid;
  }

  function paintGarden(ctx, w, h, dark, mid, light, unit) {
    ctx.fillStyle = dark;
    ctx.fillRect(0, h * 0.55, w, h * 0.45);
    for (let i = 0; i < 42; i++) {
      const x = ((i * 0.618) % 1) * w;
      const y = h * (0.2 + ((i * 0.37) % 1) * 0.8);
      const r = unit * (0.06 + ((i * 0.13) % 1) * 0.14);
      ctx.fillStyle = i % 3 === 0 ? light : i % 3 === 1 ? mid : dark;
      ctx.globalAlpha = 0.55 + (i % 4) * 0.1;
      ctx.beginPath();
      ctx.ellipse(x, y, r * 1.25, r * 0.7, (i % 7) * 0.4, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    const shaft = ctx.createLinearGradient(w * 0.2, 0, w * 0.8, h);
    shaft.addColorStop(0, "rgba(255,244,200,0.22)");
    shaft.addColorStop(1, "rgba(255,244,200,0)");
    ctx.fillStyle = shaft;
    ctx.fillRect(0, 0, w, h);
  }

  function paintSky(ctx, w, h, dark, light, unit) {
    ctx.fillStyle = dark;
    ctx.fillRect(0, h * 0.72, w, h * 0.28);
    for (let i = 0; i < 14; i++) {
      const cx = ((i * 0.37 + 0.05) % 1) * w;
      const cy = h * (0.12 + ((i * 0.29) % 1) * 0.5);
      const rx = w * (0.08 + ((i * 0.17) % 1) * 0.16);
      ctx.fillStyle = i % 3 === 0 ? light : "rgba(255,246,232,0.6)";
      ctx.beginPath();
      ctx.ellipse(cx, cy, rx, rx * 0.24, 0, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.fillStyle = "rgba(255,214,150,0.85)";
    ctx.beginPath();
    ctx.arc(w * 0.8, h * 0.3, unit * 0.075, 0, Math.PI * 2);
    ctx.fill();
  }

  function paintHarbor(ctx, w, h, dark, mid, light, unit) {
    const water = ctx.createLinearGradient(0, h * 0.55, 0, h);
    water.addColorStop(0, mid);
    water.addColorStop(1, dark);
    ctx.fillStyle = water;
    ctx.fillRect(0, h * 0.55, w, h * 0.45);
    ctx.fillStyle = "rgba(10,16,20,0.9)";
    for (let i = 0; i < 3; i++) {
      const x = w * (0.16 + i * 0.32);
      const y = h * (0.52 + (i % 2) * 0.07);
      const bw = w * 0.16;
      ctx.beginPath();
      ctx.moveTo(x - bw / 2, y);
      ctx.lineTo(x + bw / 2, y);
      ctx.lineTo(x + bw * 0.36, y + h * 0.1);
      ctx.lineTo(x - bw * 0.36, y + h * 0.1);
      ctx.closePath();
      ctx.fill();
      ctx.strokeStyle = "rgba(10,16,20,0.9)";
      ctx.lineWidth = Math.max(1, unit * 0.004);
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x, y - h * 0.2);
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(255,236,200,0.35)";
    for (let i = 0; i < 5; i++) {
      ctx.lineWidth = Math.max(1, unit * 0.004);
      ctx.beginPath();
      ctx.moveTo(w * 0.08, h * (0.66 + i * 0.06));
      ctx.bezierCurveTo(
        w * 0.4,
        h * (0.64 + i * 0.06),
        w * 0.6,
        h * (0.7 + i * 0.06),
        w * 0.92,
        h * (0.66 + i * 0.06),
      );
      ctx.stroke();
    }
    void light;
  }

  function paintStreet(ctx, w, h, dark, unit) {
    ctx.fillStyle = dark;
    ctx.fillRect(0, h * 0.5, w, h * 0.5);
    for (let i = 0; i < 18; i++) {
      const x = ((i * 0.31) % 1) * w;
      const y = h * (0.3 + ((i * 0.19) % 1) * 0.5);
      const r = unit * (0.02 + ((i * 0.11) % 1) * 0.05);
      const glow = ctx.createRadialGradient(x, y, 0, x, y, r * 4);
      glow.addColorStop(0, "rgba(255,196,110,0.8)");
      glow.addColorStop(1, "rgba(255,196,110,0)");
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(x, y, r * 4, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.strokeStyle = "rgba(255,240,210,0.14)";
    ctx.lineWidth = Math.max(1, unit * 0.004);
    for (let x = 0.1; x < 1; x += 0.14) {
      ctx.beginPath();
      ctx.moveTo(w * x, h * 0.62);
      ctx.lineTo(w * x, h);
      ctx.stroke();
    }
  }

  function paintField(ctx, w, h, dark, mid, light, unit) {
    ctx.fillStyle = mid;
    ctx.fillRect(0, h * 0.62, w, h * 0.38);
    ctx.strokeStyle = light;
    ctx.lineWidth = Math.max(1, unit * 0.003);
    for (let i = 0; i < 26; i++) {
      const x = ((i * 0.73) % 1) * w;
      const y = h * (0.6 + ((i * 0.41) % 1) * 0.4);
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.quadraticCurveTo(x + w * 0.01, y - h * 0.06, x + w * 0.03, y - h * 0.09);
      ctx.stroke();
    }
    const haze = ctx.createLinearGradient(0, h * 0.2, 0, h * 0.7);
    haze.addColorStop(0, "rgba(255,246,220,0.2)");
    haze.addColorStop(1, "rgba(255,246,220,0)");
    ctx.fillStyle = haze;
    ctx.fillRect(0, 0, w, h);
    void dark;
  }

  function imageBytes(file, mode) {
    const cache = mode === "preview" ? previewCache : thumbnailCache;
    const known = cache.get(file.path);
    if (known) return known;
    const canvas = canvasFor(file.width, file.height, mode === "preview" ? 1400 : 480);
    paintArt(file, canvas, mode === "preview");
    const bytes = bytesFromCanvas(canvas);
    cache.set(file.path, bytes);
    return bytes;
  }

  // ---- commands -------------------------------------------------------------

  const handlers = {
    async search({ query, mode, limit }) {
      await wait(24);
      return search(query, mode, limit);
    },
    async preview({ path, query }) {
      await wait(18);
      return preview(path, query);
    },
    async preview_image({ path }) {
      await wait(30);
      const file = ALL.find((item) => item.path === path);
      if (!file) throw new Error("no file at that path");
      if (file.broken) throw new Error("image decoding failed");
      return imageBytes(file, "preview");
    },
    async thumbnail({ path }) {
      await wait(12);
      const file = ALL.find((item) => item.path === path);
      if (!file) throw new Error("no file at that path");
      if (file.broken) throw new Error("image decoding failed");
      return imageBytes(file, "thumbnail");
    },
    async library({ kinds, limit }) {
      await wait(8);
      return library(kinds, limit);
    },
    async status() {
      await wait(16);
      return status();
    },
    async watch_status() {
      return { running: true, paused, log: [], summary: null };
    },
    async set_paused({ paused: next }) {
      paused = Boolean(next);
    },
    async pick_folder() {
      await wait(40);
      return "C:\\Users\\you\\Pictures\\Archive";
    },
    async index_folder({ path }) {
      await wait(320);
      if (!roots.some((root) => root.path === path)) {
        roots = roots.concat({ path, count: 0 });
      }
      return `read 0 new files in ${path}.`;
    },
    async remove_folder({ path }) {
      await wait(60);
      const prefix = String(path).toLowerCase();
      ALL = ALL.filter((file) => !file.path.toLowerCase().startsWith(prefix));
      roots = roots.filter((root) => root.path !== path);
    },
    async confirm_remove_folder({ path }) {
      const name = String(path).split(/[\\/]/).pop();
      return window.confirm(
        `Remove “${name}” from the index?\n\nTessera stops reading this folder. Your files are not touched.`,
      );
    },
    async clear_thumbnails() {
      await wait(120);
      const removed = thumbnailCache.size;
      thumbnailCache.clear();
      previewCache.clear();
      return removed;
    },
    async rebuild_index() {
      await wait(180);
      return roots.map((root) => root.path);
    },
    async confirm_rebuild() {
      return window.confirm(
        "Rebuild the index?\n\nSearches are empty until it finishes. Your files are not touched.",
      );
    },
    async install_models({ sets }) {
      await wait(900);
      for (const set of models) {
        if ((sets ?? []).includes(set.name)) {
          set.complete = true;
          set.missing = [];
        }
      }
      return ["models installed", "checksums verified"];
    },
    async autostart_enabled() {
      return autostart;
    },
    async set_autostart({ enabled }) {
      autostart = Boolean(enabled);
    },
    async reveal() {
      await wait(40);
    },
  };

  const invoke = (command, args) => {
    const handler = handlers[command];
    if (!handler) {
      return Promise.reject(new Error(`the ${command} command is unavailable here`));
    }
    return Promise.resolve(handler(args ?? {}));
  };

  const listeners = new Map();

  window.__TAURI__ = {
    core: { invoke },
    event: {
      listen(event, handler) {
        const set = listeners.get(event) ?? new Set();
        set.add(handler);
        listeners.set(event, set);
        return Promise.resolve(() => set.delete(handler));
      },
    },
  };

  // The Rust shell will emit this when the window is shown (D9). The browser
  // has no window manager, so the review flow calls the hook by hand.
  window.__tesseraShow = () => {
    for (const handler of listeners.get("tessera-show") ?? []) {
      handler({ event: "tessera-show", payload: null });
    }
  };
})();
