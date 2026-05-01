import type { ReactNode } from "react";
import { Glyph, Icon } from "./Glyph";
import type { IconName } from "./Glyph";

export type Hue =
  | "open"
  | "active"
  | "handoff"
  | "review"
  | "bug"
  | "feature"
  | "task"
  | "epic"
  | "neutral";

const HUE_TEXT: Record<Hue, string> = {
  open: "text-st-open",
  active: "text-st-active",
  handoff: "text-st-handoff",
  review: "text-st-review",
  bug: "text-st-bug",
  feature: "text-st-feature",
  task: "text-st-task",
  epic: "text-st-epic",
  neutral: "text-ink-3",
};

interface PillProps {
  hue?: Hue;
  dot?: boolean;
  children: ReactNode;
  title?: string;
  className?: string;
}

export function Pill({
  hue = "neutral",
  dot = true,
  children,
  title,
  className = "",
}: PillProps) {
  return (
    <span
      title={title}
      className={`inline-flex items-center gap-1.5 whitespace-nowrap rounded-full border border-current bg-vellum/70 px-[7px] py-[1px] font-display text-[11px] font-medium leading-none tracking-widest [font-variant-caps:all-small-caps] ${HUE_TEXT[hue]} ${className}`}
    >
      {dot && <span className="h-[5px] w-[5px] shrink-0 rounded-full bg-current" />}
      {children}
    </span>
  );
}

const STATUS: Record<string, { hue: Hue; label: string }> = {
  open: { hue: "open", label: "Open" },
  claimed: { hue: "active", label: "Claimed" },
  "in-progress": { hue: "active", label: "In flight" },
  handoff: { hue: "handoff", label: "Handoff" },
  review: { hue: "review", label: "Review" },
  done: { hue: "review", label: "Done" },
};

export function StatusPill({ status }: { status: string }) {
  const m = STATUS[status] ?? { hue: "neutral" as Hue, label: status };
  return <Pill hue={m.hue}>{m.label}</Pill>;
}

const KIND: Record<string, { hue: Hue; label: string }> = {
  feature: { hue: "feature", label: "Feature" },
  bug: { hue: "bug", label: "Bug" },
  task: { hue: "task", label: "Task" },
  epic: { hue: "epic", label: "Epic" },
};

export function KindPill({ kind }: { kind?: string }) {
  if (!kind) return null;
  const m = KIND[kind] ?? { hue: "neutral" as Hue, label: kind };
  return <Pill hue={m.hue}>{m.label}</Pill>;
}

interface KindBadgeProps {
  kind?: string;
  itemType?: "relay" | "ticket";
  isZone?: boolean;
  size?: number;
}

/* Glyph picker:
   - relay + isZone     → bookshelf (zone)
   - relay (regular)    → trumpet (relay), tinted by inner kind
   - ticket bug/feature → silhouette glyph
   - ticket task/epic   → line icon (no silhouette equivalent in set) */
export function KindBadge({
  kind,
  itemType,
  isZone,
  size = 12,
}: KindBadgeProps) {
  if (!kind && !itemType) return null;
  if (itemType === "relay") {
    const hue: Hue = isZone ? "epic" : KIND[kind ?? ""]?.hue ?? "feature";
    return (
      <span
        title={isZone ? "zone" : "relay"}
        className={`inline-flex ${HUE_TEXT[hue]}`}
      >
        <Glyph name={isZone ? "g-zone" : "g-relay"} size={size + 1} />
      </span>
    );
  }
  const hue: Hue = KIND[kind ?? ""]?.hue ?? "neutral";
  if (kind === "bug" || kind === "feature") {
    return (
      <span title={kind} className={`inline-flex ${HUE_TEXT[hue]}`}>
        <Glyph name={kind === "bug" ? "g-bug" : "g-feature"} size={size + 1} />
      </span>
    );
  }
  const iconName = (kind ?? "circle") as IconName;
  return (
    <span title={kind ?? undefined} className={`inline-flex ${HUE_TEXT[hue]}`}>
      <Icon name={iconName} size={size} />
    </span>
  );
}
