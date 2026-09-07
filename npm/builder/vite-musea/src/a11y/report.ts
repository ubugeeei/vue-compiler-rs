/**
 * A11y report generation for Musea.
 *
 * Generates HTML and JSON reports from accessibility audit results,
 * plus summary statistics computation.
 */

import type { A11yResult } from "../types/index.js";
import type { A11ySummary } from "./index.js";
import path from "node:path";

/**
 * Compute a11y summary statistics from results.
 */
export function computeA11ySummary(results: A11yResult[]): A11ySummary {
  const components = new Set(results.map((r) => r.artPath));
  const allViolations = results.flatMap((r) => r.violations);

  return {
    totalComponents: components.size,
    totalVariants: results.length,
    erroredVariants: results.filter((r) => r.error !== undefined).length,
    totalViolations: allViolations.length,
    criticalCount: allViolations.filter((v) => v.impact === "critical").length,
    seriousCount: allViolations.filter((v) => v.impact === "serious").length,
    moderateCount: allViolations.filter((v) => v.impact === "moderate").length,
    minorCount: allViolations.filter((v) => v.impact === "minor").length,
  };
}

/**
 * Generate HTML report from a11y results.
 */
export function generateA11yHtmlReport(results: A11yResult[], summary: A11ySummary): string {
  const timestamp = new Date().toLocaleString("ja-JP", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });

  const impactColor = (impact: string): string => {
    switch (impact) {
      case "critical":
        return "#f87171";
      case "serious":
        return "#fb923c";
      case "moderate":
        return "#fbbf24";
      case "minor":
        return "#60a5fa";
      default:
        return "#7b8494";
    }
  };

  const errorItems = results
    .filter((r) => r.error !== undefined)
    .map(
      (r) => `
        <div class="result">
          <div class="result-header">
            <div class="result-info">
              <span class="result-name">${escapeHtml(path.basename(r.artPath, ".art.vue"))} / ${escapeHtml(r.variantName)}</span>
              <span class="result-count">audit did not run</span>
            </div>
          </div>
          <p style="padding:0.75rem 1rem;color:#f87171"><code>${escapeHtml(r.error ?? "")}</code></p>
        </div>`,
    )
    .join("");

  const resultItems = results
    .filter((r) => r.violations.length > 0)
    .map((r) => {
      const artName = path.basename(r.artPath, ".art.vue");
      const violationRows = r.violations
        .map(
          (v) => `
            <tr>
              <td><span style="color:${impactColor(v.impact)};font-weight:600;text-transform:uppercase;font-size:0.6875rem">${escapeHtml(v.impact)}</span></td>
              <td><code>${escapeHtml(v.id)}</code></td>
              <td>${escapeHtml(v.description)}</td>
              <td>${v.nodes}</td>
              <td>${v.helpUrl ? `<a href="${escapeHtml(v.helpUrl)}" target="_blank" style="color:#60a5fa">docs</a>` : ""}</td>
            </tr>`,
        )
        .join("");

      return `
        <div class="result">
          <div class="result-header">
            <div class="result-info">
              <span class="result-name">${escapeHtml(artName)} / ${escapeHtml(r.variantName)}</span>
              <span class="result-count">${r.violations.length} violation(s)</span>
            </div>
          </div>
          <table class="violations-table">
            <thead><tr><th>Impact</th><th>Rule</th><th>Description</th><th>Nodes</th><th>Help</th></tr></thead>
            <tbody>${violationRows}</tbody>
          </table>
        </div>`;
    })
    .join("");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>A11y Report - Musea</title>
  <style>
    :root {
      --musea-bg-primary: #0d0d0d;
      --musea-bg-secondary: #1a1815;
      --musea-bg-tertiary: #252220;
      --musea-accent: #a34828;
      --musea-text: #e6e9f0;
      --musea-text-muted: #7b8494;
      --musea-border: #3a3530;
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
      background: var(--musea-bg-primary);
      color: var(--musea-text);
      min-height: 100vh;
      line-height: 1.5;
    }
    .header {
      background: var(--musea-bg-secondary);
      border-bottom: 1px solid var(--musea-border);
      padding: 1rem 2rem;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }
    .logo { font-size: 1.25rem; font-weight: 700; color: var(--musea-accent); }
    .header-meta { color: var(--musea-text-muted); font-size: 0.8125rem; }
    .main { max-width: 1200px; margin: 0 auto; padding: 2rem; }
    .summary { display: grid; grid-template-columns: repeat(auto-fit, minmax(120px, 1fr)); gap: 1rem; margin-bottom: 2rem; }
    .stat { background: var(--musea-bg-secondary); border: 1px solid var(--musea-border); border-radius: 8px; padding: 1rem; text-align: center; }
    .stat-value { font-size: 1.75rem; font-weight: 700; font-variant-numeric: tabular-nums; }
    .stat-label { color: var(--musea-text-muted); font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.08em; }
    .stat.critical .stat-value { color: #f87171; }
    .stat.serious .stat-value { color: #fb923c; }
    .stat.moderate .stat-value { color: #fbbf24; }
    .stat.minor .stat-value { color: #60a5fa; }
    .stat.total .stat-value { color: var(--musea-text); }
    .results { display: flex; flex-direction: column; gap: 1rem; }
    .result { background: var(--musea-bg-secondary); border: 1px solid var(--musea-border); border-radius: 8px; overflow: hidden; }
    .result-header { padding: 1rem; background: var(--musea-bg-tertiary); display: flex; justify-content: space-between; align-items: center; }
    .result-name { font-weight: 600; }
    .result-count { color: var(--musea-text-muted); font-size: 0.8125rem; }
    .violations-table { width: 100%; border-collapse: collapse; font-size: 0.8125rem; }
    .violations-table th { padding: 0.75rem 1rem; text-align: left; color: var(--musea-text-muted); font-weight: 500; font-size: 0.6875rem; text-transform: uppercase; letter-spacing: 0.08em; border-bottom: 1px solid var(--musea-border); }
    .violations-table td { padding: 0.75rem 1rem; border-bottom: 1px solid var(--musea-border); }
    .violations-table code { background: var(--musea-bg-tertiary); padding: 0.125rem 0.375rem; border-radius: 3px; font-size: 0.75rem; }
    .all-clear { background: rgba(74, 222, 128, 0.1); border: 1px solid rgba(74, 222, 128, 0.2); border-radius: 8px; padding: 2rem; text-align: center; }
    .all-clear-text { color: #4ade80; font-weight: 600; }
  </style>
</head>
<body>
  <header class="header">
    <div><span class="logo">Musea</span> <span style="color:var(--musea-text-muted);font-size:0.875rem;margin-left:1rem">Accessibility Report</span></div>
    <div class="header-meta">${timestamp}</div>
  </header>
  <main class="main">
    <div class="summary">
      <div class="stat total"><div class="stat-value">${summary.totalViolations}</div><div class="stat-label">Violations</div></div>
      <div class="stat critical"><div class="stat-value">${summary.criticalCount}</div><div class="stat-label">Critical</div></div>
      <div class="stat serious"><div class="stat-value">${summary.seriousCount}</div><div class="stat-label">Serious</div></div>
      <div class="stat moderate"><div class="stat-value">${summary.moderateCount}</div><div class="stat-label">Moderate</div></div>
      <div class="stat minor"><div class="stat-value">${summary.minorCount}</div><div class="stat-label">Minor</div></div>
      <div class="stat critical"><div class="stat-value">${summary.erroredVariants}</div><div class="stat-label">Not audited</div></div>
    </div>
    ${
      summary.totalViolations === 0 && summary.erroredVariants === 0
        ? `<div class="all-clear"><div class="all-clear-text">No accessibility violations found across ${summary.totalVariants} variant(s)</div></div>`
        : `<div class="results">${errorItems}${resultItems}</div>`
    }
  </main>
</body>
</html>`;
}

/**
 * Generate JSON report for CI integration.
 */
export function generateA11yJsonReport(results: A11yResult[]): string {
  const summary = computeA11ySummary(results);
  return JSON.stringify(
    {
      timestamp: new Date().toISOString(),
      summary,
      results: results.map((r) => ({
        art: path.basename(r.artPath, ".art.vue"),
        variant: r.variantName,
        violations: r.violations,
        passes: r.passes,
        incomplete: r.incomplete,
        ...(r.error === undefined ? {} : { error: r.error }),
      })),
    },
    null,
    2,
  );
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#x27;");
}
