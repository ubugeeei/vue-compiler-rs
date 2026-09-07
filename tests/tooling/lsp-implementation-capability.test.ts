import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";
import { firstLocation, isDiagnosticsForUri, offsetToPosition } from "./support/lsp/assertions.ts";
import { root, testOutputRoot } from "./support/lsp/paths.ts";
import type { ServerCapabilities } from "./support/lsp/protocol.ts";
import { LspSession } from "./support/lsp/session.ts";
import {
  requireTypecheckDependency,
  resolveTypecheckRuntime,
} from "./support/typecheck-dependency.ts";

const source = `<script setup lang="ts">
interface Formatter {
  format(value: string): string
}

class LabelFormatter implements Formatter {
  format(value: string): string {
    return value.toUpperCase()
  }
}

const formatter: Formatter = new LabelFormatter()
const message = "hello"
</script>

<template>
  <span>{{ formatter.format(message) }}</span>
</template>
`;

const tsxSource = `interface Formatter {
  format(value: string): string
}

class LabelFormatter implements Formatter {
  format(value: string): string {
    return value.toUpperCase()
  }
}

const formatter: Formatter = new LabelFormatter()

export const view = <span>{formatter.format("hello")}</span>
`;

type ImplementationCapabilities = ServerCapabilities & {
  implementationProvider?: unknown;
};

test("vize lsp maps textDocument/implementation from authored SFC interfaces", async () => {
  const testRootDir = path.join(testOutputRoot, "lsp-implementation-capability");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  fs.writeFileSync(
    path.join(workspaceDir, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          strict: true,
          target: "ES2022",
          module: "ESNext",
          moduleResolution: "bundler",
          noEmit: true,
        },
        include: ["**/*"],
      },
      null,
      2,
    ),
  );
  const filePath = path.join(workspaceDir, "Widget.vue");
  const uri = pathToFileURL(filePath).href;
  const session = new LspSession();

  try {
    const init = (await session.initialize(workspaceDir, {
      editor: true,
      lint: false,
      typecheck: true,
    })) as { capabilities?: ImplementationCapabilities };
    assert.equal(init.capabilities?.implementationProvider, true);

    fs.writeFileSync(filePath, source, "utf8");
    session.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: "vue", version: 1, text: source },
    });
    await session.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) => isDiagnosticsForUri(params, uri),
      60_000,
    );

    const position = offsetToPosition(source, source.indexOf("format(value: string): string"));
    const implementation = await session.request("textDocument/implementation", {
      textDocument: { uri },
      position,
    });
    const location = firstLocation(implementation as never);
    assert.equal(location.uri, uri);
    assert.deepEqual(
      location.range.start,
      offsetToPosition(source, source.indexOf("format(value: string): string {")),
    );
    assert.ok(!location.uri.endsWith(".vue.ts"), JSON.stringify(implementation));
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});

test("vize lsp maps textDocument/implementation from authored TSX interfaces", async (t) => {
  const corsaPath = requireTypecheckDependency(
    t,
    resolveTypecheckRuntime(root),
    "TypeScript 7/Corsa runtime for TSX implementation navigation",
    "TypeScript 7/Corsa runtime not found; skipping TSX implementation navigation test",
  );
  if (corsaPath == null) return;

  const testRootDir = path.join(testOutputRoot, "lsp-tsx-implementation-capability");
  fs.mkdirSync(testRootDir, { recursive: true });
  const workspaceDir = fs.mkdtempSync(path.join(testRootDir, "workspace-"));
  const sourceDir = path.join(workspaceDir, "src");
  fs.mkdirSync(sourceDir, { recursive: true });
  writeVueShim(sourceDir);
  fs.writeFileSync(
    path.join(workspaceDir, "vize.config.json"),
    JSON.stringify({
      lsp: { lint: false, typecheck: true },
      typeChecker: { corsaPath, jsxTypecheck: true },
    }),
    "utf8",
  );
  fs.writeFileSync(
    path.join(workspaceDir, "tsconfig.json"),
    JSON.stringify(
      {
        compilerOptions: {
          jsx: "preserve",
          jsxImportSource: "vue",
          module: "ESNext",
          moduleResolution: "bundler",
          noEmit: true,
          strict: true,
          target: "ES2022",
        },
        include: ["src/**/*"],
      },
      null,
      2,
    ),
  );
  const filePath = path.join(sourceDir, "Widget.tsx");
  const uri = pathToFileURL(filePath).href;
  const session = new LspSession();

  try {
    const init = (await session.initialize(workspaceDir, {
      editor: true,
      lint: false,
      typecheck: true,
    })) as { capabilities?: ImplementationCapabilities };
    assert.equal(init.capabilities?.implementationProvider, true);

    fs.writeFileSync(filePath, tsxSource, "utf8");
    session.notify("textDocument/didOpen", {
      textDocument: { uri, languageId: "typescriptreact", version: 1, text: tsxSource },
    });
    await session.waitForNotification(
      "textDocument/publishDiagnostics",
      (params) => isDiagnosticsForUri(params, uri),
      60_000,
    );

    const position = offsetToPosition(
      tsxSource,
      tsxSource.indexOf("format(value: string): string"),
    );
    const implementation = await session.request("textDocument/implementation", {
      textDocument: { uri },
      position,
    });
    const location = firstLocation(implementation as never);
    assert.equal(location.uri, uri);
    assert.deepEqual(
      location.range.start,
      offsetToPosition(tsxSource, tsxSource.indexOf("format(value: string): string {")),
    );
    assert.ok(!location.uri.endsWith(".jsx.ts"), JSON.stringify(implementation));
  } finally {
    await session.shutdown();
    fs.rmSync(workspaceDir, { recursive: true, force: true });
    fs.rmSync(testRootDir, { recursive: true, force: true });
  }
});

function writeVueShim(sourceDir: string): void {
  fs.writeFileSync(
    path.join(sourceDir, "vue-shim.d.ts"),
    `declare namespace JSX {
  interface IntrinsicElements {
    span: { children?: unknown }
  }
}

declare module "vue/jsx-runtime" {
  export const Fragment: unique symbol
  export function jsx(type: unknown, props: unknown): unknown
  export function jsxs(type: unknown, props: unknown): unknown
}
`,
    "utf8",
  );
}
