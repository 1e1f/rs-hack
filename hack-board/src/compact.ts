#!/usr/bin/env bun
/**
 * One-shot compactor for .yah/events/*.jsonl shards.
 *
 * Reads each ticket shard in order, reconstructs per-id state by walking
 * the genesis + delta sequence (treating any pre-delta-schema `scan`
 * with a full `ticket` as a genesis), then re-emits as the minimal
 * sequence of genesis-followed-by-deltas. Empty-diff events are dropped.
 * `archived` / `disappeared` / `renamed` and any unrecognized events
 * pass through unchanged.
 *
 * Orphan shards (`_todos.jsonl`, `_renamed.jsonl`) are skipped — their
 * events have no scan-compactable structure.
 *
 * Each original shard is renamed `<shard>.jsonl.precompact` before the
 * compacted version is written. Re-running the script is safe (it
 * refuses to recurse into `.precompact` files).
 *
 * Run: `bun run hack-board/src/compact.ts`
 *      `HACK_WORKSPACE=/path/to/repo bun run hack-board/src/compact.ts`
 */

import { readdir, readFile, writeFile, rename } from "fs/promises";
import { join, resolve } from "path";

const WORKSPACE = resolve(process.env.HACK_WORKSPACE || process.cwd());
const EVENTS_DIR = join(WORKSPACE, ".yah", "events");
const TODOS_SHARD = "_todos.jsonl";
const RENAMED_SHARD = "_renamed.jsonl";

type FieldChange = { before: any; after: any };
type AnyEvent = any;

const DIFF_IGNORE = new Set(["line"]);

// Older event shards were written with absolute `file` paths, so checkouts
// of the same repo at different locations (e.g. `/ss/nt_alt` vs
// `/ss/noisetable`) produce churn events purely due to the prefix
// differing. Strip the workspace prefix on read so diffs see
// repo-relative paths.
function toRelativeFile(p: any): any {
  if (typeof p !== "string") return p;
  const prefix = WORKSPACE + "/";
  if (p.startsWith(prefix)) return p.slice(prefix.length);
  if (p === WORKSPACE) return "";
  return p;
}

function normalizeTicket(t: any): any {
  if (!t || typeof t !== "object") return t;
  if (typeof t.file === "string") {
    const rel = toRelativeFile(t.file);
    if (rel !== t.file) return { ...t, file: rel };
  }
  return t;
}

function diffTicket(before: any, after: any): Record<string, FieldChange> {
  const changes: Record<string, FieldChange> = {};
  const keys = new Set<string>([
    ...Object.keys(before ?? {}),
    ...Object.keys(after ?? {}),
  ]);
  for (const k of Array.from(keys).sort()) {
    if (DIFF_IGNORE.has(k)) continue;
    const a = before?.[k];
    const b = after?.[k];
    if (JSON.stringify(a) !== JSON.stringify(b)) {
      changes[k] = { before: a, after: b };
    }
  }
  return changes;
}

function applyChanges(
  prior: any,
  changes: Record<string, FieldChange>
): any {
  const updated = { ...prior };
  for (const [k, v] of Object.entries(changes)) {
    if (v.after === undefined) delete updated[k];
    else updated[k] = v.after;
  }
  return updated;
}

interface CompactStats {
  in: number;
  out: number;
  bytesIn: number;
  bytesOut: number;
}

