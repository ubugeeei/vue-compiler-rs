import assert from "node:assert/strict";
import { test } from "node:test";

import { buildFairnessNotes } from "../../tools/benchmarks/scripts/benchmark-notes.mjs";
import {
  createSurface,
  rankWithinEngineClasses,
} from "../../tools/benchmarks/scripts/compare-tools-report.mjs";
import { renderMarkdown } from "../../tools/benchmarks/scripts/compare-tools.mjs";

const CHECK_VARIANTS = [
  { id: "vue-tsc", label: "vue-tsc", medianMs: 8000, throughput: "62.5 files/s", runs: [8000] },
  {
    id: "verter-tsc",
    label: "verter-tsc",
    medianMs: 1000,
    throughput: "500.0 files/s",
    runs: [1000],
  },
  {
    id: "golar-typecheck",
    label: "Golar typecheck",
    medianMs: 1500,
    throughput: "333.3 files/s",
    runs: [1500],
  },
  {
    id: "golar-default",
    label: "Golar (lint+check)",
    medianMs: 2500,
    throughput: "200.0 files/s",
    runs: [2500],
  },
  {
    id: "vize-check-1t",
    label: "Vize check (1T)",
    medianMs: 2000,
    throughput: "250.0 files/s",
    runs: [2000],
  },
  {
    id: "vize-check-max",
    label: "Vize check (max)",
    medianMs: 500,
    throughput: "1.0k files/s",
    runs: [500],
  },
];

const CHECK_ENGINE_CLASSES = {
  "golar-default": "tsgo-native",
  "golar-typecheck": "tsgo-native",
  "verter-tsc": "tsgo-native",
  "vue-tsc": "typescript-js",
  "vize-check-1t": "tsgo-native",
  "vize-check-max": "tsgo-native",
};

function checkSurfaceInput(overrides: Record<string, unknown> = {}) {
  return {
    id: "check",
    label: "Type check",
    files: 500,
    bytes: 1_000_000,
    baselineId: "vue-tsc",
    vizeSingleId: "vize-check-1t",
    vizeMaxId: "vize-check-max",
    engineClasses: CHECK_ENGINE_CLASSES,
    variants: CHECK_VARIANTS,
    ...overrides,
  };
}

test("a cross-engine surface publishes the ratio against its same-engine incumbent", () => {
  const surface = createSurface(checkSurfaceInput());

  assert.deepEqual(surface, {
    id: "check",
    label: "Type check",
    files: 500,
    bytes: 1_000_000,
    baselineId: "vue-tsc",
    vizeSingleId: "vize-check-1t",
    vizeMaxId: "vize-check-max",
    engineClasses: CHECK_ENGINE_CLASSES,
    variants: CHECK_VARIANTS,
    // verter-tsc, not vue-tsc: 1000ms / 500ms of the one native engine.
    primarySpeedup: 2,
    speedupBaselineId: "verter-tsc",
    speedupStatus: "in-class",
    engineClassRanking: [
      {
        engineClass: "typescript-js",
        label: "JS TypeScript engine (tsc)",
        rows: [{ id: "vue-tsc", label: "vue-tsc", medianMs: 8000, relativeToFastest: 1 }],
      },
      {
        engineClass: "tsgo-native",
        label: "native TypeScript engine (tsgo)",
        rows: [
          {
            id: "vize-check-max",
            label: "Vize check (max)",
            medianMs: 500,
            relativeToFastest: 1,
          },
          {
            id: "verter-tsc",
            label: "verter-tsc",
            medianMs: 1000,
            relativeToFastest: 2,
          },
          {
            id: "golar-typecheck",
            label: "Golar typecheck",
            medianMs: 1500,
            relativeToFastest: 3,
          },
          {
            id: "vize-check-1t",
            label: "Vize check (1T)",
            medianMs: 2000,
            relativeToFastest: 4,
          },
          {
            id: "golar-default",
            label: "Golar (lint+check)",
            medianMs: 2500,
            relativeToFastest: 5,
          },
        ],
      },
    ],
  });
});

test("a cross-engine surface with no same-engine incumbent still publishes nothing", () => {
  const nativeIncumbents = ["verter-tsc", "golar-typecheck", "golar-default"];
  const surface = createSurface(
    checkSurfaceInput({
      id: "custom-check",
      variants: CHECK_VARIANTS.filter((variant) => !nativeIncumbents.includes(variant.id)),
    }),
  );

  assert.equal(surface.primarySpeedup, null);
  assert.equal(surface.speedupBaselineId, null);
  assert.equal(surface.speedupStatus, "cross-engine");
});

