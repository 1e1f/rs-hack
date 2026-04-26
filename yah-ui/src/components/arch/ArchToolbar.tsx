import type { EdgeKind } from "../../types";
import { SectionHeader } from "../shared/SectionHeader";
import { RootSelector } from "./RootSelector";
import { DepthControl } from "./DepthControl";
import { EdgeKindFilters } from "./EdgeKindFilters";
import { PinnedViews, type PinnedView } from "./PinnedViews";
import { Legend } from "./Legend";

interface ArchToolbarProps {
  rootId: string;
  onRootChange: (id: string) => void;
  depth: number;
  onDepthChange: (depth: number) => void;
  enabledKinds: Set<EdgeKind>;
  onToggleKind: (kind: EdgeKind) => void;
  pinned: PinnedView[];
  onSelectPin: (pin: PinnedView) => void;
  onPinCurrent: () => void;
  onRemovePin?: (pin: PinnedView) => void;
}

/* Left rail of the Architecture view. Composes the five sections — Root,
   Depth, Edges, Pinned views, Legend — that drive the GraphPane render.
   State lives one level up in ArchView so the rail and the canvas can
   stay in sync (e.g. graphpane "Pin view" button feeds onPinCurrent). */
export function ArchToolbar({
  rootId,
  onRootChange,
  depth,
  onDepthChange,
  enabledKinds,
  onToggleKind,
  pinned,
  onSelectPin,
  onPinCurrent,
  onRemovePin,
}: ArchToolbarProps) {
  return (
    <aside
      className="flex w-[260px] shrink-0 flex-col gap-4 overflow-y-auto border-r border-rule/50 p-3.5"
      style={{
        background: "color-mix(in oklab, var(--color-paper-2) 50%, transparent)",
      }}
    >
      <section>
        <SectionHeader>Root</SectionHeader>
        <RootSelector value={rootId} onChange={onRootChange} />
        <div className="mt-1.5 text-[11px] italic text-ink-4">
          Graph rebuilt from <span className="font-mono">@arch:</span> annotations
          on demand.
        </div>
      </section>

      <section>
        <SectionHeader>Depth</SectionHeader>
        <DepthControl value={depth} onChange={onDepthChange} />
      </section>

      <section>
        <SectionHeader>Edges</SectionHeader>
        <EdgeKindFilters enabled={enabledKinds} onToggle={onToggleKind} />
      </section>

      <section>
        <SectionHeader>Pinned views</SectionHeader>
        <PinnedViews
          pins={pinned}
          onSelect={onSelectPin}
          onPinCurrent={onPinCurrent}
          onRemove={onRemovePin}
        />
      </section>

      <section className="mt-auto">
        <SectionHeader>Legend</SectionHeader>
        <Legend limit={4} />
      </section>
    </aside>
  );
}
