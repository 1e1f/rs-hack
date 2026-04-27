import { Message } from "./Message";
import { PromptInput } from "./PromptInput";
import { SessionHeader } from "./SessionHeader";
import type { Session } from "../../types";

interface SessionPaneProps {
  session: Session;
  title?: string;
  onStop?: () => void;
}

export function SessionPane({ session, title, onStop }: SessionPaneProps) {
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <SessionHeader session={session} title={title} onStop={onStop} />
      <div className="min-h-0 flex-1 overflow-y-auto bg-paper/95 px-6 py-4">
        <div className="mx-auto flex w-full max-w-[820px] flex-col gap-3.5">
          {session.events.map((e) => (
            <Message key={e.id} event={e} />
          ))}
        </div>
      </div>
      <PromptInput />
    </div>
  );
}
