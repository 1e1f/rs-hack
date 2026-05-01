import { LAYER_HUES } from "./constants";

interface LegendProps {
  /** Limit to this many entries (design shows top 4 to fit the rail).
      Defaults to all known layers. */
  limit?: number;
}

export function Legend({ limit }: LegendProps) {
  const entries = Object.entries(LAYER_HUES);
  const shown = limit ? entries.slice(0, limit) : entries;
  return (
    <div className="flex flex-col gap-1">
      {shown.map(([layer, hue]) => (
        <div
          key={layer}
          className="flex items-center gap-1.5 text-[11px] text-ink-3"
        >
          <span
            className="h-2 w-2 rounded-[2px]"
            style={{ background: hue }}
            aria-hidden
          />
          <span className="text-ink-4">layer:</span>
          <span className="font-mono text-ink-2">{layer}</span>
        </div>
      ))}
    </div>
  );
}
