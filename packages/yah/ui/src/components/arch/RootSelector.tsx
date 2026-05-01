import { useMemo, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu } from "../shared/Menu";
import { useRoots } from "../../env/hooks";
import type { NodeRef } from "../../env/types";

interface RootSelectorProps {
  rigId: string;
  value: string;
  onChange: (id: string) => void;
}

interface Choice {
  id: string;
  label: string;
  sub: string;
}

/* Build a Choice from a wire NodeRef. Roots are top-level items (modules,
   directories, types) whose `qualified` is the path you'd use to talk
   about it in code; the file/line drives the secondary line. */
function nodeRefToChoice(n: NodeRef): Choice {
  const file = n.file || "";
  const dir = file.includes("/") ? file.slice(0, file.lastIndexOf("/") + 1) : "";
  return {
    id: n.id,
    label: n.label || n.qualified || n.id.slice(0, 8),
    sub: dir || file || n.qualified || "",
  };
}

export function RootSelector({ rigId, value, onChange }: RootSelectorProps) {
  const [open, setOpen] = useState(false);
  const anchorRef = useRef<HTMLButtonElement>(null);
  const { roots, loading, error } = useRoots(rigId);

  const choices = useMemo(() => roots.map(nodeRefToChoice), [roots]);
  const active = choices.find((c) => c.id === value);
  /* Free-form ids fall through to a truncated-hex display so a 32-char
     blake3 NodeId doesn't take over the rail. */
  const display =
    active?.label ?? (value ? `${value.slice(0, 8)}…` : "Pick a root");

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
          <span className="eyebrow">Roots</span>
        </div>
        {loading && choices.length === 0 ? (
          <div className="px-2 py-1.5 text-[11px] italic text-ink-4">
            Listing roots…
          </div>
        ) : error ? (
          <div className="px-2 py-1.5 text-[11px] italic text-oxblood">
            {error.message}
          </div>
        ) : choices.length === 0 ? (
          <div className="px-2 py-1.5 text-[11px] italic text-ink-4">
            No roots reported. Pick a rig with attached source.
          </div>
        ) : (
          choices.map((c) => {
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
                {c.sub && (
                  <span className="ml-auto truncate text-[10px] italic text-ink-4">
                    {c.sub}
                  </span>
                )}
              </button>
            );
          })
        )}
      </Menu>
    </div>
  );
}
