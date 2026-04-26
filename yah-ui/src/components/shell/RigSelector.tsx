import { useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu, MenuItem } from "../shared/Menu";
import type { Rig } from "../../types";

interface RigSelectorProps {
  rigs: Rig[];
  activeId: string;
  onChange: (id: string) => void;
}

/* Rig pill in TitleBar — vellum-tinted button with a candle-pulse status dot
   (forest-green when reachable, oxblood when not). Clicking opens an anchored
   Menu listing all known rigs plus connect/open footer items. */
export function RigSelector({ rigs, activeId, onChange }: RigSelectorProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);
  const active = rigs.find((r) => r.id === activeId);

  return (
    <div className="relative">
      <button
        ref={ref}
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-2 rounded-[5px] bg-vellum/55 px-2 py-1 hover:bg-vellum"
      >
        <RigDot reachable={!!active?.reachable} />
        <span className="flex items-baseline gap-1">
          <span className="font-display text-[14px] font-medium text-ink">
            {active?.name ?? "no rig"}
          </span>
          {active?.kind === "remote" && active.host && (
            <span className="font-mono text-[10px] text-ink-3">
              {active.host}
            </span>
          )}
        </span>
        <Icon name="chevron-down" size={12} className="text-ink-3" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={ref}
        width={300}
      >
        <div className="eyebrow px-2 pb-1.5 pt-0.5">Rigs</div>
        {rigs.map((r) => (
          <button
            key={r.id}
            onClick={() => {
              onChange(r.id);
              setOpen(false);
            }}
            className={`flex w-full items-center gap-2 rounded px-2 py-[7px] text-left ${
              r.id === activeId ? "bg-vellum-2" : "hover:bg-vellum-2/60"
            }`}
          >
            <RigDot reachable={r.reachable} />
            <div className="min-w-0 flex-1">
              <div className="font-display text-[13px] font-medium text-ink">
                {r.name}
              </div>
              {(r.host || r.kind === "local") && (
                <div className="truncate font-mono text-[11px] text-ink-3">
                  {r.kind === "local" ? "local filesystem" : r.host}
                </div>
              )}
            </div>
            <span className="text-[10px] text-ink-3 [font-variant-caps:all-small-caps]">
              {r.kind}
            </span>
          </button>
        ))}
        <div className="my-1.5 border-t border-rule/40" />
        <MenuItem leading={<Icon name="plus" size={12} />}>
          Connect remote rig…
        </MenuItem>
        <MenuItem leading={<Icon name="folder" size={12} />}>
          Open local folder…
        </MenuItem>
      </Menu>
    </div>
  );
}

function RigDot({ reachable }: { reachable: boolean }) {
  return (
    <span
      className={`h-2 w-2 shrink-0 rounded-full ${
        reachable
          ? "bg-forest shadow-[0_0_0_2px_color-mix(in_oklab,var(--color-forest)_25%,transparent)] candle"
          : "bg-oxblood"
      }`}
    />
  );
}
