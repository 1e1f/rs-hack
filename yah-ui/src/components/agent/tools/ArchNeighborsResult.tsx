/* Body for `arch_neighbors` — the host returns
   `{ edges: [{ id, from, to, kind, annotations? }] }`. We only surface
   counts + the first few edge kinds — the full list lives in the JSON. */

interface NeighborsResultProps {
  result?: {
    edges?: Array<{
      kind?: { edge?: string } | string;
    }>;
  };
}

export function ArchNeighborsResult({ result }: NeighborsResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Walking neighbors…
      </div>
    );
  }
  const edges = result.edges ?? [];
  if (edges.length === 0) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        No neighboring edges.
      </div>
    );
  }
  const byKind = new Map<string, number>();
  for (const edge of edges) {
    const kindStr =
      typeof edge.kind === "string"
        ? edge.kind
        : edge.kind?.edge ?? "unknown";
    byKind.set(kindStr, (byKind.get(kindStr) ?? 0) + 1);
  }
  return (
    <div className="px-3 py-2 font-mono text-[11.5px] text-ink-2">
      <div className="text-ink">{edges.length} edges</div>
      <div className="mt-1 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px] text-ink-3">
        {[...byKind.entries()].map(([kind, count]) => (
          <span key={kind}>
            <span className="text-ink-4">{kind}</span> ×{count}
          </span>
        ))}
      </div>
    </div>
  );
}
