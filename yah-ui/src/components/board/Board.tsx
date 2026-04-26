import { useMemo } from "react";
import { DndContext, type DragEndEvent } from "@dnd-kit/core";
import { Column } from "./Column";
import type { ColumnKey, Ticket, TicketStatus } from "../../types";

const COLUMNS: { key: ColumnKey; label: string; statuses: TicketStatus[] }[] = [
  { key: "zones", label: "Zones", statuses: [] }, // zones are derived, see below
  { key: "open", label: "Open", statuses: ["open"] },
  { key: "active", label: "Active", statuses: ["claimed", "in-progress"] },
  { key: "handoff", label: "Handoff", statuses: ["handoff"] },
  { key: "review", label: "Review", statuses: ["review", "done"] },
];

const STATUS_BY_COLUMN: Record<ColumnKey, TicketStatus> = {
  zones: "in-progress",
  open: "open",
  active: "in-progress",
  handoff: "handoff",
  review: "review",
};

interface BoardProps {
  tickets: Ticket[];
  onTicketsChange: (next: Ticket[]) => void;
}

export function Board({ tickets, onTicketsChange }: BoardProps) {
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
      else if (t.status === "claimed" || t.status === "in-progress") out.active.push(t);
      else if (t.status === "handoff") out.handoff.push(t);
      else if (t.status === "review" || t.status === "done") out.review.push(t);
    }
    return out;
  }, [tickets]);

  function handleDragEnd(e: DragEndEvent) {
    const { over, active } = e;
    if (!over) return;
    const targetCol = over.id as ColumnKey;
    if (targetCol === "zones") return; // zones not movable
    const id = active.id as string;
    const ticket = tickets.find((t) => t.id === id);
    if (!ticket || ticket.isZone) return;
    onTicketsChange(
      tickets.map((t) =>
        t.id === id ? { ...t, status: STATUS_BY_COLUMN[targetCol] } : t,
      ),
    );
  }

  return (
    <DndContext onDragEnd={handleDragEnd}>
      <div className="flex h-full gap-3 overflow-x-auto p-3">
        {COLUMNS.map((c) => (
          <Column
            key={c.key}
            columnKey={c.key}
            label={c.label}
            tickets={grouped[c.key]}
          />
        ))}
      </div>
    </DndContext>
  );
}
