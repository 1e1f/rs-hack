interface DepthControlProps {
  value: number;
  onChange: (depth: number) => void;
  /** BFS levels offered. Backend currently caps at 3. */
  options?: number[];
}

export function DepthControl({
  value,
  onChange,
  options = [1, 2, 3],
}: DepthControlProps) {
  return (
    <div className="flex gap-1">
      {options.map((d) => {
        const active = value === d;
        return (
          <button
            key={d}
            onClick={() => onChange(d)}
            className={`flex flex-1 items-center justify-center gap-1 rounded-[3px] border px-0 py-1.5 transition-colors ${
              active
                ? "border-accent text-accent"
                : "border-rule/50 text-ink-2 hover:border-rule"
            }`}
            style={{
              background: active
                ? "color-mix(in oklab, var(--color-accent) 12%, var(--color-vellum))"
                : "var(--color-vellum)",
            }}
          >
            <span className="font-display text-[14px] font-medium leading-none">
              {d}
            </span>
            <span className="text-[10px] italic text-ink-3">
              hop{d > 1 ? "s" : ""}
            </span>
          </button>
        );
      })}
    </div>
  );
}
