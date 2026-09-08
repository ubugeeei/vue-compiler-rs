import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { inspect } from "node:util";
import { performance } from "node:perf_hooks";

import { repoRoot } from "../../_helpers/realworld-patch.ts";
import { writeChurnArtifacts } from "./churn-report.ts";
import {
  budgetRegistryPath,
  budgetScaleVariable,
  incrementalMetricsDir,
  resolveBudgetScale,
} from "./incremental-metrics.ts";
import { gitHead, processRssKiB, processTreeRss } from "./process-metrics.ts";

/** `id` is the metrics directory under `target/vize-tests/metrics/`; `title` heads the summary. */
export type ChurnSuite = { id: string; title: string };

/**
 * Enforced ceilings for one churn-stress LSP suite, checked into the registry.
 *
 * Unlike `lspIncrementalBudget`, lanes here may be measured repeatedly (the
 * `cycle` lane runs `2 * cyclesPerPhase` times), the RSS gates also cover the
 * whole spawned process tree (`vize lsp` plus its Corsa/tsgo worker) so a
 * leaked worker session fails the suite, and a tail-to-head latency ratio
 * bounds responsiveness decay across the churn run.
 */
export type LspChurnBudget = {
  suite: string;
  cyclesPerPhase: number;
  hardTimeoutMs: number;
  budgetsMs: Record<string, number>;
  maxPeakRssMiB: number;
  maxPeakProcessTreeRssMiB: number;
  maxProcessTreeSize: number;
  maxTailToHeadLatencyRatio: number;
};

/**
 * Reads the enforced churn ceilings for one suite from the fixture registry.
 * Exactly one project must own the suite, so the gate cannot silently run
 * without checked-in budgets.
 */
export function loadLspChurnBudget(suiteId: string): {
  fixtureId: string;
  budget: LspChurnBudget;
} {
  const registry = JSON.parse(fs.readFileSync(path.join(repoRoot, budgetRegistryPath), "utf8")) as {
    projects: Array<{ id: string; lspChurnBudget?: LspChurnBudget }>;
  };
  const owners = registry.projects.filter((project) => project.lspChurnBudget?.suite === suiteId);
  if (owners.length !== 1) {
    throw new Error(
      `Expected exactly one lspChurnBudget block with suite "${suiteId}" in ` +
        `${budgetRegistryPath}, found ${owners.length}`,
    );
  }
  const budget = owners[0].lspChurnBudget!;
  for (const field of [
    "cyclesPerPhase",
    "hardTimeoutMs",
    "maxPeakRssMiB",
    "maxPeakProcessTreeRssMiB",
    "maxProcessTreeSize",
    "maxTailToHeadLatencyRatio",
  ] as const) {
    assertPositiveInteger(budget[field], suiteId, field);
  }
  const lanes = Object.entries(budget.budgetsMs ?? {});
  if (lanes.length === 0) throw new Error(`${suiteId} budgetsMs must contain every measured lane`);
  for (const [lane, value] of lanes) {
    assertPositiveInteger(value, suiteId, `budgetsMs.${lane}`);
    if (value > budget.hardTimeoutMs) {
      throw new Error(`${suiteId} budgetsMs.${lane} must not exceed hardTimeoutMs`);
    }
  }
  return { fixtureId: owners[0].id, budget };
}

type RssSample = { label: string; serverKiB: number; treeKiB: number; treeProcesses: number };

type ChurnContext = {
  fixture: string;
  revision: string;
  vueFiles: number;
  sourceFiles: number;
  publishes: number;
};

export class ChurnMetrics {
  readonly budget: LspChurnBudget;
  readonly scale: number;
  private readonly timingsMs: Record<string, number[]> = {};
  private readonly rssSamples: RssSample[] = [];
  private readonly processId: number;
  private readonly suite: ChurnSuite;
  private readonly fixtureId: string;

  constructor(processId: number, suite: ChurnSuite) {
    this.processId = processId;
    this.suite = suite;
    const { fixtureId, budget } = loadLspChurnBudget(suite.id);
    this.fixtureId = fixtureId;
    this.budget = budget;
    this.scale = resolveBudgetScale();
  }

