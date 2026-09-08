import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { inspect } from "node:util";
import { performance } from "node:perf_hooks";

import { repoRoot } from "../../_helpers/realworld-patch.ts";
import { writeIncrementalArtifacts } from "./incremental-report.ts";
import { gitHead, processRssKiB, processTreeRss } from "./process-metrics.ts";

export type IncrementalSuite = {
  /** Metrics directory name under `target/vize-tests/metrics/`. */
  id: string;
  /** Markdown summary heading, e.g. `Misskey LSP Incremental Oracle`. */
  title: string;
};

/** Enforced ceilings for one incremental LSP suite, checked into the registry. */
export type LspIncrementalBudget = {
  suite: string;
  laneHardTimeoutMs: number;
  maxPeakRssMiB: number;
  maxPeakProcessTreeRssMiB: number;
  maxProcessTreeSize: number;
  laneBudgetsMs: Record<string, number>;
};

export const budgetRegistryPath = "tests/_fixtures/vue-ecosystem-fixtures.json";
export const budgetScaleVariable = "VIZE_PERF_BUDGET_SCALE";

export function incrementalMetricsDir(suiteId: string): string {
  return path.join(repoRoot, "target/vize-tests/metrics", suiteId);
}

/**
 * Reads the enforced latency, hang, and RSS ceilings for one incremental LSP
 * suite from the fixture registry. Exactly one project must own the suite, so
 * a new suite cannot run report-only by accident.
 */
export function loadLspIncrementalBudget(suiteId: string): {
  fixtureId: string;
  budget: LspIncrementalBudget;
} {
  const registry = JSON.parse(fs.readFileSync(path.join(repoRoot, budgetRegistryPath), "utf8")) as {
    projects: Array<{ id: string; lspIncrementalBudget?: LspIncrementalBudget }>;
  };
  const owners = registry.projects.filter(
    (project) => project.lspIncrementalBudget?.suite === suiteId,
  );
  if (owners.length !== 1) {
    throw new Error(
      `Expected exactly one lspIncrementalBudget block with suite "${suiteId}" in ` +
        `${budgetRegistryPath}, found ${owners.length}`,
    );
  }
  const budget = owners[0].lspIncrementalBudget!;
  assertPositiveInteger(budget.laneHardTimeoutMs, suiteId, "laneHardTimeoutMs");
  assertPositiveInteger(budget.maxPeakRssMiB, suiteId, "maxPeakRssMiB");
  assertPositiveInteger(budget.maxPeakProcessTreeRssMiB, suiteId, "maxPeakProcessTreeRssMiB");
  assertPositiveInteger(budget.maxProcessTreeSize, suiteId, "maxProcessTreeSize");
  if (budget.maxPeakRssMiB > budget.maxPeakProcessTreeRssMiB) {
    throw new Error(`${suiteId} maxPeakRssMiB must not exceed maxPeakProcessTreeRssMiB`);
  }
  const lanes = Object.entries(budget.laneBudgetsMs ?? {});
  if (lanes.length === 0) {
    throw new Error(`${suiteId} laneBudgetsMs must contain every measured lane`);
  }
  for (const [lane, value] of lanes) {
    assertPositiveInteger(value, suiteId, `laneBudgetsMs.${lane}`);
    if (value > budget.laneHardTimeoutMs) {
      throw new Error(`${suiteId} laneBudgetsMs.${lane} must not exceed laneHardTimeoutMs`);
    }
  }
  return { fixtureId: owners[0].id, budget };
}

/**
 * Uniform multiplier for every ceiling. CI runs at scale 1 (the vue-parity
 * step sets no override); slow local machines may loosen the ceilings with
 * e.g. `VIZE_PERF_BUDGET_SCALE=2` without editing the registry.
 */
export function resolveBudgetScale(env: NodeJS.ProcessEnv = process.env): number {
  const raw = env[budgetScaleVariable];
  if (raw == null || raw === "") return 1;
  const scale = Number(raw);
  if (!Number.isFinite(scale) || scale <= 0) {
    throw new Error(
      `${budgetScaleVariable} must be a finite number greater than 0, got ${JSON.stringify(raw)}`,
    );
  }
  return scale;
}

