// Davinci bench budget registry (plan/phase-0.md P0-4).
//
// Two suites:
//   1. Registry reconciliation — every bench id constructed in the davinci
//      bench sources (cstr!-built or literal) has a `<id> = { … }` entry under
//      [bench] in davinci-road/plan/budgets.toml, and vice versa, with the
//      offending id named on mismatch. Entries carry exactly the documented
//      field set and hold the tolerance ceilings (0.05; 0.10 for stage-window
//      benches: `_transform_`, vapor lower, ssr codegen) — ceilings, not
//      equalities, because the ratchet allows tightening.
//   2. The ratchet header — budgets.toml carries the exact machine-checked
//      ratchet line (the documented-in-file variant of the P0-4 ratchet rule).
//
// The compare gate's stdout/exit oracles live in
// tests/tooling/davinci-bench-compare.test.ts.

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { parseTomlLite } from "../../legacy-tools/davinci/toml-lite.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const budgetsPath = path.join(repoRoot, "davinci-road", "plan", "budgets.toml");

const budgetsText = fs.readFileSync(budgetsPath, "utf8");
const budgets = parseTomlLite(budgetsText) as {
  bench: Record<string, Record<string, unknown>>;
  allocation_peak: Record<string, Record<string, number>>;
};

const EXPLICIT_STAGE_WINDOWS = new Set([
  "s1_to_s2_lower_vfor_three_aliases",
  "s1_to_s2_emit_von_two_per_bucket",
  "s1_to_s2_emit_p2_11_dom_surface",
]);

// --- bench-id enumeration from the bench sources -------------------------
//
// Bench ids are constructed either as a direct string literal passed to
// `bench_with_metrics(criterion, "...")` or via `cstr!("template", args…)`
// with `{}` placeholders. The reconciler expands the templates over the
// argument domains it understands (`fixture.name` — the harness LADDER;
// `preset_name` — the file's `presets` tuple array) and fails loudly on any
// construction it cannot resolve, so a new bench pattern extends this test
// instead of silently escaping the registry.

function ladderNames(): string[] {
  const fixturesRs = fs.readFileSync(
    path.join(repoRoot, "tools", "benchmarks", "crates", "davinci_harness", "src", "fixtures.rs"),
    "utf8",
  );
  const start = fixturesRs.indexOf("pub const LADDER");
  assert.ok(start >= 0, "fixtures.rs must declare the LADDER const");
  const block = fixturesRs.slice(start, fixturesRs.indexOf("];", start));
  const names = [...block.matchAll(/name: "([^"]+)"/g)].map(([, name]) => name);
  assert.ok(names.length > 0, "the LADDER const must name at least one fixture");
  assert.deepEqual([...new Set(names)], names, "LADDER fixture names must be unique");
  return names;
}

function benchSources(): { file: string; text: string }[] {
  const sources: { file: string; text: string }[] = [];
  for (const root of ["crates", path.join("tools", "benchmarks", "crates")]) {
    for (const pkg of fs.readdirSync(path.join(repoRoot, root))) {
      const benchesDir = path.join(repoRoot, root, pkg, "benches");
      if (!fs.existsSync(benchesDir)) continue;
      for (const name of fs.readdirSync(benchesDir)) {
        if (!name.endsWith(".rs")) continue;
        const file = path.join(root, pkg, "benches", name);
        const text = fs.readFileSync(path.join(repoRoot, file), "utf8");
        if (text.includes("_with_metrics(")) sources.push({ file, text });
      }
    }
  }
  assert.ok(sources.length > 0, "no davinci bench sources found (bench_with_metrics users)");
  return sources;
}

