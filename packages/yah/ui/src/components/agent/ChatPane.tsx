import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { ApprovalRow } from "./ApprovalRow";
import { Message } from "./Message";
import { PromptBar } from "./PromptBar";
import { StreamingCursor } from "./StreamingCursor";
import { pairTools } from "./pairTools";
import { useChatSession } from "./useChatSession";
import { formatLink, formatMarkdown, formatManifest } from "./exportChat";

/* Two start modes — discriminated by the presence of `ticketId`. The
   chat-mode props (`engine`/`model`) and the relay-mode prop are
   mutually exclusive at construction time so AgentView never has to
   pass placeholder values. */
type ChatPaneProps = {
  rigId: string;
  onClose?: () => void;
  onJumpToFile?: (fileColon: string) => void;
  onYahLink?: (href: string) => void;
  /* Fires once per session when the post-Q+A auto-index lands — the
     parent uses this to refresh the SessionHistory rail so the new
     row's title appears without operator action. After the first fire
     the chat is index-stable; further refreshes are user-click only. */
  onAutoIndexed?: () => void;
} & (
  | {
      /* Unanchored chat. Engine spec — `"claude"`, `"openai"`,
         `"openai:gpt-4o"`, etc. — see backend `parse_engine_payload`. */
      ticketId?: undefined;
      engine: string;
      model?: string;
    }
  | {
      /* Relay-anchored. Daemon resolves engine + prelude from the
         ticket's `@yah:engine(...)` annotation. */
      ticketId: string;
      engine?: undefined;
      model?: undefined;
    }
);

/* Live agent session — orchestrates start/send/stop. Two modes:
   - unanchored chat (`engine` + optional `model`) → calls
     `useChatSession.start`
   - relay-anchored (`ticketId`) → calls `useChatSession.startRelay`,
     daemon assembles prelude from the ticket
   Owns one session, tears it down on unmount. */
