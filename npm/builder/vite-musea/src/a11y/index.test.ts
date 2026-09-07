import assert from "node:assert/strict";
import { createRequire } from "node:module";
import test from "node:test";

import { resolveAxeSource } from "./index.ts";

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
