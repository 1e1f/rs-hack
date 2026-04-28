
import { useCallback, useEffect, useMemo, useState } from "react";
import { ArchToolbar } from "./ArchToolbar";
import { GraphPane } from "./GraphPane";
import { ALL_EDGE_KINDS } from "./constants";
import type { PinnedView } from "./PinnedViews";
import { useArchGraph, useRoots } from "../../env/hooks";
import { subgraphToArchSubgraph } from "../../env/mapper";
import type { WireViolation } from "../../env/types";
import type { EdgeKind } from "../../types";
import { Splash } from "../shared/Splash";

const NODE_ID_RE = /^[0-9a-f]{32}$/;

interface ArchViewProps {
  rigId: string;
  rootId: string;
  onRootChange: (id: string) => void;
  depth: number;
  onDepthChange: (d: number) => void;
  onJumpToFile?: (fileColon: string) => void;
  onOpenInAgent?: (target: string) => void;
  /* Rule-validator output for the active rig — surfaced as red borders on
     offending+anchor nodes via GraphPane post-render styling. Lifted to App
     so the Board tab shares the same array. */
  violations?: WireViolation[];
}

export function ArchView({
  rigId,
  rootId,
  onRootChange,
  depth,
  onDepthChange,
  onJumpToFile,
  onOpenInAgent,
  violations,
}: ArchViewProps) {
  const [enabledKinds, setEnabledKinds] = useState<Set<EdgeKind>>(
    new Set(ALL_EDGE_KINDS),
  );
  const [pinned, setPinned] = useState<PinnedView[]>([]);

  /* Auto-pick the first available root when no valid one is selected so
     a fresh visit lands on a populated graph instead of the splash. The
     daemon's roots list is rig-scoped and folds in index_finished, so
     the auto-pick re-fires for the new rig on switch. */
  const rootsState = useRoots(rigId);
  useEffect(() => {
    if (NODE_ID_RE.test(rootId)) return;
    const first = rootsState.roots[0];
    if (first) onRootChange(first.id);
  }, [rootId, rootsState.roots, onRootChange]);

  /* Live subgraph from the daemon. The hook caches by (rigId, root, depth)
     and folds in arch:event deltas; cold-boot also hydrates from KV so a
     re-render shows the last-known graph before index_finished arrives. */
  const { data: wire, loading, error } = useArchGraph(rigId, rootId, depth);
  const subgraph = useMemo(
    () => (wire ? subgraphToArchSubgraph(wire, depth) : null),
    [wire, depth],
  );

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
        rigId={rigId}
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
        {renderCanvas()}
      </div>
    </div>
  );

  function renderCanvas() {
    if (error) {
      return (
        <div className="flex flex-1 items-center justify-center">
          <Splash
            variant="anvil"
            caption="The forge sputtered"
            sub={error.message}
          />
        </div>
      );
    }
    if (!subgraph || subgraph.nodes.length === 0) {
      const stillResolvingRoot = !NODE_ID_RE.test(rootId) && rootsState.loading;
      return (
        <div className="flex flex-1 items-center justify-center">
          <Splash
            variant="scroll"
            caption={
              loading || stillResolvingRoot
                ? "Drawing the realm…"
                : "Map of the realm not yet drawn"
            }
            sub={
              loading || stillResolvingRoot
                ? "Waiting on the daemon."
                : "No architecture nodes for this root. Pick a different one from the rail, or add @arch:layer / @arch:role doc-comments to surface modules here."
            }
          />
        </div>
      );
    }
    return (
      <GraphPane
        subgraph={subgraph}
        depth={depth}
        enabledKinds={enabledKinds}
        onReroot={onRootChange}
        onJumpToFile={onJumpToFile}
        onOpenInAgent={onOpenInAgent}
        onPinView={pinCurrent}
        violations={violations}
      />
    );
  }
}
