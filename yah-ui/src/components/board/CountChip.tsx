import type { Hue } from "../shared/Pill";

interface CountChipProps {
  n: number;
  label: string;
  hue: Hue;
}

const HUE_TEXT: Record<Hue, string> = {
  open: "text-st-open",
  active: "text-st-active",
  handoff: "text-st-handoff",
  review: "text-st-review",
  bug: "text-st-bug",
  feature: "text-st-feature",
  task: "text-st-task",
  epic: "text-st-epic",
  neutral: "text-ink-3",
};

/* Two-line count chip: serif numeral + small-caps label, separated from
   neighbours by a gilt rule. Used inside zone cards to show child-status
   tallies (`5 open · 2 active · 1 handoff`) where each tally inherits the
   status hue. The numeral keeps `currentColor` so callers control tone via
   the hue prop; the label stays muted to keep the numeral as the focal
   point. */
export function CountChip({ n, label, hue }: CountChipProps) {
  return (
    <div
      className={`flex items-baseline gap-1 border-r border-rule/50 pr-2.5 last:border-r-0 last:pr-0 mr-1.5 last:mr-0 ${HUE_TEXT[hue]}`}
    >
      <span className="font-display text-[16px] font-medium leading-none">
        {n}
      </span>
      <span className="smallcaps text-[10px] text-ink-3">{label}</span>
    </div>
  );
}
