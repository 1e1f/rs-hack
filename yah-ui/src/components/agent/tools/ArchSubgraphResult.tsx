/* Body for `arch_subgraph` — wire shape `{ root, nodes: [...], edges: [...],
   truncated }`. Surfaces counts + the truncated flag so the agent's slice
   request reads at a glance. */

interface SubgraphResultProps {
  result?: {
    nodes?: unknown[];
    edges?: unknown[];
    truncated?: boolean;
  };
}

export function ArchSubgraphResult({ result }: SubgraphResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Walking subgraph…
      </div>
    );
  }
  const nodes = result.nodes?.length ?? 0;
  const edges = result.edges?.length ?? 0;
  return (
    <div className="px-3 py-2 font-mono text-[11.5px] text-ink-2">
      <span className="text-ink">{nodes}</span>
      <span className="text-ink-4"> nodes · </span>
      <span className="text-ink">{edges}</span>
      <span className="text-ink-4"> edges</span>
      {result.truncated && (
        <span className="ml-2 text-[11px] text-st-bug">truncated</span>
      )}
    </div>
  );
}
