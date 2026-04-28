//! @yah:ticket(R002-T1, "P2: per-relay event shards + content-hash scan diff")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R002)
//! @yah:handoff("P2 implemented end-to-end. .yah/events.jsonl replaced by per-relay shards .yah/events/<id>.jsonl. New 'scan' event type keyed on FNV-1a 64 hash of canonical ticket JSON (line field excluded). Legacy log auto-migrates on first serve; preserves original timestamps and dedupes consecutive same-hash scans. Disappeared detection rewritten to walk shard tails. diffTicket / diffAndLog / snapshot / replaySnapshot all removed. rs-hack-arch/src/status.rs::scan_disappeared now reads the sharded layout (with legacy fallback) and sorts by timestamp across shards. Smoke tests: fresh workspace creates shards on first scan; legacy-workspace migration bucket-writes correctly with preserved timestamps; re-scan emits zero new events when nothing changed; orphan todos land in _todos.jsonl. Dogfooded against this repo: 18 legacy events migrated into R001/R002 shards.")
//! @yah:verify("cargo test -p rs-hack-arch status — passes, new sharded + legacy + prefers-shards tests")
//! @yah:verify("Smoke: HACK_WORKSPACE=<new> bun run hack-board/src/server.ts; check .yah/events/ contains per-relay files and that second run emits no new events")
//! @yah:verify("Legacy .yah/events.jsonl was migrated to .yah/events.jsonl.legacy; this repo's real workspace did so successfully")
//! @arch:see(architecture/multi-worktree-sync.md)

/**
 * hack-board server
 *
 * Serves the kanban UI and provides a real-time ticket feed:
 * - GET /              → static UI
 * - GET /api/tickets   → current ticket JSON
 * - GET /api/events    → SSE stream of ticket updates
 * - GET /api/archive   → events filtered to type:"archived"
 * - GET /api/history   → full event log
 * - POST /api/nudge    → manual re-scan trigger
 *
 * File watcher (Bun fs.watch) + UDP listener both trigger re-scans.
 * Source files are the single source of truth for live board state.
 * `.yah/events.jsonl` is a derivative audit log written on every rescan:
 * created / modified / archived / disappeared. On startup the log is
 * replayed into memory so a first-scan diff can catch tickets that were
 * clobbered while the server was down.
 */

import { watch, readFileSync } from "fs";
import { appendFile } from "fs/promises";
import { createSocket } from "dgram";
import { extname, join, resolve } from "path";
import { $ } from "bun";

// ── Config ──────────────────────────────────────────────────────────────

const WORKSPACE = resolve(process.env.HACK_WORKSPACE || process.cwd());

// Port allocation: workspace path → pair of adjacent ports in [3333, 3998].
// 333 slots × 2 ports each. HTTP = base, UDP = base+1. Env vars override.
function hashStr(s: string): number {
  let h = 5381;
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) | 0;
  return h >>> 0;
}
const slot = hashStr(WORKSPACE) % 333;
const PORT = parseInt(process.env.HACK_PORT || String(3333 + slot * 2));
const UDP_PORT = parseInt(process.env.HACK_UDP_PORT || String(3334 + slot * 2));
// Prefer local dev build, fall back to installed rs-hack
const RS_HACK = process.env.RS_HACK_BIN || (() => {
  const devBin = join(WORKSPACE, "target", "debug", "rs-hack");
  try {
    const stat = Bun.file(devBin);
    // We can't synchronously check existence in all Bun versions,
    // so just try the dev path first at runtime
    return devBin;
  } catch {
    return "rs-hack";
  }
})();
const DEBOUNCE_MS = 300;

// ── Ticket Scanner ──────────────────────────────────────────────────────

interface Ticket {
  id: string;
  title: string;
  item_type: "ticket" | "relay";
  kind?: string;
  status: string;
  assignee?: string;
  phase?: string;
  parent?: string;
  severity?: string;
  handoff?: string[];
  next_steps?: string[];
  cleanup?: string[];
  verify?: string[];
  depends_on: string[];
  see_also: string[];
  /** Convenience alias for `files[0].path` — the lex-first location. */
  file: string;
  /** Convenience alias for `files[0].line`. */
  line: number;
  /**
   * Always-on array of every source location declaring this ID. Sorted
   * by `(path, line)`. Length 1 is the common case; length > 1 is a
   * smell to resolve (Rule11) — see `conflicts` for any disagreeing
   * scalar metadata between the files.
   */
  files: { path: string; line: number }[];
  /**
   * Per-field disagreement when the same ID is declared in multiple
   * files with different scalar metadata. The Ticket's top-level scalar
   * holds the lex-first value; this map exposes every observed value so
   * the divergence is loud rather than silent. Empty/absent for the
   * common case.
   */
  conflicts?: Record<string, { value: string; path: string; line: number }[]>;
  is_epic?: boolean;
  epic_status?: "active" | "closed";
}

interface Summary {
  id: string;
  ticket?: string;
  author?: string;
  timestamp: number;
  text: string;
  file: string;
  promoted: boolean;
  archived?: boolean;
  relay_id?: string;
  relay_title?: string;
}

type TodoRefMode = "reference" | "refine" | "implement" | "refactor";

interface TodoRef {
  path: string;
  /**
   * Verb describing how the agent should treat this reference when synthesizing
   * the pickup prompt:
   * - reference: read for context / research; don't modify
   * - refine:    turn this doc into relay + phased tickets (`/refine`)
   * - implement: build the thing the doc describes (new relay if none exists,
   *              otherwise continue existing)
   * - refactor:  doc/code drift — decide which side is wrong, do NOT touch
   *              tickets that are already `in-progress`
   */
  mode: TodoRefMode;
}

interface Todo {
  id: string;
  text: string;
  /** feature | bug | task — inherited when promoted to a ticket */
  kind?: string;
  /** fresh | research | refine | split | ready — what the pickup agent should do next */
  stage?: string;
  /** Attached references (doc or code paths) for refinement context */
  see?: TodoRef[];
}

let currentTickets: Ticket[] = [];
let currentSummaries: Summary[] = [];
let currentTodos: Todo[] = [];
let scanCount = 0;

const TODO_PATH = join(WORKSPACE, ".yah", "todo.md");
const EVENTS_DIR = join(WORKSPACE, ".yah", "events");
const EVENTS_LEGACY = join(WORKSPACE, ".yah", "events.jsonl");
const TODOS_SHARD = "_todos";
const RENAMED_SHARD = "_renamed";

// Per-relay event shards. Source is still the single source of truth for live
// board state; `.yah/events/*.jsonl` is a derivative audit history. One file
// per relay (or per standalone ticket). Orphan events (todos, renames) live
// in `_todos.jsonl` / `_renamed.jsonl`. See
// architecture/multi-worktree-sync.md §2 for the full design.
type EventType =
  | "scan" // first-in-shard = created; hash-changed vs tail = modified
  | "archived"
  | "disappeared"
  | "todo_created"
  | "todo_removed"
  | "todo_promoted"
  | "renamed"; // emitted by `board rebase-ids` (R002 P5)

type FieldChange = { before: any; after: any };

interface TicketEvent {
  t: number; // unix seconds
  type: EventType;
  id: string;
  // Scan events are EITHER genesis (full `ticket`) OR delta (`changes`).
  // Genesis is emitted the first time a ticket is seen (or the first time
  // after an archived/disappeared event removed it from the live set).
  // All other scans carry only changed fields.
  ticket?: Ticket; // scan genesis / archived
  changes?: Record<string, FieldChange>; // scan delta
  lastTicket?: Ticket; // disappeared
  sourceLines?: string[]; // archived
  file?: string;
  line?: number;
  todo?: Todo; // todo_*
  relay_id?: string; // todo_promoted → the in-source relay/ticket it became
  from?: string; // renamed
  to?: string; // renamed
}

// ── Diff ────────────────────────────────────────────────────────────────
//
// Compare two tickets and return only the fields whose values differ. Text
// values and arrays are treated atomically — we never emit sub-field
// diffs (no word-level or element-level deltas). `line` is excluded
// because it churns on any doc-comment insertion above an annotation.

const DIFF_IGNORE = new Set(["line"]);

// Strip `line` from any nested location records before diffing. `files`
// is `{path, line}[]` and `conflicts` is `Record<string, {value, path,
// line}[]>` — both churn on every doc-comment insertion above an
// annotation, even when nothing about the ticket actually changed.
function stripLines(v: any): any {
  if (Array.isArray(v)) return v.map(stripLines);
  if (v && typeof v === "object") {
    const out: any = {};
    for (const [k, val] of Object.entries(v)) {
      if (k === "line") continue;
      out[k] = stripLines(val);
    }
    return out;
  }
  return v;
}

