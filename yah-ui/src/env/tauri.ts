// Tauri adapter — dynamically imported by env/index.ts only when running
// under the Tauri host. Calls into yah-tauri's #[tauri::command] handlers.

import type {
  ArchEventListener,
  Rpc,
  Unlisten,
} from "./index";
import type {
  GetTicketParams,
  GetTicketResult,
  IndexReason,
  KVStore,
  Lang,
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
} from "./types";

// `@tauri-apps/api` and `@tauri-apps/plugin-store` are runtime dependencies
// of yah-ui — see package.json. Imports happen inside this file (loaded
// lazily by env/index.ts) so the browser bundle never needs them.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LazyStore } from "@tauri-apps/plugin-store";

const ARCH_EVENT = "arch:event";

export const rpc: Rpc = {
  openRig(rigId: string) {
    return invoke<WalkSummary>("arch_open_rig", { rigId });
  },
  closeRig(rigId: string) {
    return invoke<void>("arch_close_rig", { rigId });
  },
  subgraph(rigId: string, params: SubgraphParams) {
    return invoke<Subgraph>("arch_subgraph", { rigId, params });
  },
  lookup(rigId: string, params: LookupParams) {
    return invoke<LookupResult>("arch_lookup", { rigId, params });
  },
  node(rigId: string, id: NodeId) {
    return invoke<NodeFull | null>("arch_node", { rigId, id });
  },
  neighbors(rigId: string, params: NeighborsParams) {
    return invoke<NeighborsResult>("arch_neighbors", { rigId, params });
  },
  roots(rigId: string, params: RootsParams) {
    return invoke<RootsResult>("arch_roots", { rigId, params });
  },
  stats(rigId: string) {
    return invoke<StatsResult>("arch_stats", { rigId });
  },
  languages(rigId: string) {
    return invoke<Lang[]>("arch_languages", { rigId });
  },
  rigList() {
    return invoke<WireRigDto[]>("rig_list");
  },
  rigSetActive(rigId: string) {
    return invoke<boolean>("rig_set_active", { rigId });
  },
  listTickets(rigId: string, params?: ListTicketsParams) {
    return invoke<ListTicketsResult>("arch_list_tickets", {
      rigId,
      params: params ?? null,
    });
  },
  listRelays(rigId: string, params?: ListRelaysParams) {
    return invoke<ListRelaysResult>("arch_list_relays", {
      rigId,
      params: params ?? null,
    });
  },
  getTicket(rigId: string, params: GetTicketParams) {
    return invoke<GetTicketResult>("arch_get_ticket", { rigId, params });
  },
  validate(rigId: string, scope?: WireScope) {
    return invoke<ValidateResult>("arch_validate", {
      rigId,
      params: scope ? { scope } : null,
    });
  },
  ticketPrompt(rigId: string, params: TicketPromptParams) {
    return invoke<TicketPromptResult>("arch_ticket_prompt", { rigId, params });
  },
  reindexPath(rigId: string, path: string, reason: IndexReason) {
    return invoke<void>("arch_reindex_path", { rigId, path, reason });
  },
  touch(rigId: string, paths: string[], tool: string, relay: string) {
    return invoke<void>("arch_touch", { rigId, paths, tool, relay });
  },
  async onEvent(listener: ArchEventListener): Promise<Unlisten> {
    const off = await listen(ARCH_EVENT, (e) => {
      // Tauri wraps payloads in `{ payload, event, ... }`. We forward
      // payload as-is — it serializes to our ArchEvent type.
      listener(e.payload as Parameters<ArchEventListener>[0]);
    });
    return () => off();
  },
};

// Single store file in the platform app-data dir; keys are namespaced
// inside the file so different subsystems can share it without colliding.
const KV_STORE_PATH = "yah-ui-kv.json";

/** Build the Tauri-backed `KVStore`. The underlying `LazyStore` defers
 *  disk I/O until the first call, so this is cheap to construct on boot. */
export async function makeKv(): Promise<KVStore> {
  const store = new LazyStore(KV_STORE_PATH, { defaults: {}, autoSave: 250 });
  return {
    async get<T = unknown>(key: string): Promise<T | null> {
      const v = await store.get<T>(key);
      return v === undefined ? null : v;
    },
    async set<T = unknown>(key: string, value: T): Promise<void> {
      await store.set(key, value);
    },
    async remove(key: string): Promise<void> {
      await store.delete(key);
    },
    async keys(): Promise<string[]> {
      return store.keys();
    },
  };
}
