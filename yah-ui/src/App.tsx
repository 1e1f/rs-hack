import { useCallback, useEffect, useMemo, useState } from "react";
import { TitleBar } from "./components/shell/TitleBar";
import { TabStrip } from "./components/shell/TabStrip";
import { ConnectionStrip } from "./components/shell/ConnectionStrip";
import { useConnectionStatus, useValidate } from "./env/hooks";
import { Board } from "./components/board/Board";
import { ArchView } from "./components/arch/ArchView";
import { AgentView } from "./components/agent/AgentView";
import { Splash, type SplashVariant } from "./components/shared/Splash";
import { getEnv } from "./env";
import { workItemToTicket } from "./env/mapper";
import type { WireRigDto } from "./env/types";
import { mockRigs, mockTickets } from "./mock";
import type { Rig, Tab, Theme, Ticket } from "./types";

/* WireRigDto → renderer Rig. The wire shape carries `path` + `lastActiveAt`
   that the selector renders directly; the runtime `Rig` type also makes
   `host` optional (only populated for remote rigs, which the daemon
   doesn't yet emit — RigKind::Remote is reserved for SSH-RPC). */
function wireToRig(w: WireRigDto): Rig {
  return {
    id: w.id,
    name: w.name,
    kind: w.kind,
    path: w.path,
    reachable: w.reachable,
    lastActiveAt: w.lastActiveAt ?? undefined,
  };
}

/* Synthetic rig that surfaces the bundled mock data — only added when the
   user has explicitly opted in via the localStorage flag below. Selecting
   it short-circuits the backend fetch and feeds <Board> from `mockTickets`
   so the UI can be exercised without a daemon. To enable from devtools:
     localStorage.setItem("yah-ui:enable-example-rig", "1"); location.reload(); */
const EXAMPLE_RIG_ID = "__example__";
const EXAMPLE_FLAG_KEY = "yah-ui:enable-example-rig";
const EXAMPLE_RIG: Rig = {
  id: EXAMPLE_RIG_ID,
  name: "example rig",
  kind: "local",
  path: "(bundled demo data)",
  reachable: true,
};

function exampleRigEnabled(): boolean {
  try {
    return (
      typeof localStorage !== "undefined" &&
      localStorage.getItem(EXAMPLE_FLAG_KEY) === "1"
    );
  } catch {
    return false;
  }
}