// ── Path normalization ──────────────────────────────────────────────────
//
// `rs-hack board tickets -f json` emits absolute paths in `ticket.file`.
// If the same repo is checked out at multiple locations (e.g. `nt_alt` and
// `noisetable`), scanning each one in turn produces spurious `file`
// deltas purely because the absolute prefix differs. Strip the workspace
// prefix so everything downstream — priorState, diffs, event shards —
// uses repo-relative paths.

function toRelativeFile(p: string | undefined | null): string | undefined {
  if (p == null) return p ?? undefined;
  const prefix = WORKSPACE + "/";
  if (p.startsWith(prefix)) return p.slice(prefix.length);
  if (p === WORKSPACE) return "";
  return p;
}

function normalizeTicket(t: Ticket | undefined | null): Ticket | undefined {
  if (!t) return t ?? undefined;
  // Relativize every path field that rs-hack might emit as absolute
  // (depends on how the caller invoked it — `HACK_WORKSPACE` is
  // canonicalized → absolute). Without this, `files[].path` and
  // `conflicts[*].path` leaked full `/Users/leif/...` into the events
  // log, and workspace-diff between siblings (noisetable vs nt_alt)
  // showed up as spurious `changes` events on every scan.
  let changed = false;
  let next: any = t;
  const relFile = typeof t.file === "string" ? toRelativeFile(t.file) : undefined;
  if (relFile !== undefined && relFile !== t.file) {
    next = { ...next, file: relFile };
    changed = true;
  }
  if (Array.isArray(t.files)) {
    const relFiles = t.files.map((loc) => {
      const rp = toRelativeFile(loc.path);
      return rp !== undefined && rp !== loc.path ? { ...loc, path: rp } : loc;
    });
    if (relFiles.some((loc, i) => loc !== t.files![i])) {
      next = { ...next, files: relFiles };
      changed = true;
    }
  }
  if (t.conflicts) {
    const relConflicts: Record<string, { value: string; path: string; line: number }[]> = {};
    let cChanged = false;
    for (const [field, vals] of Object.entries(t.conflicts)) {
      const relVals = vals.map((v) => {
        const rp = toRelativeFile(v.path);
        return rp !== undefined && rp !== v.path ? { ...v, path: rp } : v;
      });
      if (relVals.some((v, i) => v !== vals[i])) cChanged = true;
      relConflicts[field] = relVals;
    }
    if (cChanged) {
      next = { ...next, conflicts: relConflicts };
      changed = true;
    }
  }
  return changed ? (next as Ticket) : t;
}

function diffTicket(before: Ticket, after: Ticket): Record<string, FieldChange> {
  const changes: Record<string, FieldChange> = {};
  const keys = new Set<string>([
    ...Object.keys(before as any),
    ...Object.keys(after as any),
  ]);
  for (const k of Array.from(keys).sort()) {
    if (DIFF_IGNORE.has(k)) continue;
    const a = (before as any)[k];
    const b = (after as any)[k];
    // Deep equality via canonical-stringify. Good enough for the shapes
    // we actually use (scalars, arrays of strings, flat objects). For
    // `files` / `conflicts` strip nested `line` first so line-only
    // churn doesn't surface as a delta.
    const cmpA = k === "files" || k === "conflicts" ? stripLines(a) : a;
    const cmpB = k === "files" || k === "conflicts" ? stripLines(b) : b;
    if (JSON.stringify(cmpA) !== JSON.stringify(cmpB)) {
      changes[k] = { before: a, after: b };
    }
  }
  return changes;
}

// Apply a `changes` delta to a ticket in-place (returns the updated
// ticket). Used by reconstructShardState to walk deltas forward from a
// genesis snapshot.
function applyChanges(
  prior: Ticket,
  changes: Record<string, FieldChange>
): Ticket {
  const updated: any = { ...prior };
  for (const [k, v] of Object.entries(changes)) {
    if (v.after === undefined) {
      delete updated[k];
    } else {
      updated[k] = v.after;
    }
  }
  return updated as Ticket;
}

// ── Shard routing ───────────────────────────────────────────────────────
//
// Given a ticket, which shard file does it belong to?
//
// - Compound sub-ticket (`R007-T1`, `R007-B2`, `R007-F3`) → bare relay shard (`R007.jsonl`)
// - Ticket with `@yah:parent(Rxxx)` or `@yah:parent(Rxxx-Ly)` → the bare relay
// - Bare relay (`R001`) → own shard
// - Standalone (bare F/B/T with no parent) → own shard

function bareRelayOf(id: string): string | null {
  const compound = id.match(/^(R\d+)-[BFT]\d+$/);
  if (compound) return compound[1];
  if (/^R\d+$/.test(id)) return id;
  return null;
}

function shardNameForTicket(t: Ticket): string {
  const compound = bareRelayOf(t.id);
  if (compound) return compound;
  if (t.parent) {
    const viaParent = bareRelayOf(t.parent);
    if (viaParent) return viaParent;
  }
  return t.id;
}

function shardPathFor(name: string): string {
  return join(EVENTS_DIR, `${name}.jsonl`);
}

// ── I/O primitives ──────────────────────────────────────────────────────

async function ensureEventsDir(): Promise<void> {
  await $`mkdir -p ${EVENTS_DIR}`.quiet();
}

async function appendEventToShard(
  shard: string,
  e: TicketEvent
): Promise<void> {
  await ensureEventsDir();
  await appendFile(shardPathFor(shard), JSON.stringify(e) + "\n");
  if (e.id && typeof e.t === "number") {
    const prev = lastActivity[e.id] ?? 0;
    if (e.t > prev) lastActivity[e.id] = e.t;
  }
}

// ── Last-activity index ─────────────────────────────────────────────────
// id → unix seconds of the latest event in any shard. Used to render
// "N minutes ago" on the board and to sort cards within a column by
// recency. Seeded once on startup, then incremented by appendEventToShard.

const lastActivity: Record<string, number> = {};

async function seedLastActivity(): Promise<void> {
  const shards = await listShardNames();
  if (shards.length === 0) {
    for (const ev of await readLegacyEvents()) {
      if (ev.id && typeof ev.t === "number") {
        const prev = lastActivity[ev.id] ?? 0;
        if (ev.t > prev) lastActivity[ev.id] = ev.t;
      }
    }
    return;
  }
  for (const s of shards) {
    for (const ev of await readShardLines(s)) {
      if (ev.id && typeof ev.t === "number") {
        const prev = lastActivity[ev.id] ?? 0;
        if (ev.t > prev) lastActivity[ev.id] = ev.t;
      }
    }
  }
}

async function readShardLines(name: string): Promise<TicketEvent[]> {
  try {
    const file = Bun.file(shardPathFor(name));
    if (!(await file.exists())) return [];
    const text = await file.text();
    const out: TicketEvent[] = [];
    for (const line of text.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        out.push(JSON.parse(trimmed));
      } catch {}
    }
    return out;
  } catch {
    return [];
  }
}

async function listShardNames(): Promise<string[]> {
  try {
    const { readdir } = await import("fs/promises");
    const files = await readdir(EVENTS_DIR);
    return files
      .filter((f) => f.endsWith(".jsonl"))
      .map((f) => f.replace(/\.jsonl$/, ""));
  } catch {
    return [];
  }
}

async function readLegacyEvents(): Promise<TicketEvent[]> {
  try {
    const file = Bun.file(EVENTS_LEGACY);
    if (!(await file.exists())) return [];
    const text = await file.text();
    const out: any[] = [];
    for (const line of text.split("\n")) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        out.push(JSON.parse(trimmed));
      } catch {}
    }
    return out;
  } catch {
    return [];
  }
}

/**
 * Read every event across all shards (or fall back to legacy file if no
 * shards exist yet). Used by `/api/archive`, `/api/history`, and the
 * in-memory audit replay path.
 *
 * Not cheap — O(total log size). Callers that can read a single shard
 * should do so directly instead.
 */
async function readAllEvents(): Promise<TicketEvent[]> {
  const shards = await listShardNames();
  if (shards.length === 0) return await readLegacyEvents();
  const out: TicketEvent[] = [];
  for (const s of shards) out.push(...(await readShardLines(s)));
  out.sort((a, b) => (a.t ?? 0) - (b.t ?? 0));
  return out;
}

// ── Migration: legacy events.jsonl → per-relay shards ───────────────────

interface MigrationResult {
  migrated: number;
  shards: number;
}

