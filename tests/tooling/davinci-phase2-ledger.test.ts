import assert from "node:assert/strict";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import {
  createCompatibilityContext,
  readCompatibilityLedger,
  validateCompatibilityLedger,
} from "../../legacy-tools/fixtures/fixture-compatibility-ledger.mjs";
import {
  assertCurrentP2_11Installment,
  assertP2_17P2_20ExitBlockers,
  assertP2_11InstallmentFiles,
  p2_11CurrentRecordEvidence,
  recordsTaskRow,
  requiredLine,
  requiredSection,
} from "./support/davinci-phase2-ledger.ts";

const docs = {
  roadmap: new URL("../../davinci-road/roadmap.md", import.meta.url),
  readme: new URL("../../davinci-road/plan/README.md", import.meta.url),
  phase: new URL("../../davinci-road/plan/phase-2.md", import.meta.url),
  tasks: new URL("../../davinci-road/plan/phase-2-tasks.md", import.meta.url),
  tasksLater: new URL("../../davinci-road/plan/phase-2-tasks-later.md", import.meta.url),
  records: new URL("../../davinci-road/plan/phase-2-records.md", import.meta.url),
  p2_9: new URL("../../davinci-road/plan/phase-2-records/p2-9.md", import.meta.url),
  p2_9_11: new URL(
    "../../davinci-road/plan/phase-2-records/p2-9/installment-11.md",
    import.meta.url,
  ),
  p2_11: new URL("../../davinci-road/plan/phase-2-records/p2-11.md", import.meta.url),
  suites: new URL("../../davinci-road/plan/test-suites.md", import.meta.url),
  devtool: new URL("../../davinci-road/devtool.md", import.meta.url),
  questions: new URL("../../davinci-road/open-questions.md", import.meta.url),
} as const;

function read(url: URL): string {
  return fs.readFileSync(url, "utf8");
}

const text = Object.fromEntries(Object.entries(docs).map(([name, url]) => [name, read(url)])) as {
  [K in keyof typeof docs]: string;
};

const completedTasks =
  "P2-1 P2-2 P2-3 P2-4 P2-5a P2-5b P2-6 P2-7 P2-8 P2-9 P2-10 P2-11 P2-12a P2-13 P2-14 P2-15 P2-18 P2-19".split(
    " ",
  );
const activeTasks: string[] = [];
const readyTasks = ["P2-12b", "P2-16"];
const openDependencyTasks = ["P2-17", "P2-20"];

function taskIndex(source: string): Map<string, boolean> {
  const entries = [
    ...source.matchAll(/^- \[(?<checked>[ x])\] \[(?<id>P2-[^\]]+)\]\([^\n]+\)/gmu),
  ].map((match) => [match.groups!.id, match.groups!.checked === "x"] as const);
  assert.equal(new Set(entries.map(([id]) => id)).size, entries.length, "duplicate P2 task id");
  return new Map(entries);
}

function currentGroup(source: string, label: string) {
  const match = new RegExp(
    `^- \\*\\*${label}: (?<count>\\d+) of (?<total>\\d+) — (?<ids>(?:none|P2-[\\s\\S]*?))\\.\\*\\*`,
    "mu",
  ).exec(source);
  assert.ok(match?.groups, `missing ${label} current-ledger group`);
  const ids =
    match.groups.ids === "none" ? [] : (match.groups.ids.match(/P2-\d+(?:[ab])?/gu) ?? []);
  assert.equal(new Set(ids).size, ids.length, `${label} contains a duplicate task`);
  const declaredCount = Number(match.groups.count);
  assert.equal(ids.length, declaredCount, `${label} count does not match its exact set`);
  return { declaredCount, ids, total: Number(match.groups.total) };
}

function taskSection(source: string, id: string): string {
  const start = new RegExp(`^## ${id} —`, "mu").exec(source)?.index;
  if (start == null) throw new Error(`missing ${id} contract`);
  const tail = source.slice(start);
  const next = /^## P2-/mu.exec(tail.slice(1))?.index;
  return next == null ? tail : tail.slice(0, next + 1);
}

