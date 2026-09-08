import assert from "node:assert/strict";
import { test } from "node:test";

import {
  budgetRegistryPath,
  IncrementalMetrics,
} from "../performance/support/incremental-metrics.ts";
import type { LspIncrementalBudget } from "../performance/support/incremental-metrics.ts";
import { processTreeRss } from "../performance/support/process-metrics.ts";

const suite = { id: "misskey-lsp-incremental", title: "Misskey LSP Incremental Oracle" };

function mutableBudget(metrics: IncrementalMetrics): LspIncrementalBudget {
  return (metrics as unknown as { budget: LspIncrementalBudget }).budget;
}

test("process-tree RSS accounting sees the probing process itself", () => {
  const tree = processTreeRss(process.pid);
  if (process.platform === "win32") {
    assert.equal(tree, null);
    return;
  }
  assert.ok(tree != null, "ps-based tree sampling must work on POSIX hosts");
  assert.ok(tree.processes >= 1, "the root process must be part of its own tree");
  assert.ok(tree.totalKiB > 0, "a live process tree has resident memory");
});

test("incremental process-tree RSS failures point at the registry ceiling", (t) => {
  if (process.platform === "win32") {
    t.skip("process-tree RSS accounting is Linux/POSIX only");
    return;
  }
  const metrics = new IncrementalMetrics(process.pid, suite);
  const budget = mutableBudget(metrics);
  budget.maxPeakRssMiB = 1_000_000;
  budget.maxPeakProcessTreeRssMiB = 1;
  metrics.sampleRss("probe");
  assert.throws(
    () => metrics.assertBudgetsSettled(),
    (error: Error) => {
      assert.match(
        error.message,
        /sampled peak LSP process-tree RSS .* MiB is over its 1 MiB budget/,
      );
      assert.match(error.message, /raising maxPeakProcessTreeRssMiB/);
      assert.ok(error.message.includes(budgetRegistryPath));
      return true;
    },
  );
});

test("incremental process count failures catch leaked bridge workers", (t) => {
  if (process.platform === "win32") {
    t.skip("process-tree process counting is Linux/POSIX only");
    return;
  }
  const metrics = new IncrementalMetrics(process.pid, suite);
  const budget = mutableBudget(metrics);
  budget.maxPeakRssMiB = 1_000_000;
  budget.maxPeakProcessTreeRssMiB = 1_000_000;
  budget.maxProcessTreeSize = 0;
  metrics.sampleRss("probe");
  assert.throws(
    () => metrics.assertBudgetsSettled(),
    (error: Error) => {
      assert.match(error.message, /the LSP process tree grew to \d+ processes/);
      assert.match(error.message, /worker session is likely leaking/);
      assert.match(error.message, /raising maxProcessTreeSize/);
      return true;
    },
  );
});
