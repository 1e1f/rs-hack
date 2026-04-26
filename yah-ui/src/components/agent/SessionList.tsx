import type { Session } from "../../types";

interface SessionListProps {
  sessions: {
    relayId: string;
    title: string;
    status: Session["status"];
    lastActive: number;
    model: string;
  }[];
  activeRelayId: string | null;
}

const STATUS_COLOR: Record<Session["status"], string> = {
  streaming: "bg-blue pulse-dot",
  waiting: "bg-yellow",
  idle: "bg-text-muted",
  error: "bg-red",
};

function relativeTime(t: number): string {
  const diff = Date.now() - t;
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

export function SessionList({ sessions, activeRelayId }: SessionListProps) {
  return (
    <aside className="flex w-[280px] shrink-0 flex-col border-r border-border bg-surface">
      <header className="flex h-9 items-center justify-between border-b border-border px-3">
        <span className="text-[10px] font-medium uppercase tracking-wider text-text-muted">
          Sessions
        </span>
        <button
          className="text-[11px] text-text-muted hover:text-text-dim"
          title="New session"
        >
          +
        </button>
      </header>
      <div className="flex-1 overflow-y-auto">
        {sessions.map((s) => {
          const isActive = s.relayId === activeRelayId;
          return (
            <button
              key={s.relayId}
              className={`flex w-full flex-col gap-1 border-b border-border/50 px-3 py-2 text-left hover:bg-elevated ${
                isActive ? "bg-elevated" : ""
              }`}
            >
              <div className="flex items-center gap-2">
                <span
                  className={`h-1.5 w-1.5 rounded-full ${STATUS_COLOR[s.status]}`}
                />
                <span className="font-mono text-[11px] text-text-dim">
                  {s.relayId}
                </span>
                <span className="ml-auto text-[10px] text-text-muted">
                  {relativeTime(s.lastActive)}
                </span>
              </div>
              <div className="truncate text-[12px] text-text">{s.title}</div>
              <div className="font-mono text-[10px] text-text-muted">
                {s.model}
              </div>
            </button>
          );
        })}
      </div>
    </aside>
  );
}
