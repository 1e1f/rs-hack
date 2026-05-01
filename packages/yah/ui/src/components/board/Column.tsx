import { useDroppable } from "@dnd-kit/core";
import { TicketCard } from "./TicketCard";
import { EmptyColumn } from "./EmptyColumn";
import { FillerSplash } from "./FillerSplash";
import { Icon } from "../shared/Glyph";
import type { WireViolation } from "../../env/types";
import type { ColumnKey, Ticket } from "../../types";

interface ColumnProps {
  /* Forwarded to each TicketCard so its Prompt button can call
     `arch.ticket_prompt` against the right rig. */
  rigId: string;
  columnKey: ColumnKey;
  label: string;
  eyebrow: string;
  tickets: Ticket[];
  /* True while a card is being dragged but this column is not a valid drop
     target. Dims the column to mirror the server's transition matrix. */
  disallowed?: boolean;
  /* yah:// router — forwarded into each card's expanded body so the
     References chips route through App. */
  onYahLink?: (href: string) => void;
  /* Pre-bucketed violations keyed by anchor file. Forwarded to each card so
     a ticket whose anchor lives in a violating file gets a small badge. */
  violationsByFile?: Map<string, WireViolation[]>;
}

/* Board column. Renders the illuminated drop-cap header + small-caps body +
   count chip + italic eyebrow, plus the dnd-kit droppable region. Hover and
   disallowed visuals are driven from Board state — the column itself doesn't
   know transition rules. */
export function Column({
  rigId,
  columnKey,
  label,
  eyebrow,
  tickets,
  disallowed = false,
  onYahLink,
  violationsByFile,
}: ColumnProps) {
  const { setNodeRef, isOver, active } = useDroppable({ id: columnKey });
  const isDragHover = isOver && !!active && !disallowed;

  const head = label.charAt(0);
  const tail = label.slice(1).toLowerCase();

  return (
    <div
      ref={setNodeRef}
      className={`flex h-full w-[280px] shrink-0 flex-col rounded-md border transition-[background-color,border-color,opacity] duration-100 ${
        isDragHover
          ? "border-accent bg-[color-mix(in_oklab,var(--color-accent)_9%,var(--color-paper-2))]"
          : "border-rule/50 bg-[color-mix(in_oklab,var(--color-paper-2)_35%,transparent)]"
      } ${disallowed ? "opacity-45" : ""}`}
    >
      <header className="flex items-center gap-2 border-b border-rule/50 px-3 pt-2.5 pb-2">
        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-1 text-ink">
            <span className="illum-cap text-[22px] leading-none">{head}</span>
            <span className="smallcaps text-[13px] font-medium tracking-[0.16em]">
              {tail}
            </span>
            <span className="ml-0.5 rounded-full border border-rule px-[7px] py-[2px] font-mono text-[11px] leading-none text-ink-3">
              {tickets.length}
            </span>
          </div>
          <div className="font-display text-[12px] italic text-ink-3">
            {eyebrow}
          </div>
        </div>
        <button
          className="rounded p-1 text-ink-3 hover:bg-vellum/40 hover:text-ink-2"
          title="Filter"
        >
          <Icon name="filter" size={12} />
        </button>
      </header>
      <div className="flex flex-1 flex-col gap-2 overflow-y-auto p-2">
        {tickets.map((t) => (
          <TicketCard
            key={t.id}
            rigId={rigId}
            ticket={t}
            columnEyebrow={eyebrow}
            onYahLink={onYahLink}
            violations={violationsByFile?.get(t.file) ?? undefined}
          />
        ))}
        {tickets.length === 0 && <EmptyColumn columnKey={columnKey} />}
        {tickets.length > 0 && tickets.length <= 2 && (
          <FillerSplash columnKey={columnKey} />
        )}
      </div>
    </div>
  );
}
