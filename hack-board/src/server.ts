//! @hack:ticket(R002-T1, "P2: per-relay event shards + content-hash scan diff")
//! @hack:status(review)
//! @hack:assignee(agent:claude)
//! @hack:parent(R002)
//! @hack:handoff("P2 implemented end-to-end. .hack/events.jsonl replaced by per-relay shards .hack/events/<id>.jsonl. New 'scan' event type keyed on FNV-1a 64 hash of canonical ticket JSON (line field excluded). Legacy log auto-migrates on first serve; preserves original timestamps and dedupes consecutive same-hash scans. Disappeared detection rewritten to walk shard tails. diffTicket / diffAndLog / snapshot / replaySnapshot all removed. rs-hack-arch/src/status.rs::scan_disappeared now reads the sharded layout (with legacy fallback) and sorts by timestamp across shards. Smoke tests: fresh workspace creates shards on first scan; legacy-workspace migration bucket-writes correctly with preserved timestamps; re-scan emits zero new events when nothing changed; orphan todos land in _todos.jsonl. Dogfooded against this repo: 18 legacy events migrated into R001/R002 shards.")
//! @hack:verify("cargo test -p rs-hack-arch status — passes, new sharded + legacy + prefers-shards tests")
//! @hack:verify("Smoke: HACK_WORKSPACE=<new> bun run hack-board/src/server.ts; check .hack/events/ contains per-relay files and that second run emits no new events")
//! @hack:verify("Legacy .hack/events.jsonl was migrated to .hack/events.jsonl.legacy; this repo's real workspace did so successfully")
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
 * `.hack/events.jsonl` is a derivative audit log written on every rescan:
 * created / modified / archived / disappeared. On startup the log is
 * replayed into memory so a first-scan diff can catch tickets that were
 * clobbered while the server was down.
 */

import { watch } from "fs";
import { appendFile } from "fs/promises";
import { createSocket } from "dgram";
import { join, resolve } from "path";
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
  file: string;
  line: number;
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

const TODO_PATH = join(WORKSPACE, ".hack", "todo.md");
const EVENTS_DIR = join(WORKSPACE, ".hack", "events");
const EVENTS_LEGACY = join(WORKSPACE, ".hack", "events.jsonl");
const TODOS_SHARD = "_todos";
const RENAMED_SHARD = "_renamed";

// Per-relay event shards. Source is still the single source of truth for live
// board state; `.hack/events/*.jsonl` is a derivative audit history. One file
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

interface TicketEvent {
  t: number; // unix seconds
  type: EventType;
  id: string;
  hash?: string; // scan
  ticket?: Ticket; // scan / archived
  lastTicket?: Ticket; // disappeared
  sourceLines?: string[]; // archived
  file?: string;
  line?: number;
  todo?: Todo; // todo_*
  relay_id?: string; // todo_promoted → the in-source relay/ticket it became
  from?: string; // renamed
  to?: string; // renamed
}

// ── Hash ────────────────────────────────────────────────────────────────
//
// Content hash of a ticket, used to decide whether to emit a new `scan` event.
// Canonical JSON with sorted keys, excluding `line` (which churns any time
// someone inserts a doc comment above an annotation) and keeping everything
// else that actually matters for audit.

function fnv1a64(s: string): string {
  const MASK = 0xffffffffffffffffn;
  let h = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  for (let i = 0; i < s.length; i++) {
    h = (h ^ BigInt(s.charCodeAt(i) & 0xff)) & MASK;
    h = (h * prime) & MASK;
  }
  return h.toString(16).padStart(16, "0");
}

function canonicalTicketJSON(t: Ticket): string {
  const { line: _line, ...rest } = t as any;
  // Stable key order; array element order preserved (semantically meaningful
  // for next_steps / verify / etc.)
  const keys = Object.keys(rest).sort();
  const ordered: any = {};
  for (const k of keys) ordered[k] = rest[k];
  return JSON.stringify(ordered);
}

