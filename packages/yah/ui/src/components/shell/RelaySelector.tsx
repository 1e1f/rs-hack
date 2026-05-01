import { useMemo, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu } from "../shared/Menu";
import { KindBadge, Pill } from "../shared/Pill";
import type { Hue } from "../shared/Pill";
import type { Ticket, TicketStatus } from "../../types";

interface RelaySelectorProps {
  relays: Ticket[];
  activeId: string | null;
  onChange: (id: string | null) => void;
}

/* Relay pill in TitleBar with anchored Menu containing a filter input + the
   full relay list. Items show kind badge, mono ID, serif title and a status
   pill. Filter narrows on id-or-title substring (case-insensitive). */
export function RelaySelector({
  relays,
  activeId,
  onChange,
}: RelaySelectorProps) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState("");
  const ref = useRef<HTMLButtonElement>(null);
  const active = relays.find((r) => r.id === activeId);
  const filtered = useMemo(() => {
    const ql = q.trim().toLowerCase();
    if (!ql) return relays;
    return relays.filter(
      (r) =>
        r.id.toLowerCase().includes(ql) || r.title.toLowerCase().includes(ql),
    );
  }, [q, relays]);

  return (
    <div className="relative flex items-center">
      <button
        ref={ref}
        onClick={() => setOpen((v) => !v)}
        className={`flex items-center gap-2 bg-vellum/55 px-2 py-1 hover:bg-vellum ${
          active ? "rounded-l-[5px]" : "rounded-[5px]"
        }`}
      >
        {active ? (
          <>
            <KindBadge
              kind={active.kind}
              itemType={active.itemType}
              isZone={active.isZone}
              size={12}
            />
            <span className="font-mono text-[11px] text-ink-3">
              {active.id}
            </span>
            <span className="max-w-[280px] truncate font-display text-[14px] text-ink">
              {active.title}
            </span>
          </>
        ) : (
          <span className="text-ink-3 italic">no relay selected</span>
        )}
        <Icon name="chevron-down" size={12} className="text-ink-3" />
      </button>
      {active && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            onChange(null);
          }}
          title="Clear relay filter"
          className="flex items-center justify-center self-stretch rounded-r-[5px] border-l border-rule/40 bg-vellum/55 px-1.5 text-ink-3 hover:bg-vellum hover:text-ink"
        >
          <Icon name="x" size={12} />
        </button>
      )}
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={ref}
        width={420}
      >
        <div className="eyebrow flex items-center justify-between px-2 pb-1.5 pt-0.5">
          <span>Relays · {relays.length}</span>
        </div>
        <div className="mb-1.5 mx-1 flex items-center gap-1.5 rounded border border-rule/40 bg-paper-2/30 px-2 py-1">
          <Icon name="search" size={12} className="text-ink-3" />
          <input
            autoFocus
            placeholder="filter relays…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
            className="min-w-0 flex-1 bg-transparent text-[12px] text-ink outline-none placeholder:text-ink-4"
          />
          <span className="text-[10px] text-ink-3">
            <kbd>⌘K</kbd>
          </span>
        </div>
        <div className="max-h-[360px] overflow-auto">
          <button
            onClick={() => {
              onChange(null);
              setOpen(false);
            }}
            className="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-ink-3 italic hover:bg-vellum-2/60"
          >
            (clear selection)
          </button>
          {filtered.map((r) => (
            <button
              key={r.id}
              onClick={() => {
                onChange(r.id);
                setOpen(false);
              }}
              className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left ${
                activeId === r.id ? "bg-vellum-2" : "hover:bg-vellum-2/60"
              }`}
            >
              <KindBadge
                kind={r.kind}
                itemType={r.itemType}
                isZone={r.isZone}
              />
              <span className="w-16 shrink-0 font-mono text-[11px] text-ink-3">
                {r.id}
              </span>
              <span className="flex-1 truncate font-display text-[13px] text-ink">
                {r.title}
              </span>
              <Pill hue={statusHue(r.status)}>{statusLabel(r.status)}</Pill>
            </button>
          ))}
          {filtered.length === 0 && (
            <div className="px-2 py-3 text-center text-[12px] text-ink-3 italic">
              no relays match
            </div>
          )}
        </div>
      </Menu>
    </div>
  );
}

const STATUS_HUE: Record<TicketStatus, Hue> = {
  open: "open",
  claimed: "active",
  "in-progress": "active",
  handoff: "handoff",
  review: "review",
  done: "review",
};

const STATUS_LABEL: Record<TicketStatus, string> = {
  open: "Open",
  claimed: "Claimed",
  "in-progress": "Active",
  handoff: "Handoff",
  review: "Review",
  done: "Done",
};

function statusHue(s: TicketStatus): Hue {
  return STATUS_HUE[s] ?? "neutral";
}

function statusLabel(s: TicketStatus): string {
  return STATUS_LABEL[s] ?? s;
}
