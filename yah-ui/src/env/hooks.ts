// React hooks over env().rpc — the contract every tab uses to read
// architecture-graph data without touching Tauri APIs directly.
//
// All hooks await getEnv() inside their effect, so callers don't need to
// coordinate boot order. Subscriptions are torn down on unmount.

import { useEffect, useRef, useState } from "react";
import { getEnv, type Unlisten } from "./index";
import type {
  ArchEvent,
  EdgeOut,
  LookupParams,
  NodeFull,
  NodeId,
  NodeRef,
  Subgraph,
  WireScope,
  WireViolation,
} from "./types";

// ---------- useArchEvents ----------

export type ArchEventKind = ArchEvent["event"];
export type ArchEventOf<K extends ArchEventKind> = Extract<
  ArchEvent,
  { event: K }
>;

/** Subscribe to the daemon's ArchEvent stream from a component.
 *
 *  The subscription is established once per filter-change and torn down
 *  on unmount. The listener is held in a ref so each call observes the
 *  latest closure without re-subscribing.
 *
 *  Usage:
 *    useArchEvents(refetch);                          // every event
 *    useArchEvents(refetch, "index_finished");        // one kind, narrowed
 *    useArchEvents(onDelta, ["node_added", "node_removed"]);
 */
export function useArchEvents(listener: (event: ArchEvent) => void): void;
export function useArchEvents<K extends ArchEventKind>(
  listener: (event: ArchEventOf<K>) => void,
  filter: K | readonly K[],
): void;
export function useArchEvents(
  listener: (event: ArchEvent) => void,
  filter?: ArchEventKind | readonly ArchEventKind[],
): void {
  const listenerRef = useRef(listener);
  listenerRef.current = listener;

  // Stable dep key — array identity changes per render even when contents
  // are equal, so we collapse to a string for the effect's dep list.
  const filterKey = Array.isArray(filter)
    ? filter.slice().sort().join("|")
    : (filter ?? "");

  useEffect(() => {
    let cancelled = false;
    let unlisten: Unlisten | null = null;

    const matches = (event: ArchEvent): boolean => {
      if (filter === undefined) return true;
      if (Array.isArray(filter)) return filter.includes(event.event);
      return event.event === filter;
    };

    void (async () => {
      const env = await getEnv();
      const off = await env.rpc.onEvent((event) => {
        if (cancelled) return;
        if (matches(event)) listenerRef.current(event);
      });
      if (cancelled) {
        off();
      } else {
        unlisten = off;
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
    // filterKey captures the filter contents; listener is stabilized via ref.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filterKey]);
}

// ---------- useConnectionStatus ----------

export type ConnectionState = "ok" | "idle" | "error";

export interface ConnectionStatus {
  state: ConnectionState;
  /** epoch-ms of the most recent successful heartbeat or index event. */
  lastOkAt: number | null;
  /** epoch-ms of the most recent heartbeat failure (if more recent than lastOkAt). */
  lastErrorAt: number | null;
}

const HEARTBEAT_INTERVAL_MS = 15_000;
const IDLE_THRESHOLD_MS = 30_000;

/** Track backend reachability for the active rig.
 *
 *  Two truth-sources feed `lastOkAt`:
 *  - any `index_finished` event the daemon emits, and
 *  - a periodic `stats(rigId)` heartbeat (every 15s, well under the 30s
 *    idle threshold so a quiet rig still reads green).
 *
 *  State derivation:
 *    - error: last heartbeat threw (transport / RPC failure)
 *    - idle:  no error, but >30s since the last successful signal
 *    - ok:    successful signal within the last 30s
 *
 *  Local rigs almost never fail the heartbeat (same machine), so they
 *  stay green or fall to yellow when truly idle. Remote rigs flip to red
 *  on transport loss within one heartbeat interval.
 */
export function useConnectionStatus(rigId: string): ConnectionStatus {
  const [lastOkAt, setLastOkAt] = useState<number | null>(null);
  const [lastErrorAt, setLastErrorAt] = useState<number | null>(null);
  const [now, setNow] = useState<number>(() => Date.now());

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);

  useArchEvents(() => {
    setLastOkAt(Date.now());
    setLastErrorAt(null);
  }, "index_finished");

  useEffect(() => {
    if (!rigId) return;
    let cancelled = false;

    const probe = async () => {
      try {
        const env = await getEnv();
        await env.rpc.stats(rigId);
        if (cancelled) return;
        setLastOkAt(Date.now());
        setLastErrorAt(null);
      } catch {
        if (cancelled) return;
        setLastErrorAt(Date.now());
      }
    };

    void probe();
    const id = window.setInterval(probe, HEARTBEAT_INTERVAL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [rigId]);

  let state: ConnectionState;
  if (lastErrorAt !== null && (lastOkAt === null || lastErrorAt > lastOkAt)) {
    state = "error";
  } else if (lastOkAt === null || now - lastOkAt > IDLE_THRESHOLD_MS) {
    state = "idle";
  } else {
    state = "ok";
  }

  return { state, lastOkAt, lastErrorAt };
}

// ---------- useArchGraph ----------

export interface ArchGraphState {
  data: Subgraph | null;
  loading: boolean;
  error: Error | null;
}

const subgraphCache = new Map<string, Subgraph>();
const archGraphKey = (rigId: string, root: NodeId, depth: number) =>
  `${rigId}|${root}|${depth}`;

// Wire NodeIds are 32-char lowercase hex (16-byte blake3) — the daemon
// rejects anything else with `expected 32 hex chars, got N`. The hook
// short-circuits invalid roots so user input mid-paste, the empty-string
// initial seed, and stem-style fallbacks (App.tsx jumpToFile) skip the
// RPC instead of generating a wave of error toasts.
const NODE_ID_RE = /^[0-9a-f]{32}$/;
function isValidNodeId(id: string): boolean {
  return NODE_ID_RE.test(id);
}

// Persisted-snapshot LRU. Cold-boot reads through env().kv so the UI can
// render the last-known subgraph for a (rigId, root, depth) tuple before
// the daemon's first `index_finished`. Capped to keep storage bounded —
// each entry is a full Subgraph payload.
const SNAPSHOT_KEY_PREFIX = "subgraph:v1:";
const SNAPSHOT_LRU_MAX = 8;
const snapshotKey = (cacheKey: string) => SNAPSHOT_KEY_PREFIX + cacheKey;

interface PersistedSubgraph {
  ts: number;
  data: Subgraph;
}

async function loadPersistedSubgraph(
  cacheKey: string,
): Promise<Subgraph | null> {
  try {
    const env = await getEnv();
    const stored = await env.kv.get<PersistedSubgraph>(snapshotKey(cacheKey));
    return stored?.data ?? null;
  } catch {
    // KV is best-effort — never block the live fetch on a hydration miss.
    return null;
  }
}

async function persistSubgraph(
  cacheKey: string,
  data: Subgraph,
): Promise<void> {
  try {
    const env = await getEnv();
    await env.kv.set<PersistedSubgraph>(snapshotKey(cacheKey), {
      ts: Date.now(),
      data,
    });
    const allKeys = (await env.kv.keys()).filter((k) =>
      k.startsWith(SNAPSHOT_KEY_PREFIX),
    );
    if (allKeys.length <= SNAPSHOT_LRU_MAX) return;
    const stamped = await Promise.all(
      allKeys.map(async (k) => {
        const v = await env.kv.get<PersistedSubgraph>(k);
        return { k, ts: v?.ts ?? 0 };
      }),
    );
    stamped.sort((a, b) => a.ts - b.ts);
    const drop = stamped.slice(0, allKeys.length - SNAPSHOT_LRU_MAX);
    await Promise.all(drop.map(({ k }) => env.kv.remove(k)));
  } catch {
    // Persistence is opportunistic; swallow errors so the live UI keeps working.
  }
}

/** Fetch a subgraph and keep it in sync with arch:event deltas.
 *
 *  Cache is keyed by (rigId, root, depth), so re-mounting with the same
 *  params returns the last-known graph immediately while a fresh fetch
 *  runs in the background. Switching rigs naturally invalidates because
 *  rigId is part of the key.
 *
 *  Reconciliation rules:
 *  - node_added: append if not already present.
 *  - node_removed: drop the node and any edges touching it.
 *  - edge_added: append if both endpoints are in our window.
 *  - edge_removed: drop by id.
 *  - index_finished: refetch — incremental deltas can't tell us whether
 *    a new node falls within `depth` hops of `root`, so we re-anchor.
 *  - node_changed: ignored locally; the next index_finished will refetch.
 */
export function useArchGraph(
  rigId: string,
  rootNodeId: NodeId,
  depth: number,
): ArchGraphState {
  const valid = isValidNodeId(rootNodeId);
  const key = archGraphKey(rigId, rootNodeId, depth);
  const [state, setState] = useState<ArchGraphState>(() => {
    if (!valid) return { data: null, loading: false, error: null };
    const cached = subgraphCache.get(key);
    return {
      data: cached ?? null,
      loading: cached === undefined,
      error: null,
    };
  });

  useEffect(() => {
    if (!valid) {
      setState({ data: null, loading: false, error: null });
      return;
    }
    let cancelled = false;
    let unlisten: Unlisten | null = null;
    let liveLanded = false;

    const cached = subgraphCache.get(key);
    setState({
      data: cached ?? null,
      loading: cached === undefined,
      error: null,
    });

    // No in-memory cache — try the persistent KV in parallel with the live
    // fetch. Whichever returns first warms the UI; the live fetch always
    // wins the final state.
    if (cached === undefined) {
      void (async () => {
        const persisted = await loadPersistedSubgraph(key);
        if (cancelled || liveLanded || persisted === null) return;
        if (subgraphCache.has(key)) return;
        subgraphCache.set(key, persisted);
        setState((prev) =>
          prev.data === null
            ? { data: persisted, loading: true, error: prev.error }
            : prev,
        );
      })();
    }

    const fetchSubgraph = async () => {
      try {
        const env = await getEnv();
        const data = await env.rpc.subgraph(rigId, { root: rootNodeId, depth });
        if (cancelled) return;
        liveLanded = true;
        subgraphCache.set(key, data);
        setState({ data, loading: false, error: null });
        void persistSubgraph(key, data);
      } catch (err) {
        if (cancelled) return;
        setState((prev) => ({
          data: prev.data,
          loading: false,
          error: err instanceof Error ? err : new Error(String(err)),
        }));
      }
    };

    const onArchEvent = (event: ArchEvent) => {
      if (cancelled) return;
      setState((prev) => {
        if (!prev.data) return prev;
        const next = applySubgraphEvent(prev.data, event);
        if (next === prev.data) return prev;
        subgraphCache.set(key, next);
        return { data: next, loading: false, error: prev.error };
      });
      if (event.event === "index_finished") {
        void fetchSubgraph();
      }
    };

    void (async () => {
      await fetchSubgraph();
      if (cancelled) return;
      const env = await getEnv();
      const off = await env.rpc.onEvent(onArchEvent);
      if (cancelled) {
        off();
      } else {
        unlisten = off;
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [valid, key, rootNodeId, depth]);

  return state;
}

function applySubgraphEvent(graph: Subgraph, event: ArchEvent): Subgraph {
  switch (event.event) {
    case "node_added":
      return appendNode(graph, event.node);
    case "node_removed":
      return removeNode(graph, event.id);
    case "edge_added":
      return appendEdge(graph, event.edge);
    case "edge_removed":
      return removeEdge(graph, event.id);
    default:
      return graph;
  }
}

function appendNode(graph: Subgraph, node: NodeRef): Subgraph {
  if (graph.nodes.some((n) => n.id === node.id)) return graph;
  return { ...graph, nodes: [...graph.nodes, node] };
}

function removeNode(graph: Subgraph, id: NodeId): Subgraph {
  if (!graph.nodes.some((n) => n.id === id)) return graph;
  return {
    ...graph,
    nodes: graph.nodes.filter((n) => n.id !== id),
    edges: graph.edges.filter((e) => e.from !== id && e.to !== id),
  };
}

function appendEdge(graph: Subgraph, edge: EdgeOut): Subgraph {
  const fromIn = graph.nodes.some((n) => n.id === edge.from);
  const toIn = graph.nodes.some((n) => n.id === edge.to);
  if (!fromIn || !toIn) return graph;
  if (graph.edges.some((e) => e.id === edge.id)) return graph;
  return { ...graph, edges: [...graph.edges, edge] };
}

function removeEdge(graph: Subgraph, id: string): Subgraph {
  if (!graph.edges.some((e) => e.id === id)) return graph;
  return { ...graph, edges: graph.edges.filter((e) => e.id !== id) };
}

// ---------- useNode ----------

export interface NodeState {
  data: NodeFull | null;
  loading: boolean;
  error: Error | null;
}

/** Fetch a single node's full payload (incl. doc + annotations) and keep
 *  it fresh: refetches on `node_changed`/`node_removed` events for this id.
 *  Pass `null` for `id` to suspend the hook (e.g. when no node is selected). */
export function useNode(rigId: string, id: NodeId | null): NodeState {
  const [state, setState] = useState<NodeState>({
    data: null,
    loading: id !== null,
    error: null,
  });

  useEffect(() => {
    if (id === null) {
      setState({ data: null, loading: false, error: null });
      return;
    }
    let cancelled = false;
    let unlisten: Unlisten | null = null;
    setState({ data: null, loading: true, error: null });

    const fetchNode = async () => {
      try {
        const env = await getEnv();
        const data = await env.rpc.node(rigId, id);
        if (cancelled) return;
        setState({ data, loading: false, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({
          data: null,
          loading: false,
          error: err instanceof Error ? err : new Error(String(err)),
        });
      }
    };

    void (async () => {
      await fetchNode();
      if (cancelled) return;
      const env = await getEnv();
      const off = await env.rpc.onEvent((event) => {
        if (cancelled) return;
        if (
          (event.event === "node_changed" && event.id === id) ||
          (event.event === "node_removed" && event.id === id)
        ) {
          void fetchNode();
        }
      });
      if (cancelled) {
        off();
      } else {
        unlisten = off;
      }
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [rigId, id]);

  return state;
}

// ---------- useLookup ----------

export interface LookupState {
  ids: NodeId[];
  loading: boolean;
  error: Error | null;
}

/** Resolve a `path:line` chip to NodeIds. The cross-tab nav contract
 *  (App.tsx jumpToFile) eventually calls this to hop from a file
 *  reference to the right arch node. */
export function useLookup(
  rigId: string,
  file: string | null,
  line?: number,
): LookupState {
  const [state, setState] = useState<LookupState>({
    ids: [],
    loading: file !== null,
    error: null,
  });

  useEffect(() => {
    if (file === null) {
      setState({ ids: [], loading: false, error: null });
      return;
    }
    let cancelled = false;
    setState({ ids: [], loading: true, error: null });

    void (async () => {
      try {
        const env = await getEnv();
        const params: LookupParams = { file, line };
        const result = await env.rpc.lookup(rigId, params);
        if (cancelled) return;
        setState({ ids: result.ids, loading: false, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({
          ids: [],
          loading: false,
          error: err instanceof Error ? err : new Error(String(err)),
        });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [rigId, file, line]);

  return state;
}

// ---------- useValidate ----------

export interface ValidateState {
  violations: WireViolation[];
  loading: boolean;
  error: Error | null;
}

/** Fetch `arch.validate` for a rig and refresh after each `index_finished`.
 *
 *  Pass a `scope` to narrow the run (subtree / file); omit it to validate
 *  the whole rig (`Scope::All`, the common interactive case). The browser
 *  stub returns `{ violations: [] }` so dev-without-Tauri still renders.
 */
export function useValidate(rigId: string, scope?: WireScope): ValidateState {
  const [state, setState] = useState<ValidateState>({
    violations: [],
    loading: !!rigId,
    error: null,
  });

  // Stable dep key — the scope object identity changes every render even
  // when contents don't, which would re-fire the effect endlessly.
  const scopeKey = scope ? JSON.stringify(scope) : "";

  useEffect(() => {
    if (!rigId) {
      setState({ violations: [], loading: false, error: null });
      return;
    }
    let cancelled = false;
    let unlisten: Unlisten | null = null;

    const run = async () => {
      try {
        const env = await getEnv();
        const res = await env.rpc.validate(rigId, scope);
        if (cancelled) return;
        setState({ violations: res.violations, loading: false, error: null });
      } catch (err) {
        if (cancelled) return;
        setState((prev) => ({
          violations: prev.violations,
          loading: false,
          error: err instanceof Error ? err : new Error(String(err)),
        }));
      }
    };

    void (async () => {
      await run();
      if (cancelled) return;
      const env = await getEnv();
      const off = await env.rpc.onEvent((event) => {
        if (event.event === "index_finished") void run();
      });
      if (cancelled) off();
      else unlisten = off;
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
    // scopeKey captures the scope contents.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rigId, scopeKey]);

  return state;
}

// ---------- useRoots ----------

export interface RootsState {
  roots: NodeRef[];
  loading: boolean;
  error: Error | null;
}

/** Enumerate the daemon's structural roots (modules, top-level types) so
 *  the RootSelector can offer a pick-list instead of demanding a hex
 *  NodeId. Refetches on every index_finished — new files surface as
 *  new roots, removed ones drop out. */
export function useRoots(rigId: string): RootsState {
  const [state, setState] = useState<RootsState>({
    roots: [],
    loading: true,
    error: null,
  });

  useEffect(() => {
    if (!rigId) {
      setState({ roots: [], loading: false, error: null });
      return;
    }
    let cancelled = false;
    let unlisten: Unlisten | null = null;

    const fetchRoots = async () => {
      try {
        const env = await getEnv();
        const result = await env.rpc.roots(rigId, {});
        if (cancelled) return;
        setState({ roots: result.roots, loading: false, error: null });
      } catch (err) {
        if (cancelled) return;
        setState({
          roots: [],
          loading: false,
          error: err instanceof Error ? err : new Error(String(err)),
        });
      }
    };

    void (async () => {
      await fetchRoots();
      if (cancelled) return;
      const env = await getEnv();
      const off = await env.rpc.onEvent((event) => {
        if (event.event === "index_finished") void fetchRoots();
      });
      if (cancelled) off();
      else unlisten = off;
    })();

    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [rigId]);

  return state;
}
