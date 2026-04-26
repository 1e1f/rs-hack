import { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";
import type { ArchEdge, ArchSubgraph, EdgeKind } from "../../types";

mermaid.initialize({
  startOnLoad: false,
  theme: "dark",
  themeVariables: {
    background: "#11111b",
    primaryColor: "#1e1e2e",
    primaryTextColor: "#cdd6f4",
    primaryBorderColor: "#45475a",
    lineColor: "#6c7086",
    secondaryColor: "#181825",
    tertiaryColor: "#181825",
    fontFamily: "'JetBrains Mono', 'SF Mono', Menlo, monospace",
    fontSize: "12px",
  },
  flowchart: {
    htmlLabels: true,
    curve: "basis",
  },
});

const ARROW: Record<EdgeKind, string> = {
  depends_on: "-->",
  message_flow: "-.->",
  data_flow: "-->",
  bridge: "==>",
  context: "-.->|ctx|",
  implements: "-.->|impl|",
};

function toMermaid(sub: ArchSubgraph, enabled: Set<EdgeKind>): string {
  const lines: string[] = ["graph TD"];

  const byLayer = new Map<string | undefined, typeof sub.nodes>();
  for (const n of sub.nodes) {
    const arr = byLayer.get(n.layer) ?? [];
    arr.push(n);
    byLayer.set(n.layer, arr);
  }

  let i = 0;
  for (const [layer, ns] of byLayer) {
    if (layer) {
      lines.push(`  subgraph ${layer}["${layer}"]`);
    }
    for (const n of ns) {
      lines.push(`    ${n.id}["${n.shortName}"]`);
      i++;
    }
    if (layer) lines.push("  end");
  }

  const filtered: ArchEdge[] = sub.edges.filter((e) => enabled.has(e.kind));
  for (const e of filtered) {
    lines.push(`  ${e.from} ${ARROW[e.kind]} ${e.to}`);
  }

  return lines.join("\n");
}

interface GraphPaneProps {
  subgraph: ArchSubgraph;
  depth: number;
  enabledKinds: Set<EdgeKind>;
}

export function GraphPane({ subgraph, enabledKinds }: GraphPaneProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const source = toMermaid(subgraph, enabledKinds);
    const id = `arch-${Date.now()}`;
    mermaid
      .render(id, source)
      .then(({ svg }) => {
        if (cancelled || !ref.current) return;
        ref.current.innerHTML = svg;
      })
      .catch((err) => {
        if (!cancelled && ref.current) {
          ref.current.innerHTML = `<pre class="p-4 text-xs text-red">${String(err)}</pre>`;
        }
      });
    return () => {
      cancelled = true;
    };
  }, [subgraph, enabledKinds]);

  const hoveredNode = subgraph.nodes.find((n) => n.id === hovered);

  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col">
      <div className="flex h-9 items-center gap-3 border-b border-border bg-surface px-3 text-[11px] text-text-dim">
        <span className="font-mono text-text">{subgraph.rootId}</span>
        <span className="text-text-muted">·</span>
        <span>{subgraph.nodes.length} nodes</span>
        <span className="text-text-muted">·</span>
        <span>
          {subgraph.edges.filter((e) => enabledKinds.has(e.kind)).length} edges
        </span>
        <button className="ml-auto rounded px-2 py-1 text-text-muted hover:bg-elevated hover:text-text-dim">
          Pin view
        </button>
      </div>
      <div className="relative min-h-0 flex-1 overflow-auto p-6">
        <div ref={ref} className="flex justify-center" />
      </div>
      {hoveredNode && (
        <aside className="absolute right-3 top-12 w-[280px] rounded border border-border bg-elevated p-3 shadow-lg">
          <div className="font-mono text-[11px] text-text">
            {hoveredNode.shortName}
          </div>
          {hoveredNode.layer && (
            <div className="mt-1 text-[10px] text-text-muted">
              layer: {hoveredNode.layer}
            </div>
          )}
          {hoveredNode.doc && (
            <p className="mt-2 text-[11px] leading-relaxed text-text-dim">
              {hoveredNode.doc}
            </p>
          )}
          <div className="mt-2 font-mono text-[10px] text-text-muted">
            {hoveredNode.file}:{hoveredNode.line}
          </div>
        </aside>
      )}
    </div>
  );
}
