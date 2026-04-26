import { useState } from "react";
import { ToolCall } from "./ToolCall";
import type { SessionEvent } from "../../types";

interface MessageProps {
  event: SessionEvent;
}

export function Message({ event }: MessageProps) {
  if (event.role === "user") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[80%] rounded border border-blue/20 bg-blue/5 px-3 py-2 text-[12px] text-text">
          {event.content}
        </div>
      </div>
    );
  }

  if (event.role === "tool") {
    // Tool results are rendered alongside their tool_use parent in ToolCall
    // when paired; for unpaired emit a small dim line.
    return (
      <div className="font-mono text-[10px] text-text-muted">
        ← {event.tool} result
      </div>
    );
  }

  if (event.role === "assistant") {
    if (event.type === "thinking") {
      return <ThinkingBlock content={event.content} />;
    }
    if (event.type === "text") {
      return (
        <div className="text-[12px] leading-relaxed text-text">
          {event.content}
        </div>
      );
    }
    if (event.type === "tool_use") {
      return <ToolCall use={event} />;
    }
  }

  return null;
}

function ThinkingBlock({ content }: { content: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded border border-border/60 bg-surface/40">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-[10px] uppercase tracking-wider text-text-muted hover:text-text-dim"
      >
        <span>{open ? "▾" : "▸"}</span>
        <span>thinking</span>
      </button>
      {open && (
        <div className="border-t border-border/60 px-3 py-2 text-[11px] italic leading-relaxed text-text-dim">
          {content}
        </div>
      )}
    </div>
  );
}
