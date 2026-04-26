import { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";
import type { ArchEdge, ArchNode, ArchSubgraph, EdgeKind } from "../../types";

mermaid.initialize({
  startOnLoad: false,
  theme: "base",
  themeVariables: {
    background: "var(--color-paper)",
    primaryColor: "var(--color-vellum)",
    primaryTextColor: "var(--color-ink)",
    primaryBorderColor: "var(--color-rule)",
    lineColor: "var(--color-ink-3)",
    secondaryColor: "var(--color-paper-2)",
    tertiaryColor: "var(--color-paper-3)",
    fontFamily: "var(--font-display), serif",
    fontSize: "12px",
  },
  flowchart: {
    htmlLabels: true,
    curve: "basis",
  },
  securityLevel: "loose",
});

const ARROW: Record<EdgeKind, string> = {
  depends_on: "-->",
  message_flow: "-.->",
  data_flow: "-->",
  bridge: "==>",
  context: "-.->|ctx|",
  implements: "-.->|impl|",
};

const EDGE_HUE: Record<EdgeKind, string> = {
  depends_on: "var(--color-ink-3)",
  message_flow: "var(--color-midnight)",
  data_flow: "var(--color-forest)",
  bridge: "var(--color-oxblood)",
  context: "var(--color-brass)",
  implements: "var(--color-plum)",
};

const LAYER_HUES: Record<string, string> = {
  audio: "var(--color-midnight)",
  dispatch: "var(--color-brass)",
  io: "var(--color-forest)",
  state: "var(--color-plum)",
  core: "var(--color-oxblood)",
  view: "var(--color-midnight)",
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function nodeLabel(n: ArchNode): string {
  const role = n.roles[0] ?? "";
  return `<div class='arch-node'><span class='arch-node-name'>${escapeHtml(
    n.shortName,
  )}</span>${role ? `<span class='arch-node-role'>${escapeHtml(role)}</span>` : ""}</div>`;
}

function layerClassName(layer: string | undefined): string {
  return layer && LAYER_HUES[layer] ? layer : "core";
}

function buildSource(graph: ArchSubgraph, enabled: Set<EdgeKind>): string {
  const lines: string[] = ["graph LR"];

  const byLayer = new Map<string, ArchNode[]>();
  for (const n of graph.nodes) {
    const k = layerClassName(n.layer);
    const arr = byLayer.get(k) ?? [];
    arr.push(n);
    byLayer.set(k, arr);
  }

  for (const [layer, nodes] of byLayer) {
    lines.push(`  subgraph ${layer}["layer · ${layer}"]`);
    for (const n of nodes) {
      lines.push(`    ${n.id}["${nodeLabel(n)}"]`);
    }
    lines.push("  end");
  }

  const filtered: ArchEdge[] = graph.edges.filter((e) => enabled.has(e.kind));
  filtered.forEach((e) => {
    lines.push(`  ${e.from} ${ARROW[e.kind]} ${e.to}`);
  });

  for (const [layer, hue] of Object.entries(LAYER_HUES)) {
    lines.push(
      `  classDef ${layer} fill:var(--color-vellum),stroke:${hue},stroke-width:1.4px,color:var(--color-ink);`,
    );
  }
  for (const n of graph.nodes) {
    lines.push(`  class ${n.id} ${layerClassName(n.layer)};`);
  }

  filtered.forEach((e, i) => {
    lines.push(`  linkStyle ${i} stroke:${EDGE_HUE[e.kind]},stroke-width:1.4px;`);
  });

  return lines.join("\n");
}

interface GraphPaneProps {
  subgraph: ArchSubgraph;
  depth: number;
  enabledKinds: Set<EdgeKind>;
  onNodeClick?: (nodeId: string) => void;
  onNodeHover?: (nodeId: string | null) => void;
  onPinView?: () => void;
}

export function GraphPane({
  subgraph,
  enabledKinds,
  onNodeClick,
  onNodeHover,
  onPinView,
}: GraphPaneProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const source = buildSource(subgraph, enabledKinds);
    const id = `arch-${Date.now()}`;
    mermaid
      .render(id, source)
      .then(({ svg }) => {
        if (cancelled || !ref.current) return;
        ref.current.innerHTML = svg;

        ref.current
          .querySelectorAll<SVGTextElement | HTMLElement>(".edgeLabel, .edgeLabel *")
          .forEach((el) => {
            el.style.paintOrder = "stroke";
            el.style.stroke = "var(--color-paper)";
            el.style.strokeWidth = "3px";
            el.style.strokeLinejoin = "round";
          });

        ref.current
          .querySelectorAll<SVGGElement>("g.cluster rect")
          .forEach((rect) => {
            rect.style.fill = "var(--color-paper-2)";
            rect.style.stroke = "var(--color-rule)";
            rect.style.strokeDasharray = "4 4";
            rect.style.opacity = "0.55";
          });

        ref.current.querySelectorAll<SVGGElement>("g.node").forEach((node) => {
          const raw = node.id;
          const nodeId = raw.replace(/^flowchart-/, "").replace(/-\d+$/, "");
          node.style.cursor = "pointer";
          node.addEventListener("click", (ev) => {
            ev.stopPropagation();
            onNodeClick?.(nodeId);
          });
          node.addEventListener("mouseenter", () => {
            setHovered(nodeId);
            onNodeHover?.(nodeId);
          });
          node.addEventListener("mouseleave", () => {
            setHovered((cur) => (cur === nodeId ? null : cur));
            onNodeHover?.(null);
          });
        });
      })
      .catch((err) => {
        if (!cancelled && ref.current) {
          ref.current.innerHTML = `<pre class="p-4 text-xs text-oxblood">${escapeHtml(String(err))}</pre>`;
        }
      });
    return () => {
      cancelled = true;
    };
  }, [subgraph, enabledKinds, onNodeClick, onNodeHover]);

  const hoveredNode = subgraph.nodes.find((n) => n.id === hovered);

  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col">
      <div className="flex h-9 items-center gap-3 border-b border-rule bg-vellum px-3 text-[11px] text-ink-3">
        <span className="font-mono text-ink">{subgraph.rootId}</span>
        <span className="text-ink-4">·</span>
        <span>{subgraph.nodes.length} nodes</span>
        <span className="text-ink-4">·</span>
        <span>
          {subgraph.edges.filter((e) => enabledKinds.has(e.kind)).length} edges
        </span>
        <button
          onClick={onPinView}
          disabled={!onPinView}
          className="ml-auto rounded px-2 py-1 text-ink-3 hover:bg-paper-2 hover:text-ink disabled:pointer-events-none disabled:opacity-40"
        >
          Pin view
        </button>
      </div>
      <div
        className="relative min-h-0 flex-1 overflow-auto"
        style={{
          backgroundImage:
            "radial-gradient(circle, color-mix(in oklab, var(--color-ink-4) 25%, transparent) 1px, transparent 1.2px)",
          backgroundSize: "24px 24px",
          backgroundPosition: "10px 10px",
        }}
      >
        <div ref={ref} className="flex justify-center p-6" />
      </div>
      {hoveredNode && (
        <aside className="absolute right-3 top-12 w-[280px] rounded border border-rule bg-vellum p-3 shadow-lg">
          <div className="font-mono text-[11px] text-ink">
            {hoveredNode.shortName}
          </div>
          {hoveredNode.layer && (
            <div className="mt-1 text-[10px] text-ink-4">
              layer: {hoveredNode.layer}
            </div>
          )}
          {hoveredNode.doc && (
            <p className="mt-2 text-[11px] leading-relaxed text-ink-3">
              {hoveredNode.doc}
            </p>
          )}
          <div className="mt-2 font-mono text-[10px] text-ink-4">
            {hoveredNode.file}:{hoveredNode.line}
          </div>
        </aside>
      )}
    </div>
  );
}
