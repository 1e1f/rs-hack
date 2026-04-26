import { useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu } from "../shared/Menu";

interface RootSelectorProps {
  value: string;
  onChange: (id: string) => void;
}

interface Choice {
  id: string;
  label: string;
  sub: string;
}

/* Mock root choices — once the backend exposes the @arch: registry, these
   come back from a /arch/roots endpoint. Until then the picker doubles as
   a free-form id input via the bottom slot. */
const CHOICES: Choice[] = [
  { id: "voice_allocator", label: "voice_allocator.rs", sub: "src/" },
  { id: "ticket",          label: "ticket.rs",          sub: "src/" },
];

export function RootSelector({ value, onChange }: RootSelectorProps) {
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLButtonElement>(null);
  const active = CHOICES.find((c) => c.id === value);
  const display = active?.label ?? value;

  return (
    <div className="relative">
      <button
        ref={anchorRef}
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between gap-2 rounded-[3px] border border-rule/50 bg-vellum px-2.5 py-1.5 text-left hover:border-rule"
      >
        <span className="flex min-w-0 items-center gap-1.5">
          <Icon name="file" size={12} className="text-ink-4" />
          <span className="truncate font-mono text-[12px] text-ink">
            {display}
          </span>
        </span>
        <Icon name="chevron-down" size={11} className="text-ink-4" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={anchorRef}
        width="100%"
      >
        <div className="px-1.5 pb-1 pt-0.5">
          <span className="eyebrow">From file</span>
        </div>
        {CHOICES.map((c) => {
          const selected = c.id === value;
          return (
            <button
              key={c.id}
              onClick={() => {
                onChange(c.id);
                setOpen(false);
              }}
              className={`flex w-full items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[12px] ${
                selected ? "bg-vellum-2" : "hover:bg-vellum-2/70"
              }`}
            >
              <Icon name="file" size={11} className="text-ink-4" />
              <span className="truncate font-mono text-ink">{c.label}</span>
              <span className="ml-auto text-[10px] italic text-ink-4">
                {c.sub}
              </span>
            </button>
          );
        })}
        <div className="my-1 h-px bg-rule/40" />
        <div className="px-1.5 pb-1 pt-0.5">
          <span className="eyebrow">Other roots</span>
        </div>
        <button
          className="flex w-full items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[12px] text-ink-2 hover:bg-vellum-2/70"
          disabled
        >
          <Icon name="search" size={11} className="text-ink-4" /> Search by
          symbol…
        </button>
        <button
          className="flex w-full items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[12px] text-ink-2 hover:bg-vellum-2/70"
          disabled
        >
          <Icon name="scroll" size={11} className="text-ink-4" /> From current
          relay
        </button>
      </Menu>
    </div>
  );
}
