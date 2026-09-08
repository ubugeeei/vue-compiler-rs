import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { ChurnMetrics, loadLspChurnBudget } from "../performance/support/churn-metrics.ts";
import {
  budgetRegistryPath,
  budgetScaleVariable,
  IncrementalMetrics,
  loadLspIncrementalBudget,
  resolveBudgetScale,
} from "../performance/support/incremental-metrics.ts";
import type { LspIncrementalBudget } from "../performance/support/incremental-metrics.ts";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

function readBudgetOwners(): Array<{ id: string; budget: LspIncrementalBudget }> {
  const registry = JSON.parse(fs.readFileSync(path.join(repoRoot, budgetRegistryPath), "utf8")) as {
    projects: Array<{ id: string; lspIncrementalBudget?: LspIncrementalBudget }>;
  };
  return registry.projects
    .filter((project) => project.lspIncrementalBudget != null)
    .map((project) => ({ id: project.id, budget: project.lspIncrementalBudget! }));
}

const misskeySuite = { id: "misskey-lsp-incremental", title: "Misskey LSP Incremental Oracle" };

function withBudgetScale<T>(scale: string, run: () => T): T {
  const previous = process.env[budgetScaleVariable];
  process.env[budgetScaleVariable] = scale;
  try {
    return run();
  } finally {
    if (previous == null) delete process.env[budgetScaleVariable];
    else process.env[budgetScaleVariable] = previous;
  }
}

test("suite budgets resolve from the registry and unknown suites are rejected", () => {
  const misskey = loadLspIncrementalBudget("misskey-lsp-incremental");
  assert.equal(misskey.fixtureId, "misskey");
  assert.ok(misskey.budget.laneBudgetsMs.coldOpen > 0);

  const vben = loadLspIncrementalBudget("vben-lsp-incremental");
  assert.equal(vben.fixtureId, "vue-vben-admin");
  assert.ok(vben.budget.laneBudgetsMs.sharedBrokenSecondApp > 0);

  assert.throws(
    () => loadLspIncrementalBudget("unbudgeted-suite"),
    /exactly one lspIncrementalBudget block/,
  );
});

test("the budget scale escape hatch parses strictly and defaults to 1", () => {
  assert.equal(resolveBudgetScale({}), 1);
  assert.equal(resolveBudgetScale({ [budgetScaleVariable]: "" }), 1);
  assert.equal(resolveBudgetScale({ [budgetScaleVariable]: "2.5" }), 2.5);
  for (const invalid of ["0", "-1", "banana", "Infinity"]) {
    assert.throws(
      () => resolveBudgetScale({ [budgetScaleVariable]: invalid }),
      new RegExp(budgetScaleVariable),
      `${invalid} should be rejected`,
    );
  }
});

test("a lane over its latency budget fails with rebaseline instructions", async () => {
  // Scale 0.01 shrinks the checked-in coldOpen ceiling to tens of
  // milliseconds while the hard timeout stays hundreds of milliseconds
  // away, so a slightly slower operation completes and trips the latency
  // gate rather than the hang gate.
  const scale = 0.01;
  const { budget } = loadLspIncrementalBudget(misskeySuite.id);
  const scaledBudgetMs = budget.laneBudgetsMs.coldOpen * scale;
  const sleepMs = scaledBudgetMs + 70;
  assert.ok(
    sleepMs < budget.laneHardTimeoutMs * scale - 200,
    "the lane must finish safely before the scaled hard timeout",
  );
  const metrics = withBudgetScale(
    String(scale),
    () => new IncrementalMetrics(process.pid, misskeySuite),
  );
  await assert.rejects(
    metrics.measure("coldOpen", () => new Promise((resolve) => setTimeout(resolve, sleepMs))),
    (error: Error) => {
      assert.match(error.message, /lane "coldOpen" took/);
      assert.ok(error.message.includes(`over its ${scaledBudgetMs} ms budget`));
      assert.match(error.message, /Fixture: misskey\./);
      assert.match(error.message, /raising laneBudgetsMs\.coldOpen/);
      assert.ok(error.message.includes(budgetRegistryPath));
      assert.ok(error.message.includes(`${budgetScaleVariable}=2`));
      return true;
    },
  );
});

test("a lane without a registry budget entry cannot be measured", async () => {
  const metrics = new IncrementalMetrics(process.pid, misskeySuite);
  await assert.rejects(
    metrics.measure("unbudgetedLane", async () => {}),
    /Lane "unbudgetedLane" has no laneBudgetsMs entry/,
  );
});

