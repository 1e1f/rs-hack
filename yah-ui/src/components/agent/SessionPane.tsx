import { Message } from "./Message";
import { PromptInput } from "./PromptInput";
import type { Session } from "../../types";

interface SessionPaneProps {
  session: Session;
}

export function SessionPane({ session }: SessionPaneProps) {
  return (
    <div className="flex min-w-0 flex-1 flex-col">
      <header className="flex h-9 shrink-0 items-center gap-3 border-b border-border bg-surface px-3">
        <span className="font-mono text-[12px] text-text">
          {session.relayId}
        </span>
        <span className="text-text-muted">·</span>
        <span className="font-mono text-[11px] text-text-dim">
          {session.model}
        </span>
        <span className="text-text-muted">·</span>
        <span className="text-[11px] text-text-dim">
          {session.tokens.toLocaleString()} tokens
        </span>
        {session.status === "streaming" && (
          <>
            <span className="text-text-muted">·</span>
            <span className="flex items-center gap-1.5 text-[11px] text-blue">
              <span className="pulse-dot h-1.5 w-1.5 rounded-full bg-blue" />
              streaming
            </span>
          </>
        )}
        <button className="ml-auto rounded px-2 py-1 text-[11px] text-text-muted hover:bg-elevated hover:text-text-dim">
          Stop
        </button>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 py-3">
        <div className="mx-auto flex max-w-3xl flex-col gap-3">
          {session.events.map((e) => (
            <Message key={e.id} event={e} />
          ))}
        </div>
      </div>
      <PromptInput />
    </div>
  );
}
