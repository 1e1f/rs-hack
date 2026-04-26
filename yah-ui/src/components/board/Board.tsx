import { useMemo, useState } from "react";
import {
  DndContext,
  type DragEndEvent,
  type DragStartEvent,
} from "@dnd-kit/core";
import { Column } from "./Column";
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

interface BoardProps {
  tickets: Ticket[];
  onTicketsChange: (next: Ticket[]) => void;
}

export function Board({ tickets, onTicketsChange }: BoardProps) {
  const [activeId, setActiveId] = useState<string | null>(null);

  const grouped = useMemo(() => {
    const out: Record<ColumnKey, Ticket[]> = {
      zones: [],
      open: [],
      active: [],
      handoff: [],
      review: [],
    };
    for (const t of tickets) {
      if (t.isZone) out.zones.push(t);
      else if (t.status === "open") out.open.push(t);
      else if (t.status === "claimed" || t.status === "in-progress")
        out.active.push(t);
      else if (t.status === "handoff") out.handoff.push(t);
      else if (t.status === "review" || t.status === "done")
        out.review.push(t);
    }
    return out;
  }, [tickets]);

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

  return (
    <DndContext
      onDragStart={handleDragStart}
      onDragEnd={handleDragEnd}
      onDragCancel={handleDragCancel}
    >
      <div className="flex h-full gap-2.5 overflow-x-auto px-3 pt-3 pb-3.5">
        {COLUMNS.map((c) => {
          const disallowed =
            !!draggingTicket &&
            (c.key === "zones" ||
              !ALLOWED_TARGETS[draggingTicket.status].has(
                c.key as Exclude<ColumnKey, "zones">,
              ));
          return (
            <Column
              key={c.key}
              columnKey={c.key}
              label={c.label}
              eyebrow={c.eyebrow}
              tickets={grouped[c.key]}
              disallowed={disallowed}
            />
          );
        })}
      </div>
    </DndContext>
  );
}
