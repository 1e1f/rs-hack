import { Icon } from "../shared/Glyph";
import type { Session, ToolKind } from "../../types";

interface StatusStripProps {
  session: Session;
}

interface DerivedToolState {
  tool: ToolKind | null;
  file: string | null;
}

/* Walks events backwards to surface the most recent tool_use's tool kind +
   path. Used by the live "current tool / last file" segments — same shape
   the backend will eventually push as a status frame. */
function deriveLatestTool(session: Session): DerivedToolState {
  for (let i = session.events.length - 1; i >= 0; i--) {
    const ev = session.events[i];
    if (ev.role === "assistant" && ev.type === "tool_use") {
      const args = ev.args ?? {};
      const file: string | null =
        (typeof args.path === "string" && args.path) ||
        (typeof args.cmd === "string" && args.cmd) ||
        (typeof args.glob === "string" && args.glob) ||
        null;
      return { tool: ev.tool, file };
    }
  }
  return { tool: null, file: null };
}

export function StatusStrip({ session }: StatusStripProps) {
  const { tool, file } = deriveLatestTool(session);

  return (
    <div className="flex items-center gap-2.5 rounded-full border border-rule/60 bg-vellum/55 px-2.5 py-1 text-[11px] text-ink-2">
      <span className="flex items-center gap-1">
        <Icon name="cpu" size={11} className="text-ink-3" />
        <span className="font-mono">{session.model}</span>
      </span>
      <Divider />
      <span className="flex items-center gap-1">
        <span className="text-ink-3">tokens</span>
        <span className="font-mono">{session.tokens.toLocaleString()}</span>
      </span>
      {tool && (
        <>
          <Divider />
          <span className="flex items-center gap-1">
            <Icon
              name={TOOL_ICON[tool] ?? "code"}
              size={11}
              className="text-ink-3"
            />
            <span className="font-mono">{tool}</span>
          </span>
        </>
      )}
      {file && (
        <>
          <Divider />
          <span className="flex min-w-0 items-center gap-1">
            <Icon name="file" size={11} className="text-ink-3" />
            <span className="truncate font-mono" title={file}>
              {file}
            </span>
          </span>
        </>
      )}
    </div>
  );
}

const TOOL_ICON: Record<ToolKind, "code" | "file" | "search" | "terminal"> = {
  read: "file",
  edit: "code",
  write: "code",
  grep: "search",
  bash: "terminal",
};

function Divider() {
  return <span className="h-3 w-px bg-rule/60" aria-hidden />;
}
