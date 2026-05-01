//! @yah:ticket(R028-F4, "AgentView wiring: markdown render + copy-as-source + yah:// link router")
//! @yah:assignee(agent:claude)
//! @yah:status(handoff)
//! @yah:phase(P1)
//! @yah:parent(R028)
//! @yah:verify("Cmd-C over rendered code block yields raw markdown; clicking yah:// link switches tab")
//! @yah:handoff("Markdown render + copy-as-source + yah:// link router shipped end-to-end; output-conventions stanza now lives as shared PreludeSectionKind::OutputConventions in yah-kg/src/prelude.rs so chat- and ticket-mode preludes teach the same convention. Two follow-ups carved out as sub-tickets R028-F4-T1 (relay-anchored start path) and R028-F4-T2 (state-lift). Tests green: yah-kg 76/76, yah-tauri 71/71, cargo check --workspace clean.")
//!
//! @yah:ticket(R028-F4-T1, "Relay-anchored session start path through useChatSession")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R028-F4)
//! @yah:verify("cd yah-ui && bunx tsc --noEmit && bun run build:js")
//! @yah:handoff("Relay-anchored start landed end-to-end. useChatSession got startRelay(rigId, ticketId) — calls env.rpc.agent.startSession (already in env adapter as a sibling to startChatSession; daemon-side agent_start_session resolves engine + prelude from the ticket's @yah:engine annotation). ChatPane swapped its props to a discriminated union: { engine, model } for unanchored chat OR { ticketId } for relay; the on-mount effect routes to start vs startRelay accordingly, header pill renders 'Relay · <ticketId> · ticket-anchored' vs 'Chat · <engine> · unanchored'. AgentView's ActiveChat became a tagged union too — kind: 'chat' | 'relay' — with handleStartRelay(ticketId) that idempotently surfaces an existing relay chat for (rig, ticket) instead of stacking duplicates (daemon-side start_session is intentionally non-idempotent for fresh prelude forks; renderer dedupes per-pane). NoSession's onStart prop now wires into handleStartRelay so the previously-dead 'start agent on <relayId>' button works. SessionList shows relay chats as 'relay · <ticketId>' rows so the rail discriminates them. typecheck clean; bun build:js 8.64MB / 2907 modules. While here also taught the chat Markdown renderer GFM pipe tables (yah-ui/src/components/agent/messages/Markdown.tsx) so qwen-style `| col | col |` tables render as <table> instead of pipe-mash paragraphs — table-row gating prevents paragraph greedy-gobble, cells route through renderInline so inline code/links/bold inside cells survive.")
//! @yah:next("Visual verification: open a ticket from the Board, hit 'start agent on R…' in NoSession; ChatPane mounts in relay mode; first turn streams; confirm the header reads 'Relay · <id>' and the engine label appears once start_session resolves.")
//! @yah:next("R028-F4-T2 (state lift) is the next sub-ticket — profile bubble-count perf with the current keep-mounted approach before refactoring useChatSession state into AgentView's Map<chatId, ChatState>. Likely a no-op until perf bites.")
//!
//! @yah:ticket(R028-F4-T2, "Lift useChatSession state into AgentView Map<chatId, ChatState>")
//! @yah:assignee(agent:claude)
//! @yah:status(handoff)
//! @yah:parent(R028-F4)
//! @yah:next("Profile bubble-count perf with the current keep-mounted-hide-non-selected approach to confirm DOM stacking is actually a problem before refactoring")
//! @yah:next("If needed: extract the events/sessionId/status state from useChatSession into a Map<chatId, ChatState> in AgentView, with a thin per-pane hook that reads/writes the entry")
//! @yah:handoff("Perf assessment without live profiling: mounted DOM scales C×N (chats × bubbles per chat). Realistic ceiling ~1800 bubbles (3 rigs × 3 chats × 200 turns) is fine for Chromium. Each Message renders the 560-line Markdown component, but display:none on hidden panes skips layout+paint, and useChatSession listeners filter by sessionId so cross-chat updates don't re-render. State lift would hoist per-pane refs (streamingEventId, toolByCallId, approvalEventIds, autoIndexFired, autoIndexTimer) into a Map<chatId, ChatState> and reroute the single agent:event listener — meaningful race-condition surface for an unmeasured win.")
//! @yah:next("Profile work parked on sub-ticket R028-F4-T2-T1; thresholds for refactor are baked into its verify clause. T2 stays in handoff until that profile lands a verdict.")
//!
//! @yah:ticket(R037-F8, "Party roster: Agents tab sub-section + Settings deep-link + working-set pin/favorite + concurrent-instance grouping per class + name addressing")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F9, "Function + Identity + Persona panels with live card preview (class + subclass + role_prompt + default_skills, independence rule)")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F10, "Portrait crop editor: aspect-locked card+face overlays, real-pixel previews, fork-on-shared-edit confirmation flow")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F13, "Pin overrides UI: chat-header pin marker, set/clear pin flow, ConfigSwitch pill rendering")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F16, "Connection health UI: roster tile overlays (wrench/hourglass/red), chat banner, send-while-unhealthy gating + Settings deep-link")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R037-F18, "Subclass editor: primary config (preset/endpoint/model/think) + structured fallback-rule form (metric/op/threshold/use_subclass)")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R037)
//!
//! @yah:ticket(R028-F4-T2-T1, "Live perf profile: tab-switch + streaming-turn under realistic load")
//! @yah:status(open)
//! @yah:parent(R028-F4-T2)
//! @yah:next("Open Chrome DevTools Performance panel; drive ~3 rigs × 3 chats × 200 bubbles each; record a tab-switch and a streaming turn")
//! @yah:next("Capture: tab-switch frame time, typing-latency-while-hidden-chat-streams, heap snapshot of detached Message subtrees")
//! @yah:verify("Decision recorded on R028-F4-T2 handoff: refactor if tab-switch > 50ms OR typing-latency > 16ms during background stream OR heap > 100MB detached, else close as no-op")

