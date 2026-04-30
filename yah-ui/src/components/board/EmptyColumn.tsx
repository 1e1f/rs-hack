import { Splash } from "../shared/Splash";
import type { ColumnKey } from "../../types";

interface EmptyColumnProps {
  columnKey: ColumnKey;
}

/* Per-column captions — keyed to the column illustrations:
     zones    war table         (general planning the campaign)
     open     solo merchant     (waiting for callers at the stall)
     active   merchant at camp  (work underway between waypoints)
     handoff  pigeon at castle  (message awaiting dispatch)
     review   alchemy table     (refining what was delivered)
   The caption sets the mood; the sub line tells the user what would
   actually populate this column. */
const COLUMN_SPLASH: Record<
  ColumnKey,
  { caption: string; sub: string }
> = {
  zones: {
    caption: "War table dim",
    sub: "Zones are coordinator tickets — the campaign maps that direct multi-relay work. Mark a relay with @yah:kind(epic) or give it bare-R children to promote it.",
  },
  open: {
    caption: "No callers at the stall",
    sub: "Open tickets wait to be claimed. Pick a relay and write up a ticket.",
  },
  active: {
    caption: "Camp without smoke",
    sub: "Active tickets live here while an agent works them. Drag one in from Open to claim.",
  },
  handoff: {
    caption: "No wings at the sill",
    sub: "When an agent finishes a phase, tickets land here for your sign-off.",
  },
  review: {
    caption: "Alembic stilled",
    sub: "Reviewed work rests here. Archive when you're satisfied.",
  },
};

/* Empty-state for an entirely-zero column. Renders the themed Splash
   inside a dashed parchment frame so it reads as "deliberate space" not
   "broken layout". The frame stretches to fill the column body. */
export function EmptyColumn({ columnKey }: EmptyColumnProps) {
  const cfg = COLUMN_SPLASH[columnKey];
  return (
    <div className="flex flex-1 items-center justify-center rounded-md border border-dashed border-rule/50 bg-[color-mix(in_oklab,var(--color-paper-3)_18%,transparent)] px-2 py-6 text-ink-4">
      <Splash variant={columnKey} width={180} caption={cfg.caption} sub={cfg.sub} />
    </div>
  );
}
