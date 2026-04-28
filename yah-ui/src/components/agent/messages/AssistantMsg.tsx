import type { ReactNode } from "react";
import { Avatar } from "./Avatar";

interface AssistantMsgProps {
  content: string;
  /* Backed by App.tsx jumpToFile — switches to Architecture tab rooted at the
     given path:line. Optional so the component renders read-only when no
     handler is wired. */
  onJumpToFile?: (fileColon: string) => void;
}

/* Splits assistant text on backtick code spans. A span shaped `path:line`
   becomes a clickable chip that fires onJumpToFile; bare code stays as a
   plain code chip. Plain prose passes through unchanged. */
export function AssistantMsg({ content, onJumpToFile }: AssistantMsgProps) {
  return (
    <div className="flex gap-3">
      <Avatar kind="agent" />
      <div className="min-w-0 flex-1">
        <div className="eyebrow mb-1">Agent · claude</div>
        <div className="font-display text-[15px] leading-relaxed text-ink">
          {renderInline(content, onJumpToFile)}
        </div>
      </div>
    </div>
  );
}

const PATH_LINE = /^([^\s:`]+):(\d+)$/;

function renderInline(
  text: string,
  onJumpToFile?: (fileColon: string) => void,
): ReactNode[] {
  const parts = text.split(/(`[^`]+`)/g);
  return parts.map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`") && part.length >= 2) {
      const inner = part.slice(1, -1);
      if (PATH_LINE.test(inner) && onJumpToFile) {
        return (
          <button
            key={i}
            onClick={() => onJumpToFile(inner)}
            className="rounded border-b border-dashed border-current bg-accent/10 px-1.5 py-px font-mono text-[12.5px] text-accent hover:bg-accent/15"
          >
            {inner}
          </button>
        );
      }
      return (
        <code
          key={i}
          className="rounded bg-paper-3/30 px-1.5 py-px font-mono text-[12.5px]"
        >
          {inner}
        </code>
      );
    }
    return <span key={i}>{part}</span>;
  });
}