function dependencySet(source: string, id: string, taskIds: string[]): string[] {
  const section = taskSection(source, id);
  const raw = /\*\*Deps:\*\* (?<deps>[\s\S]*?) \*\*Non-goals:\*\*/u.exec(section)?.groups?.deps;
  assert.ok(raw, `missing ${id} dependency clause`);
  if (raw === "all of P2-1..P2-19.") return taskIds.filter((task) => task !== "P2-20");
  return raw.match(/P2-\d+(?:[ab])?/gu) ?? [];
}

function suiteMaximum(source: string): number {
  const ids = [...source.matchAll(/^\| TS-(?<id>\d+) \|/gmu)].map((match) =>
    Number(match.groups!.id),
  );
  assert.ok(ids.length > 0, "suite registry must not be empty");
  return Math.max(...ids);
}

function assertCurrentCount(source: string, completed: number, total: number): void {
  const expected = `${completed} of ${total}`;
  if (!source.includes(expected)) throw new Error(`stale task count: expected ${expected}`);
}

function assertSuiteRange(source: string, maximum: number): void {
  const expected = `TS-1..${maximum}`;
  if (!source.includes(expected)) throw new Error(`stale suite range: expected ${expected}`);
}

test("Phase 2 current classification is exact, disjoint, and exhaustive", () => {
  const tasks = taskIndex(text.phase);
  const complete = currentGroup(text.phase, "Complete");
  const active = currentGroup(text.phase, "Active and blocked");
  const ready = currentGroup(text.phase, "Ready");
  const openDependency = currentGroup(text.phase, "Open and dependency-blocked");
  const groups = [complete, active, ready, openDependency];
  assert.equal(tasks.size, 22);
  assert.deepEqual(complete.ids, completedTasks);
  assert.deepEqual(active.ids, activeTasks);
  assert.deepEqual(ready.ids, readyTasks);
  assert.deepEqual(openDependency.ids, openDependencyTasks);
  for (const group of groups) assert.equal(group.total, tasks.size);

  for (let left = 0; left < groups.length; left += 1) {
    for (let right = left + 1; right < groups.length; right += 1) {
      assert.deepEqual(
        groups[left].ids.filter((id) => groups[right].ids.includes(id)),
        [],
        "current-ledger groups must be disjoint",
      );
    }
  }
  const covered = groups.flatMap((group) => group.ids);
  assert.equal(new Set(covered).size, tasks.size, "current-ledger coverage must not duplicate ids");
  assert.deepEqual(
    [...covered].sort(),
    [...tasks.keys()].sort(),
    "current-ledger groups must cover all 22 TODO ids",
  );
  for (const [id, checked] of tasks) assert.equal(complete.ids.includes(id), checked, id);

  for (const source of [text.roadmap, text.readme, text.phase, text.records]) {
    assertCurrentCount(source, completedTasks.length, tasks.size);
  }
});

test("dependency edges explain every open dependency classification", () => {
  const taskIds = [...taskIndex(text.phase).keys()];
  const sources = new Map([
    ["P2-12b", text.tasks],
    ["P2-16", text.tasksLater],
    ["P2-17", text.tasksLater],
    ["P2-20", text.tasksLater],
  ]);
  const expected = new Map([
    ["P2-12b", ["P2-12a", "P2-11", "P2-3"]],
    ["P2-16", ["P2-11"]],
    ["P2-17", ["P2-11", "P2-12b", "P2-13"]],
    ["P2-20", taskIds.filter((id) => id !== "P2-20")],
  ]);
  const open = new Set([...activeTasks, ...readyTasks, ...openDependencyTasks]);
  for (const id of openDependencyTasks) {
    const dependencies = dependencySet(sources.get(id)!, id, taskIds);
    assert.deepEqual(dependencies, expected.get(id), `${id} dependency set drifted`);
    assert.ok(dependencies.every((dependency) => taskIds.includes(dependency)));
    assert.ok(!dependencies.includes(id), `${id} must not depend on itself`);
    assert.ok(
      dependencies.some((dependency) => open.has(dependency)),
      `${id} is not dependency-blocked by an open task`,
    );
  }
});

test("P2-17 and P2-20 stay blocked by explicit exit-gate dependencies", () => {
  const tasks = taskIndex(text.phase);
  assertP2_17P2_20ExitBlockers(
    text.phase,
    text.tasksLater,
    [...tasks.keys()],
    tasks.get("P2-17"),
    tasks.get("P2-20"),
  );
});

