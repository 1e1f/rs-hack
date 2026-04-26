import { useDroppable } from "@dnd-kit/core";
import { TicketCard } from "./TicketCard";
import type { ColumnKey, Ticket } from "../../types";

interface ColumnProps {
  columnKey: ColumnKey;
  label: string;
  tickets: Ticket[];
}

export function Column({ columnKey, label, tickets }: ColumnProps) {
  const { setNodeRef, isOver } = useDroppable({ id: columnKey });

  return (
    <div
      ref={setNodeRef}
      className={`flex h-full w-[300px] shrink-0 flex-col rounded border border-border bg-surface transition-colors ${
        isOver ? "border-blue/60 bg-elevated" : ""
      }`}
    >
      <header className="flex h-9 items-center justify-between border-b border-border px-3">
        <span className="text-[12px] font-medium uppercase tracking-wider text-text-dim">
          {label}
        </span>
        <span className="rounded bg-border px-1.5 py-0.5 text-[10px] text-text-dim">
          {tickets.length}
        </span>
      </header>
      <div className="flex flex-1 flex-col gap-2 overflow-y-auto p-2">
        {tickets.length === 0 ? (
          <div className="py-6 text-center text-[11px] text-text-muted">
            (empty)
          </div>
        ) : (
          tickets.map((t) => <TicketCard key={t.id} ticket={t} />)
        )}
      </div>
    </div>
  );
}
