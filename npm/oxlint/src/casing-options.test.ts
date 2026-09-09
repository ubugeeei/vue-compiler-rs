import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resetFixtureDir } from "./test-support/fixture-dir.ts";

const packageDir = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(packageDir, "../../..");
const pluginEntry = path.join(workspaceRoot, "npm/oxlint/dist/index.mjs");
const fixtureDir = path.join(
  workspaceRoot,
  "target",
  "vize-tests",
  "oxlint-plugin-vize-casing-options-test",
);
const configPath = path.join(fixtureDir, ".oxlintrc.json");
const vuePath = path.join(fixtureDir, "Casing.vue");
const selfClosingPath = path.join(fixtureDir, "SelfClosing.vue");
const oxlintEnv = { ...process.env };
delete oxlintEnv.GITHUB_ACTIONS;

function findOxlintBin() {
  const pnpmStoreDir = path.join(workspaceRoot, "node_modules", ".pnpm");
  const candidates = fs
    .readdirSync(pnpmStoreDir)
    .filter((entry) => entry.startsWith("oxlint@"))
    .sort((left, right) => right.localeCompare(left))
    .map((entry) => path.join(pnpmStoreDir, entry, "node_modules", "oxlint", "bin", "oxlint"))
    .filter((entry) => fs.existsSync(entry));
  const match = candidates[0];
  if (match == null) {
    throw new Error(`Unable to locate the oxlint binary in ${pnpmStoreDir}`);
  }
  return match;
}

function runOxlint(args: readonly string[]) {
  let output = "";
  let exitCode = 0;
  try {
    output = String(
      execFileSync(findOxlintBin(), args, {
        cwd: fixtureDir,
        encoding: "utf8",
        env: oxlintEnv,
        stdio: "pipe",
      }),
    );
  } catch (error) {
    const execError = error as {
      status?: number;
      stdout?: string | Buffer;
      stderr?: string | Buffer;
    };
    exitCode = execError.status ?? 1;
    output = String(execError.stdout ?? "") + String(execError.stderr ?? "");
  }
  return { exitCode, output: normalizeOutput(output) };
}

function normalizeOutput(output: string): string {
  return output
    .replace(/^WARNING: JS plugins are experimental and not subject to semver\.\n/gmu, "")
    .replace(
      /^Breaking changes are possible while JS plugins support is under development\.\n/gmu,
      "",
    )
    .trim();
}

resetFixtureDir(fixtureDir);
fs.writeFileSync(
  configPath,
  JSON.stringify(
    {
      plugins: ["vue"],
      jsPlugins: [pluginEntry],
      settings: { vize: { helpLevel: "none", preset: "incremental" } },
      rules: {
        "no-unused-vars": "off",
        "vize/vue/component-name-in-template-casing": ["error", "kebab-case"],
        "vize/script/custom-event-name-casing": ["error", "kebab-case"],
      },
    },
    null,
    2,
  ),
);
fs.writeFileSync(
  vuePath,
  `<template>
  <my-widget />
  <MyWidget />
</template>

<script setup lang="ts">
import MyWidget from './MyWidget.vue';

const emit = defineEmits<{ 'keep-original': []; keepOriginal: [] }>();

emit('keep-original');
emit('keepOriginal');
</script>
`,
);

const run = runOxlint(["-c", ".oxlintrc.json", "-f", "json", "Casing.vue"]);
assert.notEqual(run.exitCode, 0, "kebab-case violations should fail lint");
assert.doesNotMatch(run.output, /does not accept options/u);
const payload = JSON.parse(run.output) as {
  diagnostics: Array<{ code: string; message: string }>;
};
assert.equal(
  payload.diagnostics.filter(
    (diagnostic) => diagnostic.code === "vize(vue/component-name-in-template-casing)",
  ).length,
  1,
);
assert.equal(
  payload.diagnostics.filter(
    (diagnostic) => diagnostic.code === "vize(script/custom-event-name-casing)",
  ).length,
  1,
);
assert.match(
  payload.diagnostics.map((diagnostic) => diagnostic.message).join("\n"),
  /Custom event name 'keepOriginal' is not kebab-case/u,
);

fs.writeFileSync(
  configPath,
  JSON.stringify(
    {
      plugins: ["vue"],
      jsPlugins: [pluginEntry],
      settings: { vize: { helpLevel: "none", preset: "incremental" } },
      rules: {
        "vize/vue/html-self-closing": [
          "error",
          {
            html: {
              void: "any",
              normal: "never",
              component: "any",
            },
            svg: "any",
            math: "any",
          },
        ],
      },
    },
    null,
    2,
  ),
);
fs.writeFileSync(
  selfClosingPath,
  `<template>
  <img>
  <div />
  <MyWidget></MyWidget>
  <svg><path></path></svg>
  {{ ready }}
</template>

<script setup lang="ts">
const ready = true;
</script>
`,
);

const selfClosingRun = runOxlint(["-c", ".oxlintrc.json", "-f", "json", "SelfClosing.vue"]);
assert.notEqual(selfClosingRun.exitCode, 0, "html-self-closing violations should fail lint");
assert.doesNotMatch(selfClosingRun.output, /does not accept options/u);
const selfClosingPayload = JSON.parse(selfClosingRun.output) as {
  diagnostics: Array<{ code: string; message: string }>;
};
assert.equal(
  selfClosingPayload.diagnostics.filter(
    (diagnostic) => diagnostic.code === "vize(vue/html-self-closing)",
  ).length,
  1,
);
assert.match(
  selfClosingPayload.diagnostics.map((diagnostic) => diagnostic.message).join("\n"),
  /Element must not use self-closing syntax/u,
);

console.log("✅ oxlint-plugin-vize casing option tests passed!");
