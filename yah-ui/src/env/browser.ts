// Browser adapter — minimal stub returning empty data.
//
// In v1 we don't ship a hosted-web build that talks to a remote rig;
// running in the browser is for component-level inspection during
// development. Each call resolves with a sane empty value so screens
// render their empty states instead of throwing.
//
// When remote rigs ship, this file gets replaced with an HTTP+SSE
// implementation pointing at a daemon serving over TLS.

import type {
  ArchEventListener,
  Rpc,
  Unlisten,
} from "./index";
import type { KVStore } from "./types";

const empty = async <T>(value: T): Promise<T> => value;

export const rpc: Rpc = {
  openRig: (_rigId: string) =>
    empty({
      filesSeen: 0,
      filesIndexed: 0,
      filesSkipped: 0,
      parseErrors: 0,
    }),
  closeRig: (_rigId: string) => empty(undefined),
  subgraph: (_rigId: string, { root }) =>
    empty({ root, nodes: [], edges: [], truncated: false }),
  lookup: (_rigId: string) => empty({ ids: [] }),
  node: (_rigId: string) => empty(null),
  neighbors: (_rigId: string) => empty({ edges: [] }),
  roots: (_rigId: string) => empty({ roots: [] }),
  stats: (_rigId: string) =>
    empty({
      node_count: 0,
      edge_count: 0,
      by_lang: {},
      by_kind: {},
    }),
  languages: (_rigId: string) => empty([]),
  rigList: () => empty([]),
  rigSetActive: (_rigId: string) => empty(false),
  listTickets: (_rigId: string) => empty({ tickets: [] }),
  listRelays: (_rigId: string) => empty({ relays: [] }),
  getTicket: (_rigId: string) => empty({ ticket: null }),
  validate: (_rigId: string) => empty({ violations: [] }),
  /* Browser dev mode has no daemon to render against; the TicketCard
     falls back to the local prompt.ts builder when this returns null. */
  ticketPrompt: (_rigId: string) => empty({ markdown: null }),
  reindexPath: (_rigId: string) => empty(undefined),
  touch: (_rigId: string) => empty(undefined),
  onEvent: async (_listener: ArchEventListener): Promise<Unlisten> => {
    // No event source in the browser stub.
    return () => {};
  },
};

// Local-storage keys are page-global, so namespace everything we own
// behind a single prefix. Anyone reading the storage from devtools can
// still tell at a glance which entries belong to yah-ui.
const KV_PREFIX = "yah-ui:";

function safeStorage(): Storage | null {
  try {
    if (typeof window === "undefined") return null;
    return window.localStorage;
  } catch {
    // Some embeds (incognito quotas, sandboxed iframes) throw on access.
    return null;
  }
}

export const kv: KVStore = {
  async get<T = unknown>(key: string): Promise<T | null> {
    const s = safeStorage();
    if (!s) return null;
    const raw = s.getItem(KV_PREFIX + key);
    if (raw === null) return null;
    try {
      return JSON.parse(raw) as T;
    } catch {
      return null;
    }
  },
  async set<T = unknown>(key: string, value: T): Promise<void> {
    const s = safeStorage();
    if (!s) return;
    try {
      s.setItem(KV_PREFIX + key, JSON.stringify(value));
    } catch {
      // Quota exceeded or serialization failure — best-effort cache, drop silently.
    }
  },
  async remove(key: string): Promise<void> {
    const s = safeStorage();
    if (!s) return;
    s.removeItem(KV_PREFIX + key);
  },
  async keys(): Promise<string[]> {
    const s = safeStorage();
    if (!s) return [];
    const out: string[] = [];
    for (let i = 0; i < s.length; i++) {
      const k = s.key(i);
      if (k !== null && k.startsWith(KV_PREFIX)) {
        out.push(k.slice(KV_PREFIX.length));
      }
    }
    return out;
  },
};