test("a hung lane trips the scaled hard timeout instead of the suite timeout", async () => {
  // Scale 0.001 turns the 60s hard timeout into 60ms; the lane operation
  // stays pending for far longer than that.
  const metrics = withBudgetScale("0.001", () => new IncrementalMetrics(process.pid, misskeySuite));
  const startedAt = Date.now();
  await assert.rejects(
    metrics.measure("coldOpen", () => new Promise((resolve) => setTimeout(resolve, 5_000).unref())),
    (error: Error) => {
      assert.match(error.message, /lane "coldOpen" is still not settled after its 60 ms hard/);
      assert.match(error.message, /likely hung/);
      assert.match(error.message, /raising laneHardTimeoutMs/);
      return true;
    },
  );
  assert.ok(Date.now() - startedAt < 4_000, "hard timeout must fire well before the operation");
});

test("peak RSS over its ceiling fails settlement with rebaseline instructions", () => {
  // Scale 0.001 shrinks the 256 MiB ceiling to ~262 KiB, which any live
  // Node.js process exceeds.
  const metrics = withBudgetScale("0.001", () => new IncrementalMetrics(process.pid, misskeySuite));
  metrics.sampleRss("probe");
  assert.throws(
    () => metrics.assertBudgetsSettled(),
    (error: Error) => {
      assert.match(error.message, /sampled peak LSP RSS .* MiB is over its 0\.256 MiB budget/);
      assert.match(error.message, /raising maxPeakRssMiB/);
      assert.ok(error.message.includes(budgetRegistryPath));
      return true;
    },
  );
});

test("a budgeted lane that never ran fails settlement", () => {
  // A huge scale keeps the RSS ceiling out of the way so the completeness
  // check is what trips.
  const metrics = withBudgetScale("1000", () => new IncrementalMetrics(process.pid, misskeySuite));
  metrics.sampleRss("probe");
  assert.throws(
    () => metrics.assertBudgetsSettled(),
    /budgeted lane "initialize" was never measured/,
  );
});

const churnSuite = { id: "misskey-lsp-churn", title: "Misskey LSP Churn Stress Oracle" };

test("the churn budget resolves from the registry and unknown suites are rejected", () => {
  const churn = loadLspChurnBudget(churnSuite.id);
  assert.equal(churn.fixtureId, "misskey");
  assert.ok(churn.budget.cyclesPerPhase > 0);
  assert.deepEqual(Object.keys(churn.budget.budgetsMs).sort(), [
    "cancellationConverge",
    "closeClear",
    "coldOpen",
    "cycle",
    "fileLifecycle",
    "initialize",
    "phaseFence",
  ]);
  assert.ok(churn.budget.hardTimeoutMs < 300_000, "hard timeout must beat the suite timeout");
  assert.ok(churn.budget.maxPeakRssMiB <= churn.budget.maxPeakProcessTreeRssMiB);
  assert.throws(() => loadLspChurnBudget("unbudgeted-suite"), /exactly one lspChurnBudget block/);
});

test("a churn lane over its budget or without a budget fails with instructions", async () => {
  // Scale 0.001 shrinks the 9000 ms cycle ceiling to 9 ms while the hard
  // timeout stays at 60 ms, so a 30 ms lane completes, misses its latency
  // budget, and cannot trip the hang gate instead.
  const scale = 0.001;
  const { budget } = loadLspChurnBudget(churnSuite.id);
  const metrics = withBudgetScale(String(scale), () => new ChurnMetrics(process.pid, churnSuite));
  await assert.rejects(
    metrics.measure("cycle", () => new Promise((resolve) => setTimeout(resolve, 30))),
    (error: Error) => {
      assert.match(error.message, /lane "cycle" \(occurrence 1\) took/);
      assert.ok(error.message.includes(`over its ${budget.budgetsMs.cycle * scale} ms budget`));
      assert.match(error.message, /raising budgetsMs\.cycle/);
      assert.ok(error.message.includes(budgetRegistryPath));
      return true;
    },
  );
  await assert.rejects(
    metrics.measure("unbudgetedLane", async () => {}),
    /Lane "unbudgetedLane" has no budgetsMs entry/,
  );
});

test("a hung churn lane trips the scaled hard timeout quickly", async () => {
  const metrics = withBudgetScale("0.001", () => new ChurnMetrics(process.pid, churnSuite));
  const startedAt = Date.now();
  await assert.rejects(
    metrics.measure("cycle", () => new Promise((resolve) => setTimeout(resolve, 5_000).unref())),
    /lane "cycle" is still not settled after its 60 ms hard/,
  );
  assert.ok(Date.now() - startedAt < 4_000, "hard timeout must fire well before the operation");
});