type MetricContext = {
  fixture: string;
  revision: string;
  vueFiles: number;
  sourceFiles: number;
  baselineDiagnostics: number;
};

export class IncrementalMetrics {
  private readonly timingsMs: Record<string, number> = {};
  private readonly rssSamplesKiB: Record<string, number> = {};
  private readonly processTreeSamples: Record<string, { totalKiB: number; processes: number }> = {};
  private readonly processId: number;
  private readonly suite: IncrementalSuite;
  private readonly fixtureId: string;
  private readonly budget: LspIncrementalBudget;
  private readonly scale: number;

  constructor(processId: number, suite: IncrementalSuite) {
    this.processId = processId;
    this.suite = suite;
    const { fixtureId, budget } = loadLspIncrementalBudget(suite.id);
    this.fixtureId = fixtureId;
    this.budget = budget;
    this.scale = resolveBudgetScale();
  }

  async measure<T>(name: string, operation: () => Promise<T>): Promise<T> {
    const budgetMs = this.budget.laneBudgetsMs[name];
    if (budgetMs == null) {
      throw new Error(
        `Lane "${name}" has no laneBudgetsMs entry for suite ${this.suite.id}; add its ceiling ` +
          `to the "${this.fixtureId}" lspIncrementalBudget block in ${budgetRegistryPath}`,
      );
    }
    const startedAt = performance.now();
    let result: T;
    try {
      result = await this.raceLaneHardTimeout(name, operation());
    } finally {
      this.timingsMs[name] = performance.now() - startedAt;
      this.sampleRss(name);
    }
    const elapsedMs = this.timingsMs[name];
    if (elapsedMs > budgetMs * this.scale) {
      throw this.budgetViolation(
        `lane "${name}" took ${elapsedMs.toFixed(1)} ms, over its ${budgetMs * this.scale} ms ` +
          `budget${this.scaleSuffix()}.`,
        `laneBudgetsMs.${name}`,
      );
    }
    return result;
  }

  sampleRss(name: string): void {
    const rss = processRssKiB(this.processId);
    if (rss != null) this.rssSamplesKiB[name] = rss;
    const tree = processTreeRss(this.processId);
    if (tree != null) this.processTreeSamples[name] = tree;
  }

  /**
   * End-of-run gates: the sampled peak RSS must stay under its ceiling and
   * every budgeted lane must actually have run, so a renamed or deleted lane
   * cannot leave a stale ceiling behind. Called by `write` on success; public
   * so tooling tests can exercise it without touching metric artifacts.
   */
  assertBudgetsSettled(): void {
    const peaks = this.peakSamples();
    this.assertRss("LSP", peaks.serverKiB, this.budget.maxPeakRssMiB, "maxPeakRssMiB");
    this.assertRss(
      "LSP process-tree",
      peaks.treeKiB,
      this.budget.maxPeakProcessTreeRssMiB,
      "maxPeakProcessTreeRssMiB",
    );
    if (peaks.processes > this.budget.maxProcessTreeSize) {
      throw this.budgetViolation(
        `the LSP process tree grew to ${peaks.processes} processes, over its ` +
          `${this.budget.maxProcessTreeSize} process ceiling; a worker session is likely leaking.`,
        "maxProcessTreeSize",
      );
    }
    for (const lane of Object.keys(this.budget.laneBudgetsMs)) {
      if (this.timingsMs[lane] == null) {
        throw new Error(
          `${this.suite.title}: budgeted lane "${lane}" was never measured; fix the suite or ` +
            `remove the lane from the "${this.fixtureId}" lspIncrementalBudget block in ` +
            `${budgetRegistryPath}`,
        );
      }
    }
  }

