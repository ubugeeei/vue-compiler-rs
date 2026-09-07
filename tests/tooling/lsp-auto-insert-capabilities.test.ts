import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { testOutputRoot } from "./support/lsp/paths.ts";
import { LspSession } from "./support/lsp/session.ts";
import type { ServerCapabilities } from "./support/lsp/protocol.ts";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

test("auto-insert capability advertises each manifest sub-setting exactly once", async () => {
  await withCapabilities((capabilities) => {
    const sections = capabilities.experimental?.autoInsertionProvider?.configurationSections;
    assert.ok(sections, "auto insert should advertise configuration sections");

    const advertisedKeys = sections.flatMap((section) => section ?? []);
    assert.deepEqual(advertisedKeys, Array.from(new Set(advertisedKeys)));
    assert.deepEqual(advertisedKeys.toSorted(), manifestAutoInsertSubSettingKeys());
  });
});

async function withCapabilities(run: (capabilities: ServerCapabilities) => void): Promise<void> {
  const testRootDir = path.join(testOutputRoot, "lsp-auto-insert-capabilities");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  const session = new LspSession();

  try {
    const init = (await session.initialize(workspaceDir, {
      editor: true,
      autoInsert: true,
    })) as {
      capabilities?: ServerCapabilities;
    };
    assert.ok(init.capabilities, "initialize result should advertise capabilities");
    run(init.capabilities);
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
}

function manifestAutoInsertSubSettingKeys(): string[] {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(repoRoot, "editors", "vscode", "package.json"), "utf-8"),
  ) as {
    contributes?: {
      configuration?: {
        properties?: Record<string, unknown>;
      };
    };
  };

  return Object.keys(manifest.contributes?.configuration?.properties ?? {})
    .filter((key) => key.startsWith("vize.autoInsert.") && key !== "vize.autoInsert.enable")
    .sort();
}
