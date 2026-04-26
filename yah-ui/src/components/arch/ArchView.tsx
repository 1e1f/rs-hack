import { useCallback, useState } from "react";
import { ArchToolbar } from "./ArchToolbar";
import { GraphPane } from "./GraphPane";
import { ALL_EDGE_KINDS } from "./constants";
import type { PinnedView } from "./PinnedViews";
import { mockArchSubgraph } from "../../mock";
import type { EdgeKind } from "../../types";

export function ArchView() {
  const [rootId, setRootId] = useState<string>(mockArchSubgraph.rootId);
  const [depth, setDepth] = useState<number>(2);
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

  const selectPin = useCallback((p: PinnedView) => {
    setRootId(p.id);
    setDepth(p.depth);
  }, []);

  const removePin = useCallback((p: PinnedView) => {
    setPinned((prev) =>
      prev.filter((q) => !(q.id === p.id && q.depth === p.depth)),
    );
  }, []);

  return (
    <div className="flex h-full">
      <ArchToolbar
        rootId={rootId}
        onRootChange={setRootId}
        depth={depth}
        onDepthChange={setDepth}
        enabledKinds={enabledKinds}
        onToggleKind={toggleKind}
        pinned={pinned}
        onSelectPin={selectPin}
        onPinCurrent={pinCurrent}
        onRemovePin={removePin}
      />

      <div className="flex min-w-0 flex-1 flex-col">
        <GraphPane
          subgraph={mockArchSubgraph}
          depth={depth}
          enabledKinds={enabledKinds}
          onNodeClick={(id) => setRootId(id)}
          onPinView={pinCurrent}
        />
      </div>
    </div>
  );
}