function argDomain(arg: string, source: { file: string; text: string }): string[] {
  if (arg === "fixture.name") return ladderNames();
  if (arg === "preset_name") {
    const start = source.text.indexOf("let presets = [");
    assert.ok(start >= 0, `${source.file}: expected a \`let presets = [\` array for preset_name`);
    const block = source.text.slice(start, source.text.indexOf("];", start));
    const names = [...block.matchAll(/\(\s*"([^"]+)"/g)].map(([, name]) => name);
    assert.ok(names.length > 0, `${source.file}: presets array names no presets`);
    return names;
  }
  assert.fail(
    `${source.file}: cstr! argument \`${arg}\` is not understood by the bench-id ` +
      "reconciler — extend tests/tooling/davinci-budgets.test.ts",
  );
}

function sourceBenchIds(): string[] {
  const ids: string[] = [];
  for (const source of benchSources()) {
    for (const [, literal] of source.text.matchAll(
      /bench_with_metrics\(\s*criterion,\s*"([^"]+)"/g,
    )) {
      ids.push(literal);
    }
    for (const [, template, argsRaw] of source.text.matchAll(
      /cstr!\(\s*"([^"]+)"((?:\s*,[^)]*)?)\)/g,
    )) {
      const args = argsRaw
        .split(",")
        .map((arg) => arg.trim())
        .filter((arg) => arg.length > 0);
      const placeholders = template.split("{}").length - 1;
      assert.equal(
        placeholders,
        args.length,
        `${source.file}: cstr!("${template}") has ${placeholders} placeholders but ${args.length} arguments`,
      );
      let variants = [template];
      for (const arg of args) {
        const domain = argDomain(arg, source);
        variants = variants.flatMap((variant) =>
          domain.map((value) => variant.replace("{}", value)),
        );
      }
      ids.push(...variants);
    }
  }
  assert.deepEqual([...new Set(ids)], ids, "bench sources must not construct duplicate ids");
  return ids.sort();
}

test("every davinci bench id has a budgets.toml entry, and vice versa", () => {
  const sourceIds = sourceBenchIds();
  const budgetIds = Object.keys(budgets.bench).sort();
  const missingFromBudgets = sourceIds.filter((id) => !budgetIds.includes(id));
  const withoutBenchSource = budgetIds.filter((id) => !sourceIds.includes(id));
  assert.deepEqual(
    { missingFromBudgets, withoutBenchSource },
    { missingFromBudgets: [], withoutBenchSource: [] },
    "budgets.toml [bench] and the bench sources must reconcile exactly",
  );
  assert.deepEqual(budgetIds, sourceIds);
});

test("every budget entry carries exactly the documented fields within the tolerance ceilings", () => {
  const entries = Object.entries(budgets.bench);
  assert.ok(entries.length > 0, "budgets.toml must register at least one bench");
  for (const [id, entry] of entries) {
    assert.deepEqual(
      Object.keys(entry).sort(),
      ["allocs", "rss_peak_bytes", "wall_p50_ns", "wall_tolerance"],
      `[bench.${id}] must have exactly the documented field set`,
    );
    for (const field of ["wall_p50_ns", "allocs", "rss_peak_bytes"]) {
      const value = entry[field];
      assert.ok(
        Number.isSafeInteger(value) && (value as number) >= 0,
        `[bench.${id}] ${field} must be a non-negative integer, got ${String(value)}`,
      );
    }
    const tolerance = entry.wall_tolerance;
    assert.ok(typeof tolerance === "number", `[bench.${id}] wall_tolerance must be a number`);
    assert.ok(tolerance > 0, `[bench.${id}] wall_tolerance must be positive`);
    const stageWindow =
      id.includes("_transform_") ||
      id.startsWith("atelier_vapor_lower_") ||
      id.startsWith("atelier_ssr_codegen_") ||
      EXPLICIT_STAGE_WINDOWS.has(id);
    const ceiling = stageWindow ? 0.1 : 0.05;
    assert.ok(
      tolerance <= ceiling,
      `[bench.${id}] wall_tolerance ${tolerance} exceeds the ${ceiling} ceiling ` +
        "(ratchet: tolerances only tighten; stage windows get 0.10, whole routines 0.05)",
    );
  }
});