export function ChatPane(props: ChatPaneProps) {
  const { rigId, onClose, onJumpToFile, onYahLink, onAutoIndexed } = props;
  const session = useChatSession();
  const startedRef = useRef(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const isRelay = props.ticketId !== undefined;
  const ticketId = props.ticketId;
  const engine = isRelay ? undefined : (props as { engine: string }).engine;
  const model = isRelay ? undefined : (props as { model?: string }).model;

  // One-shot start on mount; the cleanup on unmount tears the session
  // down so an unmounted ChatPane doesn't leave an orphaned daemon
  // session running with its keychain-read api_key still loaded.
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    if (isRelay && ticketId) {
      void session.startRelay(rigId, ticketId);
    } else if (engine) {
      void session.start(rigId, engine, model);
    }
    return () => {
      void session.stop();
    };
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [rigId, engine, model, ticketId, isRelay]);

  // Auto-scroll to bottom on new events / streaming deltas. Anchors at
  // the end of the message list — same heuristic as SessionPane, kept
  // tight so a long completion doesn't flicker past the user's
  // scroll position.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
  }, [session.events, session.status]);

  /* Surface the (one-shot) auto-index completion to the parent so the
     SessionHistory rail can re-fetch and the row's title appears. */
  useEffect(() => {
    if (session.lastIndexedAt != null) onAutoIndexed?.();
  }, [session.lastIndexedAt, onAutoIndexed]);

  const headerLabel = useMemo(
    () => session.engine ?? engine ?? (isRelay ? "agent" : ""),
    [session.engine, engine, isRelay],
  );

  const [exportFlash, setExportFlash] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const exportCtx = useMemo(
    () => ({ sessionId: session.sessionId, engine: session.engine }),
    [session.sessionId, session.engine],
  );

  async function copy(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      setExportFlash(`copied · ${label}`);
    } catch {
      setExportFlash("copy failed — clipboard unavailable");
    }
    setExportOpen(false);
    setTimeout(() => setExportFlash(null), 1600);
  }

  /* Same pairing as SessionPane — fold tool_use + matching tool result
     into a single rendered card so the chat surface mirrors the
     relay-anchored history rendering. */
  const rendered = useMemo(() => pairTools(session.events), [session.events]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <header className="flex items-center gap-2 border-b border-rule/60 bg-paper-2/50 px-4 py-2">
        <span
          className={`h-[7px] w-[7px] shrink-0 rounded-full ${
            session.status === "streaming"
              ? "bg-accent candle"
              : session.status === "error"
                ? "bg-oxblood"
                : "bg-ink-4"
          }`}
        />
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="font-display text-[13px] font-medium text-ink">
            {isRelay ? `Relay · ${ticketId}` : "Chat"}
          </span>
          <span className="font-mono text-[10.5px] text-ink-3">
            {headerLabel} · {isRelay ? "ticket-anchored" : "unanchored"} ·{" "}
            {session.status === "streaming"
              ? "streaming…"
              : session.status === "error"
                ? "error"
                : "idle"}
          </span>
        </div>
        {exportFlash && (
          <span className="font-mono text-[10.5px] text-ink-3" aria-live="polite">
            {exportFlash}
          </span>
        )}
        <div className="relative">
          <button
            onClick={() => setExportOpen((v) => !v)}
            className="flex items-center gap-1 rounded border border-rule/50 px-2 py-1 text-[11px] text-ink-2 hover:border-rule hover:bg-vellum/55"
            title="Copy chat"
          >
            <Icon name="copy" size={11} />
            copy
          </button>
          {exportOpen && (
            <div
              className="absolute right-0 top-full z-20 mt-1 w-56 rounded border border-rule/60 bg-paper-2 shadow-md"
              onMouseLeave={() => setExportOpen(false)}
            >
              <ExportItem
                label="link to .jsonl"
                hint="relative path"
                onClick={() => copy(formatLink(session.sessionId), "link")}
              />
              <ExportItem
                label="markdown"
                hint="user/agent turns + tools"
                onClick={() =>
                  copy(formatMarkdown(session.events, exportCtx), "markdown")
                }
              />
              <ExportItem
                label="manifest"
                hint="terse turn + tool log"
                onClick={() =>
                  copy(formatManifest(session.events, exportCtx), "manifest")
                }
              />
            </div>
          )}
        </div>
        {onClose && (
          <button
            onClick={() => {
              void session.stop();
              onClose();
            }}
            className="flex items-center gap-1 rounded border border-rule/50 px-2 py-1 text-[11px] text-ink-2 hover:border-rule hover:bg-vellum/55"
            title="Close chat"
          >
            <Icon name="x" size={11} />
            close
          </button>
        )}
      </header>

      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-y-auto bg-paper/95 px-6 py-4"
      >
        <div className="mx-auto flex w-full max-w-[820px] flex-col gap-3.5">
          {rendered.length === 0 && session.status !== "error" && (
            <div className="rounded border border-dashed border-rule/40 bg-vellum/30 px-4 py-3 text-[12px] italic text-ink-3">
              {session.sessionId
                ? "Type below to start the conversation. The agent has no ticket or doc attached — it's a free chat."
                : "Opening session…"}
            </div>
          )}
          {rendered.map(({ event, result }) =>
            event.role === "approval" ? (
              <ApprovalRow
                key={event.id}
                event={event}
                onDecide={session.decideApproval}
              />
            ) : (
              <Message
                key={event.id}
                event={event}
                result={result}
                onJumpToFile={onJumpToFile}
                onYahLink={onYahLink}
              />
            ),
          )}
          {session.status === "streaming" && <StreamingCursor />}
          {session.status === "error" && session.error && (
            <div className="flex flex-col gap-2 rounded border border-oxblood/50 bg-oxblood/10 px-3 py-2 text-[12px] text-oxblood">
              <div>{session.error}</div>
              {!session.sessionId && onClose && (
                <div>
                  <button
                    onClick={onClose}
                    className="rounded border border-oxblood/40 bg-paper-2 px-2 py-1 text-[11px] text-ink-2 hover:border-rule/60"
                  >
                    Back to picker (try a different model)
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>

      <PromptBar
        onSend={(text) => void session.send(text)}
        onStop={() => void session.stop()}
        streaming={session.status === "streaming"}
      />
    </div>
  );
}

function ExportItem({
  label,
  hint,
  onClick,
}: {
  label: string;
  hint: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full flex-col items-start gap-0 px-3 py-2 text-left hover:bg-vellum/55"
    >
      <span className="font-mono text-[11.5px] font-medium text-ink">{label}</span>
      <span className="font-mono text-[10px] text-ink-4">{hint}</span>
    </button>
  );
}