  write(context: MetricContext, failure?: unknown): void {
    let budgetFailure: Error | null = null;
    if (failure == null) {
      try {
        this.assertBudgetsSettled();
      } catch (error) {
        budgetFailure = error as Error;
        failure = error;
      }
    }
    const outputDir = incrementalMetricsDir(this.suite.id);
    fs.mkdirSync(outputDir, { recursive: true });
    const data = {
      schemaVersion: 2,
      status: failure == null ? "passed" : "failed",
      failure:
        failure instanceof Error ? failure.message : failure == null ? null : inspect(failure),
      commit: gitHead(),
      fixture: context.fixture,
      fixtureRevision: context.revision,
      corpus: {
        vueFiles: context.vueFiles,
        vueAndTypeScriptFiles: context.sourceFiles,
        baselineDiagnostics: context.baselineDiagnostics,
      },
      runtime: {
        platform: process.platform,
        architecture: process.arch,
        node: process.version,
        cpuCount: os.cpus().length,
        cpuModel: os.cpus()[0]?.model ?? "unknown",
      },
      budget: { scale: this.scale, ...this.budget },
      timingsMs: this.timingsMs,
      rssSamplesKiB: this.rssSamplesKiB,
      processTreeSamples: this.processTreeSamples,
      sampledPeakRssKiB: this.sampledPeakRssKiB(),
      sampledPeakProcessTreeRssKiB: this.peakSamples().treeKiB,
      sampledPeakProcessTreeSize: this.peakSamples().processes,
      note:
        "Latency, RSS, process-tree, and hang ceilings are enforced from the registry " +
        "lspIncrementalBudget " +
        "block; diagnostic, completion, hover, and repair oracles are gated.",
    };
    writeIncrementalArtifacts(outputDir, this.suite.title, data);
    if (budgetFailure != null) throw budgetFailure;
  }

  private sampledPeakRssKiB(): number {
    // RSS sampling is unavailable on Windows, where the peak stays 0 and the
    // ceiling is effectively latency-only; CI enforcement runs on Linux.
    return Math.max(0, ...Object.values(this.rssSamplesKiB));
  }

  private peakSamples(): { serverKiB: number; treeKiB: number; processes: number } {
    return {
      serverKiB: this.sampledPeakRssKiB(),
      treeKiB: Math.max(
        0,
        ...Object.values(this.processTreeSamples).map((sample) => sample.totalKiB),
      ),
      processes: Math.max(
        0,
        ...Object.values(this.processTreeSamples).map((sample) => sample.processes),
      ),
    };
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

  /**
   * Fails a hung lane after its hard timeout instead of waiting for the
   * 120s diagnostics timeout or the 300s suite timeout.
   */
  private async raceLaneHardTimeout<T>(name: string, operation: Promise<T>): Promise<T> {
    const hardTimeoutMs = this.budget.laneHardTimeoutMs * this.scale;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const hardTimeout = new Promise<never>((_, reject) => {
      timer = setTimeout(() => {
        reject(
          this.budgetViolation(
            `lane "${name}" is still not settled after its ${hardTimeoutMs} ms hard ` +
              `timeout${this.scaleSuffix()}; the server is likely hung.`,
            "laneHardTimeoutMs",
          ),
        );
      }, hardTimeoutMs);
    });
    try {
      return await Promise.race([operation, hardTimeout]);
    } finally {
      clearTimeout(timer);
      // The losing operation can still reject later (for example through the
      // session transport timeout once the server is shut down); swallow that
      // so a budget failure is not followed by an unhandled rejection.
      void operation.catch(() => {});
    }
  }

  private budgetViolation(detail: string, registryField: string): Error {
    return new Error(
      `${this.suite.title}: ${detail} Fixture: ${this.fixtureId}. If this is intentional, ` +
        `rebaseline by raising ${registryField} in the "${this.fixtureId}" lspIncrementalBudget ` +
        `block of ${budgetRegistryPath}. On a slow local machine, rerun with ` +
        `${budgetScaleVariable}=2 (or higher) to scale every ceiling; CI runs at scale 1.`,
    );
  }

  private scaleSuffix(): string {
    return this.scale === 1 ? "" : ` (${budgetScaleVariable}=${this.scale})`;
  }
}

export function countFiles(
  root: string,
  extensions: ReadonlySet<string>,
  ignoreDirectories?: ReadonlySet<string>,
): number {
  let count = 0;
  for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
    const filePath = path.join(root, entry.name);
    if (entry.isDirectory()) {
      if (ignoreDirectories?.has(entry.name)) continue;
      count += countFiles(filePath, extensions, ignoreDirectories);
    } else if (extensions.has(path.extname(entry.name))) {
      count += 1;
    }
  }
  return count;
}

function assertPositiveInteger(value: number, suiteId: string, field: string): void {
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${suiteId} ${field} must be a positive safe integer, got ${inspect(value)}`);
  }
}
