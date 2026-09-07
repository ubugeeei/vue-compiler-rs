/**
 * CLI command handlers for the Musea VRT tool.
 *
 * Extracted from cli.ts to keep file sizes manageable.
 * Contains the runVrt, runApprove, runClean, and runGenerate command implementations.
 */

import fs from "node:fs";
import path from "node:path";

import { MuseaVrtRunner, generateVrtReport, generateVrtJsonReport } from "../vrt.js";
import type { ExtendedVrtOptions, VrtSummary } from "../vrt.js";
import type { ArtFileInfo } from "../types/index.js";

import type { CliOptions } from "./index.js";

export function hasCiBlockingVrtResult(summary: VrtSummary): boolean {
  return summary.failed > 0 || summary.skipped > 0;
}

export function isArtFileInput(filePath: string): boolean {
  return filePath.endsWith(".art.vue");
}

export function createVrtOptions(options: CliOptions): ExtendedVrtOptions {
  const configured = options.vrt ?? {};
  return {
    ...configured,
    snapshotDir: path.join(options.output, "snapshots"),
    threshold: options.thresholdProvided
      ? options.threshold
      : (configured.threshold ?? options.threshold),
  };
}

export async function runVrt(options: CliOptions, artFiles: ArtFileInfo[]): Promise<void> {
  const totalVariants = artFiles.reduce(
    (sum, art) => sum + art.variants.filter((v) => !v.skipVrt).length,
    0,
  );

  console.log(`  Testing ${totalVariants} variant(s) across ${artFiles.length} art file(s)\n`);

  const runner = new MuseaVrtRunner({
    ...createVrtOptions(options),
    ci: options.ci ? { failOnDiff: true, jsonReport: options.json } : undefined,
  });

  try {
    console.log("  Launching browser...");
    await runner.init();

    console.log("  Running visual regression tests...\n");

    // Run tests
    const results = await runner.runAllTests(artFiles, options.baseUrl);
    const summary = runner.getSummary(results);

    // Print results
    console.log("  Results:");
    console.log("  ---------");
    console.log(`    Passed:  ${summary.passed}`);
    console.log(`    Failed:  ${summary.failed}`);
    console.log(`    New:     ${summary.new}`);
    console.log(`    Skipped: ${summary.skipped}`);
    console.log(`    Total:   ${summary.total}`);
    console.log(`    Duration: ${(summary.duration / 1000).toFixed(2)}s\n`);

    // Run a11y audits if requested
    if (options.a11y) {
      console.log("  Running accessibility audits...\n");
      try {
        const { MuseaA11yRunner } = await import("../a11y/index.js");
        const a11yRunner = new MuseaA11yRunner();
        const a11yResults = await a11yRunner.runAudits(artFiles, options.baseUrl, runner);
        const a11ySummary = a11yRunner.getSummary(a11yResults);

        console.log("  A11y Results:");
        console.log("  -------------");
        console.log(`    Components: ${a11ySummary.totalComponents}`);
        console.log(`    Variants:   ${a11ySummary.totalVariants}`);
        console.log(`    Violations: ${a11ySummary.totalViolations}`);
        console.log(`    Critical:   ${a11ySummary.criticalCount}`);
        console.log(`    Serious:    ${a11ySummary.seriousCount}`);
        console.log(`    Not audited: ${a11ySummary.erroredVariants}\n`);

        // Generate a11y report
        const reportDir = options.output;
        await fs.promises.mkdir(reportDir, { recursive: true });

        if (options.json) {
          const a11yJson = a11yRunner.generateJsonReport(a11yResults);
          const a11yPath = path.join(reportDir, "a11y-report.json");
          await fs.promises.writeFile(a11yPath, a11yJson);
          console.log(`  A11y JSON report: ${a11yPath}\n`);
        } else {
          const a11yHtml = a11yRunner.generateHtmlReport(a11yResults);
          const a11yPath = path.join(reportDir, "a11y-report.html");
          await fs.promises.writeFile(a11yPath, a11yHtml);
          console.log(`  A11y HTML report: ${a11yPath}\n`);
        }

        // CI mode - exit with error on critical/serious violations. A variant
        // whose audit never ran fails too, but under its own message: it is
        // not an accessibility finding, and reporting it as one sends people
        // looking for a component defect that does not exist.
        if (options.ci && a11ySummary.erroredVariants > 0) {
          console.log(
            `  CI mode: ${a11ySummary.erroredVariants} variant(s) could not be audited\n`,
          );
          process.exit(1);
        }
        if (options.ci && (a11ySummary.criticalCount > 0 || a11ySummary.seriousCount > 0)) {
          console.log("  CI mode: Accessibility violations found\n");
          process.exit(1);
        }
      } catch (e) {
        console.warn("  A11y audits skipped:", e instanceof Error ? e.message : String(e));
        console.warn("  Make sure axe-core is installed: npm install axe-core\n");
      }
    }

    // Update baselines if requested
    if (options.update) {
      console.log("  Updating baselines...");
      const updated = await runner.updateBaselines(results);
      console.log(`  Updated ${updated} baseline(s)\n`);
    }

    // Generate VRT report
    const reportDir = options.output;
    await fs.promises.mkdir(reportDir, { recursive: true });

    if (options.json) {
      const jsonReport = generateVrtJsonReport(results, summary);
      const jsonPath = path.join(reportDir, "vrt-report.json");
      await fs.promises.writeFile(jsonPath, jsonReport);
      console.log(`  JSON report: ${jsonPath}\n`);
    } else {
      const htmlReport = generateVrtReport(results, summary);
      const htmlPath = path.join(reportDir, "vrt-report.html");
      await fs.promises.writeFile(htmlPath, htmlReport);
      console.log(`  HTML report: ${htmlPath}\n`);
    }

    // CI mode - exit with error if visual diffs or capture/runtime errors occurred.
    if (options.ci && hasCiBlockingVrtResult(summary)) {
      console.log("  CI mode: Exiting with error due to failures\n");
      process.exit(1);
    }
  } finally {
    await runner.close();
  }
}

