import { useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { KindBadge, StatusPill } from "../shared/Pill";
import { Glyph, Icon } from "../shared/Glyph";
import { CardExpanded } from "./CardExpanded";
import { CountChip } from "./CountChip";
import type { Ticket } from "../../types";

interface TicketCardProps {
  ticket: Ticket;
  columnEyebrow?: string;
}

/* Mapping from raw status to the column-eyebrow string. Used to suppress the
   StatusPill when it would just echo the column header — a "Handoff" pill in
   the Ready-to-review column is noise; a "Claimed" pill in In-flight still
   adds info, so we keep it. */
const STATUS_LABEL: Record<string, string> = {
  open: "Awaiting pickup",
  claimed: "In flight",
  "in-progress": "In flight",
  handoff: "Ready to review",
  review: "Validated",
  done: "Validated",
};

export function TicketCard({ ticket: t, columnEyebrow }: TicketCardProps) {
  const [expanded, setExpanded] = useState(false);
  const isRelay = t.itemType === "relay";
  const isZone = !!t.isZone;
  const isEpic = t.kind === "epic";
  const hasAgent =
    isRelay && typeof t.assignee === "string" && t.assignee.startsWith("agent:");
  /* "Live" = a relay an agent is currently driving. We don't animate per-ticket;
     ownership granularity is the relay. */
  const live =
    hasAgent &&
    (t.status === "in-progress" ||
      t.status === "claimed" ||
      t.status === "handoff");
  /* Schema-smell: a relay carrying both child relays AND child tickets.
     childCounts.{open,active,handoff} sums all live children, so a positive
     diff between that sum and relays count means there are loose tickets too. */
  const cc = t.childCounts;
  const totalLive = cc ? cc.open + cc.active + cc.handoff : 0;
  const relayCount = cc?.relays ?? 0;
  const mixed =
    isRelay && cc != null && relayCount > 0 && totalLive - relayCount > 0;
  /* Hide status pill when it just echoes the column eyebrow. */
  const echoesColumn =
    !!columnEyebrow && STATUS_LABEL[t.status] === columnEyebrow;

  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({ id: t.id, disabled: isZone });

  const style = {
    transform: CSS.Translate.toString(transform),
    opacity: isDragging ? 0.55 : 1,
  };

  return (
    <article
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      className={`lift relative rounded-md border bg-vellum text-[12px] shadow-[0_1px_2px_rgba(70,45,20,0.08)] ${
        isRelay
          ? "border-[color-mix(in_oklab,var(--color-accent)_22%,var(--color-rule))] pl-3.5"
          : "border-rule/50 pl-3"
      } px-3 py-2.5 ${isZone ? "cursor-pointer" : "cursor-grab active:cursor-grabbing"} ${
        isDragging ? "dragging" : ""
      }`}
    >
      {isRelay && (
        <span
          aria-hidden
          className={`absolute left-0 top-0 bottom-0 w-1 rounded-[2px] ${live ? "candle-rail" : ""}`}
          style={{
            background:
              "linear-gradient(to bottom, transparent, var(--color-accent), transparent)",
            filter: live
              ? "drop-shadow(0 0 6px color-mix(in oklab, var(--color-accent) 65%, transparent))"
              : "none",
          }}
        />
      )}
      {isRelay && (
        <span
          aria-hidden
          className="absolute -top-px right-3"
          style={{
            width: isEpic ? 9 : 7,
            height: isEpic ? 16 : 12,
            background: isEpic
              ? "linear-gradient(to right, var(--color-accent-2) 0%, var(--color-accent) 35%, color-mix(in oklab, var(--color-accent) 50%, var(--color-vellum)) 50%, var(--color-accent) 65%, var(--color-accent-2) 100%)"
              : "var(--color-accent)",
            clipPath:
              "polygon(0 0, 100% 0, 100% 100%, 50% calc(100% - 4px), 0 100%)",
            filter: live
              ? "drop-shadow(0 1px 2px color-mix(in oklab, var(--color-accent) 60%, transparent))"
              : "drop-shadow(0 1px 1px rgba(0,0,0,0.1))",
          }}
        />
      )}

      <header className="flex items-start gap-2">
        <button
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((v) => !v);
          }}
          onPointerDown={(e) => e.stopPropagation()}
          className="mt-0.5 text-ink-3 hover:text-ink-2"
          title={expanded ? "Collapse" : "Expand"}
        >
          <Icon name={expanded ? "chevron-down" : "chevron-right"} size={12} />
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <KindBadge kind={t.kind} itemType={t.itemType} isZone={isZone} />
            <span className="whitespace-nowrap font-mono text-[11px] text-ink-3">
              {t.id}
              {t.parent && (
                <span className="opacity-55"> ↪ {t.parent}</span>
              )}
            </span>
            {!echoesColumn && !isZone && <StatusPill status={t.status} />}
            {mixed && (
              <span
                title="Mixed children — this relay holds both child relays and loose tickets. Move tickets into a child relay."
                className="inline-flex cursor-help items-center text-st-handoff"
              >
                <Icon name="bug" size={12} />
              </span>
            )}
            {t.phase && (
              <span className="font-mono text-[11px] text-ink-3">
                {t.phase}
              </span>
            )}
          </div>
          <h3 className="mt-1 font-display text-[15px] font-medium leading-[1.3] text-ink">
            {t.title}
          </h3>

          {t.assignee && (
            <div className="mt-1 flex items-center gap-1.5 text-[11px] text-ink-3">
              <span className="h-1.5 w-1.5 rounded-full bg-[color-mix(in_oklab,var(--color-midnight)_70%,var(--color-ink))]" />
              <span className="font-mono">{t.assignee}</span>
            </div>
          )}

          {isZone && t.childCounts && (
            <ZoneChildCounts counts={t.childCounts} />
          )}
        </div>
      </header>

      {expanded && <CardExpanded ticket={t} />}

      <footer className="mt-2 flex items-center justify-end gap-1 border-t border-rule/50 pt-1.5">
        <span
          className="flex-1 truncate font-mono text-[11px] text-ink-3"
          title={`${t.file}:${t.line}`}
        >
          {t.file}:{t.line}
        </span>
        <CardButton title="Open in Architecture">
          <Glyph name="g-arch" size={12} /> graph
        </CardButton>
        <CardButton title="Open in Agent">
          <Glyph name="g-agent" size={12} /> agent
        </CardButton>
        <CardButton title="Copy pickup prompt">
          <Icon name="copy" size={11} /> prompt
        </CardButton>
      </footer>
    </article>
  );
}

function ZoneChildCounts({
  counts,
}: {
  counts: { open: number; active: number; handoff: number };
}) {
  return (
    <div className="mt-1.5 flex items-baseline">
      <CountChip n={counts.open} label="open" hue="open" />
      <CountChip n={counts.active} label="active" hue="active" />
      <CountChip n={counts.handoff} label="handoff" hue="handoff" />
    </div>
  );
}

function CardButton({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <button
      title={title}
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
      className="inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-ink-3 hover:bg-vellum-2 hover:text-ink-2"
    >
      {children}
    </button>
  );
}
