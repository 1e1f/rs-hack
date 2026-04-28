import { useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu, MenuItem } from "../shared/Menu";
import type { ConnectionState } from "../../env/hooks";
import type { Rig } from "../../types";

interface RigSelectorProps {
  rigs: Rig[];
  activeId: string;
  onChange: (id: string) => void;
  /** Live state of the active rig's backend connection. Drives the dot in
   *  the closed pill (green = ok, brass = idle, oxblood = error). Menu rows
   *  for inactive rigs still use the static `reachable` flag. */
  connectionState?: ConnectionState;
}

/* Rig pill in TitleBar — vellum-tinted button with a candle-pulse status dot
   (forest/brass/oxblood matching the footer ConnectionStrip). Clicking opens
   an anchored Menu listing all known rigs plus connect/open footer items. The
   toolbar dot grows an oxblood pip when *any* attached rig has handoff work
   waiting, and each menu row carries a brass pill with that rig's count. */
export function RigSelector({
  rigs,
  activeId,
  onChange,
  connectionState,
}: RigSelectorProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);
  const active = rigs.find((r) => r.id === activeId);
  const anyAttention = rigs.some((r) => (r.needsAttention ?? 0) > 0);
  const activeState: ConnectionState = active?.reachable === false
    ? "error"
    : connectionState ?? "idle";

  return (
    <div className="relative">
      <button
        ref={ref}
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-2 rounded-[5px] bg-vellum/55 px-2 py-1 hover:bg-vellum"
      >
        <RigStateDot state={activeState} pip={anyAttention} />
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
                  {r.kind === "local" ? r.path ?? "local filesystem" : r.host}
                </div>
              )}
            </div>
            {(r.needsAttention ?? 0) > 0 && (
              <AttentionPill count={r.needsAttention!} />
            )}
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

function RigDot({ reachable, pip = false }: { reachable: boolean; pip?: boolean }) {
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      <span
        className={`h-2 w-2 rounded-full ${
          reachable
            ? "bg-forest shadow-[0_0_0_2px_color-mix(in_oklab,var(--color-forest)_25%,transparent)] candle"
            : "bg-oxblood"
        }`}
      />
      {pip && (
        <span
          aria-label="rigs with attention"
          className="absolute -right-0.5 -top-0.5 h-[6px] w-[6px] rounded-full bg-oxblood shadow-[0_0_0_1.5px_var(--color-paper-2)]"
        />
      )}
    </span>
  );
}

/* Tri-state variant for the active-rig pill: green/brass/oxblood mirroring
   the footer ConnectionStrip. Only `ok` gets the candle-pulse halo; idle
   and error are static so they don't read as "live but tinted". */
function RigStateDot({
  state,
  pip = false,
}: {
  state: ConnectionState;
  pip?: boolean;
}) {
  const dotClass =
    state === "ok"
      ? "bg-forest shadow-[0_0_0_2px_color-mix(in_oklab,var(--color-forest)_25%,transparent)] candle"
      : state === "idle"
      ? "bg-brass"
      : "bg-oxblood";
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      <span className={`h-2 w-2 rounded-full ${dotClass}`} />
      {pip && (
        <span
          aria-label="rigs with attention"
          className="absolute -right-0.5 -top-0.5 h-[6px] w-[6px] rounded-full bg-oxblood shadow-[0_0_0_1.5px_var(--color-paper-2)]"
        />
      )}
    </span>
  );
}

function AttentionPill({ count }: { count: number }) {
  return (
    <span
      title={`${count} item${count === 1 ? "" : "s"} awaiting attention`}
      className="inline-flex h-[16px] min-w-[16px] items-center justify-center rounded-full bg-brass px-1 font-mono text-[10px] font-semibold leading-none text-paper-2"
    >
      {count}
    </span>
  );
}
