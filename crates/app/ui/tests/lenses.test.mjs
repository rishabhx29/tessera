import assert from "node:assert/strict";
import test from "node:test";

import { LENSES, lensFor } from "../js/lenses.js";

test("the lens table is the four questions in order", () => {
  assert.deepEqual(
    LENSES.map((lens) => lens.id),
    ["all", "pictures", "words", "documents"],
  );
});

test("every lens is lowercase, named for people, and teaches its field", () => {
  for (const lens of LENSES) {
    assert.equal(lens.label, lens.id);
    assert.equal(lens.label, lens.label.toLowerCase());
    assert.ok(lens.aria.length > 0, `${lens.id} needs an assistive name`);
    assert.ok(lens.placeholder.length > 0, `${lens.id} needs a placeholder`);
  }
  assert.equal(new Set(LENSES.map((lens) => lens.placeholder)).size, LENSES.length);
});

test("an unknown lens is all, never an error", () => {
  assert.equal(lensFor("screenshots").id, "all");
  assert.equal(lensFor(undefined).id, "all");
});
