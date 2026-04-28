//!
//!
//!
//!
//!

// Host-environment adapter.
//
// The same React bundle runs under two hosts:
// * Tauri desktop (primary): commands go through Tauri's IPC, events
//   come off the window event bus.
// * Browser dev (secondary): no Tauri APIs available; the adapter
//   serves mock data so screens are still inspectable.
//
// Components must NEVER import `@tauri-apps/api/*` directly. They go
// through `env` here so the browser bundle stays tree-shakeable
// (`@tauri-apps/api` is loaded lazily only when running under Tauri).

import type {
  ArchEvent,
  GetTicketParams,
  GetTicketResult,
  KVStore,
  ListRelaysParams,
  ListRelaysResult,
  ListTicketsParams,
  ListTicketsResult,
  LookupParams,
  LookupResult,
  NeighborsParams,
  NeighborsResult,
  NodeFull,
  NodeId,
  RootsParams,
  RootsResult,
  StatsResult,
  Subgraph,
  SubgraphParams,
  TicketPromptParams,
  TicketPromptResult,
  ValidateResult,
  WalkSummary,
  WireRigDto,
  WireScope,
  IndexReason,
  Lang,
} from "./types";

export type ArchEventListener = (event: ArchEvent) => void;
export type Unlisten = () => void;

/** Surface every component talks to. Implementations in tauri.ts / browser.ts.
 *
 *  Every arch-scoped read/mutation takes a `rigId` because the daemon
 *  multiplexes one KgService per attached rig (see
 *  app/tauri/src/commands.rs `svc_by_id`). The renderer tracks the
 *  active rigId in <App> and threads it down. */
export interface Rpc {
  /** Boot a rig that's already attached; idempotent — a second call rebinds. */
  openRig(rigId: string): Promise<WalkSummary>;
  closeRig(rigId: string): Promise<void>;

  // Read queries — all serializable; refer to ./types for shapes.
  subgraph(rigId: string, params: SubgraphParams): Promise<Subgraph>;
  lookup(rigId: string, params: LookupParams): Promise<LookupResult>;
  node(rigId: string, id: NodeId): Promise<NodeFull | null>;
  neighbors(rigId: string, params: NeighborsParams): Promise<NeighborsResult>;
  roots(rigId: string, params: RootsParams): Promise<RootsResult>;
  stats(rigId: string): Promise<StatsResult>;
  languages(rigId: string): Promise<Lang[]>;

  // Rig registry — Board needs the active rigId to scope its fetches.
  rigList(): Promise<WireRigDto[]>;
  /** Mark a rig as the focused one. Persists `lastActiveAt` server-side
   *  so the next session opens on the same rig. Returns `false` if the
   *  id isn't attached. */
  rigSetActive(rigId: string): Promise<boolean>;

  // Work items (relays + tickets) — feed the Board tab.
  listTickets(rigId: string, params?: ListTicketsParams): Promise<ListTicketsResult>;
  listRelays(rigId: string, params?: ListRelaysParams): Promise<ListRelaysResult>;
  getTicket(rigId: string, params: GetTicketParams): Promise<GetTicketResult>;

  /** Run the `@yah:rule(...)` validator. Omit `scope` (or pass nothing)
   *  to validate the whole rig. */
  validate(rigId: string, scope?: WireScope): Promise<ValidateResult>;

  /** Render the canonical pickup or review markdown for a work-item id.
   *  Both the CLI's `yah board show <id> --prompt` and this RPC flow
   *  through the same renderer (`yah_kg::prompt::render`) so the output
   *  is byte-identical for the same id+mode. `result.markdown` is `null`
   *  when the id isn't on the board. */
  ticketPrompt(rigId: string, params: TicketPromptParams): Promise<TicketPromptResult>;

  // Mutations.
  reindexPath(rigId: string, path: string, reason: IndexReason): Promise<void>;
  touch(rigId: string, paths: string[], tool: string, relay: string): Promise<void>;

  /** Subscribe to the daemon's ArchEvent stream. Returns a teardown fn. */
  onEvent(listener: ArchEventListener): Promise<Unlisten>;
}

export interface Env {
  kind: "tauri" | "browser";
  rpc: Rpc;
  /** Persistent JSON-shaped key-value store. See `./types#KVStore`. */
  kv: KVStore;
}

/** Detect Tauri by checking for the IPC handle. Done via a runtime probe so
 *  the browser build never needs the Tauri imports at all. */
function isTauri(): boolean {
  // Tauri 2 exposes __TAURI_INTERNALS__ on the window. The check is in a
  // try/catch because some test environments stub `window` weirdly.
  try {
    return typeof window !== "undefined"
      // @ts-expect-error — runtime probe; no @tauri-apps types yet
      && typeof window.__TAURI_INTERNALS__ !== "undefined";
  } catch {
    return false;
  }
}

let cached: Env | null = null;

/** Lazy singleton. The first call detects the host and dynamic-imports the
 *  matching adapter; subsequent calls reuse it. */
export async function getEnv(): Promise<Env> {
  if (cached) return cached;
  if (isTauri()) {
    const mod = await import("./tauri");
    cached = { kind: "tauri", rpc: mod.rpc, kv: await mod.makeKv() };
  } else {
    const mod = await import("./browser");
    cached = { kind: "browser", rpc: mod.rpc, kv: mod.kv };
  }
  return cached;
}

/** Synchronous accessor for components that have already awaited `getEnv`
 *  during app boot. Throws if called before the first await. */
export function env(): Env {
  if (!cached) {
    throw new Error("env() called before getEnv() has resolved");
  }
  return cached;
}

export type { ArchEvent } from "./types";