export async function runApprove(options: CliOptions, artFiles: ArtFileInfo[]): Promise<void> {
  const runner = new MuseaVrtRunner(createVrtOptions(options));

  try {
    console.log("  Launching browser...");
    await runner.init();

    console.log("  Running tests to find diffs...\n");

    const results = await runner.runAllTests(artFiles, options.baseUrl);
    const failed = results.filter((r) => !r.passed && !r.error);

    if (failed.length === 0) {
      console.log("  No failed tests to approve.\n");
      return;
    }

    const pattern = options.pattern;
    if (pattern) {
      console.log(`  Approving snapshots matching: ${pattern}\n`);
    } else {
      console.log(`  Approving all ${failed.length} failed snapshot(s)...\n`);
    }

    const approved = await runner.approveResults(results, pattern);
    console.log(`  Approved ${approved} snapshot(s)\n`);
  } finally {
    await runner.close();
  }
}

export async function runClean(options: CliOptions, artFiles: ArtFileInfo[]): Promise<void> {
  const runner = new MuseaVrtRunner(createVrtOptions(options));

  console.log("  Scanning for orphaned snapshots...\n");

  const cleaned = await runner.cleanOrphans(artFiles);

  if (cleaned === 0) {
    console.log("  No orphaned snapshots found.\n");
  } else {
    console.log(`\n  Cleaned ${cleaned} orphaned snapshot(s)\n`);
  }
}

export async function runGenerate(options: CliOptions): Promise<void> {
  if (!options.componentPath) {
    console.error("  Error: Missing component path.");
    console.error("  Usage: musea-vrt generate <component.vue>\n");
    process.exit(1);
  }

  const componentPath = path.resolve(options.componentPath);
  if (isArtFileInput(componentPath)) {
    console.error(
      "  Error: musea-vrt generate expects a source .vue component, not a .art.vue file.",
    );
    console.error("  Pass the component file that the art file should wrap.\n");
    process.exit(1);
  }

  // Check file exists
  try {
    await fs.promises.access(componentPath);
  } catch {
    console.error(`  Error: File not found: ${componentPath}\n`);
    process.exit(1);
  }

  console.log(`  Generating art file for: ${path.relative(process.cwd(), componentPath)}\n`);

  try {
    const { writeArtFile } = await import("../autogen/index.js");
    const outputPath = await writeArtFile(componentPath);
    const relOutput = path.relative(process.cwd(), outputPath);

    console.log(`  Generated: ${relOutput}\n`);
  } catch (e) {
    console.error("  Generation failed:", e instanceof Error ? e.message : String(e));
    process.exit(1);
  }
}
