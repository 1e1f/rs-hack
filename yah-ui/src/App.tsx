import { useCallback, useEffect, useState } from "react";
import { TitleBar } from "./components/shell/TitleBar";
import { TabStrip } from "./components/shell/TabStrip";
import { Board } from "./components/board/Board";
import { ArchView } from "./components/arch/ArchView";
import { AgentView } from "./components/agent/AgentView";
import { Splash, type SplashVariant } from "./components/shared/Splash";
import { mockArchSubgraph, mockRigs, mockTickets } from "./mock";
import type { Tab, Theme, Ticket } from "./types";

export function App() {
  const [tab, setTab] = useState<Tab>("board");
  const [theme, setTheme] = useState<Theme>("light");
  const [rigId, setRigId] = useState<string>(mockRigs[0].id);
  const [relayId, setRelayId] = useState<string | null>("R012-T2");
  const [splitMode, setSplitMode] = useState<Tab | null>(null);
  const [tickets, setTickets] = useState<Ticket[]>(mockTickets);
  /* Lifted out of ArchView so cross-tab nav (jumpToFile / NodeActionMenu's
     "Open in agent") can re-root the graph from anywhere. */
  const [archRoot, setArchRoot] = useState<string>(mockArchSubgraph.rootId);
  const [archDepth, setArchDepth] = useState<number>(2);

  /* Theme is a CSS-variable swap on [data-theme=...] — see globals.css. */
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

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

  return (
    <div className="flex h-full flex-col bg-paper text-ink">
      <TitleBar
        rigs={mockRigs}
        activeRigId={rigId}
        onRigChange={setRigId}
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
        />
      </main>
    </div>
  );
}

function TabPane({
  tab,
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
}: {
  tab: Tab;
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
}) {
  switch (tab) {
    case "board":
      return <Board tickets={tickets} onTicketsChange={setTickets} />;
    case "arch":
      return (
        <ArchView
          rootId={archRoot}
          onRootChange={setArchRoot}
          depth={archDepth}
          onDepthChange={setArchDepth}
          onJumpToFile={jumpToFile}
          onOpenInAgent={openInAgent}
        />
      );
    case "agent":
      return <AgentView relayId={relayId} onSelectRelay={setRelayId} />;
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
