import assert from "node:assert/strict";
import { test } from "node:test";

import { metadata, workspacePackage, type Package } from "./support/davinci-stage-dependencies.ts";

const publishedDavinciStages = new Set(["vize_davinci", "vize_s1", "vize_s2", "vize_s1_to_s2"]);

function isPublishable(pkg: Package): boolean {
  return pkg.publish === null || pkg.publish.length > 0;
}

function versionRequirement(packageName: string): string {
  return `=${workspacePackage(metadata, packageName).version}`;
}

type FeatureOwner = Pick<Package, "dependencies">;

function featureDependencyKey(featureValue: string): string {
  return featureValue.replace(/^dep:/u, "").split(/[?/]/u, 1)[0];
}

function referencedWorkspaceFeaturePackage(pkg: FeatureOwner, featureValue: string): string | null {
  const dependencyKey = featureDependencyKey(featureValue);
  const dependency = pkg.dependencies.find(
    (dependency) => (dependency.rename ?? dependency.name) === dependencyKey,
  );
  return dependency?.name ?? null;
}

test("Feature values resolve unpublished stages through dependency keys", () => {
  const pkg: FeatureOwner = {
    dependencies: [
      {
        name: "vize_s1_to_s2",
        features: [],
        rename: "stage_alias",
        kind: null,
        optional: true,
        req: "*",
      },
    ],
  };

  assert.equal(referencedWorkspaceFeaturePackage(pkg, "dep:stage_alias"), "vize_s1_to_s2");
  assert.equal(referencedWorkspaceFeaturePackage(pkg, "stage_alias?/legacy"), "vize_s1_to_s2");
  assert.equal(referencedWorkspaceFeaturePackage(pkg, "vize_s1"), null);
  assert.equal(referencedWorkspaceFeaturePackage(pkg, "local_feature"), null);
});

test("Davinci stage crates are published before a production feature can select them", () => {
  for (const packageName of publishedDavinciStages) {
    assert.ok(
      isPublishable(workspacePackage(metadata, packageName)),
      `${packageName} is not publishable`,
    );
  }
});

test("DOM production keeps the published S2 renderer available for profiling", () => {
  const dom = workspacePackage(metadata, "vize_atelier_dom");
  assert.deepEqual(dom.features, {
    legacy: ["vize_atelier_core/legacy"],
    // Test-only surface: exposes `compile_template_legacy_with_options` so
    // the DOM differential lanes have a real old side. Its value list is
    // empty, so unlike `legacy` it selects nothing at all — it cannot pull
    // an unpublished stage in, which is what this firewall exists to stop.
    // Asserted below rather than trusted from the name.
    "davinci-differential": [],
  });
  assert.deepEqual(
    dom.features["davinci-differential"],
    [],
    "a test-only feature must select nothing",
  );

  const stageEdges = dom.dependencies
    .filter((dependency) => publishedDavinciStages.has(dependency.name))
    .map((dependency) => ({
      name: dependency.name,
      kind: dependency.kind,
      req: dependency.req,
      rename: dependency.rename,
      optional: dependency.optional,
      features: dependency.features,
    }))
    .sort((left, right) => left.name.localeCompare(right.name));

  assert.deepEqual(stageEdges, [
    {
      name: "vize_davinci",
      kind: "dev",
      req: versionRequirement("vize_davinci"),
      rename: null,
      optional: false,
      features: [],
    },
    {
      name: "vize_s1_to_s2",
      kind: null,
      req: versionRequirement("vize_s1_to_s2"),
      rename: null,
      optional: false,
      // TypeScript templates are part of the public DOM compiler contract, so
      // the profiled DOM renderer enables the stage library's opt-in erasure.
      features: ["typescript"],
    },
  ]);
});
