import { useEffect, useState } from "react";
import { TitleBar } from "./components/shell/TitleBar";
import { TabStrip } from "./components/shell/TabStrip";
import { Board } from "./components/board/Board";
import { ArchView } from "./components/arch/ArchView";
import { AgentView } from "./components/agent/AgentView";
import { mockRigs, mockTickets } from "./mock";
import type { Tab, Theme, Ticket } from "./types";

export function App() {
  const [tab, setTab] = useState<Tab>("board");
  const [theme, setTheme] = useState<Theme>("light");
  const [rigId, setRigId] = useState<string>(mockRigs[0].id);
  const [relayId, setRelayId] = useState<string | null>("R012-T2");
  const [splitMode, setSplitMode] = useState<Tab | null>(null);
  const [tickets, setTickets] = useState<Ticket[]>(mockTickets);

  /* Theme is a CSS-variable swap on [data-theme=...] — see globals.css. */
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

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
        <TabPane tab={tab} relayId={relayId} tickets={tickets} setTickets={setTickets} />
      </main>
    </div>
  );
}

function TabPane({
  tab,
  relayId,
  tickets,
  setTickets,
}: {
  tab: Tab;
  relayId: string | null;
  tickets: Ticket[];
  setTickets: (t: Ticket[]) => void;
}) {
  switch (tab) {
    case "board":
      return <Board tickets={tickets} onTicketsChange={setTickets} />;
    case "arch":
      return <ArchView />;
    case "agent":
      return <AgentView relayId={relayId} />;
    case "terminal":
    case "preview":
    case "files":
    case "services":
      return <ComingSoon tab={tab} />;
  }
}

function ComingSoon({ tab }: { tab: Tab }) {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="text-center">
        <div className="font-display text-[28px] text-ink-2 [font-variant-caps:all-small-caps]">
          {tab}
        </div>
        <div className="mt-1 font-display text-[13px] text-ink-3 italic">
          coming soon
        </div>
      </div>
    </div>
  );
}
