import { useCallback, useState } from "react";
import { ArchToolbar } from "./ArchToolbar";
import { GraphPane } from "./GraphPane";
import { ALL_EDGE_KINDS } from "./constants";
import type { PinnedView } from "./PinnedViews";
import { mockArchSubgraph } from "../../mock";
import type { EdgeKind } from "../../types";
import { Splash } from "../shared/Splash";

interface ArchViewProps {
  rootId: string;
  onRootChange: (id: string) => void;
  depth: number;
  onDepthChange: (d: number) => void;
  onJumpToFile?: (fileColon: string) => void;
  onOpenInAgent?: (target: string) => void;
}

export function ArchView({
  rootId,
  onRootChange,
  depth,
  onDepthChange,
  onJumpToFile,
  onOpenInAgent,
}: ArchViewProps) {
  const [enabledKinds, setEnabledKinds] = useState<Set<EdgeKind>>(
    new Set(ALL_EDGE_KINDS),
  );
  const [pinned, setPinned] = useState<PinnedView[]>([
    {
      id: mockArchSubgraph.rootId,
      depth: 2,
      label: `${mockArchSubgraph.rootId} · 2 hops`,
    },
  ]);

  const toggleKind = useCallback((k: EdgeKind) => {
    setEnabledKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }, []);

  const pinCurrent = useCallback(() => {
    setPinned((prev) => {
      if (prev.some((p) => p.id === rootId && p.depth === depth)) return prev;
      return [
        ...prev,
        { id: rootId, depth, label: `${rootId} · ${depth} hop${depth > 1 ? "s" : ""}` },
      ];
    });
  }, [rootId, depth]);

  const selectPin = useCallback(
    (p: PinnedView) => {
      onRootChange(p.id);
      onDepthChange(p.depth);
    },
    [onRootChange, onDepthChange],
  );

  const removePin = useCallback((p: PinnedView) => {
    setPinned((prev) =>
      prev.filter((q) => !(q.id === p.id && q.depth === p.depth)),
    );
  }, []);

  return (
    <div className="flex h-full">
      <ArchToolbar
        rootId={rootId}
        onRootChange={onRootChange}
        depth={depth}
        onDepthChange={onDepthChange}
        enabledKinds={enabledKinds}
        onToggleKind={toggleKind}
        pinned={pinned}
        onSelectPin={selectPin}
        onPinCurrent={pinCurrent}
        onRemovePin={removePin}
      />

      <div className="flex min-w-0 flex-1 flex-col">
        {mockArchSubgraph.nodes.length === 0 ? (
          <div className="flex flex-1 items-center justify-center">
            <Splash
              variant="scroll"
              caption="Map of the realm not yet drawn"
              sub="No architecture annotations were found for this rig. Add @arch:layer / @arch:role doc-comments to surface modules here."
            />
          </div>
        ) : (
          <GraphPane
            subgraph={mockArchSubgraph}
            depth={depth}
            enabledKinds={enabledKinds}
            onReroot={onRootChange}
            onJumpToFile={onJumpToFile}
            onOpenInAgent={onOpenInAgent}
            onPinView={pinCurrent}
          />
        )}
      </div>
    </div>
  );
}