function hashTicket(t: Ticket): string {
  return fnv1a64(canonicalTicketJSON(t));
}

// ── Shard routing ───────────────────────────────────────────────────────
//
// Given a ticket, which shard file does it belong to?
//
// - Compound sub-ticket (`R007-T1`) → bare relay shard (`R007.jsonl`)
// - Ticket with `@hack:parent(Rxxx)` or `@hack:parent(Rxxx-Ty)` → the bare relay
// - Bare relay (`R001`) → own shard
// - Standalone (bare F/B/T with no parent) → own shard

function bareRelayOf(id: string): string | null {
  const compound = id.match(/^(R\d+)-T\d+$/);
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

  // Walk the legacy log in order, maintaining per-id state so each `scan`
  // event we emit reflects the ticket's state AT THAT POINT IN TIME — not
  // the final post-migration state. Dedupe consecutive same-hash scans so
  // no-op modifications (e.g. a `line` field flip) don't produce noise.
  const stateById = new Map<string, Ticket>();
  const lastHashById = new Map<string, string>();

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

    // Advance per-id state for ticket-lifecycle events.
    if (tKind === "created" && ev.ticket) {
      stateById.set(ev.id, ev.ticket);
    } else if (tKind === "modified" && (ev as any).changes) {
      const prev = stateById.get(ev.id);
      if (prev) {
        const updated: any = { ...prev };
        for (const [k, v] of Object.entries(
          (ev as any).changes as Record<string, { after: any }>
        )) {
          updated[k] = v.after;
        }
        stateById.set(ev.id, updated);
      }
    } else if (tKind === "archived" && ev.ticket) {
      stateById.set(ev.id, ev.ticket);
    }

    const t =
      stateById.get(ev.id) ?? ev.ticket ?? ev.lastTicket ?? null;
    const shard = t ? shardNameForTicket(t) : ev.id;

    if (tKind === "created" || tKind === "modified") {
      if (!t) continue;
      const h = hashTicket(t);
      if (lastHashById.get(ev.id) === h) continue; // same state as last emitted
      lastHashById.set(ev.id, h);
      const scanEv: TicketEvent = {
        t: ev.t,
        type: "scan",
        id: ev.id,
        hash: h,
        ticket: t,
      };
      push(shard, JSON.stringify(scanEv));
      continue;
    }
    // archived / disappeared — carry through with original shape
    push(shard, JSON.stringify(ev));
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
// Then: enumerate shards, find ids whose tail is `scan` (not archived /
// disappeared) and who are absent from the current source → emit a
// `disappeared` event.
//
// Replaces the old diffAndLog + diffTicket + in-memory snapshot. No
// startup replay needed; the tail of each shard IS the snapshot.

async function tailStateFor(shardName: string): Promise<Map<string, TicketEvent>> {
  const lines = await readShardLines(shardName);
  const byId = new Map<string, TicketEvent>();
  for (const ev of lines) {
    if (ev.id) byId.set(ev.id, ev);
  }
  return byId;
}

async function scanAndLog(current: Ticket[]): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const currentIds = new Set(current.map((t) => t.id));

  // Cache each shard's tail state so we only read each file once even if
  // many tickets map to the same shard (common — a relay and its sub-tickets).
  const tailCache = new Map<string, Map<string, TicketEvent>>();
  const tailFor = async (shardName: string) => {
    if (!tailCache.has(shardName)) {
      tailCache.set(shardName, await tailStateFor(shardName));
    }
    return tailCache.get(shardName)!;
  };

  // 1) Emit `scan` events for tickets whose hash changed.
  for (const t of current) {
    const shard = shardNameForTicket(t);
    const tails = await tailFor(shard);
    const prior = tails.get(t.id);
    const priorHash =
      prior && prior.type === "scan" ? prior.hash ?? null : null;
    const newHash = hashTicket(t);
    if (priorHash !== newHash) {
      const scanEv: TicketEvent = {
        t: now,
        type: "scan",
        id: t.id,
        hash: newHash,
        ticket: t,
      };
      await appendEventToShard(shard, scanEv);
      // Keep cache coherent for downstream disappeared detection.
      tails.set(t.id, scanEv);
    }
  }

  // 2) Disappeared detection: every shard id whose tail is a live `scan`
  //    event but who is absent from source got clobbered.
  const shards = await listShardNames();
  for (const shard of shards) {
    if (shard === TODOS_SHARD || shard === RENAMED_SHARD) continue;
    const tails = await tailFor(shard);
    for (const [id, ev] of tails) {
      if (ev.type !== "scan") continue;
      if (currentIds.has(id)) continue;
      const disappearedEv: TicketEvent = {
        t: now,
        type: "disappeared",
        id,
        lastTicket: ev.ticket,
      };
      await appendEventToShard(shard, disappearedEv);
      tails.set(id, disappearedEv);
    }
  }
}