async function compactShard(fileName: string): Promise<CompactStats> {
  const path = join(EVENTS_DIR, fileName);
  const raw = await readFile(path, "utf-8");
  const lines = raw.split("\n").filter((l) => l.trim().length > 0);
  const events: AnyEvent[] = [];
  for (const line of lines) {
    try {
      events.push(JSON.parse(line));
    } catch {}
  }

  // Rolling per-id state built from reading the input — used to
  // reconstruct the ticket at each point for the diff computation.
  const stateById = new Map<string, any>();
  // Tracks what we've already emitted into the output so we can decide
  // genesis vs delta vs skip.
  const lastEmittedById = new Map<string, any>();
  const out: AnyEvent[] = [];

  for (const ev of events) {
    if (ev.type === "scan" && ev.id) {
      let state: any;
      if (ev.ticket) {
        state = normalizeTicket(ev.ticket);
      } else if (ev.changes && stateById.has(ev.id)) {
        state = normalizeTicket(applyChanges(stateById.get(ev.id), ev.changes));
      } else {
        // Orphan delta (reference state we can't reconstruct). Skip
        // rather than emit a corrupt entry.
        continue;
      }
      stateById.set(ev.id, state);

      const prior = lastEmittedById.get(ev.id);
      if (!prior) {
        out.push({ t: ev.t, type: "scan", id: ev.id, ticket: state });
        lastEmittedById.set(ev.id, state);
      } else {
        const changes = diffTicket(prior, state);
        if (Object.keys(changes).length === 0) continue;
        // A delta duplicates every changed value (before AND after). When
        // many fields change at once the delta can exceed the size of a
        // fresh full-ticket snapshot — in that case emit a genesis
        // instead. Semantics are preserved because reconstruction treats
        // any `scan.ticket` as an authoritative overwrite.
        const deltaEv = { t: ev.t, type: "scan", id: ev.id, changes };
        const genesisEv = { t: ev.t, type: "scan", id: ev.id, ticket: state };
        if (JSON.stringify(deltaEv).length <= JSON.stringify(genesisEv).length) {
          out.push(deltaEv);
        } else {
          out.push(genesisEv);
        }
        lastEmittedById.set(ev.id, state);
      }
      continue;
    }

    // Pass through non-scan events, normalizing any embedded ticket
    // paths. archived / disappeared clear the id so a subsequent
    // re-appearance starts a fresh genesis.
    const passed: any = { ...ev };
    if (passed.ticket) passed.ticket = normalizeTicket(passed.ticket);
    if (passed.lastTicket) passed.lastTicket = normalizeTicket(passed.lastTicket);
    if (typeof passed.file === "string") passed.file = toRelativeFile(passed.file);
    out.push(passed);
    if (ev.type === "archived" || ev.type === "disappeared") {
      if (ev.id) {
        stateById.delete(ev.id);
        lastEmittedById.delete(ev.id);
      }
    }
  }

  const outText = out.length > 0 ? out.map((e) => JSON.stringify(e)).join("\n") + "\n" : "";
  await rename(path, path + ".precompact");
  await writeFile(path, outText);

  return {
    in: events.length,
    out: out.length,
    bytesIn: raw.length,
    bytesOut: outText.length,
  };
}

async function main() {
  let entries: string[] = [];
  try {
    entries = await readdir(EVENTS_DIR);
  } catch {
    console.error(`no ${EVENTS_DIR} — nothing to compact`);
    process.exit(0);
  }

  const shards = entries.filter(
    (f) =>
      f.endsWith(".jsonl") &&
      !f.endsWith(".jsonl.precompact") &&
      f !== TODOS_SHARD &&
      f !== RENAMED_SHARD
  );

  if (shards.length === 0) {
    console.error("no ticket shards to compact");
    process.exit(0);
  }

  const report: Array<{ shard: string; r: CompactStats }> = [];
  let totalIn = 0,
    totalOut = 0,
    totalBytesIn = 0,
    totalBytesOut = 0;
  for (const f of shards) {
    const r = await compactShard(f);
    report.push({ shard: f, r });
    totalIn += r.in;
    totalOut += r.out;
    totalBytesIn += r.bytesIn;
    totalBytesOut += r.bytesOut;
  }

  console.log("compacted:");
  for (const { shard, r } of report) {
    const dEvents = r.in > 0 ? Math.round((1 - r.out / r.in) * 100) : 0;
    const dBytes = r.bytesIn > 0 ? Math.round((1 - r.bytesOut / r.bytesIn) * 100) : 0;
    console.log(
      `  ${shard}: ${r.in} → ${r.out} events (-${dEvents}%), ` +
        `${r.bytesIn} → ${r.bytesOut} bytes (-${dBytes}%)`
    );
  }
  const dEvents =
    totalIn > 0 ? Math.round((1 - totalOut / totalIn) * 100) : 0;
  const dBytes =
    totalBytesIn > 0 ? Math.round((1 - totalBytesOut / totalBytesIn) * 100) : 0;
  console.log(
    `total: ${totalIn} → ${totalOut} events (-${dEvents}%), ${totalBytesIn} → ${totalBytesOut} bytes (-${dBytes}%)`
  );
  console.log("originals preserved as <shard>.jsonl.precompact — delete when satisfied");
}

main();
