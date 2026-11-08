import assert from "node:assert/strict";
import test from "node:test";

import { layoutRows } from "../lib/justified.js";

const close = (actual, expected, slack = 0.01) =>
  assert.ok(
    Math.abs(actual - expected) <= slack,
    `expected ${actual} to be within ${slack} of ${expected}`,
  );

test("a full row fills the width exactly", () => {
  const items = [{ aspect: 1.5 }, { aspect: 1.5 }, { aspect: 1.5 }, { aspect: 1.5 }];
  const [row] = layoutRows(items, 900, 168, 6);
  const used = row.items.reduce((sum, item) => sum + item.width, 0) + 6 * 3;
  close(used, 900);
  close(row.height, (900 - 18) / 6);
});

test("a last row is not stretched", () => {
  const [row] = layoutRows([{ aspect: 1 }], 900, 168, 6);
  close(row.height, 168);
  close(row.items[0].width, 168);
});

test("gaps are counted", () => {
  const items = [{ aspect: 1 }, { aspect: 1 }, { aspect: 1 }];
  const [row] = layoutRows(items, 520, 168, 10);
  close(row.height, (520 - 20) / 3);
  const used = row.items.reduce((sum, item) => sum + item.width, 0) + 10 * 2;
  close(used, 520);
});

test("an empty wall has no rows", () => {
  assert.deepEqual(layoutRows([], 900), []);
});

test("a missing aspect uses four by three", () => {
  const [row] = layoutRows([{ aspect: null }], 200, 168, 6);
  close(row.items[0].width, 200);
  close(row.height, 150);
});