  /**
   * Runs one budgeted lane occurrence. Unlike the incremental harness a lane
   * may run many times (`cycle` runs once per churn cycle); every occurrence
   * must finish under `budgetsMs[lane]` and under the shared hard timeout.
   */
  async measure<T>(lane: string, operation: () => Promise<T>): Promise<T> {
    const budgetMs = this.budget.budgetsMs[lane];
    if (budgetMs == null) {
      throw new Error(
        `Lane "${lane}" has no budgetsMs entry for suite ${this.suite.id}; add its ceiling to ` +
          `the "${this.fixtureId}" lspChurnBudget block in ${budgetRegistryPath}`,
      );
    }
    const startedAt = performance.now();
    let result: T;
    try {
      result = await this.raceHardTimeout(lane, operation());
    } finally {
      (this.timingsMs[lane] ??= []).push(performance.now() - startedAt);
    }
    const runs = this.timingsMs[lane];
    const elapsedMs = runs[runs.length - 1];
    if (elapsedMs > budgetMs * this.scale) {
      throw this.budgetViolation(
        `lane "${lane}" (occurrence ${runs.length}) took ${elapsedMs.toFixed(1)} ms, over its ` +
          `${budgetMs * this.scale} ms budget${this.scaleSuffix()}.`,
        `budgetsMs.${lane}`,
      );
    }
    return result;
  }

  sampleRss(label: string): void {
    const tree = processTreeRss(this.processId);
    this.rssSamples.push({
      label,
      serverKiB: processRssKiB(this.processId) ?? 0,
      treeKiB: tree?.totalKiB ?? 0,
      treeProcesses: tree?.processes ?? 0,
    });
  }

  /**
   * End-of-run gates: peak server RSS, peak process-tree RSS, process-tree
   * size (a leaked worker session adds a process), tail-to-head cycle latency
   * decay, the checked-in cycle count, and that every budgeted lane ran.
   */
  assertSettled(): void {
    const { serverKiB, treeKiB, processes: peakProcesses } = this.peakSamples();
    this.assertRss("server", serverKiB, this.budget.maxPeakRssMiB, "maxPeakRssMiB");
    const treeField = "maxPeakProcessTreeRssMiB";
    this.assertRss("process-tree", treeKiB, this.budget[treeField], treeField);
    if (peakProcesses > this.budget.maxProcessTreeSize) {
      throw this.budgetViolation(
        `the server process tree grew to ${peakProcesses} processes, over its ` +
          `${this.budget.maxProcessTreeSize} process ceiling; a worker session is likely leaking.`,
        "maxProcessTreeSize",
      );
    }
    const cycles = this.timingsMs.cycle ?? [];
    const expectedCycles = this.budget.cyclesPerPhase * 2;
    if (cycles.length !== expectedCycles) {
      throw new Error(
        `${this.suite.title}: measured ${cycles.length} cycles, expected ${expectedCycles} ` +
          `(2 x cyclesPerPhase from the "${this.fixtureId}" lspChurnBudget block)`,
      );
    }
    const decile = Math.max(1, Math.floor(cycles.length / 10));
    const headMs = median(cycles.slice(0, decile));
    const tailMs = median(cycles.slice(-decile));
    if (tailMs > headMs * this.budget.maxTailToHeadLatencyRatio) {
      throw this.budgetViolation(
        `median cycle latency degraded from ${headMs.toFixed(1)} ms (first ${decile} cycles) to ` +
          `${tailMs.toFixed(1)} ms (last ${decile} cycles), over the ` +
          `${this.budget.maxTailToHeadLatencyRatio}x ceiling; churn is degrading responsiveness.`,
        "maxTailToHeadLatencyRatio",
      );
    }
    for (const lane of Object.keys(this.budget.budgetsMs)) {
      if (this.timingsMs[lane] == null) {
        throw new Error(
          `${this.suite.title}: budgeted lane "${lane}" was never measured; fix the suite or ` +
            `remove the lane from the "${this.fixtureId}" lspChurnBudget block in ` +
            `${budgetRegistryPath}`,
        );
      }
    }
    this.assertProcessTreeObserved(peakProcesses);
  }