async function migrateLegacyEventsIfNeeded(): Promise<MigrationResult | null> {
  const legacyExists = await Bun.file(EVENTS_LEGACY).exists();
  if (!legacyExists) return null;
  // If shards already present, skip — somebody already migrated or is
  // running the new server for the first time after deletion.
  const existingShards = await listShardNames();
  if (existingShards.length > 0) return null;

  const legacy = await readLegacyEvents();

  // Walk the legacy log in order, maintaining per-id state. First time
  // we see an id → emit a genesis `scan` with the full ticket. Every
  // subsequent `modified` → emit a delta `scan` with only the changed
  // fields. Skip if the change would produce an empty delta (e.g. a
  // `line`-only legacy modification).
  const stateById = new Map<string, Ticket>();

  const buckets = new Map<string, string[]>();
  const push = (shard: string, line: string) => {
    if (!buckets.has(shard)) buckets.set(shard, []);
    buckets.get(shard)!.push(line);
  };

  for (const ev of legacy) {
    const tKind = (ev as any).type;
    if (
      tKind === "todo_created" ||
      tKind === "todo_removed" ||
      tKind === "todo_promoted"
    ) {
      push(TODOS_SHARD, JSON.stringify(ev));
      continue;
    }
    if (tKind === "renamed") {
      push(RENAMED_SHARD, JSON.stringify(ev));
      continue;
    }

    // Handle ticket-lifecycle events: compute the post-event state,
    // diff against what we last emitted for this id, and emit genesis
    // or delta accordingly.
    if (tKind === "created" || tKind === "modified" || tKind === "archived") {
      const prev = stateById.get(ev.id);
      let next: Ticket | null = null;

      if (tKind === "created" && ev.ticket) {
        next = ev.ticket;
      } else if (tKind === "modified" && (ev as any).changes && prev) {
        const updated: any = { ...prev };
        for (const [k, v] of Object.entries(
          (ev as any).changes as Record<string, { after: any }>
        )) {
          updated[k] = v.after;
        }
        next = updated as Ticket;
      } else if (tKind === "archived") {
        next = ev.ticket ?? prev ?? null;
      }

      if (tKind === "archived" && next) {
        // Archive is a distinct event, not a scan. Carry through with
        // the full ticket so the audit trail stays readable.
        const shard = shardNameForTicket(next);
        push(shard, JSON.stringify({ ...ev, ticket: next }));
        stateById.delete(ev.id);
        continue;
      }

      if (!next) continue;
      const shard = shardNameForTicket(next);

      if (!prev) {
        // Genesis
        push(
          shard,
          JSON.stringify({
            t: ev.t,
            type: "scan",
            id: ev.id,
            ticket: next,
          })
        );
      } else {
        // Delta
        const changes = diffTicket(prev, next);
        if (Object.keys(changes).length === 0) {
          // no-op (e.g. legacy change recorded only on `line`)
          stateById.set(ev.id, next);
          continue;
        }
        push(
          shard,
          JSON.stringify({
            t: ev.t,
            type: "scan",
            id: ev.id,
            changes,
          })
        );
      }
      stateById.set(ev.id, next);
      continue;
    }

    // disappeared — carry through; we don't reconstruct its shard here
    // because state is already removed on a prior event ordering.
    const t = stateById.get(ev.id) ?? ev.ticket ?? ev.lastTicket ?? null;
    const shard = t ? shardNameForTicket(t) : ev.id;
    push(shard, JSON.stringify(ev));
    if (tKind === "disappeared") stateById.delete(ev.id);
  }

  await ensureEventsDir();
  for (const [shard, lines] of buckets) {
    await Bun.write(shardPathFor(shard), lines.join("\n") + "\n");
  }

  const { rename } = await import("fs/promises");
  await rename(EVENTS_LEGACY, EVENTS_LEGACY + ".legacy");

  return { migrated: legacy.length, shards: buckets.size };
}

// ── Scan + log ──────────────────────────────────────────────────────────
//
// For each ticket, compute its hash. Read its shard's tail state (id →
// last-seen-scan-hash). If the new hash differs, append a `scan` event.
// Then: anything in priorState not in the current scan got clobbered
// (archived without an archive event, or doc-comment deletion) → emit a
// `disappeared` event.
//
// In-memory `priorState` per id. Seeded from shard reconstruction on
// startup so the first scan after server restart can detect while-down
// drift. Updated after every emit so subsequent scans diff against the
// most recent known state.

/**
 * Walk a shard's events in order and reconstruct the last-known full
 * ticket state per id. Genesis (scan with `ticket`) sets the state;
 * deltas (scan with `changes`) apply forward; archived/disappeared
 * remove the id from the live set.
 */
async function reconstructShardState(
  shardName: string
): Promise<Map<string, Ticket>> {
  const lines = await readShardLines(shardName);
  const state = new Map<string, Ticket>();
  for (const ev of lines) {
    if (!ev.id) continue;
    if (ev.type === "scan") {
      if (ev.ticket) {
        // Normalize as we read — older shards may carry absolute paths,
        // so the in-memory state always holds the repo-relative form.
        state.set(ev.id, normalizeTicket(ev.ticket)!);
      } else if (ev.changes && state.has(ev.id)) {
        const next = applyChanges(state.get(ev.id)!, ev.changes);
        state.set(ev.id, normalizeTicket(next)!);
      }
    } else if (ev.type === "archived" || ev.type === "disappeared") {
      state.delete(ev.id);
    }
  }
  return state;
}

// In-memory snapshot. Seeded by `loadPriorStateFromShards` on startup.
let priorState = new Map<string, Ticket>();
let priorTodos = new Map<string, Todo>();

async function loadPriorStateFromShards(): Promise<void> {
  priorState = new Map();
  const shards = await listShardNames();
  for (const shard of shards) {
    if (shard === TODOS_SHARD || shard === RENAMED_SHARD) continue;
    const state = await reconstructShardState(shard);
    for (const [id, t] of state) priorState.set(id, t);
  }

  // Reconstruct live todos from the _todos shard: todo_created brings an
  // id into the live set; todo_removed / todo_promoted removes it.
  priorTodos = new Map();
  const todoEvents = await readShardLines(TODOS_SHARD);
  for (const ev of todoEvents) {
    if (!ev.id) continue;
    if (ev.type === "todo_created" && ev.todo) {
      priorTodos.set(ev.id, ev.todo);
    } else if (ev.type === "todo_removed" || ev.type === "todo_promoted") {
      priorTodos.delete(ev.id);
    }
  }
}

async function scanAndLog(current: Ticket[]): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  // Same-id-in-multiple-files merge happens upstream in
  // `rs-hack-arch::ticket::TicketBoard::from_annotations`: a stable
  // sort by `(file, line)` picks the canonical occurrence and a `files`
  // array surfaces the smell. `current` is therefore already one entry
  // per id and stable across scans.
  const currentIds = new Set(current.map((t) => t.id));

  // 1) For each current ticket: emit genesis if unseen, delta if changed,
  //    skip if no change.
  for (const t of current) {
    const prior = priorState.get(t.id);
    const shard = shardNameForTicket(t);
    if (!prior) {
      await appendEventToShard(shard, {
        t: now,
        type: "scan",
        id: t.id,
        ticket: t,
      });
      priorState.set(t.id, t);
      continue;
    }
    const changes = diffTicket(prior, t);
    if (Object.keys(changes).length === 0) continue;
    await appendEventToShard(shard, {
      t: now,
      type: "scan",
      id: t.id,
      changes,
    });
    priorState.set(t.id, t);
  }

  // 2) Disappeared: anything in priorState not in the current source scan.
  for (const [id, lastTicket] of Array.from(priorState)) {
    if (currentIds.has(id)) continue;
    const shard = shardNameForTicket(lastTicket);
    await appendEventToShard(shard, {
      t: now,
      type: "disappeared",
      id,
      lastTicket,
    });
    priorState.delete(id);
  }
}

async function scanAndLogTodos(current: Todo[]): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const currentSet = new Set(current.map((t) => t.id));

  for (const t of current) {
    if (!priorTodos.has(t.id)) {
      await appendEventToShard(TODOS_SHARD, {
        t: now,
        type: "todo_created",
        id: t.id,
        todo: t,
      });
      priorTodos.set(t.id, t);
    }
  }
  for (const id of Array.from(priorTodos.keys())) {
    if (!currentSet.has(id)) {
      await appendEventToShard(TODOS_SHARD, {
        t: now,
        type: "todo_removed",
        id,
      });
      priorTodos.delete(id);
    }
  }
}

/**
 * Column buckets as surfaced on the UI — each maps to one or more status
 * values. Dragging to a bucket writes the canonical value shown here.
 */
