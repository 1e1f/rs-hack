import { useState, type ReactNode } from "react";
import { Icon, type IconName } from "../../shared/Glyph";
import type { ToolKind } from "../../../types";

interface ToolFrameProps {
  tool: ToolKind;
  /* Headline mono text — path / pattern / cmd, derived per-tool by the caller. */
  headline?: ReactNode;
  /* Right-aligned meta tags surfaced in the header. EditDiff sets +N/-M;
     BashOutput sets exit/duration. Read/Grep usually leave these undefined. */
  additions?: number;
  deletions?: number;
  exit?: number;
  duration?: number;
  /* Edit + grep open by default — the diff and matches are the load-bearing
     content; read and bash are summary-shaped and stay closed. */
  defaultOpen?: boolean;
  children?: ReactNode;
}

const META: Record<ToolKind, { icon: IconName; label: string; hueClass: string }> = {
  read: { icon: "file", label: "read", hueClass: "text-ink-3" },
  grep: { icon: "search", label: "grep", hueClass: "text-midnight" },
  edit: { icon: "code", label: "edit", hueClass: "text-accent" },
  bash: { icon: "terminal", label: "bash", hueClass: "text-brass" },
  write: { icon: "file", label: "write", hueClass: "text-accent" },
};

export function ToolFrame({
  tool,
  headline,
  additions,
  deletions,
  exit,
  duration,
  defaultOpen,
  children,
}: ToolFrameProps) {
  const meta = META[tool];
  const [open, setOpen] = useState(
    defaultOpen ?? (tool === "edit" || tool === "grep"),
  );

  return (
    <div className="flex gap-3">
      <div className="flex w-7 shrink-0 justify-center pt-1.5">
        <span
          className={`h-3.5 w-3.5 rounded-full opacity-60 ${dotBg(tool)}`}
          aria-hidden
        />
      </div>
      <div className="min-w-0 flex-1 rounded border border-rule/60 bg-vellum/60">
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className={`flex w-full items-center gap-2 px-2.5 py-1.5 text-left ${
            open ? "border-b border-rule/50" : ""
          }`}
        >
          <Icon name={meta.icon} size={12} className={meta.hueClass} />
          <span className="font-mono text-[12px] font-medium text-ink">
            {meta.label}
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-ink-3">
            {headline}
          </span>
          {additions != null && (
            <span className="font-mono text-[11px] text-st-review">
              +{additions}
            </span>
          )}
          {deletions != null && deletions > 0 && (
            <span className="font-mono text-[11px] text-st-bug">−{deletions}</span>
          )}
          {exit != null && (
            <span
              className={`font-mono text-[11px] ${
                exit === 0 ? "text-st-review" : "text-st-bug"
              }`}
            >
              exit {exit}
            </span>
          )}
          {duration != null && (
            <span className="font-mono text-[11px] text-ink-4">{duration}s</span>
          )}
          <Icon
            name={open ? "chevron-up" : "chevron-down"}
            size={11}
            className="text-ink-4"
          />
        </button>
        {open && children}
      </div>
    </div>
  );
}

function dotBg(tool: ToolKind): string {
  switch (tool) {
    case "read":
      return "bg-ink-3";
    case "grep":
      return "bg-midnight";
    case "edit":
    case "write":
      return "bg-accent";
    case "bash":
      return "bg-brass";
  }
}