// Inline-table entry lines belonging to one `[section]` header, in file order.
// Exported shape is duplicated in davinci-traversal-budgets.test.ts rather
// than shared: these are node:test files, not a module graph.
function inlineTableLines(section: string): string[] {
  const lines = budgetsText.split("\n");
  const start = lines.indexOf(`[${section}]`);
  assert.ok(start >= 0, `budgets.toml must declare a [${section}] section`);
  const rest = lines.slice(start + 1);
  const end = rest.findIndex((line) => line.startsWith("["));
  const body = end === -1 ? rest : rest.slice(0, end);
  return body.filter((line) => /^[A-Za-z0-9._-]+ = \{/.test(line));
}

test("every bench entry is one single-line inline table", () => {
  const entryLines = inlineTableLines("bench");
  assert.equal(
    entryLines.length,
    Object.keys(budgets.bench).length,
    "every [bench] entry must be exactly one `<id> = { … }` line (the registry grows per bench, " +
      "so the six-line table form would push budgets.toml past the source-length ceiling)",
  );
  for (const line of entryLines) {
    assert.match(
      line,
      /^[A-Za-z0-9._-]+ = \{ wall_p50_ns = \d+, allocs = \d+, rss_peak_bytes = \d+, wall_tolerance = 0\.\d+ \}$/,
      `bench entries must keep the canonical field order and spacing:\n${line}`,
    );
  }
});

test("compact-storage peaks are exact per-platform allocation budgets", () => {
  assert.deepEqual(budgets.allocation_peak, {
    s1_to_s2_lower_vfor_three_aliases: { linux: 1671, macos: 1246 },
    s1_to_s2_emit_von_two_per_bucket: { linux: 648, macos: 648 },
  });
  for (const [id, peaks] of Object.entries(budgets.allocation_peak)) {
    assert.ok(id in budgets.bench, `${id} must also have a [bench] budget`);
    assert.deepEqual(
      Object.keys(peaks).sort(),
      ["linux", "macos"],
      `${id} must fail closed unless every required platform is explicit`,
    );
    for (const peak of Object.values(peaks)) {
      assert.ok(Number.isSafeInteger(peak) && peak >= 0);
    }
  }
});

test("compact-storage probes stay wall-report-only with exact alloc gates", () => {
  // The sub-2us stage windows vary more than the 10% wall ceiling across CI
  // runners (#4857, #4877), so the wall side stays unrecorded until a
  // reference-runner methodology lands: no committed baseline report, and the
  // wall_p50_ns = 0 seed. The deterministic alloc and per-platform peak gates
  // still fail closed.
  const expected = {
    s1_to_s2_lower_vfor_three_aliases: { allocs: 14, allocBytesPeakLinux: 1671 },
    s1_to_s2_emit_von_two_per_bucket: { allocs: 18, allocBytesPeakLinux: 648 },
  };
  for (const [id, row] of Object.entries(expected)) {
    const reportPath = path.join(
      repoRoot,
      "tools",
      "benchmarks",
      "results",
      "davinci",
      "baseline",
      `${id}.json`,
    );
    assert.equal(
      fs.existsSync(reportPath),
      false,
      `${id} must stay wall-unbaselined until a reference-runner recording lands`,
    );
    assert.equal(budgets.bench[id]?.wall_p50_ns, 0);
    assert.equal(budgets.bench[id]?.wall_tolerance, 0.1);
    assert.equal(budgets.bench[id]?.allocs, row.allocs);
    assert.equal(budgets.allocation_peak[id]?.linux, row.allocBytesPeakLinux);
  }
});

test("P2-11 S2 DOM emit probe gates exact allocations", () => {
  const id = "s1_to_s2_emit_p2_11_dom_surface";
  const reportPath = path.join(
    repoRoot,
    "tools",
    "benchmarks",
    "results",
    "davinci",
    "baseline",
    `${id}.json`,
  );
  assert.equal(fs.existsSync(reportPath), false);
  assert.equal(budgets.bench[id]?.wall_p50_ns, 0);
  assert.equal(budgets.bench[id]?.wall_tolerance, 0.1);
  assert.equal(budgets.bench[id]?.allocs, 60);
});

test("toml-lite parses inline tables and rejects malformed ones", () => {
  assert.deepEqual(parseTomlLite("[bench]\na = { x = 1, y = 0.5 }\n"), {
    bench: { a: { x: 1, y: 0.5 } },
  });
  assert.throws(() => parseTomlLite("[bench]\na = { x = 1,\n"), /unterminated inline table/);
  assert.throws(
    () => parseTomlLite("[bench]\na = { x = 1 y = 2 }\n"),
    /expected `,` or `\}` in inline table/,
  );
  assert.throws(
    () => parseTomlLite("[bench]\na = { x = 1, x = 2 }\n"),
    /duplicate key x in inline table/,
  );
});

test("budgets.toml carries the exact ratchet header line", () => {
  const ratchetLine =
    "# ratchet: numbers may only tighten; loosening requires " +
    "budget-loosen: <charter ref> in the commit body";
  assert.ok(
    budgetsText.split("\n").includes(ratchetLine),
    `budgets.toml must contain the exact line:\n${ratchetLine}`,
  );
});