test("churn settlement gates cycle count, RSS ceilings, and latency decay", async () => {
  const { budget } = loadLspChurnBudget(churnSuite.id);
  const short = new ChurnMetrics(process.pid, churnSuite);
  await short.measure("cycle", async () => {});
  assert.throws(
    () => short.assertSettled(),
    new RegExp(`measured 1 cycles, expected ${budget.cyclesPerPhase * 2}`),
  );

  const leaky = withBudgetScale("0.0001", () => new ChurnMetrics(process.pid, churnSuite));
  leaky.sampleRss("probe");
  assert.throws(
    () => leaky.assertSettled(),
    (error: Error) => {
      assert.match(error.message, /sampled peak server RSS .* MiB is over its/);
      assert.match(error.message, /raising maxPeakRssMiB/);
      return true;
    },
  );

  const degraded = new ChurnMetrics(process.pid, churnSuite);
  for (let cycle = 0; cycle < budget.cyclesPerPhase * 2; cycle += 1) {
    const slow = cycle >= budget.cyclesPerPhase * 2 - 4;
    await degraded.measure("cycle", () =>
      slow ? new Promise((resolve) => setTimeout(resolve, 40)) : Promise.resolve(),
    );
  }
  assert.throws(
    () => degraded.assertSettled(),
    (error: Error) => {
      assert.match(error.message, /median cycle latency degraded from/);
      assert.match(error.message, /raising maxTailToHeadLatencyRatio/);
      return true;
    },
  );
});

test("incremental LSP suites carry complete enforced budget blocks", () => {
  const owners = readBudgetOwners();

  assert.deepEqual(
    owners
      .map(({ id, budget }) => ({ id, suite: budget.suite }))
      .sort((a, b) => a.id.localeCompare(b.id)),
    [
      { id: "misskey", suite: "misskey-lsp-incremental" },
      { id: "vue-vben-admin", suite: "vben-lsp-incremental" },
    ],
  );

  const sharedLanes = [
    "initialize",
    "coldOpen",
    "completion",
    "hover",
    "warmNoop",
    "leafBroken",
    "leafRepaired",
    "sharedBroken",
    "sharedRepaired",
  ];
  const expectedLanes: Record<string, string[]> = {
    "misskey-lsp-incremental": sharedLanes,
    "vben-lsp-incremental": [
      ...sharedLanes,
      "coldOpenSecondApp",
      "sharedBrokenSecondApp",
      "sharedRepairedSecondApp",
      "warmNoopSecondApp",
      "cancellationConverge",
    ],
  };

  for (const { id, budget } of owners) {
    assert.deepEqual(
      Object.keys(budget.laneBudgetsMs).sort(),
      [...expectedLanes[budget.suite]].sort(),
      `${id} should budget exactly the lanes its suite measures`,
    );
    assert.ok(
      Number.isSafeInteger(budget.laneHardTimeoutMs) && budget.laneHardTimeoutMs > 0,
      `${id} laneHardTimeoutMs must be a positive integer`,
    );
    assert.ok(
      budget.laneHardTimeoutMs < 300_000,
      `${id} hard timeout must fire before the 300s suite timeout`,
    );
    assert.ok(
      Number.isSafeInteger(budget.maxPeakRssMiB) && budget.maxPeakRssMiB > 0,
      `${id} maxPeakRssMiB must be a positive integer`,
    );
    assert.ok(
      Number.isSafeInteger(budget.maxPeakProcessTreeRssMiB) && budget.maxPeakProcessTreeRssMiB > 0,
      `${id} maxPeakProcessTreeRssMiB must be a positive integer`,
    );
    assert.ok(
      budget.maxPeakRssMiB <= budget.maxPeakProcessTreeRssMiB,
      `${id} process-tree RSS budget must cover the server RSS budget`,
    );
    assert.ok(
      Number.isSafeInteger(budget.maxProcessTreeSize) && budget.maxProcessTreeSize > 0,
      `${id} maxProcessTreeSize must be a positive integer`,
    );
    for (const [lane, budgetMs] of Object.entries(budget.laneBudgetsMs)) {
      assert.ok(
        Number.isSafeInteger(budgetMs) && budgetMs > 0,
        `${id} ${lane} budget must be a positive integer`,
      );
      assert.ok(
        budgetMs <= budget.laneHardTimeoutMs,
        `${id} ${lane} budget must fit under the hard timeout`,
      );
    }
  }
});
