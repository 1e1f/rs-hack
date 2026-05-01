import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import mermaid from "mermaid";
import type { ArchEdge, ArchNode, ArchSubgraph, EdgeKind } from "../../types";
import type { WireViolation } from "../../env/types";
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
  /* Rule-validator output. Nodes whose id matches a violation's `offending`
     (preferred) or `anchor` (fallback) get a red border + a `title` tooltip
     listing the offending rule kinds — both applied via post-render
     querySelector since mermaid's classDef doesn't take a per-node hue. */
  violations?: WireViolation[];
}

interface ActionMenuState {
  node: ArchNode;
  x: number;
  y: number;
}

interface Transform {
  x: number;
  y: number;
  scale: number;
}

const IDENTITY: Transform = { x: 0, y: 0, scale: 1 };
const MIN_SCALE = 0.1;
const MAX_SCALE = 4;
const PAN_THRESHOLD_PX = 4;

export function GraphPane({
  subgraph,
  enabledKinds,
  onReroot,
  onJumpToFile,
  onOpenInAgent,
  onNodeHover,
  onPinView,
  violations,
}: GraphPaneProps) {
  /* Bucket violations by the affected node id. We prefer `offending` (the
     node that broke the rule) over `anchor` (the node that authored the
     rule); both ids surface a marker so the user sees both ends of a
     failing rule. Memoized so the post-render styling effect doesn't
     re-fire on unrelated re-renders. */
  const violationsByNode = useMemo(() => {
    const map = new Map<string, WireViolation[]>();
    if (!violations) return map;
    for (const v of violations) {
      const ids = new Set<string>();
      if (v.offending) ids.add(v.offending);
      ids.add(v.anchor);
      for (const id of ids) {
        const arr = map.get(id) ?? [];
        arr.push(v);
        map.set(id, arr);
      }
    }
    return map;
  }, [violations]);
  const ref = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [hovered, setHovered] = useState<string | null>(null);
  const [menu, setMenu] = useState<ActionMenuState | null>(null);
  /* Bumped by a MutationObserver on [data-theme] so re-themes re-render. */
  const [themeTick, setThemeTick] = useState(0);
  /* Pan is a CSS translate on the wrapper; zoom is applied by writing
     the SVG's own width/height. CSS-scaling the wrapper rasterizes the
     foreignObject HTML labels mermaid emits (browsers snapshot HTML in
     SVG before transforming), which goes blurry past ~1.5x. Resizing
     the SVG directly forces a vector rerender at every step. */
  const [transform, setTransform] = useState<Transform>(IDENTITY);
  const transformRef = useRef<Transform>(IDENTITY);
  const naturalSizeRef = useRef<{ w: number; h: number } | null>(null);
  /* Tracks whether the current rootId has been auto-fit yet. Each
     mermaid re-render bumps `renderTick`, but we only want to reset the
     viewport on the *first* render for a given root — subsequent
     re-renders (caused by index_finished refetches, theme flips, edge-
     filter toggles) should preserve whatever pan/zoom the user is on.
     Reset to false in the rootId-change effect below. */
  const hasFittedRef = useRef(false);
  const dragRef = useRef<{
    startX: number;
    startY: number;
    startTx: number;
    startTy: number;
    moved: boolean;
  } | null>(null);
  /* Renderer revision so the fit effect knows when a fresh SVG landed
     (the mermaid render pass writes innerHTML asynchronously). */
  const [renderTick, setRenderTick] = useState(0);

  useEffect(() => {
    transformRef.current = transform;
  }, [transform]);

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
        /* Mermaid emits a viewBox + a width/height baked from layout.
           Capture the natural dimensions, then strip width/height/maxWidth
           so our zoom effect can drive them via inline width/height in
           pixels at every scale step. */
        const svgEl = ref.current.querySelector<SVGSVGElement>("svg");
        if (svgEl) {
          const vb = svgEl.viewBox.baseVal;
          let w = vb && vb.width > 0 ? vb.width : svgEl.clientWidth;
          let h = vb && vb.height > 0 ? vb.height : svgEl.clientHeight;
          if (!w || !h) {
            const rect = svgEl.getBoundingClientRect();
            w = w || rect.width;
            h = h || rect.height;
          }
          naturalSizeRef.current = { w, h };
          svgEl.style.maxWidth = "none";
          svgEl.removeAttribute("width");
          svgEl.removeAttribute("height");
          svgEl.style.display = "block";
        }

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
          /* Paint violations on the node's fill rect: an oxblood stroke
             (heavier than the layer hue) + a native title tooltip listing
             the offending rule kinds. We don't reach for SVG <title>
             elements directly because mermaid would clobber them on
             re-render; the DOM `title` attr is enough for hover hints. */
          const nodeViolations = violationsByNode.get(nodeId);
          if (nodeViolations && nodeViolations.length > 0) {
            const hasError = nodeViolations.some((v) => v.severity === "error");
            const stroke = hasError
              ? cssVar("--color-oxblood", "#7a2a2a")
              : cssVar("--color-brass", "#9c7a2a");
            node
              .querySelectorAll<SVGElement>("rect, polygon, circle, path")
              .forEach((shape) => {
                shape.style.stroke = stroke;
                shape.style.strokeWidth = "2.4px";
              });
            const tip = nodeViolations
              .map((v) => `${v.severity === "error" ? "✗" : "⚠"} ${v.rule_kind}: ${v.message}`)
              .join("\n");
            node.setAttribute("data-violations", String(nodeViolations.length));
            const titleEl = document.createElementNS(
              "http://www.w3.org/2000/svg",
              "title",
            );
            titleEl.textContent = tip;
            node.insertBefore(titleEl, node.firstChild);
          }
          node.style.cursor = "pointer";
          node.addEventListener("click", (ev) => {
            /* Drag-to-pan can end on a node — suppress the click in that
               case so the action menu doesn't pop after a pan release. */
            if (dragRef.current?.moved) return;
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
        setRenderTick((t) => t + 1);
      })
      .catch((err) => {
        if (!cancelled && ref.current) {
          ref.current.innerHTML = `<pre class="p-4 text-xs text-oxblood">${escapeHtml(String(err))}</pre>`;
        }
      });
    return () => {
      cancelled = true;
    };
  }, [subgraph, enabledKinds, onNodeHover, themeTick, violationsByNode]);

  /* Fit-to-view: scale the graph to ~90% of the container's smaller axis
     and center it. Reads natural dimensions from the captured viewBox,
     not from getBoundingClientRect (which would already include any
     prior scale). */
  const fitToView = useCallback(() => {
    const container = containerRef.current;
    const natural = naturalSizeRef.current;
    if (!container || !natural) return;
    const cw = container.clientWidth;
    const ch = container.clientHeight;
    if (cw === 0 || ch === 0 || natural.w === 0 || natural.h === 0) return;
    const scale = Math.min(
      MAX_SCALE,
      Math.max(MIN_SCALE, Math.min(cw / natural.w, ch / natural.h) * 0.9),
    );
    const x = (cw - natural.w * scale) / 2;
    const y = (ch - natural.h * scale) / 2;
    setTransform({ x, y, scale });
    hasFittedRef.current = true;
  }, []);

  /* Re-arm auto-fit when the rootId changes — switching to a new root
     (or opening a pinned view) should always recenter, since the user's
     prior viewport is meaningless against a different graph. */
  useEffect(() => {
    hasFittedRef.current = false;
  }, [subgraph.rootId]);

  useLayoutEffect(() => {
    if (renderTick === 0) return;
    if (hasFittedRef.current) return;
    fitToView();
  }, [renderTick, fitToView]);

  /* Push scale into the SVG's width/height so each zoom step rerenders
     vectors + foreignObject contents crisply instead of being a CSS
     bitmap stretch. Runs after every transform change. */
  useLayoutEffect(() => {
    const inner = ref.current;
    const natural = naturalSizeRef.current;
    if (!inner || !natural) return;
    const svg = inner.querySelector<SVGSVGElement>("svg");
    if (!svg) return;
    svg.style.width = `${natural.w * transform.scale}px`;
    svg.style.height = `${natural.h * transform.scale}px`;
  }, [transform.scale, renderTick]);

  const handleWheel = useCallback((ev: React.WheelEvent<HTMLDivElement>) => {
    ev.preventDefault();
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const mx = ev.clientX - rect.left;
    const my = ev.clientY - rect.top;
    setTransform((prev) => {
      /* Wheel deltas vary wildly by input device — clamp to a stable
         per-tick zoom factor so trackpad pinch and discrete-wheel mice
         feel similar. */
      const factor = Math.exp(-ev.deltaY * 0.0015);
      const next = Math.min(
        MAX_SCALE,
        Math.max(MIN_SCALE, prev.scale * factor),
      );
      if (next === prev.scale) return prev;
      const ratio = next / prev.scale;
      return {
        x: mx - (mx - prev.x) * ratio,
        y: my - (my - prev.y) * ratio,
        scale: next,
      };
    });
  }, []);

  const handleMouseDown = useCallback(
    (ev: React.MouseEvent<HTMLDivElement>) => {
      /* Only initiate pan from background mousedowns — node mousedowns
         bubble up too, but their click handler runs first and stops
         propagation, so the threshold + moved flag still gate things. */
      if (ev.button !== 0) return;
      dragRef.current = {
        startX: ev.clientX,
        startY: ev.clientY,
        startTx: transformRef.current.x,
        startTy: transformRef.current.y,
        moved: false,
      };
    },
    [],
  );

  useEffect(() => {
    function onMove(ev: MouseEvent) {
      const drag = dragRef.current;
      if (!drag) return;
      const dx = ev.clientX - drag.startX;
      const dy = ev.clientY - drag.startY;
      if (!drag.moved && Math.hypot(dx, dy) < PAN_THRESHOLD_PX) return;
      drag.moved = true;
      setTransform((prev) => ({
        x: drag.startTx + dx,
        y: drag.startTy + dy,
        scale: prev.scale,
      }));
    }
    function onUp() {
      const drag = dragRef.current;
      if (!drag) return;
      /* Keep `moved` true through the click-bubble tick so node click
         handlers can read it; clear on the next animation frame. */
      if (drag.moved) {
        requestAnimationFrame(() => {
          dragRef.current = null;
        });
      } else {
        dragRef.current = null;
      }
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

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
        <span className="ml-auto tabular-nums text-ink-4">
          {Math.round(transform.scale * 100)}%
        </span>
        <button
          onClick={fitToView}
          className="rounded px-2 py-1 text-ink-3 hover:bg-paper-2 hover:text-ink"
          title="Fit to view"
        >
          Fit
        </button>
        <button
          onClick={onPinView}
          disabled={!onPinView}
          className="rounded px-2 py-1 text-ink-3 hover:bg-paper-2 hover:text-ink disabled:pointer-events-none disabled:opacity-40"
        >
          Pin view
        </button>
      </div>
      <div
        ref={containerRef}
        onWheel={handleWheel}
        onMouseDown={handleMouseDown}
        className="relative min-h-0 flex-1 select-none overflow-hidden"
        style={{
          backgroundImage:
            "radial-gradient(circle, color-mix(in oklab, var(--color-ink-4) 25%, transparent) 1px, transparent 1.2px)",
          backgroundSize: `${24 * transform.scale}px ${24 * transform.scale}px`,
          backgroundPosition: `${10 + transform.x}px ${10 + transform.y}px`,
          cursor: dragRef.current?.moved ? "grabbing" : "grab",
        }}
      >
        <div
          ref={ref}
          className="absolute left-0 top-0"
          style={{
            transform: `translate(${transform.x}px, ${transform.y}px)`,
            transformOrigin: "0 0",
            willChange: "transform",
          }}
        />
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