async function scanAndLogTodos(current: Todo[]): Promise<void> {
  const now = Math.floor(Date.now() / 1000);

  // Reconstruct the current set of known-live todo ids from the tail of
  // _todos.jsonl — per-id last event wins.
  const lines = await readShardLines(TODOS_SHARD);
  const lastById = new Map<string, TicketEvent>();
  for (const ev of lines) {
    if (ev.id) lastById.set(ev.id, ev);
  }
  const known = new Set<string>();
  for (const [id, ev] of lastById) {
    if (ev.type === "todo_created") known.add(id);
  }

  const currentSet = new Set(current.map((t) => t.id));
  for (const t of current) {
    if (!known.has(t.id)) {
      await appendEventToShard(TODOS_SHARD, {
        t: now,
        type: "todo_created",
        id: t.id,
        todo: t,
      });
    }
  }
  for (const id of known) {
    if (!currentSet.has(id)) {
      await appendEventToShard(TODOS_SHARD, {
        t: now,
        type: "todo_removed",
        id,
      });
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
 * Rewrite the `@hack:status(...)` line inside the contiguous doc-comment
 * block surrounding `lineNum`. If no status line exists, insert one
 * immediately after the `@hack:ticket/relay(...)` declaration (so the
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
    lines.push("## Stated remaining work (from @hack:next)");
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
      "⚠ No `@hack:verify(...)` was declared on this ticket. Decide how to confirm the work yourself (run tests, read diff, reproduce scenario)."
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
    "   **Approve** → click the `archive` button on the ticket card. That strips the `@hack:` annotations from source (ticket leaves the board) and appends an `archived` event to `.hack/events.jsonl` for audit."
  );
  lines.push("");
  lines.push(
    "   **Reject** → edit the source file to set `@hack:status(handoff)`, rewrite `@hack:handoff(\"...\")` with a concrete description of what still needs fixing, and replace any stale `@hack:next(\"...\")` items. The next agent picks up from there."
  );
  lines.push("");
  lines.push(
    "Do not leave a reviewed ticket sitting in `review` indefinitely — decide one way or the other."
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

  const statusRe = /^(\s*\/\/[!/])\s*@hack:status\([^)]*\)\s*$/;
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(statusRe);
    if (m) {
      lines[i] = `${m[1]} @hack:status(${newStatus})`;
      return { newContent: lines.join("\n"), changed: true };
    }
  }

  // Not present — insert after the defining @hack:ticket(...) or @hack:relay(...) line.
  const declRe = /^(\s*\/\/[!/])\s*@hack:(ticket|relay)\(/;
  for (let i = start; i <= end; i++) {
    const m = lines[i].match(declRe);
    if (m) {
      lines.splice(i + 1, 0, `${m[1]} @hack:status(${newStatus})`);
      return { newContent: lines.join("\n"), changed: true };
    }
  }
  return { newContent: content, changed: false };
}

/**
 * Remove `@hack:` doc-comment lines belonging to the ticket defined at `lineNum`.
 * Walks up/down across the contiguous doc-comment block and strips only lines
 * that match `//! @hack:` or `/// @hack:`. Non-hack annotations (e.g. @arch:)
 * and regular doc text are preserved.
 */
function stripHackAnnotations(
  content: string,
  lineNum: number
): { newContent: string; removed: string[] } {
  const lines = content.split("\n");
  const isDocLine = (l: string) => /^\s*\/\/[!/]/.test(l);
  const isHackLine = (l: string) => /^\s*\/\/[!/]\s*@hack:/.test(l);

  const idx = lineNum - 1;
  if (idx < 0 || idx >= lines.length) {
    return { newContent: content, removed: [] };
  }

  let start = idx;
  while (start > 0 && isDocLine(lines[start - 1])) start--;
  let end = idx;
  while (end < lines.length - 1 && isDocLine(lines[end + 1])) end++;

  const removed: string[] = [];
  const result: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (i >= start && i <= end && isHackLine(lines[i])) {
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
    const tickets = JSON.parse(result) as Ticket[];
    scanCount++;
    return tickets;
  } catch (e) {
    console.error(`[scan] failed:`, e);
    return currentTickets; // keep last good state
  }
}

async function scanSummaries(): Promise<Summary[]> {
  const dir = join(WORKSPACE, ".hack", "summaries");
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
    relay_id: fm.relay_id || undefined,
    relay_title: fm.relay_title || undefined,
  };
}

// ── SSE Clients ─────────────────────────────────────────────────────────

const sseClients = new Set<ReadableStreamDefaultController>();

function broadcast(tickets: Ticket[], summaries: Summary[], todos: Todo[]) {
  const data = JSON.stringify({ tickets, summaries, todos });
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

console.log(`[watch] ${WORKSPACE}`);
const watcher = watch(WORKSPACE, { recursive: true }, (event, filename) => {
  if (
    filename &&
    (filename.endsWith(".rs") ||
      filename.includes(".hack/summaries/") ||
      filename.endsWith(".hack/todo.md") ||
      filename === ".hack/todo.md")
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
        await $`mkdir -p ${join(WORKSPACE, ".hack")}`.quiet();
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
        ".hack",
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
            `Each doc below describes planned work that should be built. **If no relay exists** for a doc, create one (see \`/handoff\` → new-relay flow) with \`@hack:status(in-progress)\`. **If a relay already exists**, continue it (same R-number).`,
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
            `Each doc below is a plan that should become a relay + phased tickets. Run \`/refine\`: pick an R-number, create tickets with \`@hack:phase(Pn)\`, then claim P1 by setting \`@hack:status(in-progress)\`.`,
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
        `Tickets and relays live as \`@hack:\` annotations inside Rust source. The board at hack-board scans source with \`rs-hack board tickets\` and renders columns from each ticket's \`@hack:status(...)\`.`,
        ``,
        `**Key annotations:**`,
        `- \`@hack:ticket(ID, "title")\` / \`@hack:relay(ID, "title")\` — define a work item`,
        `- \`@hack:status(open|claimed|in-progress|handoff|review|done)\` — column`,
        `- \`@hack:assignee(agent:name)\` — who's working on it`,
        `- \`@hack:handoff("...")\` — message for the next agent`,
        `- \`@hack:next("...")\` — a next step (repeatable)`,
        `- \`@hack:verify("...")\` — verification step (repeatable)`,
        `- \`@arch:see(path/to/doc.md)\` — reference doc`,
        ``,
        `**To promote this todo into in-source work:**`,
        `1. Decide scope. If it's multiple tickets / phases, run \`/refine\` to generate a relay + tickets + architecture doc.`,
        `2. Otherwise pick a source file that's the natural home for the work and add \`@hack:ticket(${todo.id.replace("T-", "T")}, "...")\` annotations at the top of the relevant mod/fn/struct.`,
        `3. Set \`@hack:status(in-progress)\` as your first action (this is the claim signal).`,
        `4. **Archive this todo** so it drops off the Open column:`,
        `   - Simple: delete the \`## ${todo.id}\` block from \`.hack/todo.md\``,
        `   - Better (records the link to your relay in the audit log):`,
        `     \`curl -sX POST http://localhost:${PORT}/api/todos/${encodeURIComponent(todo.id)}/promote -H 'content-type: application/json' -d '{"relay_id":"RXXX"}'\``,
        ``,
        `**To move ticket columns later:** edit the \`@hack:status(...)\` line in source and save.`,
        `**When done:** click the \`archive\` button on the ticket card — it strips the \`@hack:\` lines from source and logs to \`.hack/events.jsonl\`.`,
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

    // API: promote a summary to a relay ticket
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

      try {
        // Find next R-number from existing tickets
        const existingRelays = currentTickets
          .filter((t) => t.id.startsWith("R"))
          .map((t) => parseInt(t.id.slice(1)) || 0);
        // Also check summaries that were already promoted
        const promotedRelays = currentSummaries
          .filter((s) => s.promoted)
          .map((s) => {
            const match = (s as any).relay_id?.match(/R(\d+)/);
            return match ? parseInt(match[1]) : 0;
          });
        const nextNum =
          Math.max(0, ...existingRelays, ...promotedRelays) + 1;
        const relayId = `R${String(nextNum).padStart(3, "0")}`;

        const title = summary.text.split("\n")[0].slice(0, 80);

        // Update the summary file in place: add relay_id, set promoted: true
        const summaryPath = join(
          WORKSPACE,
          ".hack",
          "summaries",
          `${summary.id}.md`
        );
        let content = await Bun.file(summaryPath).text();
        content = content.replace("promoted: false", "promoted: true");
        // Add relay_id to frontmatter
        content = content.replace(
          "promoted: true",
          `promoted: true\nrelay_id: ${relayId}\nrelay_title: ${title}`
        );
        await Bun.write(summaryPath, content);

        // Trigger rescan
        triggerRescan("promote");

        return Response.json({
          ok: true,
          relayId,
          summaryFile: summaryPath,
          message: `Promoted to ${relayId}`,
        });
      } catch (e: any) {
        return Response.json(
          { error: `Promote failed: ${e.message}` },
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
          ticket.line
        );
        if (removed.length === 0) {
          return Response.json(
            {
              error: `No @hack: annotations found at ${ticket.file}:${ticket.line}`,
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
        // The next scan's tail-read of the shard sees `archived` as the
        // latest event for this id and won't re-emit as `disappeared`.
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

    // API: drag-to-move — rewrite @hack:status(...) to a target bucket.
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
              error: `Could not locate @hack: doc block at ${ticket.file}:${ticket.line}`,
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

// One-shot migration: if a legacy `.hack/events.jsonl` exists and no shards
// are present, bucket the legacy entries into per-relay shards and rename
// the old file to `.hack/events.jsonl.legacy` for audit.
const migration = await migrateLegacyEventsIfNeeded();
if (migration) {
  console.log(
    `[hack-board] migrated ${migration.migrated} legacy events into ${migration.shards} shards; old file preserved as events.jsonl.legacy`
  );
}

[currentTickets, currentSummaries, currentTodos] = await Promise.all([
  scanTickets(),
  scanSummaries(),
  scanTodos(),
]);
// No startup replay — the tail of each shard IS the snapshot. scanAndLog
// reads tails directly and only emits events when hashes differ.
await scanAndLog(currentTickets);
await scanAndLogTodos(currentTodos);
console.log(
  `[hack-board] http://localhost:${PORT} (udp:${UDP_PORT}, slot:${slot}) | ${currentTickets.length} tickets, ${currentSummaries.length} summaries, ${currentTodos.length} todos | workspace: ${WORKSPACE}`
);