test("every completion joins a merged PR to honest current evidence", () => {
  const expectedPrs = new Map([
    ["P2-1", "4452"],
    ["P2-2", "4452"],
    ["P2-3", "4452"],
    ["P2-4", "4496"],
    ["P2-5a", "4509"],
    ["P2-5b", "4509"],
    ["P2-6", "4509"],
    ["P2-7", "4502"],
    ["P2-8", "4544"],
    ["P2-9", "5367"],
    ["P2-10", "4642"],
    ["P2-11", "5860"],
    ["P2-12a", "4452"],
    ["P2-13", "4509"],
    ["P2-14", "4509"],
    ["P2-15", "4547"],
    ["P2-18", "4543"],
    ["P2-19", "4543"],
  ]);
  const rows = new Map(
    [...text.records.matchAll(/^\| (?<id>P2-[^ |]+)\s+\| \[#(?<pr>\d+)\]\([^\n]+$/gmu)].map(
      (match) => [match.groups!.id, match.groups!.pr],
    ),
  );
  assert.deepEqual(rows, expectedPrs);
  assert.match(text.records, /current evidence/);
  const p2_19 = /^\| P2-19\s+\|[^\n]+$/mu.exec(text.records)?.[0] ?? "";
  assert.match(p2_19, /Review evidence/);
  assert.match(p2_19, /p2-19\.md/);
  assert.match(p2_19, /not a transport implementation witness/);
  assert.doesNotMatch(p2_19, /davinci-phase2-ledger/);
});

test("P2-9 records the hydrated residual completion honestly", () => {
  const p2_9Section = taskSection(text.tasks, "P2-9");
  const recordsRow = requiredLine(
    text.records,
    /^\| P2-9\s+\| \[#5367\][^\n]+$/mu,
    "P2-9 current evidence row",
  );
  for (const source of [text.roadmap, text.readme, text.phase, text.records]) {
    assert.match(source, /18 of 22/);
    assert.match(source, /11\.73%/);
  }
  assert.match(text.phase, /P2-9, P2-10/);
  assert.match(p2_9Section, /41,580 files compiled/);
  assert.match(p2_9Section, /admitted=801305/);
  assert.match(p2_9Section, /legacy_total=106532/);
  assert.match(p2_9Section, /residual \*\*11\.73%\*\*/);
  assert.match(recordsRow, /#5367/);
  assert.match(recordsRow, /41,580 compiled files/);
  assert.match(recordsRow, /11\.73% residual/);
  assert.match(text.p2_9, /Installment 11/);
  assert.match(text.p2_9, /legacy expression subtree/);
  assert.match(text.p2_9_11, /scope=canonical/);
  assert.match(text.p2_9_11, /submodules=146/);
  assert.match(text.p2_9_11, /files=41580 compiled=41580/);
  assert.match(text.p2_9_11, /admitted=801305 legacy_total=106532/);
  assert.match(text.p2_9_11, /admitted_pct=88\.27/);
  assert.match(text.p2_9_11, /= 11\.73%/);
  assert.match(text.p2_9_11, /does not\s+delete `steps\/expression\/`/);
  assert.doesNotMatch(text.p2_9_11, /Unknown this run/);
});

test("P2-11 records current installments without presenting stale remainders", () => {
  const currentEvidence = {
    roadmap: requiredSection(
      text.roadmap,
      /^\*\*Current execution ledger/mu,
      /^\*\*Exit gate:/mu,
      "roadmap current execution ledger",
    ),
    readme: requiredLine(text.readme, /^\| \[phase-2\.md\][^\n]+$/mu, "plan README phase 2 row"),
    tasks: requiredSection(
      text.tasks,
      /^\*\*Current series evidence/mu,
      /^\*\*Steps:\*\*/mu,
      "P2-11 current series evidence",
    ),
    records: recordsTaskRow(text.records, "P2-11"),
    p2_11: p2_11CurrentRecordEvidence(text.p2_11),
  };
  for (const [label, source] of Object.entries(currentEvidence))
    assertCurrentP2_11Installment(source, label);
  for (const pr of [
    4933, 5011, 5178, 5183, 5198, 5200, 5203, 5205, 5207, 5210, 5212, 5214, 5359, 5360, 4862, 5363,
    5373, 5376, 5379, 5380, 5381, 5386, 5387, 5390, 5391, 5396, 5398, 5399, 5400, 5401, 5404, 5405,
    5467, 5515, 5520, 5531, 5536, 5533, 5543, 5552, 5562, 5563, 5564, 5565, 5566, 5567, 5568, 5569,
    5572, 5573, 5576, 5582, 5583, 5585, 5586,
  ]) {
    assert.match(text.p2_11, new RegExp(`#${pr}`, "u"));
  }
  for (const pr of [4919, 4921, 4924, 4927, 4929])
    assert.match(text.p2_11, new RegExp(`#${pr}`, "u"));
  assert.doesNotMatch(text.p2_11, /Current named remainder \(after #5531\)/);
  assert.doesNotMatch(text.p2_11, /comparison-count blocker/u);
  assert.doesNotMatch(text.p2_11, /remaining patch-flag (?:equivalence )?program/u);
  assert.doesNotMatch(text.p2_11, /dynamic-argument bind names \/ modifiers/);
  assert.doesNotMatch(text.p2_11, /\*\*malformed slot fact gaps\*\*/);
  assertP2_11InstallmentFiles();
});
test("suite registry debt and the TS-52 transport decision stay resolved", () => {
  const maximum = suiteMaximum(text.suites);
  const p2Suites = requiredLine(text.suites, /^\| P2\s+\|[^\n]+$/mu, "P2 suite map row");
  assert.equal(maximum, 52);
  assertSuiteRange(text.readme, maximum);
  assert.match(text.suites, /^\| TS-25 \|[^\n]*P2-9[^\n]*P2-11[^\n]*P2-16/mu);
  assert.match(text.suites, /^\| TS-52 \|[^\n]*Spolvero feed payload/mu);
  assert.match(p2Suites, /TS-11 empty for DOM and JSX/);
  assert.match(taskSection(text.tasksLater, "P2-16"), /JSX corpus projects' rows in TS-11 empty/);
  assert.match(text.phase, /\*\*Registry maintenance this phase owes\*\*/);
  assert.match(text.phase, /P2-18 must add the entry in its own PR/);
  assert.match(text.phase, /Current resolution \(2026-08-25\): registry maintenance is resolved/);
  assert.match(text.phase, /recorded deviation from the re-cut's\s+own-PR condition/);
  assert.match(text.devtool, /Decided: document over JSON-RPC/);
  assert.match(text.questions, /DevTool protocol[^\n]*Transport/);
});

test("corpus project counts come from the executable compatibility inventory", () => {
  const validated = validateCompatibilityLedger(
    readCompatibilityLedger(),
    createCompatibilityContext(),
  );
  const ecosystem = [...validated.fixtureMap.values()].filter((fixture) =>
    fixture.memberships.includes("ecosystem"),
  ).length;
  assert.equal(validated.fixtureMap.size, 146);
  assert.equal(ecosystem, 142);
  for (const source of [text.roadmap, text.readme, text.phase, text.records]) {
    assert.match(source, /146 gitlinks/);
    assert.match(source, /142 ecosystem\s+projects/);
  }
});
test("validator rejects a stale task count or suite range", () => {
  const tasks = taskIndex(text.phase);
  const maximum = suiteMaximum(text.suites);
  assert.throws(
    () => assertCurrentCount(text.readme.replace("18 of 22", "17 of 22"), 18, tasks.size),
    /stale task count: expected 18 of 22/,
  );
  assert.throws(
    () => assertSuiteRange(text.readme.replace("TS-1..52", "TS-1..51"), maximum),
    /stale suite range: expected TS-1\.\.52/,
  );
});

test("local links in the reconciled ledger exist", () => {
  for (const key of Object.keys(docs) as Array<keyof typeof docs>) {
    for (const match of text[key].matchAll(/\]\((?<target>[^)]+)\)/gu)) {
      const target = match.groups!.target.split("#", 1)[0];
      if (target === "" || /^[a-z]+:/u.test(target)) continue;
      const resolved = fileURLToPath(new URL(target, docs[key]));
      assert.ok(fs.existsSync(resolved), `${key} has a missing link: ${match.groups!.target}`);
    }
  }
});
