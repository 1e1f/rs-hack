import { useState } from "react";
import { Icon } from "../../shared/Glyph";
import { Avatar } from "./Avatar";

interface ThinkingMsgProps {
  content: string;
  /* Optional duration — surfaced in the eyebrow as "thinking · Ns". Mock data
     doesn't always carry it; backend pi-mono frames will. */
  duration?: number;
}

export function ThinkingMsg({ content, duration }: ThinkingMsgProps) {
  const [open, setOpen] = useState(false);
  return (
    <div className="flex gap-3">
      <Avatar kind="agent" muted />
      <div className="min-w-0 flex-1">
        <button
          onClick={() => setOpen((v) => !v)}
          className="flex items-center gap-1.5 text-ink-3 hover:text-ink-2"
        >
          <Icon name={open ? "chevron-down" : "chevron-right"} size={11} />
          <span className="eyebrow italic">
            thinking{duration != null && ` · ${duration}s`}
          </span>
        </button>
        {open && (
          <div className="mt-1 border-l border-dashed border-rule/60 pl-4 font-display text-[13px] italic leading-relaxed text-ink-3">
            {content}
          </div>
        )}
      </div>
    </div>
  );
}