  write(context: ChurnContext, failure?: unknown): void {
    let settlementFailure: Error | null = null;
    if (failure == null) {
      try {
        this.assertSettled();
      } catch (error) {
        settlementFailure = error as Error;
        failure = error;
      }
    }
    const cycles = this.timingsMs.cycle ?? [];
    const data = {
      schemaVersion: 1,
      status: failure == null ? "passed" : "failed",
      failure:
        failure instanceof Error ? failure.message : failure == null ? null : inspect(failure),
      commit: gitHead(),
      fixture: context.fixture,
      fixtureRevision: context.revision,
      corpus: { vueFiles: context.vueFiles, vueAndTypeScriptFiles: context.sourceFiles },
      runtime: {
        platform: process.platform,
        architecture: process.arch,
        node: process.version,
        cpuCount: os.cpus().length,
        cpuModel: os.cpus()[0]?.model ?? "unknown",
      },
      budget: { scale: this.scale, ...this.budget },
      publishes: context.publishes,
      cycleStats:
        cycles.length === 0
          ? null
          : {
              cycles: cycles.length,
              minMs: Math.min(...cycles),
              medianMs: median(cycles),
              maxMs: Math.max(...cycles),
            },
      timingsMs: this.timingsMs,
      rssSamples: this.rssSamples,
      peakRssKiB: this.peakSamples().serverKiB,
      peakProcessTreeRssKiB: this.peakSamples().treeKiB,
      note:
        "Latency, hang, RSS, process-tree, and responsiveness-decay ceilings are enforced from " +
        "the registry lspChurnBudget block; publish determinism, ordering, and convergence are " +
        "hard assertions in the suite.",
    };
    writeChurnArtifacts(incrementalMetricsDir(this.suite.id), this.suite.title, data);
    if (settlementFailure != null) throw settlementFailure;
  }

  private peakSamples(): { serverKiB: number; treeKiB: number; processes: number } {
    return {
      serverKiB: Math.max(0, ...this.rssSamples.map((s) => s.serverKiB)),
      treeKiB: Math.max(0, ...this.rssSamples.map((s) => s.treeKiB)),
      processes: Math.max(0, ...this.rssSamples.map((s) => s.treeProcesses)),
    };
  }

  private assertProcessTreeObserved(peakProcesses: number): void {
    if (process.platform === "win32") return;
    if (this.rssSamples.length > 0 && peakProcesses > 0) return;
    throw new Error(
      `${this.suite.title}: process-tree RSS sampling did not observe LSP server process ` +
        `${this.processId}; cannot enforce maxPeakProcessTreeRssMiB or maxProcessTreeSize.`,
    );
  }

  private assertRss(name: string, peakKiB: number, ceilingMiB: number, field: string): void {
    if (peakKiB > ceilingMiB * 1024 * this.scale) {
      throw this.budgetViolation(
        `sampled peak ${name} RSS ${(peakKiB / 1024).toFixed(1)} MiB is over its ` +
          `${ceilingMiB * this.scale} MiB budget${this.scaleSuffix()}.`,
        field,
      );
    }
  }

  private async raceHardTimeout<T>(lane: string, operation: Promise<T>): Promise<T> {
    const hardTimeoutMs = this.budget.hardTimeoutMs * this.scale;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const hardTimeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        reject(
          this.budgetViolation(
            `lane "${lane}" is still not settled after its ${hardTimeoutMs} ms hard ` +
              `timeout${this.scaleSuffix()}; the server is likely hung.`,
            "hardTimeoutMs",
          ),
        );
      }, hardTimeoutMs);
    });
    try {
      return await Promise.race([operation, hardTimeout]);
    } finally {
      clearTimeout(timer);
      // The losing operation can still reject later; swallow that so a budget
      // failure is not followed by an unhandled rejection.
      void operation.catch(() => {});
    }
  }

  private budgetViolation(detail: string, registryField: string): Error {
    return new Error(
      `${this.suite.title}: ${detail} Fixture: ${this.fixtureId}. If this is intentional, ` +
        `rebaseline by raising ${registryField} in the "${this.fixtureId}" lspChurnBudget ` +
        `block of ${budgetRegistryPath}. On a slow local machine, rerun with ` +
        `${budgetScaleVariable}=2 (or higher) to scale every ceiling; CI runs at scale 1.`,
    );
  }

  private scaleSuffix(): string {
    return this.scale === 1 ? "" : ` (${budgetScaleVariable}=${this.scale})`;
  }
}

export function median(values: number[]): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)] ?? 0;
}

function assertPositiveInteger(value: number, suiteId: string, field: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${suiteId} ${field} must be a positive safe integer, got ${inspect(value)}`);
  }
}
