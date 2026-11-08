import test from "node:test";
import assert from "node:assert/strict";

import { cropAround, cropStyle } from "../lib/proof.js";

const near = (actual, expected, epsilon = 1e-6) =>
  assert.ok(
    Math.abs(actual - expected) <= epsilon,
    `${actual} should be within ${epsilon} of ${expected}`,
  );

test("a centered mark gets the card's aspect around it", () => {
  const crop = cropAround(
    { x0: 0.4, y0: 0.4, x1: 0.6, y1: 0.6 },
    { width: 1500, height: 1000, cardAspect: 1.5 },
  );
  near(crop.x0, 0.26);
  near(crop.y0, 0.26);
  near(crop.x1, 0.74);
  near(crop.y1, 0.74);
  near(((crop.x1 - crop.x0) * 1500) / ((crop.y1 - crop.y0) * 1000), 1.5);
});

test("a mark at the left edge keeps the frame full and inside the picture", () => {
  const crop = cropAround(
    { x0: 0, y0: 0.45, x1: 0.1, y1: 0.55 },
    { width: 1000, height: 1000, cardAspect: 1.5 },
  );
  near(crop.x0, 0);
  assert.ok(crop.y0 >= 0);
  assert.ok(crop.y1 <= 1);
  assert.ok(crop.x1 > 0.1);
});

test("a wide picture keeps the crop inside the picture", () => {
  const crop = cropAround(
    { x0: 0.45, y0: 0.45, x1: 0.55, y1: 0.55 },
    { width: 3000, height: 1000, cardAspect: 1.5 },
  );
  assert.ok(crop.x1 - crop.x0 <= 1);
  assert.ok(crop.y1 - crop.y0 <= 1);
  near(((crop.x1 - crop.x0) * 3000) / ((crop.y1 - crop.y0) * 1000), 1.5);
});

test("a mark is always inside the crop", () => {
  const mark = { x0: 0.72, y0: 0.18, x1: 0.9, y1: 0.34 };
  const crop = cropAround(mark, { width: 1600, height: 900, cardAspect: 1.5 });
  assert.ok(crop.x0 <= mark.x0);
  assert.ok(crop.y0 <= mark.y0);
  assert.ok(crop.x1 >= mark.x1);
  assert.ok(crop.y1 >= mark.y1);
});

test("no usable mark means no crop", () => {
  assert.equal(cropAround(null, { width: 1000, height: 1000 }), null);
  assert.equal(cropAround({}, { width: 1000, height: 1000 }), null);
  assert.equal(
    cropAround({ x0: 0.1, y0: 0.1, x1: 0.2, y1: 0.2 }, { width: 0, height: 0 }),
    null,
  );
});

test("cropStyle maps a region onto the full picture box", () => {
  const style = cropStyle({ x0: 0.25, y0: 0.2, x1: 0.75, y1: 0.8 });
  near(parseFloat(style.width), 200);
  near(parseFloat(style.height), 166.66666666666669, 1e-9);
  near(parseFloat(style.left), -50);
  near(parseFloat(style.top), -33.33333333333333, 1e-9);
});
