import assert from "node:assert/strict";
import { test } from "node:test";

import { createSurface } from "../../tools/benchmarks/scripts/compare-tools-report.mjs";

const CHECK_ENGINE_CLASSES = {
  "golar-default": "tsgo-native",
  "golar-typecheck": "tsgo-native",
  "verter-tsc": "tsgo-native",
  "vue-tsc": "typescript-js",
  "vize-check-1t": "tsgo-native",
  "vize-check-max": "tsgo-native",
};

const CHECK_VARIANTS = [
  { id: "vue-tsc", label: "vue-tsc", medianMs: 8000, throughput: "62.5 files/s", runs: [8000] },
  {
    id: "verter-tsc",
    label: "verter-tsc",
    medianMs: 1000,
    throughput: "500 files/s",
    runs: [1000],
  },
  {
    id: "golar-typecheck",
    label: "Golar typecheck",
    medianMs: 1500,
    throughput: "333 files/s",
    runs: [1500],
  },
  {
    id: "golar-default",
    label: "Golar (lint+check)",
    medianMs: 2500,
    throughput: "200 files/s",
    runs: [2500],
  },
  {
    id: "vize-check-1t",
    label: "Vize check (1T)",
    medianMs: 2000,
    throughput: "250 files/s",
    runs: [2000],
  },
  {
    id: "vize-check-max",
    label: "Vize check (max)",
    medianMs: 500,
    throughput: "1k files/s",
    runs: [500],
  },
];

function checkSurface(variants = CHECK_VARIANTS) {
  return {
    id: "check",
    label: "Type check",
    files: 500,
    bytes: 1_000_000,
    baselineId: "vue-tsc",
    vizeSingleId: "vize-check-1t",
    vizeMaxId: "vize-check-max",
    engineClasses: CHECK_ENGINE_CLASSES,
    variants,
  };
}

test("generated type-check benchmark surfaces require every engine-class row", () => {
  for (const id of Object.keys(CHECK_ENGINE_CLASSES)) {
    assert.throws(
      () => createSurface(checkSurface(CHECK_VARIANTS.filter((variant) => variant.id !== id))),
      new RegExp(`compare-tools: check is missing required engine-class variants: ${id}`),
      id,
    );
  }
});

test("complete type-check benchmark surfaces still publish the in-class ratio", () => {
  const surface = createSurface(checkSurface());

  assert.equal(surface.speedupStatus, "in-class");
  assert.equal(surface.speedupBaselineId, "verter-tsc");
  assert.equal(surface.primarySpeedup, 2);
});
