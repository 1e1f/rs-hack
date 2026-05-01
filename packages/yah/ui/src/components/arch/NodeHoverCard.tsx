import type { ArchNode } from "../../types";

interface NodeHoverCardProps {
  node: ArchNode | null;
}

/* Absolute-positioned aside that surfaces the hovered node's doc + location.
   Lives in the upper-right of GraphPane's canvas; suppressed when an action
   menu is open so the two don't overlap. */
export function NodeHoverCard({ node }: NodeHoverCardProps) {
  if (!node) return null;

  return (
    <aside className="pointer-events-none absolute right-3 top-12 w-[280px] rounded border border-rule bg-vellum p-3 shadow-lg">
      <div className="font-mono text-[11px] text-ink">{node.shortName}</div>
      {node.layer && (
        <div className="mt-1 text-[10px] text-ink-4">layer: {node.layer}</div>
      )}
      {node.roles.length > 0 && (
        <div className="mt-0.5 text-[10px] italic text-ink-4">
          {node.roles.join(" · ")}
        </div>
      )}
      {node.doc && (
        <p className="mt-2 text-[11px] leading-relaxed text-ink-3">{node.doc}</p>
      )}
      <div className="mt-2 font-mono text-[10px] text-ink-4">
        {node.file}:{node.line}
      </div>
    </aside>
  );
}
