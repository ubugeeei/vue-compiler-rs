import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { scanConsumerMigrationSurfaces } from "../../legacy-tools/davinci/lib/consumer-migration-scan.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const sfcPreferredS0Rows = [
  ["crates/vize_atelier_sfc/src/bundler/asset_rewrite.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/bundler/asset_rewrite/replacements.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/bundler/assets.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/bundler/blocks.rs", "source", 2],
  ["crates/vize_atelier_sfc/src/bundler/css.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/bundler/scope.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile/bindings.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile/diagnostics.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile/empty_component.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile_template.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile_template.rs", "test", 1],
  ["crates/vize_atelier_sfc/src/compile_template/extraction.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile_template/string_tracking.rs", "source", 1],
  ["crates/vize_atelier_sfc/src/compile_template/section_offsets_tests.rs", "test", 1],
  ["crates/vize_atelier_sfc/src/compile_template/vapor.rs", "source", 5],
  ["crates/vize_atelier_sfc/tests/allocation_budget.rs", "test", 1],
  ["crates/vize_atelier_sfc/tests/component_spread_props.rs", "test", 1],
  ["crates/vize_atelier_sfc/tests/css_engine_panic_boundary.rs", "test", 5],
  ["crates/vize_atelier_sfc/tests/css_nesting_guard.rs", "test", 6],
  ["crates/vize_atelier_sfc/tests/custom_directive_patch_flag.rs", "test", 1],
  ["crates/vize_atelier_sfc/tests/davinci_arena_pool.rs", "test", 1],
  ["crates/vize_atelier_sfc/tests/emitter_javascript_output.rs", "test", 3],
  ["crates/vize_atelier_sfc/tests/imported_pick_intersection_props.rs", "test", 1],
  ["crates/vize_atelier_sfc/tests/imported_type_cache.rs", "test", 2],
  ["crates/vize_atelier_sfc/tests/workspace_prop_types.rs", "test", 2],
];

function compiler() {
  const scan = scanConsumerMigrationSurfaces();
  const consumer = scan.consumers.find((candidate) => candidate.id === "compiler");
  assert.ok(consumer);
  return consumer;
}

void test("Atelier SFC declares the S0 dependency through the preferred name", () => {
  const cargoToml = fs.readFileSync(
    path.join(repoRoot, "crates", "vize_atelier_sfc", "Cargo.toml"),
    "utf8",
  );

  assert.match(cargoToml, /^vize_s0\.workspace = true$/m);
  assert.doesNotMatch(cargoToml, /^vize_carton\.workspace = true$/m);
});

void test("Atelier SFC selected compiler and integration test slices import S0 through the preferred name", () => {
  const rows = compiler().fileRows;

  for (const [relPath, mode, sites] of sfcPreferredS0Rows) {
    const row = rows.find((candidate) => candidate.relPath === relPath && candidate.mode === mode);
    assert.ok(row, `${relPath} (${mode})`);
    assert.equal(row.surfaceCounts.s0, sites, relPath);
    assert.equal(row.surfaceNameCounts.s0.vize_s0, sites, relPath);
    assert.equal(row.surfaceNameCounts.s0.vize_carton ?? 0, 0, relPath);

    const source = fs.readFileSync(path.join(repoRoot, relPath), "utf8");
    assert.doesNotMatch(source, /\bvize_carton\b/u, relPath);
  }
});
