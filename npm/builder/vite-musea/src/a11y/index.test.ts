import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

import type { A11yResult } from "../types/index.ts";
import type { MuseaVrtRunner } from "../vrt.ts";
import {
  MuseaA11yRunner,
  computeA11ySummary,
  generateA11yHtmlReport,
  generateA11yJsonReport,
  resolveAxeSource,
} from "./index.ts";

/**
 * axe-core ships CommonJS, so `(await import("axe-core")).source` is
 * `undefined` under ESM — the named export is not interop-detected. The
 * audit used to inject that `undefined`, which injected nothing and failed
 * one step later as `Cannot read properties of undefined (reading 'run')`.
 *
 * The oracle is axe-core's own CommonJS export, which is what the bundle
 * `axe.run` lives in; the ESM path has to produce exactly that string.
 */
void test("the injectable axe-core bundle survives the ESM interop", async () => {
  const require = createRequire(import.meta.url);
  const expected = (require("axe-core") as { source: string }).source;

  // Guard against a vacuous comparison: an empty oracle would let a broken
  // resolver pass by returning an empty string.
  assert.equal(typeof expected, "string");
  assert.ok(
    expected.length > 10_000,
    `axe-core's bundle looks truncated: ${expected.length} bytes`,
  );

  assert.equal(await resolveAxeSource(), expected);
});

/**
 * The other half of #5890: a variant whose audit never ran was recorded as a
 * synthetic `critical` violation, so a completely broken run reported "79
 * critical violations" and failed `--ci` citing accessibility. Nothing about
 * accessibility had been measured. The failure has to read as a failure.
 */
const errored: A11yResult = {
  artPath: "src/Button.art.vue",
  variantName: "primary",
  violations: [],
  passes: 0,
  incomplete: 0,
  error: "page.evaluate: TypeError: axe is undefined",
};

const audited: A11yResult = {
  artPath: "src/Card.art.vue",
  variantName: "default",
  violations: [],
  passes: 12,
  incomplete: 0,
};

void test("an audit that never ran is not counted as an accessibility violation", () => {
  const summary = computeA11ySummary([errored]);

  assert.equal(summary.erroredVariants, 1);
  assert.equal(summary.totalViolations, 0);
  assert.equal(summary.criticalCount, 0);
  assert.equal(summary.totalVariants, 1);
});

void test("the HTML report neither hides an unaudited variant nor calls it all-clear", () => {
  const html = generateA11yHtmlReport([errored], computeA11ySummary([errored]));

  assert.equal(html.includes("No accessibility violations found"), false);
  assert.equal(html.includes("audit did not run"), true);
  assert.equal(html.includes("page.evaluate: TypeError: axe is undefined"), true);
});

void test("a genuinely clean run still reads as clean", () => {
  const html = generateA11yHtmlReport([audited], computeA11ySummary([audited]));

  assert.equal(computeA11ySummary([audited]).erroredVariants, 0);
  assert.equal(html.includes("No accessibility violations found"), true);
});

void test("the JSON report carries the audit error through to CI", () => {
  const parsed = JSON.parse(generateA11yJsonReport([errored, audited])) as {
    summary: { erroredVariants: number; criticalCount: number };
    results: Array<{ variant: string; error?: string; violations: unknown[] }>;
  };

  assert.equal(parsed.summary.erroredVariants, 1);
  assert.equal(parsed.summary.criticalCount, 0);
  assert.equal(parsed.results[0]?.error, errored.error);
  assert.deepEqual(parsed.results[0]?.violations, []);
  assert.equal(parsed.results[1]?.error, undefined);
});

/**
 * The path the issue actually walked: `runAudits` catches a per-variant
 * failure. It used to turn that into `{ id: "audit-error", impact:
 * "critical" }`, which is how a broken run produced one "critical
 * accessibility violation" per variant.
 */
void test("a failed audit is recorded as an error, not a fabricated violation", async () => {
  const runner = new MuseaA11yRunner();
  const failingVrtRunner = {
    createPage: () => Promise.reject(new Error("browser is gone")),
  } as unknown as MuseaVrtRunner;

  const results = await runner.runAudits(
    [
      {
        path: "src/Button.art.vue",
        metadata: { title: "Button", tags: [], status: "ready" },
        variants: [{ name: "primary", template: "<button />", isDefault: true, skipVrt: false }],
        hasScriptSetup: false,
        hasScript: false,
        styleCount: 0,
      },
    ],
    "http://localhost:5173/__musea__",
    failingVrtRunner,
  );

  assert.equal(results.length, 1);
  assert.deepEqual(results[0]?.violations, []);
  assert.equal(results[0]?.error, "browser is gone");

  const summary = runner.getSummary(results);
  assert.equal(summary.criticalCount, 0);
  assert.equal(summary.totalViolations, 0);
  assert.equal(summary.erroredVariants, 1);
});
