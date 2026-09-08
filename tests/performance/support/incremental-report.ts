import fs from "node:fs";
import path from "node:path";

import type { LspIncrementalBudget } from "./incremental-metrics.ts";

export type IncrementalReportData = {
  status: string;
  fixture: string;
  fixtureRevision: string;
  corpus: { vueFiles: number; vueAndTypeScriptFiles: number; baselineDiagnostics: number };
  budget: LspIncrementalBudget & { scale: number };
  timingsMs: Record<string, number>;
  sampledPeakRssKiB: number;
  sampledPeakProcessTreeRssKiB: number;
  sampledPeakProcessTreeSize: number;
};

export function writeIncrementalArtifacts(
  outputDir: string,
  title: string,
  data: IncrementalReportData,
): void {
  fs.mkdirSync(outputDir, { recursive: true });
  fs.writeFileSync(path.join(outputDir, "metrics.json"), `${JSON.stringify(data, null, 2)}\n`);
  fs.writeFileSync(path.join(outputDir, "summary.md"), renderMarkdown(title, data));
}

function renderMarkdown(title: string, data: IncrementalReportData): string {
  const scale = data.budget.scale;
  const asMiB = (kib: number) => (kib > 0 ? `${(kib / 1024).toFixed(1)} MiB` : "unavailable");
  const lines = [
    `## ${title}`,
    "",
    `Status: **${data.status}**. Fixture: \`${data.fixture}@${data.fixtureRevision}\`.`,
    "",
    `Corpus: ${data.corpus.vueFiles} Vue files; ${data.corpus.vueAndTypeScriptFiles} Vue/TS files; ${data.corpus.baselineDiagnostics} baseline diagnostics.`,
    "",
    "| Stage | Time | Budget |",
    "| --- | ---: | ---: |",
  ];
  for (const [stage, milliseconds] of Object.entries(data.timingsMs)) {
    const budgetMs = data.budget.laneBudgetsMs[stage];
    const budgetCell = budgetMs == null ? "-" : `${budgetMs * scale} ms`;
    lines.push(`| ${stage} | ${milliseconds.toFixed(1)} ms | ${budgetCell} |`);
  }
  lines.push(
    "",
    `Sampled peak LSP RSS: ${asMiB(data.sampledPeakRssKiB)} ` +
      `(budget ${data.budget.maxPeakRssMiB * scale} MiB).`,
    `Sampled peak process-tree RSS: ${asMiB(data.sampledPeakProcessTreeRssKiB)} ` +
      `(budget ${data.budget.maxPeakProcessTreeRssMiB * scale} MiB, max ` +
      `${data.budget.maxProcessTreeSize} processes; observed ${data.sampledPeakProcessTreeSize}).`,
    "",
    `Latency, RSS, process-tree, and hang ceilings are enforced at scale ${scale} from the registry ` +
      "lspIncrementalBudget block. The clean/broken/repaired diagnostics, completion, hover, " +
      "and dependency propagation are hard assertions.",
    "",
  );
  return lines.join("\n");
}