const BUCKET_TO_STATUS: Record<string, string> = {
  open: "open",
  active: "in-progress",
  handoff: "handoff",
  review: "review",
};
const STATUS_TO_BUCKET: Record<string, string> = {
  open: "open",
  claimed: "active",
  "in-progress": "active",
  handoff: "handoff",
  review: "review",
  done: "review",
};
/**
 * Allowed UI transitions. Keys are source buckets; values are the target
 * buckets the user may drag the card into. open → active is the only way
 * to reach active from the top; active → open is the admin correction
 * for when an agent leaves a ticket stuck after abandoning it.
 */
const TRANSITIONS: Record<string, string[]> = {
  open: ["active"],
  active: ["open", "handoff", "review"],
  handoff: ["active", "review"],
  review: ["handoff"],
};

/**
 * Rewrite the `@yah:status(...)` line inside the contiguous doc-comment
 * block surrounding `lineNum`. If no status line exists, insert one
 * immediately after the `@yah:ticket/relay(...)` declaration (so the
 * order inside the block stays natural).
 */
/**
 * Build a review-mode prompt for a ticket awaiting sign-off. Distinct from
 * the pickup prompt: the reviewer's job is to verify completeness, not
 * claim the work. Runs entirely in TS so we don't depend on a specialized
 * Rust flag.
 */
function synthesizeReviewPrompt(t: Ticket, recentSummaries: Summary[]): string {
  const lines: string[] = [];
  lines.push(`# Review: ${t.id} — ${t.title}`);
  lines.push("");
  lines.push("## Context");
  lines.push("");
  lines.push(`\`${t.id}\` is **awaiting review**.`);
  if (t.assignee) lines.push(`- assignee: ${t.assignee}`);
  lines.push(`- source: \`${t.file}:${t.line}\``);
  if (t.phase) lines.push(`- phase: ${t.phase}`);
  if (t.parent) lines.push(`- parent relay: ${t.parent}`);
  lines.push("");

  if (t.handoff && t.handoff.length > 0) {
    lines.push("## What the previous agent said they finished");
    lines.push("");
    if (t.handoff.length === 1) {
      lines.push(t.handoff[0]);
    } else {
      for (const h of t.handoff) lines.push(`- ${h}`);
    }
    lines.push("");
  }

  if (t.next_steps && t.next_steps.length > 0) {
    lines.push("## Stated remaining work (from @yah:next)");
    lines.push("");
    for (const n of t.next_steps) lines.push(`- ${n}`);
    lines.push("");
    lines.push(
      "If items above are still unfinished, the ticket is probably **not** ready for review — send back to handoff."
    );
    lines.push("");
  }

  if (t.verify && t.verify.length > 0) {
    lines.push("## Verification commands");
    lines.push("");
    lines.push("Run each of these. They must all succeed:");
    lines.push("");
    for (const v of t.verify) {
      lines.push("```bash");
      lines.push(v);
      lines.push("```");
    }
    lines.push("");
  } else {
    lines.push("## Verification commands");
    lines.push("");
    lines.push(
      "⚠ No `@yah:verify(...)` was declared on this ticket. Decide how to confirm the work yourself (run tests, read diff, reproduce scenario)."
    );
    lines.push("");
  }

  if (t.see_also.length > 0) {
    lines.push("## Reference docs");
    lines.push("");
    for (const s of t.see_also) lines.push(`- \`${s}\``);
    lines.push("");
  }

  if (recentSummaries.length > 0) {
    lines.push("## Recent progress notes");
    lines.push("");
    for (const s of recentSummaries) {
      if (s.author) lines.push(`**${s.author}:**`);
      lines.push(s.text);
      lines.push("");
    }
  }

  lines.push("## Your task");
  lines.push("");
  lines.push(
    "1. Read the source at the location above — what did the agent actually change?"
  );
  lines.push(
    "2. Run every verification command. Read the output. Don't accept on faith."
  );
  lines.push("3. Decide:");
  lines.push("");
  lines.push(
    "   **Approve** → archive the ticket yourself. Because this ticket is already in `review`, agents are allowed to archive it directly:"
  );
  lines.push("");
  lines.push("   ```bash");
  lines.push(`   rs-hack board archive ${t.id}`);
  lines.push("   ```");
  lines.push("");
  lines.push(
    "   That strips the `@yah:` annotations from source and writes an `archived` event to `.yah/events/`. The snapshot stays in the shard, so the ticket can still be inspected via `rs-hack board show " +
      t.id +
      "` and unarchived if needed. No server / port lookup required."
  );
  lines.push("");
  lines.push(
    "   **Reject** → edit the source file to set `@yah:status(handoff)`, rewrite `@yah:handoff(\"...\")` with a concrete description of what still needs fixing, and replace any stale `@yah:next(\"...\")` items. The next agent picks up from there."
  );
  lines.push("");
  lines.push(
    "Do not leave a reviewed ticket sitting in `review` indefinitely — decide one way or the other. Note: the archive endpoint refuses tickets in `claimed` / `in-progress` — those must be moved to `review` (or `handoff`) first. This prompt is the only context where an agent should self-archive."
  );

  return lines.join("\n");
}

function setHackStatus(
  content: string,
  lineNum: number,
  newStatus: string
): { newContent: string; changed: boolean } {
  const lines = content.split("\n");
  const idx = lineNum - 1;
  if (idx < 0 || idx >= lines.length) {
    return { newContent: content, changed: false };
  }
  const isDoc = (l: string) => /^\s*\/\/[!/]/.test(l);

  let start = idx;
  while (start > 0 && isDoc(lines[start - 1])) start--;
  let end = idx;
  while (end < lines.length - 1 && isDoc(lines[end + 1])) end++;

  const statusRe = /^(\s*\/\/[!/])\s*@yah:status\([^)]*\)\s*$/;
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(statusRe);
    if (m) {
      lines[i] = `${m[1]} @yah:status(${newStatus})`;
      return { newContent: lines.join("\n"), changed: true };
    }
  }

  // Not present — insert after the defining @yah:ticket(...) or @yah:relay(...) line.
  const declRe = /^(\s*\/\/[!/])\s*@yah:(ticket|relay)\(/;
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(declRe);
    if (m) {
      lines.splice(i + 1, 0, `${m[1]} @yah:status(${newStatus})`);
      return { newContent: lines.join("\n"), changed: true };
    }
  }
  return { newContent: content, changed: false };
}

/**
 * Per-extension rules for finding the contiguous "annotation block"
 * around a ticket's defining line, and for spotting the `@yah:` lines
 * inside it. Mirrors the extractor's prefix table in
 * `rs-hack-arch/src/extract.rs::line_extract`.
 */
interface AnnotationStripRules {
  isBlockLine: (line: string) => boolean;
  isHackLine: (line: string) => boolean;
}

function annotationStripRulesFor(filePath: string): AnnotationStripRules | null {
  switch (extname(filePath).toLowerCase()) {
    case ".rs":
      return {
        isBlockLine: (l) => /^\s*\/\/[!/]/.test(l),
        isHackLine: (l) => /^\s*\/\/[!/]\s*@yah:/.test(l),
      };
    case ".ts":
    case ".tsx":
    case ".js":
    case ".jsx":
      // `//`, `//!`, `///`, `/**`, `/*`, or `*` (JSDoc body).
      return {
        isBlockLine: (l) => /^\s*(\/\/|\/\*|\*)/.test(l),
        isHackLine: (l) =>
          /^\s*(\/\/[!/]?\s*@yah:|\*\s*@yah:|\/\*\*?\s*@yah:)/.test(l),
      };
    case ".md":
      // No prefix; block bounded by blank lines.
      return {
        isBlockLine: (l) => l.trim() !== "",
        isHackLine: (l) => /^\s*@yah:/.test(l),
      };
    case ".toml":
    case ".yaml":
    case ".yml":
      return {
        isBlockLine: (l) => /^\s*#/.test(l),
        isHackLine: (l) => /^\s*#\s*@yah:/.test(l),
      };
    default:
      return null;
  }
}

/**
 * Remove `@yah:` annotation lines belonging to the ticket defined at
 * `lineNum`. Walks up/down across the contiguous annotation block (per
 * the per-extension rules above) and strips only `@yah:` lines —
 * `@arch:` and surrounding doc text are preserved.
 *
 * Returns `removed: []` for unsupported extensions or when the line is
 * out of range; the caller surfaces that as a 500 ("no annotations
 * found").
 */
