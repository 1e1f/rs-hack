import { Icon } from "../shared/Glyph";
import { Pill } from "../shared/Pill";
import { StatusStrip } from "./StatusStrip";
import type { Session } from "../../types";

interface SessionHeaderProps {
  session: Session;
  title?: string;
  onStop?: () => void;
}

const STATUS_COPY: Record<Session["status"], { hue: "active" | "review" | "handoff" | "bug" | "neutral"; label: string; pulse: boolean }> = {
  streaming: { hue: "active", label: "streaming", pulse: true },
  waiting: { hue: "handoff", label: "waiting", pulse: true },
  idle: { hue: "neutral", label: "idle", pulse: false },
  error: { hue: "bug", label: "error", pulse: false },
};

export function SessionHeader({ session, title, onStop }: SessionHeaderProps) {
  const s = STATUS_COPY[session.status];
  return (
    <div className="flex shrink-0 items-center gap-3.5 border-b border-rule/60 bg-paper-2/50 px-4 py-2.5">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="font-mono text-[11px] text-ink-3">
            {session.relayId}
          </span>
          <Pill hue={s.hue} dot={false}>
            {s.pulse && (
              <span className="candle mr-0.5 inline-block h-[5px] w-[5px] rounded-full bg-current" />
            )}
            {s.label}
          </Pill>
        </div>
        <div className="mt-0.5 truncate font-display text-[16px] leading-snug text-ink">
          {title ?? session.relayId}
        </div>
      </div>
      <StatusStrip session={session} />
      <button
        onClick={onStop}
        className="flex items-center gap-1.5 rounded border border-rule/60 bg-vellum/60 px-2.5 py-1 text-[12px] text-ink-2 hover:border-accent/60 hover:text-accent"
      >
        <Icon name="stop" size={12} />
        stop
      </button>
    </div>
  );
}
