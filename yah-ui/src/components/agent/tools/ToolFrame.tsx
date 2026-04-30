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
  /* Tool result `ok`. Undefined while pending or for legacy mocks; `false`
     gets a red FAIL pill so retries downstream don't look like duplicates. */
  ok?: boolean;
  /* One-line summary stamped onto the result by the host
     (`agent_tools::stamp_smell`) — surfaced as muted subtext under the
     headline. Lets a recap quote a stable line and makes failure shape
     legible without expanding the card. */
  smell?: string;
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
  list_dir: { icon: "folder", label: "list_dir", hueClass: "text-ink-3" },
  arch_node: { icon: "atlas", label: "arch_node", hueClass: "text-midnight" },
  arch_neighbors: { icon: "branch", label: "arch_neighbors", hueClass: "text-midnight" },
  arch_subgraph: { icon: "atlas", label: "arch_subgraph", hueClass: "text-midnight" },
  arch_lookup: { icon: "search", label: "arch_lookup", hueClass: "text-midnight" },
  read_arch_doc: { icon: "scroll", label: "read_arch_doc", hueClass: "text-ink-3" },
};

export function ToolFrame({
  tool,
  headline,
  additions,
  deletions,
  exit,
  duration,
  ok,
  smell,
  defaultOpen,
  children,
}: ToolFrameProps) {
  const meta = META[tool];
  const failed = ok === false;
  const [open, setOpen] = useState(
    defaultOpen ?? (tool === "edit" || tool === "grep" || failed),
  );

  return (
    <div className="flex gap-3">
      <div className="flex w-7 shrink-0 justify-center pt-1.5">
        <span
          className={`h-3.5 w-3.5 rounded-full opacity-60 ${
            failed ? "bg-oxblood" : dotBg(tool)
          }`}
          aria-hidden
        />
      </div>
      <div
        className={`min-w-0 flex-1 rounded border ${
          failed ? "border-oxblood/50 bg-oxblood/5" : "border-rule/60 bg-vellum/60"
        }`}
      >
        <button
          type="button"
          onClick={() => setOpen((v) => !v)}
          className={`flex w-full flex-col items-stretch gap-0.5 px-2.5 py-1.5 text-left ${
            open ? `border-b ${failed ? "border-oxblood/40" : "border-rule/50"}` : ""
          }`}
        >
          <div className="flex items-center gap-2">
            <Icon name={meta.icon} size={12} className={meta.hueClass} />
            <span className="font-mono text-[12px] font-medium text-ink">
              {meta.label}
            </span>
            <span
              className={`min-w-0 flex-1 truncate font-mono text-[11px] ${
                failed ? "text-ink-3 line-through decoration-oxblood/60" : "text-ink-3"
              }`}
            >
              {headline}
            </span>
            {failed && (
              <span className="rounded border border-oxblood/50 bg-oxblood/10 px-1.5 py-0.5 font-mono text-[10px] font-medium text-oxblood">
                FAIL
              </span>
            )}
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
          </div>
          {smell && (
            <div className="ml-[18px] truncate font-mono text-[10.5px] text-ink-4">
              {smell}
            </div>
          )}
        </button>
        {open && children}
      </div>
    </div>
  );
}

function dotBg(tool: ToolKind): string {
  switch (tool) {
    case "read":
    case "list_dir":
    case "read_arch_doc":
      return "bg-ink-3";
    case "grep":
    case "arch_lookup":
    case "arch_node":
    case "arch_neighbors":
    case "arch_subgraph":
      return "bg-midnight";
    case "edit":
    case "write":
      return "bg-accent";
    case "bash":
      return "bg-brass";
  }
}
