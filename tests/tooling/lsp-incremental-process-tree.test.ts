import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { test } from "node:test";

import { ChurnMetrics } from "../performance/support/churn-metrics.ts";
import type { LspChurnBudget } from "../performance/support/churn-metrics.ts";
import {
  budgetRegistryPath,
  IncrementalMetrics,
} from "../performance/support/incremental-metrics.ts";
import type { LspIncrementalBudget } from "../performance/support/incremental-metrics.ts";
import { processTreeRss } from "../performance/support/process-metrics.ts";

const suite = { id: "misskey-lsp-incremental", title: "Misskey LSP Incremental Oracle" };
const churnSuite = { id: "misskey-lsp-churn", title: "Misskey LSP Churn Stress Oracle" };
const absentPid = Number.MAX_SAFE_INTEGER;

const delay = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

function mutableBudget(metrics: IncrementalMetrics): LspIncrementalBudget {
  return (metrics as unknown as { budget: LspIncrementalBudget }).budget;
}

function mutableChurnBudget(metrics: ChurnMetrics): LspChurnBudget {
  return (metrics as unknown as { budget: LspChurnBudget }).budget;
}

test("process-tree RSS accounting sees the probing process and a child", async () => {
  const tree = processTreeRss(process.pid);
  if (process.platform === "win32") {
    assert.equal(tree, null);
    return;
  }
  const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
    stdio: "ignore",
  });
  try {
    await once(child, "spawn");
    let sampled = tree;
    for (let tries = 0; tries < 50; tries += 1) {
      sampled = processTreeRss(process.pid);
      if (sampled != null && sampled.processes >= 2) break;
      await delay(20);
    }
    assert.ok(sampled != null, "ps-based tree sampling must work on POSIX hosts");
    assert.ok(sampled.processes >= 2, "the root and child processes must both be counted");
    assert.ok(sampled.totalKiB > 0, "a live process tree has resident memory");
  } finally {
    child.kill();
    await Promise.race([once(child, "exit"), delay(1_000)]);
  }
});

test("process-tree RSS accounting returns null when the root process is absent", () => {
  const tree = processTreeRss(absentPid);
  if (process.platform === "win32") {
    assert.equal(tree, null);
    return;
  }
  assert.equal(tree, null);
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

test("incremental settlement fails when process-tree sampling misses the server pid", (t) => {
  if (process.platform === "win32") {
    t.skip("process-tree RSS accounting is Linux/POSIX only");
    return;
  }
  const metrics = new IncrementalMetrics(absentPid, suite);
  const budget = mutableBudget(metrics);
  budget.maxPeakRssMiB = 1_000_000;
  budget.maxPeakProcessTreeRssMiB = 1_000_000;
  budget.maxProcessTreeSize = 1_000_000;
  budget.laneBudgetsMs = {};
  metrics.sampleRss("missing-server");
  assert.throws(
    () => metrics.assertBudgetsSettled(),
    (error: Error) => {
      assert.match(error.message, /process-tree RSS sampling did not observe LSP server process/);
      assert.match(error.message, /cannot enforce maxPeakProcessTreeRssMiB or maxProcessTreeSize/);
      return true;
    },
  );
});

test("churn settlement fails when process-tree sampling misses the server pid", (t) => {
  if (process.platform === "win32") {
    t.skip("process-tree RSS accounting is Linux/POSIX only");
    return;
  }
  const metrics = new ChurnMetrics(absentPid, churnSuite);
  const budget = mutableChurnBudget(metrics);
  budget.cyclesPerPhase = 0;
  budget.maxPeakRssMiB = 1_000_000;
  budget.maxPeakProcessTreeRssMiB = 1_000_000;
  budget.maxProcessTreeSize = 1_000_000;
  budget.budgetsMs = {};
  metrics.sampleRss("missing-server");
  assert.throws(
    () => metrics.assertSettled(),
    (error: Error) => {
      assert.match(error.message, /process-tree RSS sampling did not observe LSP server process/);
      assert.match(error.message, /cannot enforce maxPeakProcessTreeRssMiB or maxProcessTreeSize/);
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