import { useEffect, useMemo, useRef, useState } from "react";
import { ChatPane } from "./ChatPane";
import { NoSession } from "./NoSession";
import { SessionList, type SessionRow } from "./SessionList";
import { SessionHistory } from "./SessionHistory";

interface AgentViewProps {
  rigId: string | null;
  relayId: string | null;
  onSelectRelay?: (relayId: string) => void;
  onJumpToFile?: (fileColon: string) => void;
  onOpenTerminalTab?: () => void;
  /* yah:// link router. Wires `[label](yah://file/<path>#L<n>)` and
     `[label](yah://arch/<symbol>)` clicks emitted by the agent into the
     App's tab navigation (Arch graph re-root, Files tab pickup). App
     owns the parse + dispatch; AgentView just plumbs it through to
     ChatPane → Message → Markdown. */
  onYahLink?: (href: string) => void;
}

/* Two flavours of chat — unanchored ("chat") or relay-anchored
   ("relay"). Relay sessions resolve their engine/model from the
   ticket's `@yah:engine(...)` annotation, daemon-side, so the
   renderer only needs the `ticketId`. */
type ActiveChat =
  | {
      kind: "chat";
      /* Renderer-side stable id. Independent of the backend's
         SessionId, since the chat's renderer pane exists *before*
         the backend session starts (engine/model picker →
         start_session resolves later). */
      id: string;
      rigId: string;
      engine: string;
      model?: string;
      startedAt: number;
    }
  | {
      kind: "relay";
      id: string;
      rigId: string;
      ticketId: string;
      startedAt: number;
    };

let chatIdCounter = 0;
const nextChatId = () => `chat:${++chatIdCounter}`;

