import { useMemo } from "react";
import { Message } from "./Message";
import { PromptBar } from "./PromptBar";
import { SessionHeader } from "./SessionHeader";
import { StreamingCursor } from "./StreamingCursor";
import type { Session, SessionEvent } from "../../types";

interface SessionPaneProps {
  session: Session;
  title?: string;
  onStop?: () => void;
  onSend?: (text: string) => void;
  onJumpToFile?: (fileColon: string) => void;
}

interface RenderedItem {
  event: SessionEvent;
  result?: SessionEvent;
}

export function SessionPane({
  session,
  title,
  onStop,
  onSend,
  onJumpToFile,
}: SessionPaneProps) {
  /* Pair each tool_use with its immediately-following tool result, and drop
     standalone tool result events from the rendered list. The tool surface
     (T3 ToolFrame + Read/Grep/Edit/Bash bodies) renders args + result as a
     single card. */
  const rendered = useMemo<RenderedItem[]>(() => {
    const items: RenderedItem[] = [];
    const evs = session.events;
    for (let i = 0; i < evs.length; i++) {
      const e = evs[i];
      if (e.role === "tool") continue;
      if (e.role === "assistant" && e.type === "tool_use") {
        const next = evs[i + 1];
        const result =
          next && next.role === "tool" && next.tool === e.tool ? next : undefined;
        items.push({ event: e, result });
        continue;
      }
      items.push({ event: e });
    }
    return items;
  }, [session.events]);

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <SessionHeader session={session} title={title} onStop={onStop} />
      <div className="min-h-0 flex-1 overflow-y-auto bg-paper/95 px-6 py-4">
        <div className="mx-auto flex w-full max-w-[820px] flex-col gap-3.5">
          {rendered.map(({ event, result }) => (
            <Message
              key={event.id}
              event={event}
              result={result}
              onJumpToFile={onJumpToFile}
            />
          ))}
          {session.status === "streaming" && <StreamingCursor />}
        </div>
      </div>
      <PromptBar
        onSend={onSend}
        onStop={onStop}
        streaming={session.status === "streaming"}
      />
    </div>
  );
}
