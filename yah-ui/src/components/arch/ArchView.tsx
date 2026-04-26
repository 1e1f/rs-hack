import { useState } from "react";
import { RootSelector } from "./RootSelector";
import { GraphPane } from "./GraphPane";
import { mockArchSubgraph } from "../../mock";
import type { EdgeKind } from "../../types";

const ALL_EDGE_KINDS: EdgeKind[] = [
  "depends_on",
  "message_flow",
  "data_flow",
  "bridge",
  "context",
  "implements",
];

export function ArchView() {
  const [rootId, setRootId] = useState<string>(mockArchSubgraph.rootId);
  const [depth, setDepth] = useState<number>(2);
  const [enabledKinds, setEnabledKinds] = useState<Set<EdgeKind>>(
    new Set(ALL_EDGE_KINDS),
  );

  function toggleKind(k: EdgeKind) {
    setEnabledKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });
  }

  return (
    <div className="flex h-full">
      <aside className="flex w-[260px] shrink-0 flex-col gap-4 border-r border-border bg-surface p-3">
        <RootSelector value={rootId} onChange={setRootId} />

        <section>
          <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">
            Depth
          </div>
          <div className="flex gap-1">
            {[1, 2, 3].map((d) => (
              <button
                key={d}
                onClick={() => setDepth(d)}
                className={`flex h-7 w-7 items-center justify-center rounded text-[12px] ${
                  depth === d
                    ? "bg-blue/20 text-blue"
                    : "bg-elevated text-text-dim hover:bg-border"
                }`}
              >
                {d}
              </button>
            ))}
          </div>
        </section>

        <section>
          <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">
            Edge kinds
          </div>
          <div className="flex flex-col gap-1">
            {ALL_EDGE_KINDS.map((k) => (
              <label
                key={k}
                className="flex cursor-pointer items-center gap-2 rounded px-1 py-0.5 text-[11px] hover:bg-elevated"
              >
                <input
                  type="checkbox"
                  checked={enabledKinds.has(k)}
                  onChange={() => toggleKind(k)}
                  className="accent-blue"
                />
                <span className="text-text-dim">{k}</span>
              </label>
            ))}
          </div>
        </section>

        <section className="mt-auto">
          <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">
            Pinned views
          </div>
          <div className="text-[11px] italic text-text-muted">none yet</div>
        </section>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <GraphPane
          subgraph={mockArchSubgraph}
          depth={depth}
          enabledKinds={enabledKinds}
        />
      </div>
    </div>
  );
}
