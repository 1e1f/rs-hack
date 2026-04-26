import { Splash } from "../shared/Splash";
import type { ColumnKey } from "../../types";

interface EmptyColumnProps {
  columnKey: ColumnKey;
}

/* Per-column captions — folk-tale flavour (mirrors the wayfarer set in
   the design return). The caption sets the mood; the sub line tells the
   user what would actually populate this column. */
const COLUMN_SPLASH: Record<
  ColumnKey,
  { caption: string; sub: string }
> = {
  zones: {
    caption: "Tomes lie shut",
    sub: "Zones are coordinator tickets. Mark a relay with @yah:kind(epic) or give it bare-R children to promote it.",
  },
  open: {
    caption: "No travelers tonight",
    sub: "Nothing waiting in the queue. Pick a relay and write up a ticket.",
  },
  active: {
    caption: "Sign hangs empty",
    sub: "No tickets currently being worked. Drag one in from Open.",
  },
  handoff: {
    caption: "Cookpot's cold",
    sub: "When an agent finishes a phase, tickets land here for your sign-off.",
  },
  review: {
    caption: "Bottles all corked",
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
