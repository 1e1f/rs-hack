/* Body for `arch_lookup` — `{ ids: NodeId[] }`. The order is inner-most
   first (per the host's docstring), so the first id is what the agent
   most likely wanted. */

interface LookupResultProps {
  result?: { ids?: string[] };
}

export function ArchLookupResult({ result }: LookupResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Looking up…
      </div>
    );
  }
  const ids = result.ids ?? [];
  if (ids.length === 0) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        No node spans this position.
      </div>
    );
  }
  return (
    <div className="px-3 py-2 font-mono text-[11px] text-ink-3">
      {ids.map((id, i) => (
        <div key={id} className={i === 0 ? "text-ink-2" : ""}>
          {i === 0 ? "▸ " : "  "}
          {id}
        </div>
      ))}
    </div>
  );
}