export function AgentView({
  rigId,
  relayId,
  onSelectRelay,
  onJumpToFile,
  onOpenTerminalTab,
  onYahLink,
}: AgentViewProps) {
  /* Active chats list. Keeping all panes mounted (with visibility
     toggling) preserves each chat's per-pane state across switches —
     useChatSession's events array is per-hook, so a remount on
     selection change would drop the visible history. The Tauri event
     listener inside each useChatSession instance filters by sessionId
     so cross-session bleed isn't a concern.

     Chats are scoped per-rig: every chat carries the rigId active at
     creation; the rail and visible panes filter by current rigId.
     Chats from other rigs stay mounted but hidden so their event
     streams keep accumulating in the background. */
  const [chats, setChats] = useState<ActiveChat[]>([]);
  const [activeChatId, setActiveChatId] = useState<string | null>(null);
  /* Bumped each time any ChatPane reports its post-Q+A auto-index
     landed. Passed to SessionHistory as a refresh dep so the new row
     surfaces without operator action. */
  const [historyRefreshKey, setHistoryRefreshKey] = useState(0);
  const bumpHistory = () => setHistoryRefreshKey((n) => n + 1);

  /* Per-rig active-chat memory. Switching rigs restores whichever
     chat was last active for the new rig (or null = NoSession
     picker). Stored in a ref so updates don't cause renders. */
  const lastActiveByRigRef = useRef<Map<string | "null", string | null>>(new Map());
  const prevRigKeyRef = useRef<string | "null">(rigId ?? "null");

  useEffect(() => {
    const newKey = rigId ?? "null";
    const prevKey = prevRigKeyRef.current;
    if (prevKey === newKey) return;
    // Persist the previous rig's selection (only if it actually
    // belonged to that rig).
    const currentActive = activeChatId
      ? chats.find((c) => c.id === activeChatId)
      : null;
    if (currentActive && currentActive.rigId === prevKey) {
      lastActiveByRigRef.current.set(prevKey, currentActive.id);
    }
    // Restore (or clear) for the new rig.
    const restore = lastActiveByRigRef.current.get(newKey) ?? null;
    const restoreExists = restore && chats.some((c) => c.id === restore);
    setActiveChatId(restoreExists ? restore : null);
    prevRigKeyRef.current = newKey;
  }, [rigId, activeChatId, chats]);

  function handleStartChat(engine: string, model?: string) {
    if (!rigId) return;
    const id = nextChatId();
    setChats((prev) => [
      ...prev,
      { kind: "chat", id, rigId, engine, model, startedAt: Date.now() },
    ]);
    setActiveChatId(id);
  }

  function handleStartRelay(ticketId: string) {
    if (!rigId) return;
    /* Idempotent at the renderer level: if a relay chat is already
       open for this (rig, ticket), surface that one rather than
       stacking duplicates. The daemon-side `agent_start_session` is
       deliberately *not* idempotent (each call is a fresh prelude
       fork) — we just don't want a misclick to spawn extras. */
    const existing = chats.find(
      (c) => c.kind === "relay" && c.rigId === rigId && c.ticketId === ticketId,
    );
    if (existing) {
      setActiveChatId(existing.id);
      return;
    }
    const id = nextChatId();
    setChats((prev) => [
      ...prev,
      { kind: "relay", id, rigId, ticketId, startedAt: Date.now() },
    ]);
    setActiveChatId(id);
  }

  function handleCloseChat(id: string) {
    setChats((prev) => prev.filter((c) => c.id !== id));
    setActiveChatId((prev) => (prev === id ? null : prev));
  }

  /* Visible chats = those owned by the current rig. Hidden chats
     (other rigs) stay in the `chats` array and their <ChatPane>s
     stay mounted in the layered stack below so event streams keep
     flowing. */
  const visibleChats = useMemo(
    () => chats.filter((c) => c.rigId === rigId),
    [chats, rigId],
  );

  /* Project visible chats into the SessionList's row shape. The
     backend's `agent.listSessions()` is canonical for cross-process
     visibility (e.g. another window in the same rig), but this view
     only renders chats *this* AgentView started — that's sufficient
     until multi-window session sharing lands. */
  const sessionRows: SessionRow[] = visibleChats.map((c) =>
    c.kind === "chat"
      ? {
          relayId: c.id,
          title: c.engine + (c.model ? ` · ${c.model}` : ""),
          status: "idle",
          lastActive: c.startedAt,
          model: c.model ?? "—",
        }
      : {
          relayId: c.id,
          title: `relay · ${c.ticketId}`,
          status: "idle",
          lastActive: c.startedAt,
          model: "—",
        },
  );

  const activeChat = visibleChats.find((c) => c.id === activeChatId) ?? null;

  return (
    <div className="flex h-full min-h-0">
      <SessionList
        sessions={sessionRows}
        activeRelayId={activeChatId}
        onSelect={(id) => {
          // SessionList rows can be either local chats or (eventually)
          // relay-anchored sessions. Today the rows are 1:1 with chats,
          // so route a click to the chat selector. Relay rows would
          // route to onSelectRelay; the discriminator lands when relay
          // sessions are real.
          if (visibleChats.some((c) => c.id === id)) {
            setActiveChatId(id);
          } else {
            onSelectRelay?.(id);
          }
        }}
        onNewChat={() => {
          // Drop selection so the NoSession picker renders. Existing
          // chats stay mounted under hidden visibility so their state
          // survives.
          setActiveChatId(null);
        }}
      >
        <SessionHistory rigId={rigId} refreshKey={historyRefreshKey} />
      </SessionList>
      {/* Keep all chats mounted (across rigs); toggle visibility on
          the active one. Other-rig chats stay rendered with
          `hidden` so their useChatSession event subscriptions keep
          accumulating in the background, and switching rigs picks
          up the previously-active chat for the new rig with no
          remount or history loss. */}
      {chats.map((c) => (
        <div
          key={c.id}
          className={`flex flex-1 min-w-0 ${
            c.id === activeChatId && c.rigId === rigId ? "" : "hidden"
          }`}
        >
          {c.kind === "chat" ? (
            <ChatPane
              rigId={c.rigId}
              engine={c.engine}
              model={c.model}
              onClose={() => handleCloseChat(c.id)}
              onJumpToFile={onJumpToFile}
              onYahLink={onYahLink}
              onAutoIndexed={bumpHistory}
            />
          ) : (
            <ChatPane
              rigId={c.rigId}
              ticketId={c.ticketId}
              onClose={() => handleCloseChat(c.id)}
              onJumpToFile={onJumpToFile}
              onYahLink={onYahLink}
              onAutoIndexed={bumpHistory}
            />
          )}
        </div>
      ))}
      {!activeChat && (
        <NoSession
          relayId={relayId}
          rigId={rigId}
          onStart={handleStartRelay}
          onStartChat={handleStartChat}
          onOpenTerminalTab={onOpenTerminalTab}
        />
      )}
    </div>
  );
}
