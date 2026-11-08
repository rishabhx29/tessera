// The words lens answers with the line itself: a crop of the picture around
// the matched text, the OCR line that holds it, and the file the line came
// from. A picture with no usable geometry still answers with its text.

import {
  appendSegments,
  el,
  kindGlyph,
  loadThumb,
  markLayer,
  thumbUrl,
} from "./dom.js";
import { cropAround, cropStyle } from "../lib/proof.js";

export function renderEvidence(
  container,
  rows,
  { startIndex = 0, reasonOf = null } = {},
) {
  container.classList.add("evidence");
  container.replaceChildren();
  rows.forEach((row, position) => {
    container.append(
      evidenceCard(row, {
        index: startIndex + position,
        reason: reasonOf ? reasonOf(row) : "",
      }),
    );
  });
}

function evidenceCard(row, { index, reason = "" }) {
  const button = el("button", "evidence-card");
  button.type = "button";
  button.id = `result-${index}`;
  button.dataset.index = String(index);
  button.dataset.path = row.path;
  button.setAttribute("role", "option");
  button.setAttribute("aria-selected", "false");
  button.tabIndex = -1;
  button.title = row.path;

  const frame = el("span", "evidence-frame");
  frame.dataset.state = "idle";

  const fallback = el("span", "evidence-fallback");
  fallback.append(kindGlyph("image"), el("span", "", "no preview"));
  frame.append(fallback);

  const crop = cropAround(row.marks?.[0], {
    width: row.width,
    height: row.height,
  });
  const picture = el("span", crop ? "evidence-image" : "evidence-image whole");
  if (crop) Object.assign(picture.style, cropStyle(crop));

  const img = document.createElement("img");
  img.alt = "";
  img.decoding = "async";
  img.draggable = false;
  img.addEventListener("load", () => {
    if (frame.isConnected) frame.dataset.state = "ready";
  });
  img.addEventListener("error", () => {
    img.remove();
    frame.dataset.state = "failed";
  });
  picture.append(img);
  if (crop && row.marks?.length) picture.append(markLayer(row.marks));
  frame.append(picture);

  const known = thumbUrl(row.path);
  if (known) img.src = known;
  else {
    loadThumb(row.path).then((url) => {
      if (url && img.isConnected) img.src = url;
    });
  }

  const plate = el("span", "evidence-plate");
  const head = el("span", "evidence-head");
  head.append(el("span", "evidence-name", row.name));
  if (row.page) head.append(el("span", "evidence-page mono", `p.${row.page}`));
  plate.append(head);

  const meta = el("span", "evidence-meta");
  if (row.folder) meta.append(el("span", "evidence-folder", row.folder));
  if (reason) meta.append(el("span", "evidence-reason", reason));
  plate.append(meta);

  if (row.snippet?.length) {
    const text = el("span", "evidence-text");
    appendSegments(text, row.snippet);
    plate.append(text);
  }

  button.append(frame, plate);
  return button;
}
