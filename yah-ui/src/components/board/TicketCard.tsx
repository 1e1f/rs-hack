import { useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { KindPill, StatusPill } from "../shared/Pill";
import type { Ticket } from "../../types";

interface TicketCardProps {
  ticket: Ticket;
}

export function TicketCard({ ticket }: TicketCardProps) {
  const [expanded, setExpanded] = useState(false);
  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({ id: ticket.id, disabled: ticket.isZone });

  const style = {
    transform: CSS.Translate.toString(transform),
    opacity: isDragging ? 0.4 : 1,
  };

  return (
    <article
      ref={setNodeRef}
      style={style}
      className={`group rounded border border-border bg-elevated p-2 text-[12px] ${
        ticket.isZone ? "border-purple/30" : "hover:border-border-strong"
      } ${isDragging ? "shadow-lg" : ""}`}
    >
      <header
        className="flex cursor-grab items-start gap-2 active:cursor-grabbing"
        {...listeners}
        {...attributes}
      >
        <button
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((v) => !v);
          }}
          className="mt-0.5 text-text-muted hover:text-text-dim"
          title={expanded ? "Collapse" : "Expand"}
        >
          {expanded ? "▾" : "▸"}
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <span className="font-mono text-[11px] text-text-muted">
              {ticket.id}
            </span>
            <KindPill kind={ticket.kind} />
            {!ticket.isZone && <StatusPill status={ticket.status} />}
          </div>
          <h3 className="mt-1 leading-snug text-text">{ticket.title}</h3>
        </div>
      </header>

      {ticket.assignee && (
        <div className="mt-1.5 flex items-center gap-1 pl-6 text-[11px] text-text-muted">
          <span className="h-1.5 w-1.5 rounded-full bg-cyan" />
          {ticket.assignee}
        </div>
      )}

      {ticket.isZone && ticket.childCounts && (
        <div className="mt-2 flex items-center gap-2 pl-6 text-[11px] text-text-dim">
          <span>{ticket.childCounts.open} open</span>
          <span className="text-text-muted">·</span>
          <span>{ticket.childCounts.active} in-flight</span>
          <span className="text-text-muted">·</span>
          <span>{ticket.childCounts.handoff} handoff</span>
        </div>
      )}

      {expanded && (
        <div className="mt-2 space-y-2 border-t border-border pt-2 pl-6">
          {ticket.handoff && ticket.handoff.length > 0 && (
            <Section label="Handoff">
              {ticket.handoff.map((h, i) => (
                <p key={i} className="text-[11px] leading-relaxed text-text-dim">
                  {h}
                </p>
              ))}
            </Section>
          )}
          {ticket.nextSteps && ticket.nextSteps.length > 0 && (
            <Section label="Next">
              <ul className="space-y-1">
                {ticket.nextSteps.map((n, i) => (
                  <li
                    key={i}
                    className="text-[11px] leading-relaxed text-text-dim before:mr-1 before:text-text-muted before:content-['·']"
                  >
                    {n}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {ticket.gotchas && ticket.gotchas.length > 0 && (
            <Section label="Gotchas" tone="warning">
              <ul className="space-y-1">
                {ticket.gotchas.map((g, i) => (
                  <li
                    key={i}
                    className="text-[11px] leading-relaxed text-yellow"
                  >
                    {g}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          {ticket.verify && ticket.verify.length > 0 && (
            <Section label="Verify">
              <ul className="space-y-1">
                {ticket.verify.map((v, i) => (
                  <li
                    key={i}
                    className="font-mono text-[10.5px] leading-relaxed text-text-dim"
                  >
                    {v}
                  </li>
                ))}
              </ul>
            </Section>
          )}
          <div className="font-mono text-[10px] text-text-muted">
            {ticket.file}:{ticket.line}
          </div>
        </div>
      )}

      <footer className="mt-2 flex items-center justify-end gap-1 pl-6 opacity-0 transition-opacity group-hover:opacity-100">
        <button className="rounded px-1.5 py-0.5 text-[10px] text-text-muted hover:bg-border hover:text-text-dim">
          prompt
        </button>
        <button className="rounded px-1.5 py-0.5 text-[10px] text-text-muted hover:bg-red/15 hover:text-red">
          archive
        </button>
      </footer>
    </article>
  );
}

function Section({
  label,
  tone = "neutral",
  children,
}: {
  label: string;
  tone?: "neutral" | "warning";
  children: React.ReactNode;
}) {
  const labelClass = tone === "warning" ? "text-yellow" : "text-text-muted";
  return (
    <div>
      <div
        className={`mb-1 text-[10px] font-medium uppercase tracking-wider ${labelClass}`}
      >
        {label}
      </div>
      {children}
    </div>
  );
}
