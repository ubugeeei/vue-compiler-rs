import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";

import {
  cleanup,
  readJson,
  run,
  setup,
  writeJson,
} from "./_helpers/typecheck-divergence-report-fixture.ts";

const duplicateListenerDifference = {
  project: "fixture",
  file: "src/App.vue",
  severity: "error",
  line: 3,
  column: 1,
  vize: null,
  baseline: {
    code: 1117,
    message: "An object literal cannot have multiple properties with the same name.",
  },
  issue: 5722,
  reason:
    "vue-tsc reports a duplicate generated listener key for legal paired event directives; " +
    "Vize intentionally keeps listener directives out of the props literal.",
};

function writeDocumentedDifferences(
  fixture: ReturnType<typeof setup>,
  differences = [duplicateListenerDifference],
) {
  const ledgerPath = path.join(fixture.fixtureRoot, "documented-differences.json");
  writeJson(ledgerPath, {
    schema: "vize.compatDocumentedDifferences",
    version: 1,
    differences,
  });
  return ledgerPath;
}

test("typecheck divergence report accepts a reproducing documented difference", () => {
  const fixture = setup({
    baselineOutput:
      "src/App.vue(1,1): error TS2322: shared\n" +
      "src/App.vue(3,1): error TS1117: An object literal cannot have multiple properties with the same name.\n",
  });
  try {
    const result = run(fixture, {}, [
      "--documented-differences",
      writeDocumentedDifferences(fixture),
    ]);
    assert.equal(result.status, 0, result.stderr);
    const artifact = readJson(path.join(fixture.reportDir, "fixture-typecheck-divergence.json"));
    assert.equal(artifact.divergence.summary.documentedDifferenceCount, 1);
    assert.equal(artifact.divergence.summary.falseNegativeCount, 0);
  } finally {
    cleanup(fixture);
  }
});

test("typecheck divergence report rejects a stale documented difference", () => {
  const fixture = setup();
  try {
    const secondStaleDifference = {
      ...duplicateListenerDifference,
      file: "src/Second.vue",
      line: 4,
      column: 2,
    };
    const result = run(fixture, {}, [
      "--documented-differences",
      writeDocumentedDifferences(fixture, [duplicateListenerDifference, secondStaleDifference]),
    ]);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Documented typecheck difference ledger is stale for fixture/);
    assert.match(result.stderr, /src\/App\.vue:3:1/);
    assert.match(result.stderr, /src\/Second\.vue:4:2/);
    assert.equal(
      fs.existsSync(path.join(fixture.reportDir, "fixture-typecheck-divergence.json")),
      false,
    );
  } finally {
    cleanup(fixture);
  }
});

test("typecheck divergence report rejects stale documented differences in record-only mode", () => {
  const fixture = setup();
  try {
    const result = run(fixture, {}, [
      "--budget-mode",
      "record-only",
      "--documented-differences",
      writeDocumentedDifferences(fixture),
    ]);
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Documented typecheck difference ledger is stale for fixture/);
    assert.equal(
      fs.existsSync(path.join(fixture.reportDir, "fixture-typecheck-divergence.json")),
      false,
    );
  } finally {
    cleanup(fixture);
  }
});
