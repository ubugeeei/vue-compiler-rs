/**
 * Accessibility (a11y) testing module for Musea.
 * Uses axe-core for automated accessibility auditing via Playwright.
 */

import type { Page } from "playwright";
import type {
  ArtFileInfo,
  A11yResult,
  A11yViolation,
  A11yOptions,
  ViewportConfig,
} from "../types/index.js";
import type { MuseaVrtRunner } from "../vrt.js";

import { computeA11ySummary, generateA11yHtmlReport, generateA11yJsonReport } from "./report.js";

// Re-export report functions so consumers importing from a11y.js still work
export { computeA11ySummary, generateA11yHtmlReport, generateA11yJsonReport } from "./report.js";

/**
 * A11y audit summary.
 */
export interface A11ySummary {
  totalComponents: number;
  totalVariants: number;
  /** Variants whose audit could not run at all; see `A11yResult.error`. */
  erroredVariants: number;
  totalViolations: number;
  criticalCount: number;
  seriousCount: number;
  moderateCount: number;
  minorCount: number;
}

/**
 * axe-core result shape (subset).
 */
interface AxeResult {
  violations: Array<{
    id: string;
    impact: string;
    description: string;
    helpUrl: string;
    nodes: Array<unknown>;
  }>;
  passes: Array<unknown>;
  incomplete: Array<unknown>;
}

/**
 * Read axe-core's injectable browser bundle.
 *
 * axe-core ships CommonJS (`"main": "axe.js"`, no `exports` map), so under
 * ESM its exports arrive on the namespace's `default` and `source` is not
 * interop-detected as a named export: `(await import("axe-core")).source`
 * is `undefined`. Injecting that injects nothing, leaves `window.axe`
 * undefined, and the audit dies later as `Cannot read properties of
 * undefined (reading 'run')` — a failure that points at the wrong line. So
 * read both shapes and fail here, where the cause is still legible.
 */
export async function resolveAxeSource(): Promise<string> {
  let namespace: { source?: unknown; default?: { source?: unknown } };
  try {
    namespace = await import("axe-core");
  } catch (cause) {
    throw new Error(
      "axe-core is not installed. Install it as a peer dependency: npm install axe-core",
      { cause },
    );
  }

  const source = namespace.source ?? namespace.default?.source;
  if (typeof source !== "string" || source.length === 0) {
    throw new Error(
      "axe-core is installed but exposes no `source` bundle to inject. " +
        "Expected a non-empty string on either the module namespace or its " +
        "default export; check that the installed axe-core version is >= 4.",
    );
  }
  return source;
}

/**
 * A11y runner using axe-core via Playwright.
 */
export class MuseaA11yRunner {
  private options: Required<A11yOptions>;

  constructor(options: A11yOptions = {}) {
    this.options = {
      enabled: options.enabled ?? true,
      includeRules: options.includeRules ?? [],
      excludeRules: options.excludeRules ?? [],
      level: options.level ?? "AA",
    };
  }

  /**
   * Run a11y audits on all art file variants.
   * Reuses VRT runner's browser if available.
   */
  async runAudits(
    artFiles: ArtFileInfo[],
    baseUrl: string,
    vrtRunner?: MuseaVrtRunner,
  ): Promise<A11yResult[]> {
    const results: A11yResult[] = [];
    const defaultViewport: ViewportConfig = { width: 1280, height: 720, name: "desktop" };

    for (const art of artFiles) {
      for (const variant of art.variants) {
        if (variant.skipVrt) continue;

        let page: Page | null = null;
        let context: { page: Page; context: { close(): Promise<void> } } | null = null;

        try {
          if (vrtRunner) {
            context = await vrtRunner.createPage(defaultViewport);
            page = context.page;
          } else {
            // Standalone mode: launch own browser
            const { chromium } = await import("playwright");
            const browser = await chromium.launch({ headless: true });
            const ctx = await browser.newContext({
              viewport: { width: defaultViewport.width, height: defaultViewport.height },
            });
            page = await ctx.newPage();
            context = { page, context: ctx };
          }

          const variantUrl = this.buildVariantUrl(baseUrl, art.path, variant.name);
          await page.goto(variantUrl, { waitUntil: "networkidle" });
          await page.waitForSelector(".musea-variant", { timeout: 10000 });
          await page.waitForTimeout(200);

          const result = await this.auditPage(page, art.path, variant.name);
          results.push(result);
        } catch (error) {
          results.push({
            artPath: art.path,
            variantName: variant.name,
            violations: [],
            passes: 0,
            incomplete: 0,
            error: error instanceof Error ? error.message : String(error),
          });
        } finally {
          if (context) {
            await context.context.close();
          }
        }
      }
    }

    return results;
  }

  /**
   * Audit a single page using axe-core.
   */
  async auditPage(page: Page, artPath: string, variantName: string): Promise<A11yResult> {
    // Inject axe-core into the page
    const axeSource = await resolveAxeSource();
    await page.evaluate(axeSource);

    // Build axe-core run options
    const runOptions = this.buildAxeOptions();

    // Run axe-core
    const axeResult = (await page.evaluate((opts) => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      return (window as any).axe.run(document, opts);
    }, runOptions)) as AxeResult;

    // Map to our result format
    const violations: A11yViolation[] = axeResult.violations.map((v) => ({
      id: v.id,
      impact: v.impact as A11yViolation["impact"],
      description: v.description,
      helpUrl: v.helpUrl,
      nodes: v.nodes.length,
    }));

    return {
      artPath,
      variantName,
      violations,
      passes: axeResult.passes.length,
      incomplete: axeResult.incomplete.length,
    };
  }

  /**
   * Get summary statistics from results.
   */
  getSummary(results: A11yResult[]): A11ySummary {
    return computeA11ySummary(results);
  }

  /**
   * Generate HTML report.
   */
  generateHtmlReport(results: A11yResult[]): string {
    const summary = this.getSummary(results);
    return generateA11yHtmlReport(results, summary);
  }

  /**
   * Generate JSON report for CI integration.
   */
  generateJsonReport(results: A11yResult[]): string {
    return generateA11yJsonReport(results);
  }

  /**
   * Build axe-core run options from configuration.
   */
  private buildAxeOptions(): Record<string, unknown> {
    const runOnly: Record<string, unknown> = {};

    // Set WCAG level
    const tags: string[] = [];
    switch (this.options.level) {
      case "A":
        tags.push("wcag2a", "wcag21a");
        break;
      case "AA":
        tags.push("wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa");
        break;
      case "AAA":
        tags.push("wcag2a", "wcag2aa", "wcag2aaa", "wcag21a", "wcag21aa", "wcag22aa");
        break;
    }

    if (tags.length > 0) {
      runOnly.type = "tag";
      runOnly.values = tags;
    }

    const rules: Record<string, { enabled: boolean }> = {};

    for (const ruleId of this.options.includeRules) {
      rules[ruleId] = { enabled: true };
    }
    for (const ruleId of this.options.excludeRules) {
      rules[ruleId] = { enabled: false };
    }

    return {
      ...(Object.keys(runOnly).length > 0 ? { runOnly } : {}),
      ...(Object.keys(rules).length > 0 ? { rules } : {}),
    };
  }

  private buildVariantUrl(baseUrl: string, artPath: string, variantName: string): string {
    const encodedPath = encodeURIComponent(artPath);
    const encodedVariant = encodeURIComponent(variantName);
    return `${baseUrl}/__musea__/preview?art=${encodedPath}&variant=${encodedVariant}`;
  }
}