test("a same-engine surface keeps its ranked primary speedup", () => {
  const surface = createSurface({
    id: "fmt",
    label: "Format",
    files: 500,
    bytes: 1_000_000,
    baselineId: "prettier-cli",
    vizeSingleId: "vize-fmt-1t",
    vizeMaxId: "vize-fmt-max",
    variants: [
      { id: "prettier-cli", label: "prettier", medianMs: 4000, throughput: "n/a", runs: [4000] },
      { id: "vize-fmt-1t", label: "Vize fmt (1T)", medianMs: 800, throughput: "n/a", runs: [800] },
      {
        id: "vize-fmt-max",
        label: "Vize fmt (max)",
        medianMs: 200,
        throughput: "n/a",
        runs: [200],
      },
    ],
  });

  assert.equal(surface.primarySpeedup, 20);
  assert.equal(surface.speedupBaselineId, "prettier-cli");
  assert.equal(surface.speedupStatus, "ranked");
  assert.equal(surface.engineClassRanking, null);
});

test("a surface without a measurable Vize lane reports no speedup at all", () => {
  const surface = createSurface({
    id: "custom-check",
    label: "Type check",
    files: 1,
    bytes: 1,
    baselineId: "vue-tsc",
    vizeSingleId: null,
    vizeMaxId: "vize-check-max",
    variants: [
      { id: "vue-tsc", label: "vue-tsc", medianMs: 10, throughput: "n/a", runs: [10] },
      {
        id: "vize-check-max",
        label: "Vize check (max)",
        medianMs: 0,
        throughput: "n/a",
        runs: [0],
      },
    ],
  });

  assert.equal(surface.primarySpeedup, null);
  assert.equal(surface.speedupStatus, "unavailable");
});

test("every variant of a class-declaring surface must carry an engine class", () => {
  assert.throws(
    () =>
      rankWithinEngineClasses({
        id: "check",
        engineClasses: { "vue-tsc": "typescript-js" },
        variants: CHECK_VARIANTS,
      }),
    {
      name: "Error",
      message: "compare-tools: variant verter-tsc of surface check has no engine class",
    },
  );
});

