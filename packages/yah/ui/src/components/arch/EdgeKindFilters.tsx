import type { EdgeKind } from "../../types";
import { EDGE_KINDS, strokeDasharray } from "./constants";

interface EdgeKindFiltersProps {
  enabled: Set<EdgeKind>;
  onToggle: (kind: EdgeKind) => void;
}

export function EdgeKindFilters({ enabled, onToggle }: EdgeKindFiltersProps) {
  return (
    <div className="flex flex-col gap-0.5">
      {EDGE_KINDS.map((k) => (
        <label
          key={k.id}
          className="flex cursor-pointer items-center gap-2 rounded-[3px] px-1.5 py-1 hover:bg-vellum-2/70"
        >
          <input
            type="checkbox"
            checked={enabled.has(k.id)}
            onChange={() => onToggle(k.id)}
            className="accent-accent"
          />
          <svg width="22" height="6" className="shrink-0" aria-hidden>
            <line
              x1="0"
              y1="3"
              x2="22"
              y2="3"
              stroke={k.color}
              strokeWidth="1.4"
              strokeDasharray={strokeDasharray(k.stroke)}
            />
          </svg>
          <span className="text-[12px] text-ink-2">{k.label}</span>
        </label>
      ))}
    </div>
  );
}