export function App() {
  const [tab, setTab] = useState<Tab>("board");
  const [theme, setTheme] = useState<Theme>("light");
  /* Cold boot: empty rig list + empty board. Real rigs land via the
     rig-list effect below (Tauri only); under browser dev the user opts in
     to the synthetic example rig via `EXAMPLE_FLAG_KEY` to populate
     `mockTickets`. Picking a rig calls `setRigId`, which the board fetch
     effect listens to. */
  const initialRigs: Rig[] = exampleRigEnabled() ? [EXAMPLE_RIG] : [];
  const [rigs, setRigs] = useState<Rig[]>(initialRigs);
  const [rigId, setRigId] = useState<string>(initialRigs[0]?.id ?? "");
  const [relayId, setRelayId] = useState<string | null>(null);
  const [splitMode, setSplitMode] = useState<Tab | null>(null);
  const [tickets, setTickets] = useState<Ticket[]>([]);
  /* Lifted out of ArchView so cross-tab nav (jumpToFile / NodeActionMenu's
     "Open in agent") can re-root the graph from anywhere. The empty-string
     seed produces an empty splash on first render — the user picks a real
     root via the toolbar (or jumps in from a file chip) once tickets land. */
  const [archRoot, setArchRoot] = useState<string>("");
  const [archDepth, setArchDepth] = useState<number>(2);

  /* Theme is a CSS-variable swap on [data-theme=...] — see globals.css. */
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  /* Seed the rig registry from the daemon (Tauri only; browser stub
     returns []). Last-active wins as the initial selection; first
     attached is the fallback. Runs once on mount — adding rigs at runtime
     (via the not-yet-built attach UI) will need to refresh this manually.
     The example rig (when enabled) is appended so it stays selectable
     alongside real rigs but ranks last in the lastActive sort. */
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const list = await env.rpc.rigList();
        if (cancelled) return;
        const real = list.map(wireToRig);
        /* Empty rigs.json on the backend → fall back to the example rig as
           a first-run welcome surface. The explicit flag still works on top
           of real rigs so devs can flip into demo mode any time. */
        const showExample = real.length === 0 || exampleRigEnabled();
        const next = showExample ? [...real, EXAMPLE_RIG] : real;
        if (next.length === 0) return;
        setRigs(next);
        const active = real
          .slice()
          .sort((a, b) => (b.lastActiveAt ?? 0) - (a.lastActiveAt ?? 0))[0];
        if (active) setRigId(active.id);
        else setRigId(EXAMPLE_RIG_ID);
      } catch (err) {
        console.warn("[rigs] rig_list failed", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /* Board fetch + index_finished subscription, scoped to the active rig.
     Re-runs whenever `rigId` changes so picking from the selector retargets
     the daemon. The synthetic example rig short-circuits to mock data; an
     empty rigId clears the board. `rig_set_active` is best-effort — ids
     that don't match an attached rig fail silently. */
  useEffect(() => {
    if (!rigId) {
      setTickets([]);
      return;
    }
    if (rigId === EXAMPLE_RIG_ID) {
      setTickets(mockTickets);
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    async function refetch() {
      try {
        const env = await getEnv();
        const [t, r] = await Promise.all([
          env.rpc.listTickets(rigId),
          env.rpc.listRelays(rigId),
        ]);
        if (cancelled) return;
        const merged = [...r.relays, ...t.tickets].map(workItemToTicket);
        setTickets(merged);
      } catch (err) {
        console.warn("[board] tickets fetch failed", err);
      }
    }

    void (async () => {
      const env = await getEnv();
      try {
        await env.rpc.rigSetActive(rigId);
      } catch {
        /* unattached id during dev; ignore */
      }
      if (cancelled) return;
      await refetch();
      if (cancelled) return;
      const off = await env.rpc.onEvent((e) => {
        if (e.event !== "index_finished") return;
        const wrapped = e as { rig_id?: string };
        if (wrapped.rig_id && wrapped.rig_id !== rigId) return;
        void refetch();
      });
      if (cancelled) off();
      else unlisten = off;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [rigId]);

  /* Cross-tab nav contract (impl-guide §6). Each handler is one named action
     callable from any tab; we pass them down explicitly rather than via an
     event bus. */
  const jumpToFile = useCallback((fileColon: string) => {
    /* `path:line` → for v1 we re-root the graph to the file path's basename
       sans extension (closest stand-in for an arch node id until the
       backend serves a real path→node lookup). */
    const path = fileColon.split(":")[0] ?? fileColon;
    const base = path.split("/").pop() ?? path;
    const stem = base.replace(/\.[^.]+$/, "");
    setArchRoot(stem);
    setTab("arch");
  }, []);
  const openInAgent = useCallback((target: string) => {
    /* Target may be an arch node id or a relay id; the agent view uses
       relayId to pick the session, so we set it directly. Mock data has no
       node→relay map, so for a node-id target the agent view falls back to
       the no-session pane — that's expected for v1. */
    setRelayId(target);
    setTab("agent");
  }, []);

  /* Attention badge derivation (R024-T3): once tickets carry a rigId, this
     groups handoff count per rig. Today tickets are rig-less, so the live
     count attributes to the active rig only and other rigs use the seeded
     mock value. The fallback chain keeps the UI exercisable without
     waiting on the backend rigId column. */
  const rigsWithAttention = useMemo(() => {
    const liveActiveCount = tickets.filter((t) => t.status === "handoff").length;
    return rigs.map((r) =>
      r.id === rigId
        ? { ...r, needsAttention: liveActiveCount || r.needsAttention }
        : r,
    );
  }, [tickets, rigId, rigs]);

  /* Lifted so the title-bar rig dot and the footer ConnectionStrip share a
     single heartbeat instead of double-probing the daemon. */
  const connectionStatus = useConnectionStatus(rigId);

  /* Rig-wide rule validation. The hook refetches on every index_finished
     so violations stay in sync with code edits. Browser stub returns
     `{ violations: [] }`, so this is a no-op outside Tauri. Both the
     Architecture and Board tabs read from the same array. */
  const { violations } = useValidate(rigId);

  return (
    <div className="flex h-full flex-col bg-paper text-ink">
      <TitleBar
        rigs={rigsWithAttention}
        activeRigId={rigId}
        onRigChange={setRigId}
        connectionState={connectionStatus.state}
        relays={tickets.filter((t) => t.itemType === "relay" || t.parent)}
        activeRelayId={relayId}
        onRelayChange={setRelayId}
        theme={theme}
        onThemeChange={setTheme}
        activeTab={tab}
        splitMode={splitMode}
        onSplitModeChange={setSplitMode}
      />
      <TabStrip active={tab} onChange={setTab} />
      <main className="min-h-0 flex-1 overflow-hidden">
        <TabPane
          tab={tab}
          rigId={rigId}
          relayId={relayId}
          setRelayId={setRelayId}
          tickets={tickets}
          setTickets={setTickets}
          archRoot={archRoot}
          setArchRoot={setArchRoot}
          archDepth={archDepth}
          setArchDepth={setArchDepth}
          jumpToFile={jumpToFile}
          openInAgent={openInAgent}
          violations={violations}
        />
      </main>
      <ConnectionStrip status={connectionStatus} />
    </div>
  );
}

function TabPane({
  tab,
  rigId,
  relayId,
  setRelayId,
  tickets,
  setTickets,
  archRoot,
  setArchRoot,
  archDepth,
  setArchDepth,
  jumpToFile,
  openInAgent,
  violations,
}: {
  tab: Tab;
  rigId: string;
  relayId: string | null;
  setRelayId: (id: string | null) => void;
  tickets: Ticket[];
  setTickets: (t: Ticket[]) => void;
  archRoot: string;
  setArchRoot: (s: string) => void;
  archDepth: number;
  setArchDepth: (n: number) => void;
  jumpToFile: (fileColon: string) => void;
  openInAgent: (target: string) => void;
  violations: import("./env/types").WireViolation[];
}) {
  switch (tab) {
    case "board":
      return (
        <Board
          rigId={rigId}
          tickets={tickets}
          onTicketsChange={setTickets}
          activeRelayId={relayId}
          onClearRelayFilter={() => setRelayId(null)}
          violations={violations}
        />
      );
    case "arch":
      return (
        <ArchView
          rigId={rigId}
          rootId={archRoot}
          onRootChange={setArchRoot}
          depth={archDepth}
          onDepthChange={setArchDepth}
          onJumpToFile={jumpToFile}
          onOpenInAgent={openInAgent}
          violations={violations}
        />
      );
    case "agent":
      return (
        <AgentView
          relayId={relayId}
          onSelectRelay={setRelayId}
          onJumpToFile={jumpToFile}
        />
      );
    case "terminal":
    case "preview":
    case "files":
    case "services":
      return <ComingSoon tab={tab} />;
  }
}

/* Run-cluster tabs ship as splash placeholders in v1 (see impl-guide §3 row
   for the run cluster). Each tab gets a column-flavoured wayfarer scene and
   a flavour caption so the empty state reads as deliberate, not unfinished. */
const RUN_TAB_SPLASH: Record<
  "terminal" | "preview" | "files" | "services",
  { variant: SplashVariant; caption: string; sub: string }
> = {
  terminal: {
    variant: "camp",
    caption: "Campfire not yet lit",
    sub: "A scrollback terminal with cross-linked file/grep results lands in v2 — for now, run commands in your usual shell.",
  },
  preview: {
    variant: "scroll",
    caption: "Pages still being scribed",
    sub: "Live preview of the rig's web output (dev server mirror) lands in v2. The agent can't drive a browser yet.",
  },
  files: {
    variant: "signpost",
    caption: "No map of the parchments",
    sub: "An in-app file browser with diff overlay lands in v2. Until then, jump from Architecture or Agent into your editor of choice.",
  },
  services: {
    variant: "anvil",
    caption: "The forge stands quiet",
    sub: "Long-running services (db, queues, workers) and their logs surface here in v2. For now, manage services from the rig directly.",
  },
};

function ComingSoon({
  tab,
}: {
  tab: "terminal" | "preview" | "files" | "services";
}) {
  const cfg = RUN_TAB_SPLASH[tab];
  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="font-display text-[24px] text-ink-2 [font-variant-caps:all-small-caps]">
          {tab}
        </div>
        <Splash variant={cfg.variant} caption={cfg.caption} sub={cfg.sub} />
      </div>
    </div>
  );
}
