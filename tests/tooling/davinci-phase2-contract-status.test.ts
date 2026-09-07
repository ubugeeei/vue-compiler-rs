import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

const docs = {
  phase: new URL("../../davinci-road/plan/phase-2.md", import.meta.url),
  tasks: new URL("../../davinci-road/plan/phase-2-tasks.md", import.meta.url),
  tasksLater: new URL("../../davinci-road/plan/phase-2-tasks-later.md", import.meta.url),
  records: new URL("../../davinci-road/plan/phase-2-records.md", import.meta.url),
} as const;

function read(url: URL): string {
  return fs.readFileSync(url, "utf8");
}

const text = Object.fromEntries(Object.entries(docs).map(([name, url]) => [name, read(url)])) as {
  [K in keyof typeof docs]: string;
};

function taskIndex(source: string): Map<string, boolean> {
  const entries = [
    ...source.matchAll(/^- \[(?<checked>[ x])\] \[(?<id>P2-[^\]]+)\]\([^\n]+\)/gmu),
  ].map((match) => [match.groups!.id, match.groups!.checked === "x"] as const);
  assert.equal(new Set(entries.map(([id]) => id)).size, entries.length, "duplicate P2 task id");
  return new Map(entries);
}

function currentGroup(source: string, label: string): string[] {
  const match = new RegExp(
    `^- \\*\\*${label}: (?<count>\\d+) of (?<total>\\d+) — (?<ids>(?:none|P2-[\\s\\S]*?))\\.\\*\\*`,
    "mu",
  ).exec(source);
  assert.ok(match?.groups, `missing ${label} current-ledger group`);
  const ids =
    match.groups.ids === "none" ? [] : (match.groups.ids.match(/P2-\d+(?:[ab])?/gu) ?? []);
  assert.equal(new Set(ids).size, ids.length, `${label} current-ledger group has duplicates`);
  assert.equal(ids.length, Number(match.groups.count), `${label} current-ledger count drifted`);
  return ids;
}

function taskSection(id: string): string {
  const major = Number(/^P2-(?<major>\d+)/u.exec(id)?.groups?.major);
  const source = major >= 15 ? text.tasksLater : text.tasks;
  const start = new RegExp(`^## ${id} `, "mu").exec(source)?.index;
  if (start == null) throw new Error(`missing ${id} contract`);
  const tail = source.slice(start);
  const next = /^## P2-/mu.exec(tail.slice(1))?.index;
  return next == null ? tail : tail.slice(0, next + 1);
}

function currentEvidenceIds(source: string = text.records): string[] {
  const section = sectionBetween(
    source,
    /^## Current completion evidence /mu,
    /^## Historical task records/mu,
    "current completion evidence",
  );
  return uniqueTaskIds(
    [...section.matchAll(/^\| (?<id>P2-[^ |]+)\s+\| \[#\d+\]/gmu)].map((match) => match.groups!.id),
    "current completion evidence",
  );
}

function historicalRecordIds(source: string = text.records): string[] {
  const section = sectionBetween(
    source,
    /^## Historical task records/mu,
    /$a/mu,
    "historical task records",
  );
  return uniqueTaskIds(
    [...section.matchAll(/^\| \[(?<id>P2-[^\]]+)\]/gmu)].map((match) => match.groups!.id),
    "historical task records",
  );
}

function uniqueTaskIds(ids: string[], label: string): string[] {
  const duplicates = ids.filter((id, index) => ids.indexOf(id) !== index);
  assert.equal(
    duplicates.length,
    0,
    `${label} has duplicate task id(s): ${[...new Set(duplicates)].join(", ")}`,
  );
  return ids;
}

function sectionBetween(source: string, start: RegExp, end: RegExp, label: string): string {
  const startMatch = start.exec(source);
  assert.ok(startMatch, `missing ${label}`);
  const tail = source.slice(startMatch.index);
  const endMatch = end.exec(tail.slice(startMatch[0].length));
  if (endMatch == null) return tail;
  return tail.slice(0, startMatch[0].length + endMatch.index);
}

function recordFile(id: string): string {
  return fileURLToPath(
    new URL(`../../davinci-road/plan/phase-2-records/${id.toLowerCase()}.md`, import.meta.url),
  );
}

function duplicateLine(source: string, pattern: RegExp, label: string): string {
  const match = pattern.exec(source);
  assert.ok(match, `missing ${label}`);
  return `${source.slice(0, match.index)}${match[0]}\n${source.slice(match.index)}`;
}

test("task contracts and record files reflect the current Phase 2 status", () => {
  const tasks = taskIndex(text.phase);
  const completed = new Set([...tasks].filter(([, checked]) => checked).map(([id]) => id));
  const active = new Set(currentGroup(text.phase, "Active and blocked"));
  const ready = new Set(currentGroup(text.phase, "Ready"));
  const blocked = new Set(currentGroup(text.phase, "Open and dependency-blocked"));

  assert.deepEqual(new Set(currentEvidenceIds()), completed);
  assert.deepEqual(new Set(historicalRecordIds()), new Set([...completed, ...active]));

  for (const [id, checked] of tasks) {
    const section = taskSection(id);
    if (checked) {
      assert.match(section, new RegExp(`phase-2-records/${id.toLowerCase()}\\.md`, "u"));
      assert.ok(fs.existsSync(recordFile(id)), `${id} needs an individual record file`);
      continue;
    }

    assert.doesNotMatch(section, /^\*\*Landed /mu, `${id} must not claim landed status`);
    if (active.has(id)) {
      assert.match(section, /^\*\*Current series evidence/mu, `${id} needs series evidence`);
      assert.ok(fs.existsSync(recordFile(id)), `${id} series record file must exist`);
    }
    if (ready.has(id)) {
      assert.ok(!fs.existsSync(recordFile(id)), `${id} must not get a record file before landing`);
    }
    if (blocked.has(id)) {
      assert.ok(!fs.existsSync(recordFile(id)), `${id} must not get a record file before landing`);
    }
  }
});

test("completion evidence tables reject duplicate task ids", () => {
  const duplicatedCurrent = duplicateLine(
    text.records,
    /^\| P2-1\s+\| \[#4452\][^\n]+$/mu,
    "P2-1 current evidence row",
  );
  const duplicatedHistorical = duplicateLine(
    text.records,
    /^\| \[P2-1\]\([^\n]+$/mu,
    "P2-1 historical records row",
  );

  assert.throws(
    () => currentEvidenceIds(duplicatedCurrent),
    /current completion evidence has duplicate task id\(s\): P2-1/u,
  );
  assert.throws(
    () => historicalRecordIds(duplicatedHistorical),
    /historical task records has duplicate task id\(s\): P2-1/u,
  );
});
