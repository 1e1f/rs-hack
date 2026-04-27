import { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";
import type { ArchEdge, ArchNode, ArchSubgraph, EdgeKind } from "../../types";
import { NodeHoverCard } from "./NodeHoverCard";
import { NodeActionMenu } from "./NodeActionMenu";

/* Mermaid's color parser doesn't resolve `var(...)`, so theme values have to
   be literal strings. Resolve from the live computed style each render so a
   theme flip on [data-theme] re-themes the graph. */
function cssVar(name: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}

function initMermaid() {
  mermaid.initialize({
    startOnLoad: false,
    theme: "base",
    themeVariables: {
      background: cssVar("--color-paper", "#f5efe1"),
      primaryColor: cssVar("--color-vellum", "#ede4cf"),
      primaryTextColor: cssVar("--color-ink", "#2a1f12"),
      primaryBorderColor: cssVar("--color-rule", "#b7a98a"),
      lineColor: cssVar("--color-ink-3", "#6b5a3e"),
      secondaryColor: cssVar("--color-paper-2", "#ebe2ca"),
      tertiaryColor: cssVar("--color-paper-3", "#e2d8bd"),
      fontFamily: "Charter, Georgia, serif",
      fontSize: "12px",
    },
    flowchart: {
      htmlLabels: true,
      curve: "basis",
    },
    securityLevel: "loose",
  });
}

const ARROW: Record<EdgeKind, string> = {
  depends_on: "-->",
  message_flow: "-.->",
  data_flow: "-->",
  bridge: "==>",
  context: "-.->|ctx|",
  implements: "-.->|impl|",
};

interface Palette {
  vellum: string;
  ink: string;
  edgeHue: Record<EdgeKind, string>;
  layerHue: Record<string, string>;
}

/* Resolve all CSS vars to literal strings before they hit mermaid's parser
   (it rejects `var(...)` in classDef / linkStyle). Called per-render so a
   theme flip on [data-theme] picks up the new palette. */
function buildPalette(): Palette {
  return {
    vellum: cssVar("--color-vellum", "#ede4cf"),
    ink: cssVar("--color-ink", "#2a1f12"),
    edgeHue: {
      depends_on: cssVar("--color-ink-3", "#6b5a3e"),
      message_flow: cssVar("--color-midnight", "#1f3a5f"),
      data_flow: cssVar("--color-forest", "#2f5b3a"),
      bridge: cssVar("--color-oxblood", "#7a2a2a"),
      context: cssVar("--color-brass", "#9c7a2a"),
      implements: cssVar("--color-plum", "#5a2f5b"),
    },
    layerHue: {
      audio: cssVar("--color-midnight", "#1f3a5f"),
      dispatch: cssVar("--color-brass", "#9c7a2a"),
      io: cssVar("--color-forest", "#2f5b3a"),
      state: cssVar("--color-plum", "#5a2f5b"),
      core: cssVar("--color-oxblood", "#7a2a2a"),
      view: cssVar("--color-midnight", "#1f3a5f"),
    },
  };
}

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

function layerClassName(layer: string | undefined, layerHue: Record<string, string>): string {
  return layer && layerHue[layer] ? layer : "core";
}

function buildSource(
  graph: ArchSubgraph,
  enabled: Set<EdgeKind>,
  palette: Palette,
): string {
  const lines: string[] = ["graph LR"];

  const byLayer = new Map<string, ArchNode[]>();
  for (const n of graph.nodes) {
    const k = layerClassName(n.layer, palette.layerHue);
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

  for (const [layer, hue] of Object.entries(palette.layerHue)) {
    lines.push(
      `  classDef ${layer} fill:${palette.vellum},stroke:${hue},stroke-width:1.4px,color:${palette.ink};`,
    );
  }
  for (const n of graph.nodes) {
    lines.push(`  class ${n.id} ${layerClassName(n.layer, palette.layerHue)};`);
  }

  filtered.forEach((e, i) => {
    lines.push(`  linkStyle ${i} stroke:${palette.edgeHue[e.kind]},stroke-width:1.4px;`);
  });

  return lines.join("\n");
}

interface GraphPaneProps {
  subgraph: ArchSubgraph;
  depth: number;
  enabledKinds: Set<EdgeKind>;
  onReroot?: (nodeId: string) => void;
  onJumpToFile?: (fileColon: string) => void;
  onOpenInAgent?: (nodeId: string) => void;
  onNodeHover?: (nodeId: string | null) => void;
  onPinView?: () => void;
}

interface ActionMenuState {
  node: ArchNode;
  x: number;
  y: number;
}

export function GraphPane({
  subgraph,
  enabledKinds,
  onReroot,
  onJumpToFile,
  onOpenInAgent,
  onNodeHover,
  onPinView,
}: GraphPaneProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [menu, setMenu] = useState<ActionMenuState | null>(null);
  /* Bumped by a MutationObserver on [data-theme] so re-themes re-render. */
  const [themeTick, setThemeTick] = useState(0);

  useEffect(() => {
    const obs = new MutationObserver(() => setThemeTick((t) => t + 1));
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => obs.disconnect();
  }, []);

  useEffect(() => {
    let cancelled = false;
    initMermaid();
    const palette = buildPalette();
    const source = buildSource(subgraph, enabledKinds, palette);
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
            const archNode = subgraph.nodes.find((n) => n.id === nodeId);
            if (!archNode) return;
            setMenu({ node: archNode, x: ev.clientX, y: ev.clientY });
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
  }, [subgraph, enabledKinds, onNodeHover, themeTick]);

  const hoveredNode = subgraph.nodes.find((n) => n.id === hovered) ?? null;

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
      <NodeHoverCard node={menu ? null : hoveredNode} />
      {menu && (
        <NodeActionMenu
          node={menu.node}
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          onJumpToSource={(n) => onJumpToFile?.(`${n.file}:${n.line}`)}
          onReroot={(n) => onReroot?.(n.id)}
          onOpenInAgent={(n) => onOpenInAgent?.(n.id)}
        />
      )}
    </div>
  );
}