function stripHackAnnotations(
  content: string,
  lineNum: number,
  filePath: string
): { newContent: string; removed: string[] } {
  const rules = annotationStripRulesFor(filePath);
  if (!rules) {
    return { newContent: content, removed: [] };
  }

  const lines = content.split("\n");
  const idx = lineNum - 1;
  if (idx < 0 || idx >= lines.length) {
    return { newContent: content, removed: [] };
  }

  let start = idx;
  while (start > 0 && rules.isBlockLine(lines[start - 1])) start--;
  let end = idx;
  while (end < lines.length - 1 && rules.isBlockLine(lines[end + 1])) end++;

  const removed: string[] = [];
  const result: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (i >= start && i <= end && rules.isHackLine(lines[i])) {
      removed.push(lines[i]);
      continue;
    }
    result.push(lines[i]);
  }
  return { newContent: result.join("\n"), removed };
}

function parseTodos(content: string): Todo[] {
  const todos: Todo[] = [];
  const blocks = content.split(/^## +/m).slice(1);
  const tagPattern = /^\s*(kind|stage|see):\s*(.+?)\s*$/;
  // `see:` accepts "see: <mode> <path>" or legacy "see: <path>" (mode = reference)
  const seePattern =
    /^(reference|refine|implement|refactor)\s+(.+)$/;
  for (const block of blocks) {
    const lines = block.split("\n");
    const id = lines[0].trim();
    if (!id) continue;
    const bodyLines: string[] = [];
    const see: TodoRef[] = [];
    let kind: string | undefined;
    let stage: string | undefined;
    for (const line of lines.slice(1)) {
      const m = line.match(tagPattern);
      if (m) {
        const key = m[1];
        const val = m[2];
        if (key === "kind") kind = val;
        else if (key === "stage") stage = val;
        else if (key === "see") {
          const sm = val.match(seePattern);
          if (sm) {
            see.push({ mode: sm[1] as TodoRefMode, path: sm[2].trim() });
          } else {
            see.push({ mode: "reference", path: val });
          }
        }
      } else {
        bodyLines.push(line);
      }
    }
    const text = bodyLines.join("\n").trim();
    if (text || see.length > 0) {
      todos.push({
        id,
        text,
        kind: kind || undefined,
        stage: stage || undefined,
        see: see.length > 0 ? see : undefined,
      });
    }
  }
  return todos;
}

function serializeTodos(todos: Todo[]): string {
  let out = "# Todos\n\n";
  for (const t of todos) {
    out += `## ${t.id}\n`;
    if (t.kind) out += `kind: ${t.kind}\n`;
    if (t.stage) out += `stage: ${t.stage}\n`;
    out += `${t.text}\n`;
    for (const s of t.see || []) out += `see: ${s.mode} ${s.path}\n`;
    out += "\n";
  }
  return out;
}

async function scanTodos(): Promise<Todo[]> {
  try {
    const file = Bun.file(TODO_PATH);
    if (!(await file.exists())) return [];
    const content = await file.text();
    return parseTodos(content);
  } catch {
    return [];
  }
}

function newTodoId(): string {
  return `T-${Date.now().toString(36)}`;
}

async function scanTickets(): Promise<Ticket[]> {
  try {
    const result = await $`${RS_HACK} board tickets -f json -p ${WORKSPACE}`
      .text();
    const raw = JSON.parse(result) as Ticket[];
    const tickets = raw.map((t) => normalizeTicket(t) ?? t);
    scanCount++;
    return tickets;
  } catch (e) {
    console.error(`[scan] failed:`, e);
    return currentTickets; // keep last good state
  }
}

async function scanSummaries(): Promise<Summary[]> {
  const dir = join(WORKSPACE, ".yah", "summaries");
  try {
    const glob = new Bun.Glob("*.md");
    const summaries: Summary[] = [];
    for await (const path of glob.scan(dir)) {
      try {
        const content = await Bun.file(join(dir, path)).text();
        const summary = parseSummary(path, content);
        if (summary) summaries.push(summary);
      } catch {}
    }
    summaries.sort((a, b) => b.timestamp - a.timestamp);
    return summaries;
  } catch {
    return [];
  }
}

function parseSummary(
  filename: string,
  content: string
): Summary | null {
  const id = filename.replace(/\.md$/, "");
  if (!content.startsWith("---\n")) {
    return {
      id,
      timestamp: 0,
      text: content.trim(),
      file: filename,
      promoted: false,
    };
  }
  const endIdx = content.indexOf("---\n", 4);
  if (endIdx === -1) return null;

  const frontmatter = content.slice(4, endIdx);
  const body = content.slice(endIdx + 4).trim();

  const fm: Record<string, string> = {};
  for (const line of frontmatter.split("\n")) {
    const colonIdx = line.indexOf(":");
    if (colonIdx > 0) {
      fm[line.slice(0, colonIdx).trim()] = line.slice(colonIdx + 1).trim();
    }
  }

  return {
    id,
    ticket: fm.ticket || undefined,
    author: fm.author || undefined,
    timestamp: parseInt(fm.timestamp || "0"),
    text: body,
    file: filename,
    promoted: fm.promoted === "true",
    archived: fm.archived === "true",
    relay_id: fm.relay_id || undefined,
    relay_title: fm.relay_title || undefined,
  };
}

// ── SSE Clients ─────────────────────────────────────────────────────────

const sseClients = new Set<ReadableStreamDefaultController>();

function broadcast(tickets: Ticket[], summaries: Summary[], todos: Todo[]) {
  const data = JSON.stringify({
    tickets,
    summaries,
    todos,
    lastActivity,
  });
  const msg = `data: ${data}\n\n`;
  for (const controller of sseClients) {
    try {
      controller.enqueue(new TextEncoder().encode(msg));
    } catch {
      sseClients.delete(controller);
    }
  }
}

// ── Debounced Re-scan ───────────────────────────────────────────────────

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

function triggerRescan(reason: string) {
  if (debounceTimer) clearTimeout(debounceTimer);
  debounceTimer = setTimeout(async () => {
    console.log(`[scan] triggered by ${reason}`);
    const [tickets, summaries, todos] = await Promise.all([
      scanTickets(),
      scanSummaries(),
      scanTodos(),
    ]);
    currentTickets = tickets;
    currentSummaries = summaries;
    currentTodos = todos;
    await scanAndLog(tickets);
    await scanAndLogTodos(todos);
    broadcast(tickets, summaries, todos);
  }, DEBOUNCE_MS);
}

// ── File Watcher ────────────────────────────────────────────────────────

// Load root .gitignore once at startup so we skip re-scans for files under
// ignored dirs (target/, node_modules/, dist/, …). Stale after startup is
// fine — a .gitignore edit is rare and a server restart picks it up.
interface GitignoreRule {
  negate: boolean;
  pattern: string;
  anchored: boolean; // starts with "/"
  hasSlash: boolean; // pattern contains "/" (after anchor strip)
}

function loadGitignore(root: string): GitignoreRule[] {
  let text: string;
  try {
    text = readFileSync(join(root, ".gitignore"), "utf8");
  } catch {
    return [];
  }
  const rules: GitignoreRule[] = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const negate = line.startsWith("!");
    let pattern = negate ? line.slice(1) : line;
    const anchored = pattern.startsWith("/");
    if (anchored) pattern = pattern.slice(1);
    if (pattern.endsWith("/")) pattern = pattern.slice(0, -1);
    rules.push({ negate, pattern, anchored, hasSlash: pattern.includes("/") });
  }
  return rules;
}

function globSegment(pattern: string, text: string): boolean {
  // Minimal glob: * matches anything except "/", ? matches one char.
  const re = new RegExp(
    "^" +
      pattern
        .replace(/[.+^${}()|[\]\\]/g, "\\$&")
        .replace(/\*/g, "[^/]*")
        .replace(/\?/g, "[^/]") +
      "$",
  );
  return re.test(text);
}

function isIgnored(path: string, rules: GitignoreRule[]): boolean {
  const segments = path.split("/");
  let ignored = false;
  for (const rule of rules) {
    let matched = false;
    const { pattern, anchored, hasSlash } = rule;
    if (anchored) {
      if (hasSlash) {
        matched = path === pattern || path.startsWith(pattern + "/");
      } else if (pattern.includes("*") || pattern.includes("?")) {
        matched = !path.includes("/") && globSegment(pattern, path);
      } else {
        matched = path === pattern || path.startsWith(pattern + "/");
      }
    } else if (hasSlash) {
      matched =
        path === pattern ||
        path.startsWith(pattern + "/") ||
        path.includes("/" + pattern + "/") ||
        path.endsWith("/" + pattern);
    } else {
      // bare name — match any path segment
      matched = pattern.includes("*") || pattern.includes("?")
        ? segments.some((s) => globSegment(pattern, s))
        : segments.includes(pattern);
    }
    if (matched) ignored = !rule.negate;
  }
  return ignored;
}

