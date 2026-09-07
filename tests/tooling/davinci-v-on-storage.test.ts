import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const thisGate = "tests/tooling/davinci-v-on-storage.test.ts";

// Reviewed scope: Git-tracked template files, documentation, and source/test
// formats that carry inline template fixtures. Binary/generated/untracked
// files are outside the inventory. JS-family carriers are explicit so a
// fixture moved from TS to JS cannot silently disappear from the evidence.
const templateCarrierExtensions = new Set([
  ".astro",
  ".cjs",
  ".cts",
  ".html",
  ".js",
  ".jsx",
  ".md",
  ".mjs",
  ".mts",
  ".rs",
  ".svelte",
  ".ts",
  ".tsx",
  ".vue",
]);

// Baseline: 9e18d171c3ef3a16021dff4debeab21195f99017, immediately before the
// SmallVec change. Continue scanning the current Git-tracked corpus so future
// natural spellings force an intentional capacity/evidence update, while the
// marked synthetic boundary cases never justify their own chosen capacity.
const syntheticBoundary =
  /\/\/ v-on-storage-synthetic:start[\s\S]*?\/\/ v-on-storage-synthetic:end/gu;
const modifiedOnName = /^(?:@|v-on:)(?!\[)[^\s=./>]+(?:\.[^\s=./>]+)+$/u;

type Buckets = { options: number; event: number; keys: number };

function trackedNaturalSources(): Array<{ file: string; source: string }> {
  const tracked = spawnSync("git", ["ls-files", "-z"], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  assert.equal(tracked.status, 0, tracked.stderr);

  return tracked.stdout
    .split("\0")
    .filter(
      (file) =>
        file !== "" && file !== thisGate && templateCarrierExtensions.has(path.extname(file)),
    )
    .map((file) => ({
      file,
      source: fs.readFileSync(path.join(repoRoot, file), "utf8").replace(syntheticBoundary, ""),
    }));
}

function startTags(source: string): string[] {
  const tags: string[] = [];
  for (let start = 0; start < source.length - 1; start += 1) {
    if (source[start] !== "<" || !/[A-Za-z]/u.test(source[start + 1])) continue;

    let quote: '"' | "'" | undefined;
    for (let end = start + 2; end < source.length; end += 1) {
      const character = source[end];
      if (quote !== undefined) {
        if (character === quote) quote = undefined;
      } else if (character === '"' || character === "'") {
        quote = character;
      } else if (character === ">") {
        tags.push(source.slice(start, end + 1));
        start = end;
        break;
      } else if (character === "<") {
        break;
      }
    }
  }
  return tags;
}

function attributeNames(tag: string): string[] {
  const names: string[] = [];
  let cursor = 1;
  while (cursor < tag.length && !/[\s/>]/u.test(tag[cursor])) cursor += 1;

  while (cursor < tag.length) {
    while (/\s/u.test(tag[cursor] ?? "")) cursor += 1;
    if (tag[cursor] === "/" || tag[cursor] === ">" || tag[cursor] === undefined) break;

    const nameStart = cursor;
    while (!/[\s=/>]/u.test(tag[cursor] ?? ">")) cursor += 1;
    names.push(tag.slice(nameStart, cursor));
    while (/\s/u.test(tag[cursor] ?? "")) cursor += 1;
    if (tag[cursor] !== "=") continue;

    cursor += 1;
    while (/\s/u.test(tag[cursor] ?? "")) cursor += 1;
    const escapedQuote =
      tag[cursor] === "\\" && (tag[cursor + 1] === '"' || tag[cursor + 1] === "'");
    const quote = escapedQuote ? tag[cursor + 1] : tag[cursor];
    if (quote === '"' || quote === "'") {
      cursor += escapedQuote ? 2 : 1;
      while (
        cursor < tag.length &&
        (escapedQuote ? tag[cursor] !== "\\" || tag[cursor + 1] !== quote : tag[cursor] !== quote)
      ) {
        cursor += 1;
      }
      cursor += escapedQuote ? 2 : 1;
    } else {
      while (!/[\s>]/u.test(tag[cursor] ?? ">")) cursor += 1;
    }
  }
  return names;
}

function modifiedOnSpellings(source: string): string[] {
  return startTags(source).flatMap((tag) =>
    attributeNames(tag).filter((name) => modifiedOnName.test(name)),
  );
}

function classify(spelling: string): Buckets {
  const normalized = spelling.startsWith("@") ? spelling.slice(1) : spelling.slice("v-on:".length);
  const [name, ...modifiers] = normalized.split(".");
  const keyboard = name === "keydown" || name === "keyup" || name === "keypress";
  const buckets: Buckets = { options: 0, event: 0, keys: 0 };

  for (const modifier of modifiers) {
    if (modifier === "native") continue;
    if (modifier === "capture" || modifier === "once" || modifier === "passive") {
      buckets.options += 1;
    } else if ((modifier === "left" || modifier === "right") && keyboard) {
      buckets.keys += 1;
    } else if (
      [
        "stop",
        "prevent",
        "self",
        "ctrl",
        "shift",
        "alt",
        "meta",
        "middle",
        "exact",
        "left",
        "right",
      ].includes(modifier)
    ) {
      buckets.event += 1;
    } else {
      buckets.keys += 1;
    }
  }
  return buckets;
}

test("the natural committed v-on corpus fits the two-entry inline buckets", () => {
  const sources = trackedNaturalSources();
  const spellings = sources.flatMap(({ source }) => modifiedOnSpellings(source));
  const dynamicEntryFixture = sources.find(
    ({ file }) => file === "crates/vize_s1_to_s2/tests/emit_create_slots/dynamic_entries.rs",
  );
  const maxima = spellings.map(classify).reduce<Buckets>(
    (max, buckets) => ({
      options: Math.max(max.options, buckets.options),
      event: Math.max(max.event, buckets.event),
      keys: Math.max(max.keys, buckets.keys),
    }),
    { options: 0, event: 0, keys: 0 },
  );

  assert.ok(dynamicEntryFixture, "the dynamic slot entry parity fixture remains tracked");
  assert.ok(
    modifiedOnSpellings(dynamicEntryFixture.source).includes("@click.prevent"),
    "the Mealie-shaped dynamic slot fixture keeps its natural modified v-on spelling",
  );
  assert.deepEqual(classify("@click.prevent"), { options: 0, event: 1, keys: 0 });
  assert.equal(spellings.length, 226, "update the measured corpus evidence intentionally");
  assert.deepEqual(maxima, { options: 2, event: 2, keys: 2 });
});

test("the inventory recognizes both static modified v-on attribute spellings", () => {
  assert.deepEqual(
    modifiedOnSpellings(`
      <button title="1 > 0" @click.stop="go" v-on:keyup.enter.prevent='go'>save</button>
      <Panel\n  @update:modelValue.once="save"\n/>
    `),
    ["@click.stop", "v-on:keyup.enter.prevent", "@update:modelValue.once"],
  );
});

test("the inventory reads escaped inline fixtures carried by source files", () => {
  const embedded = String.raw`const template = "<button @click.stop=\"go\" v-on:keyup.enter=\"go\">";`;
  assert.deepEqual(modifiedOnSpellings(embedded), ["@click.stop", "v-on:keyup.enter"]);
});

test("the inventory rejects non-attribute lookalikes", () => {
  assert.deepEqual(
    modifiedOnSpellings(`
      Contact dev@click.stop or install @scope/pkg.mod.
      macro_rules! route { (@click.stop) => {} }
      const token = "@click.stop";
      <p>text @click.stop</p>
      <div title="please use @click.stop here" data-example="v-on:keyup.enter">x</div>
      <button @[event].stop="dynamic names are outside the static classifier" />
    `),
    [],
  );
});

test("the corpus inventory excludes marked synthetic storage boundaries", () => {
  const source = `<button @click.stop="natural" />
// v-on-storage-synthetic:start
<button @click.stop.prevent.self="synthetic" />
// v-on-storage-synthetic:end`;
  assert.deepEqual(modifiedOnSpellings(source.replace(syntheticBoundary, "")), ["@click.stop"]);
});
