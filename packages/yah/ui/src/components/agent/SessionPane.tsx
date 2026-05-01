import { useMemo } from "react";
import { Message } from "./Message";
import { PromptBar } from "./PromptBar";
import { SessionHeader } from "./SessionHeader";
import { StreamingCursor } from "./StreamingCursor";
import { pairTools } from "./pairTools";
import type { Session } from "../../types";

interface SessionPaneProps {
  session: Session;
  title?: string;
  onStop?: () => void;
  onSend?: (text: string) => void;
  onJumpToFile?: (fileColon: string) => void;
  onYahLink?: (href: string) => void;
}

export function SessionPane({
  session,
  title,
  onStop,
  onSend,
  onJumpToFile,
  onYahLink,
}: SessionPaneProps) {
  const rendered = useMemo(() => pairTools(session.events), [session.events]);

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
              onYahLink={onYahLink}
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