const gitignoreRules = loadGitignore(WORKSPACE);
console.log(`[watch] ${WORKSPACE} (${gitignoreRules.length} gitignore rules)`);
const watcher = watch(WORKSPACE, { recursive: true }, (event, filename) => {
  if (
    filename &&
    (filename.endsWith(".rs") ||
      filename.includes(".yah/summaries/") ||
      filename.endsWith(".yah/todo.md") ||
      filename === ".yah/todo.md") &&
    !isIgnored(filename, gitignoreRules)
  ) {
    triggerRescan(`fs:${filename}`);
  }
});

// ── UDP Listener (fire-and-forget nudge from rs-hack) ───────────────────

const udp = createSocket("udp4");
udp.on("message", (msg) => {
  triggerRescan(`udp:${msg.toString().trim()}`);
});
udp.on("error", (err) => {
  console.error(`[udp] error:`, err);
});
udp.bind(UDP_PORT, "127.0.0.1", () => {
  console.log(`[udp] listening on 127.0.0.1:${UDP_PORT}`);
});

// ── HTTP Server ─────────────────────────────────────────────────────────

const publicDir = join(import.meta.dir, "..", "public");

const server = Bun.serve({
  port: PORT,
  async fetch(req) {
    const url = new URL(req.url);

    // API: get current tickets
    if (url.pathname === "/api/tickets") {
      return Response.json(currentTickets);
    }

    // API: get summaries
    if (url.pathname === "/api/summaries") {
      return Response.json(currentSummaries);
    }

    // API: todos — list
    if (url.pathname === "/api/todos" && req.method === "GET") {
      return Response.json(currentTodos);
    }

    // API: todos — add
    if (url.pathname === "/api/todos" && req.method === "POST") {
      try {
        const body = (await req.json()) as {
          text?: string;
          kind?: string;
          stage?: string;
          see?: Array<{ path: string; mode?: string }>;
        };
        const text = (body.text || "").trim();
        if (!text) {
          return Response.json(
            { error: "text required" },
            { status: 400 }
          );
        }
        const todos = await scanTodos();
        const id = newTodoId();
        const validModes: TodoRefMode[] = [
          "reference",
          "refine",
          "implement",
          "refactor",
        ];
        const seeArr: TodoRef[] = (body.see || [])
          .map((s) => {
            const path = (s.path || "").trim();
            const mode = (validModes.includes(
              (s.mode || "reference") as TodoRefMode
            )
              ? s.mode
              : "reference") as TodoRefMode;
            return { path, mode };
          })
          .filter((s) => s.path.length > 0);
        todos.push({
          id,
          text,
          kind: body.kind?.trim() || undefined,
          stage: body.stage?.trim() || undefined,
          see: seeArr.length > 0 ? seeArr : undefined,
        });
        await $`mkdir -p ${join(WORKSPACE, ".yah")}`.quiet();
        await Bun.write(TODO_PATH, serializeTodos(todos));
        triggerRescan("api:todo-add");
        return Response.json({ ok: true, id });
      } catch (e: any) {
        return Response.json(
          { error: `Add failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: files — search workspace for files by extension, filtered by
    // substring. Used by the TodoForm ref-picker. Returns workspace-relative
    // paths, excludes the usual noise dirs.
    if (url.pathname === "/api/files" && req.method === "GET") {
      const q = (url.searchParams.get("q") || "").toLowerCase();
      const ext = (url.searchParams.get("ext") || "md")
        .replace(/^\./, "")
        .replace(/[^a-z0-9]/gi, "");
      const limit = Math.min(
        100,
        parseInt(url.searchParams.get("limit") || "30")
      );
      const exclude = [
        "node_modules",
        "target",
        "dist",
        "build",
        ".git",
        ".next",
        ".yah",
      ];
      const matches: string[] = [];
      try {
        const glob = new Bun.Glob(`**/*.${ext}`);
        for await (const path of glob.scan({ cwd: WORKSPACE })) {
          if (
            exclude.some(
              (d) =>
                path === d ||
                path.startsWith(d + "/") ||
                path.includes("/" + d + "/")
            )
          ) {
            continue;
          }
          if (!q || path.toLowerCase().includes(q)) {
            matches.push(path);
            if (matches.length >= limit) break;
          }
        }
      } catch {}
      matches.sort((a, b) => {
        // Prefer shorter / shallower paths when equally matchy
        const da = a.split("/").length;
        const db = b.split("/").length;
        return da - db || a.localeCompare(b);
      });
      return Response.json(matches);
    }

    // API: todos — promote (agent converted todo into in-source relay/ticket)
    if (
      url.pathname.startsWith("/api/todos/") &&
      url.pathname.endsWith("/promote") &&
      req.method === "POST"
    ) {
      const parts = url.pathname.split("/");
      const id = decodeURIComponent(parts[3] || "");
      try {
        const body = (await req
          .json()
          .catch(() => ({}))) as { relay_id?: string };
        const todos = await scanTodos();
        const target = todos.find((t) => t.id === id);
        if (!target) {
          return Response.json(
            { error: `Todo '${id}' not found` },
            { status: 404 }
          );
        }
        await Bun.write(
          TODO_PATH,
          serializeTodos(todos.filter((t) => t.id !== id))
        );
        await appendEventToShard(TODOS_SHARD, {
          t: Math.floor(Date.now() / 1000),
          type: "todo_promoted",
          id,
          todo: target,
          relay_id: body.relay_id,
        });
        // The next scan's tail-read of _todos.jsonl sees todo_promoted as
        // the latest event and won't re-emit as todo_removed.
        triggerRescan("api:todo-promote");
        return Response.json({ ok: true, id, relay_id: body.relay_id });
      } catch (e: any) {
        return Response.json(
          { error: `Promote failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: todos — delete
    if (
      url.pathname.startsWith("/api/todos/") &&
      req.method === "DELETE"
    ) {
      const id = decodeURIComponent(
        url.pathname.split("/").pop() || ""
      );
      try {
        const todos = (await scanTodos()).filter((t) => t.id !== id);
        await Bun.write(TODO_PATH, serializeTodos(todos));
        triggerRescan("api:todo-delete");
        return Response.json({ ok: true });
      } catch (e: any) {
        return Response.json(
          { error: `Delete failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: todo prompt — generate prompt text with hack-board usage info
    if (url.pathname.startsWith("/api/todo-prompt/")) {
      const id = decodeURIComponent(
        url.pathname.split("/").pop() || ""
      );
      const todo = currentTodos.find((t) => t.id === id);
      if (!todo) {
        return Response.json(
          { error: `Todo '${id}' not found` },
          { status: 404 }
        );
      }
      const tagLine = [
        todo.kind ? `**kind:** ${todo.kind}` : "",
        todo.stage ? `**stage:** ${todo.stage}` : "",
      ]
        .filter(Boolean)
        .join("  |  ");

      // Group refs by mode so we can emit mode-specific instructions.
      const byMode: Record<TodoRefMode, string[]> = {
        reference: [],
        refine: [],
        implement: [],
        refactor: [],
      };
      for (const r of todo.see || []) byMode[r.mode].push(r.path);

      const sections: string[] = [];
      if (byMode.implement.length > 0) {
        sections.push(
          [
            `### Implement`,
            ``,
            `Each doc below describes planned work that should be built. **If no relay exists** for a doc, create one (see \`/handoff\` → new-relay flow) with \`@yah:status(in-progress)\`. **If a relay already exists**, continue it (same R-number).`,
            ``,
            ...byMode.implement.map((p) => `- \`${p}\``),
          ].join("\n")
        );
      }
      if (byMode.refine.length > 0) {
        sections.push(
          [
            `### Refine`,
            ``,
            `Each doc below is a plan that should become a relay + phased tickets. Run \`/refine\`: pick an R-number, create tickets with \`@yah:phase(Pn)\`, then claim P1 by setting \`@yah:status(in-progress)\`.`,
            ``,
            ...byMode.refine.map((p) => `- \`${p}\``),
          ].join("\n")
        );
      }
      if (byMode.refactor.length > 0) {
        sections.push(
          [
            `### Refactor — doc / code drift`,
            ``,
            `Each item below is a place where the doc and the code have diverged. Read both. Decide whether the doc is wrong, the code is wrong, or both, and make the minimal fix. Create a relay only if the scope warrants one.`,
            ``,
            `**⚠ Do not modify any ticket or relay currently in \`in-progress\` state.** Only touch items in \`open\` or \`review\` — an \`in-progress\` item has an active agent you'd be stomping on.`,
            ``,
            ...byMode.refactor.map((p) => `- \`${p}\``),
          ].join("\n")
        );
      }
      if (byMode.reference.length > 0) {
        sections.push(
          [
            `### Reference (research / planning)`,
            ``,
            `Read for context. These docs describe the larger picture this todo sits inside. Before making changes, consider entering plan mode to structure the approach.`,
            ``,
            ...byMode.reference.map((p) => `- \`${p}\``),
          ].join("\n")
        );
      }
      const refsBlock =
        sections.length > 0 ? "\n## References\n\n" + sections.join("\n\n") : "";

      const prompt = [
        `# Todo: ${todo.id}`,
        tagLine ? `\n${tagLine}` : "",
        ``,
        todo.text,
        refsBlock,
        ``,
        `---`,
        ``,
        `## hack-board usage`,
        ``,
        `Tickets and relays live as \`@yah:\` annotations inside Rust source. The board at hack-board scans source with \`rs-hack board tickets\` and renders columns from each ticket's \`@yah:status(...)\`.`,
        ``,
        `**Key annotations:**`,
        `- \`@yah:ticket(ID, "title")\` / \`@yah:relay(ID, "title")\` — define a work item`,
        `- \`@yah:status(open|claimed|in-progress|handoff|review|done)\` — column`,
        `- \`@yah:assignee(agent:name)\` — who's working on it`,
        `- \`@yah:handoff("...")\` — message for the next agent`,
        `- \`@yah:next("...")\` — a next step (repeatable)`,
        `- \`@yah:verify("...")\` — verification step (repeatable)`,
        `- \`@arch:see(path/to/doc.md)\` — reference doc`,
        ``,
        `**To promote this todo into in-source work:**`,
        `1. Decide scope. If it's multiple tickets / phases, run \`/refine\` to generate a relay + tickets + architecture doc.`,
        `2. Otherwise pick a source file that's the natural home for the work and add \`@yah:ticket(${todo.id.replace("T-", "T")}, "...")\` annotations at the top of the relevant mod/fn/struct.`,
        `3. Set \`@yah:status(in-progress)\` as your first action (this is the claim signal).`,
        `4. **Archive this todo** so it drops off the Open column:`,
        `   - Simple: delete the \`## ${todo.id}\` block from \`.yah/todo.md\``,
        `   - Better (records the link to your relay in the audit log):`,
        `     \`curl -sX POST http://localhost:${PORT}/api/todos/${encodeURIComponent(todo.id)}/promote -H 'content-type: application/json' -d '{"relay_id":"RXXX"}'\``,
        ``,
        `**To move ticket columns later:** edit the \`@yah:status(...)\` line in source and save.`,
        `**When done:** click the \`archive\` button on the ticket card — it strips the \`@yah:\` lines from source and logs to \`.yah/events.jsonl\`.`,
        ``,
        `The board auto-refreshes on file changes.`,
      ].join("\n");
      return new Response(prompt, {
        headers: { "Content-Type": "text/markdown" },
      });
    }

    // API: SSE event stream
    if (url.pathname === "/api/events") {
      const stream = new ReadableStream({
        start(controller) {
          sseClients.add(controller);
          // Send current state immediately
          const data = JSON.stringify({
            tickets: currentTickets,
            summaries: currentSummaries,
            todos: currentTodos,
            lastActivity,
          });
          controller.enqueue(
            new TextEncoder().encode(`data: ${data}\n\n`)
          );
        },
        cancel(controller) {
          sseClients.delete(controller);
        },
      });
      return new Response(stream, {
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache",
          Connection: "keep-alive",
          "Access-Control-Allow-Origin": "*",
        },
      });
    }

    // API: generate continuation or review prompt for a ticket.
    //
    // For open/handoff/active/claimed statuses we delegate to the Rust
    // side (`rs-hack board tickets --prompt`), which bakes in the SDLC
    // playbook. For review/done we synthesize a review-oriented prompt
    // locally — the reviewer's job is very different from a pickup's.
    if (url.pathname.startsWith("/api/prompt/")) {
      const ticketId = decodeURIComponent(
        url.pathname.split("/").pop() || ""
      );
      const ticket = currentTickets.find((t) => t.id === ticketId);
      if (ticket && (ticket.status === "review" || ticket.status === "done")) {
        const relSummaries = currentSummaries
          .filter((s) => s.ticket === ticketId)
          .slice(0, 3);
        return new Response(synthesizeReviewPrompt(ticket, relSummaries), {
          headers: { "Content-Type": "text/markdown" },
        });
      }
      try {
        const result =
          await $`${RS_HACK} board tickets --prompt ${ticketId} -p ${WORKSPACE}`.text();
        return new Response(result, {
          headers: { "Content-Type": "text/markdown" },
        });
      } catch (e: any) {
        return Response.json(
          { error: `Ticket '${ticketId}' not found` },
          { status: 404 }
        );
      }
    }

    // API: generate relay doc for a ticket
    if (url.pathname.startsWith("/api/relay-doc/")) {
      const ticketId = url.pathname.split("/").pop();
      try {
        const result =
          await $`${RS_HACK} board tickets --relay-doc ${ticketId} -p ${WORKSPACE}`.text();
        return new Response(result, {
          headers: { "Content-Type": "text/markdown" },
        });
      } catch (e: any) {
        return Response.json(
          { error: `Ticket '${ticketId}' not found` },
          { status: 404 }
        );
      }
    }

    // API: promote a summary to a relay ticket.
    //
    // Body: { target_file: string, title?: string, assignee?: string }
    // `target_file` is required and must be a workspace-relative path to a
    // `.rs` file — that's where the @yah:relay annotation gets written.
    //
    // Allocation, file write, and frontmatter update all happen inside
    // `rs-hack board promote`, which holds the workspace ID lock so it
    // serializes against `board claim`.
    if (
      url.pathname.startsWith("/api/promote/") &&
      req.method === "POST"
    ) {
      const summaryId = decodeURIComponent(
        url.pathname.split("/").pop() || ""
      );
      const summary = currentSummaries.find((s) => s.id === summaryId);
      if (!summary) {
        return Response.json(
          { error: `Summary '${summaryId}' not found` },
          { status: 404 }
        );
      }

      let body: { target_file?: string; title?: string; assignee?: string } = {};
      try {
        body = (await req.json()) ?? {};
      } catch {
        // empty body is fine; target_file check below handles it
      }
      if (!body.target_file) {
        return Response.json(
          {
            error:
              "Missing 'target_file' in request body. " +
              "Pass a workspace-relative path to a .rs file where the relay annotation will be written.",
          },
          { status: 400 }
        );
      }

      try {
        const args = [
          "board",
          "promote",
          "--summary-id",
          summary.id,
          "--file",
          body.target_file,
          "--path",
          WORKSPACE,
          "--json",
        ];
        if (body.title) args.push("--title", body.title);
        if (body.assignee) args.push("--assignee", body.assignee);

        const proc = Bun.spawn([RS_HACK, ...args], {
          stdout: "pipe",
          stderr: "pipe",
        });
        const [stdout, stderr, exitCode] = await Promise.all([
          new Response(proc.stdout).text(),
          new Response(proc.stderr).text(),
          proc.exited,
        ]);
        if (exitCode !== 0) {
          return Response.json(
            { error: `Promote failed: ${stderr.trim() || stdout.trim()}` },
            { status: 500 }
          );
        }

        const result = JSON.parse(stdout.trim());
        triggerRescan("promote");

        return Response.json({
          ok: true,
          relayId: result.relay_id,
          relayTitle: result.relay_title,
          file: result.file,
          line: result.line,
          summaryFile: result.summary_file,
          message: `Promoted to ${result.relay_id}`,
        });
      } catch (e: any) {
        return Response.json(
          { error: `Promote failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: archive an inbox summary — marks `archived: true` in frontmatter
    // so it drops out of the inbox view without losing the file on disk.
    if (
      url.pathname.startsWith("/api/summaries/") &&
      url.pathname.endsWith("/archive") &&
      req.method === "POST"
    ) {
      const parts = url.pathname.split("/");
      const summaryId = decodeURIComponent(parts[parts.length - 2] || "");
      const summary = currentSummaries.find((s) => s.id === summaryId);
      if (!summary) {
        return Response.json(
          { error: `Summary '${summaryId}' not found` },
          { status: 404 }
        );
      }

      try {
        const summaryPath = join(
          WORKSPACE,
          ".yah",
          "summaries",
          `${summary.id}.md`
        );
        let content = await Bun.file(summaryPath).text();
        if (!content.startsWith("---\n")) {
          return Response.json(
            { error: `Summary '${summaryId}' has no frontmatter` },
            { status: 400 }
          );
        }
        if (/^archived:\s*/m.test(content)) {
          content = content.replace(/^archived:\s*\S+/m, "archived: true");
        } else {
          const endIdx = content.indexOf("---\n", 4);
          if (endIdx === -1) {
            return Response.json(
              { error: `Summary '${summaryId}' has malformed frontmatter` },
              { status: 400 }
            );
          }
          content =
            content.slice(0, endIdx) + "archived: true\n" + content.slice(endIdx);
        }
        await Bun.write(summaryPath, content);
        triggerRescan("summary:archive");
        return Response.json({ ok: true, id: summaryId });
      } catch (e: any) {
        return Response.json(
          { error: `Archive failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: archive — list archived tickets (filtered from events log)
    if (url.pathname === "/api/archive" && req.method === "GET") {
      const all = await readAllEvents();
      const archived = all
        .filter((e) => e.type === "archived")
        .sort((a, b) => b.t - a.t);
      return Response.json(archived);
    }

    // API: history — full event log (audit). Reads every shard + sorts
    // globally; cheap for a typical project, scales with total log size.
    if (url.pathname === "/api/history" && req.method === "GET") {
      const all = await readAllEvents();
      all.sort((a, b) => b.t - a.t);
      return Response.json(all);
    }

    // API: archive — archive a ticket by id
    if (
      url.pathname.startsWith("/api/archive/") &&
      req.method === "POST"
    ) {
      const id = decodeURIComponent(
        url.pathname.split("/").pop()?.split("?")[0] || ""
      );
      const ticket = currentTickets.find((t) => t.id === id);
      if (!ticket) {
        return Response.json(
          { error: `Ticket '${id}' not found` },
          { status: 404 }
        );
      }
      // Active-state guard: an agent should never archive straight from
      // `claimed` / `in-progress`. Force the work through `review` (or
      // `handoff`) first so a second pair of eyes sees it. Humans can still
      // bypass via direct source edit if they really need to.
      if (ticket.status === "claimed" || ticket.status === "in-progress") {
        return Response.json(
          {
            error: `Cannot archive '${id}' — ticket is ${ticket.status}. Move to review or handoff first.`,
            status: ticket.status,
            hint: "Drag the card to Review (or set @yah:status(review) in source), then archive from there.",
          },
          { status: 409 }
        );
      }
      // Epic guard: refuse to archive an epic while children still exist in source.
      if (ticket.is_epic) {
        const liveChildren = currentTickets.filter(
          (t) => t.parent === ticket.id
        );
        if (liveChildren.length > 0) {
          return Response.json(
            {
              error: `Epic '${id}' has ${liveChildren.length} live child relay${liveChildren.length > 1 ? "s" : ""}. Archive each child first.`,
              epic: id,
              blockingChildren: liveChildren.map((c) => ({
                id: c.id,
                title: c.title,
                status: c.status,
              })),
              hint: "Cascade archiving is not yet supported — archive each child explicitly.",
            },
            { status: 409 }
          );
        }
      }
      try {
        const sourcePath = ticket.file.startsWith("/")
          ? ticket.file
          : join(WORKSPACE, ticket.file);
        const content = await Bun.file(sourcePath).text();
        const { newContent, removed } = stripHackAnnotations(
          content,
          ticket.line,
          sourcePath
        );
        if (removed.length === 0) {
          return Response.json(
            {
              error: `No @yah: annotations found at ${ticket.file}:${ticket.line}`,
            },
            { status: 500 }
          );
        }
        await Bun.write(sourcePath, newContent);
        const archiveShard = shardNameForTicket(ticket);
        await appendEventToShard(archiveShard, {
          t: Math.floor(Date.now() / 1000),
          type: "archived",
          id,
          ticket,
          sourceLines: removed,
          file: ticket.file,
          line: ticket.line,
        });
        // Drop the id from priorState so the follow-up rescan's step-2
        // "disappeared" pass doesn't double-log this removal. `disappeared`
        // is reserved for clobbered annotations (a hand-delete with no
        // archive event). Without this, archive always emits the pair
        // (archived, disappeared) back-to-back.
        priorState.delete(id);
        triggerRescan("api:archive");
        return Response.json({
          ok: true,
          id,
          removedLines: removed.length,
        });
      } catch (e: any) {
        return Response.json(
          { error: `Archive failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: drag-to-move — rewrite @yah:status(...) to a target bucket.
    // Enforces the allowed-transitions matrix; returns 409 otherwise.
    if (
      url.pathname.startsWith("/api/status/") &&
      req.method === "POST"
    ) {
      const id = decodeURIComponent(
        url.pathname.split("/").pop()?.split("?")[0] || ""
      );
      const ticket = currentTickets.find((t) => t.id === id);
      if (!ticket) {
        return Response.json(
          { error: `Ticket '${id}' not found` },
          { status: 404 }
        );
      }
      // Epics don't move via drag — their status is computed.
      if (ticket.is_epic) {
        return Response.json(
          { error: "Epic status is computed from children; cannot be set." },
          { status: 409 }
        );
      }
      const body = (await req.json().catch(() => ({}))) as { to?: string };
      const toBucket = (body.to || "").toLowerCase();
      const newStatus = BUCKET_TO_STATUS[toBucket];
      if (!newStatus) {
        return Response.json(
          {
            error: `Unknown target bucket '${body.to}'. Expected: open | active | handoff | review.`,
          },
          { status: 400 }
        );
      }
      const fromBucket = STATUS_TO_BUCKET[ticket.status] || ticket.status;
      if (fromBucket === toBucket) {
        return Response.json({ ok: true, noop: true, id, status: ticket.status });
      }
      const allowed = TRANSITIONS[fromBucket] || [];
      if (!allowed.includes(toBucket)) {
        return Response.json(
          {
            error: `Transition ${fromBucket} → ${toBucket} is not allowed.`,
            allowed,
            from: fromBucket,
          },
          { status: 409 }
        );
      }
      try {
        const sourcePath = ticket.file.startsWith("/")
          ? ticket.file
          : join(WORKSPACE, ticket.file);
        const content = await Bun.file(sourcePath).text();
        const { newContent, changed } = setHackStatus(
          content,
          ticket.line,
          newStatus
        );
        if (!changed) {
          return Response.json(
            {
              error: `Could not locate @yah: doc block at ${ticket.file}:${ticket.line}`,
            },
            { status: 500 }
          );
        }
        await Bun.write(sourcePath, newContent);
        triggerRescan(`api:status:${id}`);
        return Response.json({
          ok: true,
          id,
          from: ticket.status,
          to: newStatus,
        });
      } catch (e: any) {
        return Response.json(
          { error: `Status change failed: ${e.message}` },
          { status: 500 }
        );
      }
    }

    // API: manual nudge
    if (url.pathname === "/api/nudge" && req.method === "POST") {
      triggerRescan("api:nudge");
      return Response.json({ ok: true });
    }

    // API: scan stats
    if (url.pathname === "/api/status") {
      return Response.json({
        workspace: WORKSPACE,
        ticketCount: currentTickets.length,
        summaryCount: currentSummaries.length,
        todoCount: currentTodos.length,
        scanCount,
        sseClients: sseClients.size,
      });
    }

    // Static files
    let filePath = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(join(publicDir, filePath));
    if (await file.exists()) {
      return new Response(file);
    }

    // Try dist/ for built assets
    const distFile = Bun.file(join(publicDir, "dist", filePath));
    if (await distFile.exists()) {
      return new Response(distFile);
    }

    return new Response("Not found", { status: 404 });
  },
});

// ── Initial scan ────────────────────────────────────────────────────────

// One-shot migration: if a legacy `.yah/events.jsonl` exists and no shards
// are present, bucket the legacy entries into per-relay shards and rename
// the old file to `.yah/events.jsonl.legacy` for audit.
const migration = await migrateLegacyEventsIfNeeded();
if (migration) {
  console.log(
    `[hack-board] migrated ${migration.migrated} legacy events into ${migration.shards} shards; old file preserved as events.jsonl.legacy`
  );
}

await seedLastActivity();

// Reconstruct per-id state from each shard (walk genesis + deltas
// forward, skip archived/disappeared). priorState / priorTodos then
// serve as the baseline for the first scan so we can detect
// while-down drift and emit deltas rather than full-ticket snapshots.
await loadPriorStateFromShards();

[currentTickets, currentSummaries, currentTodos] = await Promise.all([
  scanTickets(),
  scanSummaries(),
  scanTodos(),
]);
await scanAndLog(currentTickets);
await scanAndLogTodos(currentTodos);
console.log(
  `[hack-board] http://localhost:${PORT} (udp:${UDP_PORT}, slot:${slot}) | ${currentTickets.length} tickets, ${currentSummaries.length} summaries, ${currentTodos.length} todos | workspace: ${WORKSPACE}`
);
