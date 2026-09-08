import { spawnSync } from "node:child_process";

import { repoRoot } from "../../_helpers/realworld-patch.ts";

export function processRssKiB(processId: number): number | null {
  if (process.platform === "win32") return null;
  const result = spawnSync("ps", ["-o", "rss=", "-p", String(processId)], { encoding: "utf8" });
  if (result.status !== 0) return null;
  const value = Number.parseInt(result.stdout.trim(), 10);
  return Number.isFinite(value) && value > 0 ? value : null;
}

/**
 * Resident-set total and process count for a process and every live descendant,
 * so leaked Corsa/tsgo worker sessions show up even when the `vize lsp` parent
 * remains within its own RSS budget. Unavailable on Windows, where CI relies on
 * the Linux vue-parity lane for process-tree enforcement.
 */
export function processTreeRss(rootPid: number): { totalKiB: number; processes: number } | null {
  if (process.platform === "win32") return null;
  const result = spawnSync("ps", ["-Ao", "pid=,ppid=,rss="], { encoding: "utf8" });
  if (result.status !== 0) return null;
  const children = new Map<number, number[]>();
  const rssByPid = new Map<number, number>();
  for (const line of result.stdout.trim().split("\n")) {
    const [pid, ppid, rss] = line.trim().split(/\s+/).map(Number);
    if (!Number.isSafeInteger(pid) || !Number.isSafeInteger(ppid)) continue;
    rssByPid.set(pid, Number.isFinite(rss) ? rss : 0);
    const siblings = children.get(ppid);
    if (siblings == null) {
      children.set(ppid, [pid]);
    } else {
      siblings.push(pid);
    }
  }
  if (!rssByPid.has(rootPid)) return null;
  let totalKiB = 0;
  let processes = 0;
  const stack = [rootPid];
  while (stack.length > 0) {
    const pid = stack.pop()!;
    if (rssByPid.has(pid)) {
      totalKiB += rssByPid.get(pid)!;
      processes += 1;
    }
    stack.push(...(children.get(pid) ?? []));
  }
  return { totalKiB, processes };
}

export function gitHead(): string {
  const result = spawnSync("git", ["rev-parse", "HEAD"], { cwd: repoRoot, encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : "unknown";
}