test("the type-check summary ranks engine classes and rates the in-class one", () => {
  const surface = createSurface(checkSurfaceInput());
  const markdown = renderMarkdown({
    schemaVersion: 1,
    kind: "tool-comparison",
    generatedAt: "2026-06-01T00:00:00.000Z",
    commit: { sha: "0123456789abcdef", ref: "main", repository: "o/r", runUrl: "" },
    runner: {
      label: "local",
      blacksmithMaxSpec: "",
      cpuCount: 8,
      cpuModel: "test cpu",
      platform: "linux",
      arch: "x64",
      osRelease: "6.0.0",
      node: "v24.0.0",
    },
    versions: {
      vize: "vize 0.303.0",
      tsgo: "7.0.0-dev",
      vueTsc: "3.2.0",
      verterTsc: "verter-tsc 0.0.1-beta.3",
      golar: "golar 0.1.10",
      typescript: "5.9.0",
      vue: "3.6.0",
      eslint: "9.0.0",
      prettier: "3.4.0",
      node: "v24.0.0",
    },
    binaries: {
      vize: "d".repeat(64),
      tsgo: "e".repeat(64),
      vueTsc: null,
      verterTsc: "f".repeat(64),
      golar: "g".repeat(64),
    },
    backend: {
      engine: "tsgo-native",
      corsaPath: "/repo/node_modules/.bin/tsgo",
      corsaVersion: "7.0.0-dev",
      ready: true,
      reason: null,
    },
    input: {
      dir: "/tmp/bench",
      fileCount: 500,
      totalBytes: 1_000_000,
      checkFileCount: 500,
      viteFileCount: 0,
      nuxtFileCount: 0,
      museaFileCount: 0,
      largeBlocks: 0,
      largeSfcBytes: 0,
    },
    settings: { runs: 1, warmups: 1, tasks: ["check"] },
    commands: { workflowDispatch: "dispatch", generate: "generate", benchmark: "benchmark" },
    fairness: ["only note"],
    surfaces: [surface],
  });

  assert.deepEqual(markdown.split("\n"), [
    "## Tool Benchmark",
    "",
    "Measured: 2026-06-01T00:00:00.000Z",
    "Commit: `0123456789ab`",
    "Runner: `local` (8 logical CPU, test cpu)",
    "Input: 500 generated SFC files (976.6 KB). Median of 1 measured run(s) after 1 warmup run(s).",
    "Versions: vize `vize 0.303.0` · tsgo `7.0.0-dev` · vue-tsc `3.2.0` (typescript `5.9.0`) · verter-tsc `verter-tsc 0.0.1-beta.3` · Golar `golar 0.1.10` · vue `3.6.0` · eslint `9.0.0` · prettier `3.4.0` · node `v24.0.0`",
    `Binaries (sha256): vize \`${"d".repeat(64)}\` tsgo \`${"e".repeat(64)}\` vueTsc n/a verterTsc \`${"f".repeat(64)}\` golar \`${"g".repeat(64)}\``,
    "Backend: native TypeScript engine ready at `/repo/node_modules/.bin/tsgo`. Planted-diagnostic gating for the type-check rows lives in tools/benchmarks/scripts/check-gate.mjs (.github/workflows/check-bench.yml).",
    "",
    "| Surface | Files | Existing tool | Existing median | Vize 1T | Vize max | Speedup |",
    "| --- | ---: | --- | ---: | ---: | ---: | ---: |",
    "| Type check | 500 | verter-tsc | 1.00s | 2.00s | 500.0ms | 2.0x |",
    "",
    "#### Type check — engine classes ranked separately",
    "",
    "| Engine class | Row | Median | Relative to fastest in class |",
    "| --- | --- | ---: | ---: |",
    "| JS TypeScript engine (tsc) | vue-tsc | 8.00s | 1.00x |",
    "| native TypeScript engine (tsgo) | Vize check (max) | 500.0ms | 1.00x |",
    "| native TypeScript engine (tsgo) | verter-tsc | 1.00s | 2.00x |",
    "| native TypeScript engine (tsgo) | Golar typecheck | 1.50s | 3.00x |",
    "| native TypeScript engine (tsgo) | Vize check (1T) | 2.00s | 4.00x |",
    "| native TypeScript engine (tsgo) | Golar (lint+check) | 2.50s | 5.00x |",
    "",
    "The Type check ratio compares Vize with verter-tsc, the incumbent that runs the same native tsgo engine, so it is the Vue layer alone. vue-tsc is listed above as a same-run reference timing and never as a ratio: it drives the JavaScript TypeScript compiler, so a single number against it would credit TypeScript's Go rewrite to the Vue layer.",
    "",
    "Fairness notes:",
    "- only note",
    "",
    "Commands:",
    "",
    "```sh",
    "dispatch",
    "generate",
    "benchmark",
    "```",
    "",
    "<details>",
    "<summary>Variant details and raw run times</summary>",
    "",
    "### Type check",
    "",
    "| Variant | Median | Throughput | Raw measured runs |",
    "| --- | ---: | ---: | --- |",
    "| vue-tsc | 8.00s | 62.5 files/s | 8.00s |",
    "| verter-tsc | 1.00s | 500.0 files/s | 1.00s |",
    "| Golar typecheck | 1.50s | 333.3 files/s | 1.50s |",
    "| Golar (lint+check) | 2.50s | 200.0 files/s | 2.50s |",
    "| Vize check (1T) | 2.00s | 250.0 files/s | 2.00s |",
    "| Vize check (max) | 500.0ms | 1.0k files/s | 500.0ms |",
    "",
    "</details>",
    "",
    "",
  ]);
});

test("the fairness notes name the incumbent the published ratio is measured against", () => {
  assert.deepEqual(
    buildFairnessNotes(500).filter((note) => note.startsWith("Type-check rows")),
    [
      "Type-check rows span two TypeScript engines: vue-tsc runs the JavaScript compiler while Vize check runs native tsgo (Corsa). Their ratio is never published — it would credit TypeScript's Go rewrite to the Vue layer. The published type-check speedup is measured against verter-tsc instead, the incumbent Vue type checker that drives the same native tsgo binary, so it is the Vue layer alone; vue-tsc stays in the table as a same-run reference timing ranked inside its own engine class, and a run where no same-engine incumbent resolves publishes no ratio at all. tools/benchmarks/scripts/check-gate.mjs publishes the same per-engine-class split with planted-diagnostic gating.",
    ],
  );
});
