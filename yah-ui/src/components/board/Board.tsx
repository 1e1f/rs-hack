//!
//!

import { useMemo, useState } from "react";
import {
  DndContext,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { Column } from "./Column";
import { Splash } from "../shared/Splash";
import type { WireViolation } from "../../env/types";
import type { ColumnKey, Ticket, TicketStatus } from "../../types";

const COLUMNS: { key: ColumnKey; label: string; eyebrow: string }[] = [
  { key: "zones", label: "Zones", eyebrow: "Coordination" },
  { key: "open", label: "Open", eyebrow: "Awaiting pickup" },
  { key: "active", label: "Active", eyebrow: "In flight" },
  { key: "handoff", label: "Handoff", eyebrow: "Ready to review" },
  { key: "review", label: "Review", eyebrow: "Validated" },
];

/* Column → status mapping for the canonical drop target. The "active" column
   maps to in-progress (claim-on-pickup is server-side; the UI just lands the
   card there). */
const STATUS_BY_COLUMN: Record<Exclude<ColumnKey, "zones">, TicketStatus> = {
  open: "open",
  active: "in-progress",
  handoff: "handoff",
  review: "review",
};

/* Lifecycle rules — Rule01–Rule04 in CLAUDE.md. The board mirrors the server's
   transition matrix so disallowed columns dim during drag (server returns 409
   on any other transition). Zones are never a valid drop target. */
const ALLOWED_TARGETS: Record<TicketStatus, Set<Exclude<ColumnKey, "zones">>> =
  {
    open: new Set(["active"]),
    claimed: new Set(["open", "handoff", "review"]),
    "in-progress": new Set(["open", "handoff", "review"]),
    handoff: new Set(["active", "review"]),
    review: new Set(["handoff"]),
    done: new Set(["handoff"]),
  };

const ID_COLLATOR = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function byRecencyThenId(a: Ticket, b: Ticket): number {
  const ta = a.lastModifiedTs ?? 0;
  const tb = b.lastModifiedTs ?? 0;
  if (ta !== tb) return tb - ta;
  return ID_COLLATOR.compare(a.id, b.id);
}

interface BoardProps {
  /* Rig the Board is currently scoped to. Threaded down to TicketCard so
     the Prompt button can call `arch.ticket_prompt` against the right
     daemon. */
  rigId: string;
  tickets: Ticket[];
  onTicketsChange: (next: Ticket[]) => void;
  /* When set, the Board narrows to the selected relay + its direct children
     (id-match OR parent-match). Null/undefined shows everything. The relay
     selector in TitleBar is the source of truth; clearing the filter goes
     through `onClearRelayFilter`. */
  activeRelayId?: string | null;
  onClearRelayFilter?: () => void;
  /* Rule violations for the current rig — surfaced as a header strip and
     per-card badges. Tickets and `@yah:rule` annotations co-locate by file
     today, so we attribute violations to tickets by anchor-file match. */
  violations?: WireViolation[];
}

export function Board({
  rigId,
  tickets,
  onTicketsChange,
  activeRelayId,
  onClearRelayFilter,
  violations,
}: BoardProps) {
  const [activeId, setActiveId] = useState<string | null>(null);

  /* Pointer activation distance — without this, any micro-jitter on
     mousedown starts a drag and swallows the click, so click-to-expand
     on a card never fires. 4px is small enough to feel instant on a real
     drag and large enough to absorb tremor on a stationary press. */
  const dndSensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 4 } }),
  );

  /* Apply the relay filter once, before grouping. Drag-end logic still
     references the full `tickets` list — the filter is presentation-only,
     so a card moved into a filtered-out column doesn't vanish. */
  const filteredTickets = useMemo(() => {
    if (!activeRelayId) return tickets;
    return tickets.filter(
      (t) => t.id === activeRelayId || t.parent === activeRelayId,
    );
  }, [tickets, activeRelayId]);

  const grouped = useMemo(() => {
    const out: Record<ColumnKey, Ticket[]> = {
      zones: [],
      open: [],
      active: [],
      handoff: [],
      review: [],
    };
    for (const t of filteredTickets) {
      if (t.isZone) out.zones.push(t);
      else if (t.status === "open") out.open.push(t);
      else if (t.status === "claimed" || t.status === "in-progress")
        out.active.push(t);
      else if (t.status === "handoff") out.handoff.push(t);
      else if (t.status === "review" || t.status === "done")
        out.review.push(t);
    }
    // Sort within each column: most-recently-touched first; ties break on
    // id with a natural-numeric compare so R025-T2 lands before R025-T10.
    for (const key of Object.keys(out) as ColumnKey[]) {
      out[key].sort(byRecencyThenId);
    }
    return out;
  }, [filteredTickets]);

  const isFiltered = activeRelayId != null;

  /* Pre-bucket violations by anchor file so each TicketCard's lookup is O(1).
     A rule's anchor is a structural node, but tickets and rules co-locate in
     the same file (the ticket's lex-first anchor matches the file the rule
     was authored in), so file-match is the cheapest reasonable attribution
     until the daemon ships an explicit ticket→violation index. */
  const violationsByFile = useMemo(() => {
    const map = new Map<string, WireViolation[]>();
    if (!violations) return map;
    for (const v of violations) {
      const arr = map.get(v.anchor_file) ?? [];
      arr.push(v);
      map.set(v.anchor_file, arr);
    }
    return map;
  }, [violations]);
  const totalViolations = violations?.length ?? 0;
  const errorViolations = useMemo(
    () =>
      violations?.filter((v) => v.severity === "error").length ?? 0,
    [violations],
  );

  const draggingTicket = activeId
    ? tickets.find((t) => t.id === activeId) ?? null
    : null;

  function handleDragStart(e: DragStartEvent) {
    setActiveId(e.active.id as string);
  }

  function handleDragEnd(e: DragEndEvent) {
    const { over, active } = e;
    setActiveId(null);
    if (!over) return;
    const targetCol = over.id as ColumnKey;
    if (targetCol === "zones") return;
    const id = active.id as string;
    const ticket = tickets.find((t) => t.id === id);
    if (!ticket || ticket.isZone) return;
    if (!ALLOWED_TARGETS[ticket.status].has(targetCol)) return;
    onTicketsChange(
      tickets.map((t) =>
        t.id === id
          ? { ...t, status: STATUS_BY_COLUMN[targetCol] }
          : t,
      ),
    );
  }

  function handleDragCancel() {
    setActiveId(null);
  }

  if (isFiltered && filteredTickets.length === 0) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-2">
          <Splash
            variant="signpost"
            caption={`No children for ${activeRelayId}`}
            sub="The selected relay has no live tickets in this rig. Clear the filter to see everything, or pick a different relay from the title bar."
          />
          {onClearRelayFilter && (
            <button
              type="button"
              onClick={onClearRelayFilter}
              className="rounded border border-rule px-3 py-1 text-[12px] text-ink-2 hover:bg-vellum-2"
            >
              Clear filter
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <DndContext
      sensors={dndSensors}
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      <div className="flex h-full flex-col">
        {totalViolations > 0 && (
          <ValidationStrip
            total={totalViolations}
            errors={errorViolations}
            violations={violations ?? []}
          />
        )}
        <div className="flex flex-1 gap-2.5 overflow-x-auto px-3 pt-3 pb-3.5">
          {COLUMNS.map((c) => {
            const disallowed =
              !!draggingTicket &&
              (c.key === "zones" ||
                !ALLOWED_TARGETS[draggingTicket.status].has(
                  c.key as Exclude<ColumnKey, "zones">,
                ));
            /* Stretch from R025-T4: when a relay filter is active, hint it in
               every column eyebrow so the narrowed view doesn't read as a
               daemon glitch. */
            const eyebrow = isFiltered
              ? `${c.eyebrow} · filtered to ${activeRelayId}`
              : c.eyebrow;
            return (
              <Column
                key={c.key}
                rigId={rigId}
                columnKey={c.key}
                label={c.label}
                eyebrow={eyebrow}
                tickets={grouped[c.key]}
                disallowed={disallowed}
                violationsByFile={violationsByFile}
              />
            );
          })}
        </div>
      </div>
    </DndContext>
  );
}

