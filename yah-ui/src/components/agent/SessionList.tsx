import { Icon } from "../shared/Glyph";
import { SectionHeader } from "../shared/SectionHeader";
import type { Session } from "../../types";

export interface SessionRow {
  relayId: string;
  title: string;
  status: Session["status"];
  lastActive: number;
  model: string;
}

interface SessionListProps {
  sessions: SessionRow[];
  activeRelayId: string | null;
  onSelect?: (relayId: string) => void;
}

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

export function SessionList({
  sessions,
  activeRelayId,
  onSelect,
}: SessionListProps) {
  return (
    <aside className="flex w-[260px] shrink-0 flex-col border-r border-rule/60 bg-paper-2/50">
      <div className="border-b border-rule/60 px-3.5 pt-3 pb-1.5">
        <SectionHeader>Sessions</SectionHeader>
        <label className="mt-1 flex items-center gap-1.5 rounded border border-rule/50 bg-vellum/60 px-2 py-1 focus-within:border-accent/60">
          <Icon name="search" size={12} className="text-ink-3" />
          <input
            placeholder="filter sessions…"
            className="flex-1 bg-transparent text-[12px] text-ink placeholder:text-ink-4 focus:outline-none"
          />
        </label>
      </div>
      <div className="flex flex-1 flex-col gap-1 overflow-y-auto p-2">
        {sessions.map((s) => {
          const isActive = s.relayId === activeRelayId;
          const streaming = s.status === "streaming";
          return (
            <button
              key={s.relayId}
              onClick={() => onSelect?.(s.relayId)}
              className={`flex flex-col rounded border px-2.5 py-2 text-left transition-colors ${
                isActive
                  ? "border-accent/70 bg-vellum shadow-[0_0_0_1px_color-mix(in_oklab,_var(--color-accent)_25%,_transparent)]"
                  : "border-rule/50 bg-vellum/70 hover:border-rule"
              }`}
            >
              <div className="mb-0.5 flex items-center gap-1.5">
                <span
                  className={`h-[7px] w-[7px] shrink-0 rounded-full ${
                    streaming ? "bg-accent candle" : "bg-ink-4"
                  }`}
                />
                <span className="font-mono text-[10.5px] text-ink-3">
                  {s.relayId}
                </span>
                <span className="ml-auto text-[10.5px] text-ink-3">
                  {relativeTime(s.lastActive)}
                </span>
              </div>
              <div className="mb-0.5 truncate font-display text-[13px] leading-snug text-ink">
                {s.title}
              </div>
              <div className="truncate font-mono text-[10.5px] text-ink-4">
                {s.model}
              </div>
            </button>
          );
        })}
        <button className="mt-1 flex items-center justify-center gap-1.5 rounded px-2 py-1.5 font-display text-[12px] italic text-ink-3 hover:bg-vellum/60">
          <Icon name="plus" size={11} />
          start session on selected relay
        </button>
      </div>
    </aside>
  );
}
