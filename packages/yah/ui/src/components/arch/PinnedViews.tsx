import { Icon } from "../shared/Glyph";

export interface PinnedView {
  id: string;
  depth: number;
  label: string;
}

interface PinnedViewsProps {
  pins: PinnedView[];
  onSelect: (pin: PinnedView) => void;
  onPinCurrent: () => void;
  onRemove?: (pin: PinnedView) => void;
}

export function PinnedViews({
  pins,
  onSelect,
  onPinCurrent,
  onRemove,
}: PinnedViewsProps) {
  return (
    <div className="flex flex-col gap-1">
      {pins.length === 0 && (
        <div className="px-1.5 text-[11px] italic text-ink-4">
          none yet — pin a view to remember it
        </div>
      )}
      {pins.map((p) => (
        <div
          key={`${p.id}-${p.depth}`}
          className="group flex items-center gap-1.5 rounded-[3px] px-2 py-[5px]"
          style={{
            background: "color-mix(in oklab, var(--color-vellum) 50%, transparent)",
          }}
        >
          <button
            onClick={() => onSelect(p)}
            className="flex flex-1 items-center gap-1.5 text-left"
          >
            <Icon name="pin" size={11} className="text-ink-4" />
            <span className="flex-1 truncate font-display text-[12px] text-ink-2">
              {p.label}
            </span>
          </button>
          {onRemove && (
            <button
              onClick={() => onRemove(p)}
              className="text-ink-4 opacity-0 transition-opacity hover:text-ink-2 group-hover:opacity-100"
              title="Remove pin"
            >
              <Icon name="x" size={10} />
            </button>
          )}
        </div>
      ))}
      <button
        onClick={onPinCurrent}
        className="flex items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[11px] italic text-ink-3 hover:bg-vellum-2/70"
      >
        <Icon name="plus" size={10} /> pin current view
      </button>
    </div>
  );
}