/* Top-of-board summary of `arch.validate` output. The full list collapses
   behind a click so a quiet rig stays visually quiet — only the count +
   severity dot is always visible. */
function ValidationStrip({
  total,
  errors,
  violations,
}: {
  total: number;
  errors: number;
  violations: WireViolation[];
}) {
  const [open, setOpen] = useState(false);
  const tone = errors > 0 ? "text-st-bug" : "text-st-handoff";
  return (
    <div className="border-b border-rule/50 bg-[color-mix(in_oklab,var(--color-paper-2)_55%,transparent)] px-3 py-1.5 text-[11px]">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className={`flex w-full items-center gap-2 ${tone} hover:underline`}
      >
        <span aria-hidden>{errors > 0 ? "✗" : "⚠"}</span>
        <span className="font-mono">
          {total} rule violation{total === 1 ? "" : "s"}
          {errors > 0 ? ` (${errors} error${errors === 1 ? "" : "s"})` : ""}
        </span>
        <span className="ml-auto text-ink-3">{open ? "Hide" : "Show"}</span>
      </button>
      {open && (
        <ul className="mt-2 max-h-40 space-y-1 overflow-y-auto pl-5 text-ink-2">
          {violations.map((v, i) => (
            <li key={`${v.anchor}-${i}`} className="font-mono">
              <span className={v.severity === "error" ? "text-st-bug" : "text-st-handoff"}>
                {v.severity === "error" ? "✗" : "⚠"}
              </span>{" "}
              <span className="text-ink-3">{v.rule_kind}</span>{" "}
              <span>{v.message}</span>{" "}
              <span className="text-ink-4">
                ({v.anchor_file}:{v.anchor_line})
              </span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
