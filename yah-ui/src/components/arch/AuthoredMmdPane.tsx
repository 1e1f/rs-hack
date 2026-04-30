//! @yah:relay(R036, "Arch tab: markdown rendering + ticket-linked docs (retire architecture/ root)")
//! @yah:status(handoff)
//! @yah:assignee(agent:claude)
//! @yah:handoff("Three-axis relay: (1) extend .yah/arch/authored/ to accept .md (today only .mmd is rendered); (2) surface @arch:see(...) on ticket cards as clickable yah:// links to the arch tab; (3) migrate architecture/*.md into .yah/arch/authored/ and retire the architecture/ root. Reusable Markdown component already lives at yah-ui/src/components/agent/messages/Markdown.tsx (R028-F4 — handles fenced code, headings, lists, tables, yah:// link routing, copy-as-source). Ticket scanner already captures @arch:see lines on the work-item via see_also (yah-kg-anno/src/parser.rs:130-141) — purely a renderer/UI lift, no scanner changes.")
//! @yah:next("Children sequence: F1 (renderer) → T3 (migration) → F2 (ticket-card links) — F1 unblocks T3 (don't move docs before they render); F2 is independent of both.")
//! @yah:next("Open question: archive policy for @arch:see when a ticket archives. Recommended: leave the line in source as a durable breadcrumb (git log + grep keeps ticket↔doc trace). Alternative: move the doc to .yah/arch/archive/ (only matters if doc itself goes stale, orthogonal axis).")
//!
//! @yah:ticket(R036-F1, "Render .md files in arch tab (reuse Markdown.tsx + mermaid-fence specialization)")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R036)
//! @yah:next("Add AuthoredMdPane (or extend AuthoredMmdPane) to render .md via yah-ui/src/components/agent/messages/Markdown.tsx")
//! @yah:next("Specialize ```mermaid fences inside .md to render as diagrams (today they fall through as generic fenced code)")
//! @yah:next("Extend AuthoredFilesPicker.tsx glob to include .md alongside .mmd")

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import mermaid from "mermaid";
import { useAuthoredFileContent } from "../../env/hooks";
import { Splash } from "../shared/Splash";

/* Shared theme init with GraphPane — kept local rather than imported so a
   future divergence (e.g. authored diagrams want a different palette) is
   one local edit. */
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
    flowchart: { htmlLabels: true, curve: "basis" },
    securityLevel: "loose",
  });
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

interface Transform {
  x: number;
  y: number;
  scale: number;
}

const IDENTITY: Transform = { x: 0, y: 0, scale: 1 };
const MIN_SCALE = 0.1;
const MAX_SCALE = 4;

interface AuthoredMmdPaneProps {
  rigId: string;
  relPath: string;
}

/* Renders a raw `.mmd` file fetched via `arch.read_authored_file`. Unlike
   GraphPane this has no node interactivity (no reroot / open-in-agent /
   violation overlay) — authored diagrams are opaque pictures, not graph
   queries. Just the canvas + pan/zoom + a fit button. */
export function AuthoredMmdPane({ rigId, relPath }: AuthoredMmdPaneProps) {
  const { content, loading, error } = useAuthoredFileContent(rigId, relPath);

  const ref = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const naturalSizeRef = useRef<{ w: number; h: number } | null>(null);
  const hasFittedRef = useRef(false);
  const transformRef = useRef<Transform>(IDENTITY);
  const [transform, setTransform] = useState<Transform>(IDENTITY);
  const [renderTick, setRenderTick] = useState(0);
  const [renderError, setRenderError] = useState<string | null>(null);
  const dragRef = useRef<{
    startX: number;
    startY: number;
    startTx: number;
    startTy: number;
    moved: boolean;
  } | null>(null);
  const [themeTick, setThemeTick] = useState(0);

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

  /* Re-arm auto-fit on file switch — the natural dimensions of the new
     diagram have nothing to do with the prior one's pan/zoom. */
  useEffect(() => {
    hasFittedRef.current = false;
  }, [relPath]);

  useEffect(() => {
    if (!content) return;
    let cancelled = false;
    initMermaid();
    setRenderError(null);
    /* Mermaid needs a DOM-stable id; collisions across renders cause it to
       reuse the previous SVG's defs which then mis-style. Time-based id
       sidesteps that. */
    const id = `authored-${Date.now()}`;
    mermaid
      .render(id, content)
      .then(({ svg }) => {
        if (cancelled || !ref.current) return;
        ref.current.innerHTML = svg;
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
        setRenderTick((t) => t + 1);
      })
      .catch((err) => {
        if (cancelled) return;
        setRenderError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [content, themeTick]);

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

  useLayoutEffect(() => {
    if (renderTick === 0) return;
    if (hasFittedRef.current) return;
    fitToView();
  }, [renderTick, fitToView]);

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
      const factor = Math.exp(-ev.deltaY * 0.0015);
      const next = Math.min(MAX_SCALE, Math.max(MIN_SCALE, prev.scale * factor));
      if (next === prev.scale) return prev;
      const ratio = next / prev.scale;
      return {
        x: mx - (mx - prev.x) * ratio,
        y: my - (my - prev.y) * ratio,
        scale: next,
      };
    });
  }, []);

  const handleMouseDown = useCallback((ev: React.MouseEvent<HTMLDivElement>) => {
    if (ev.button !== 0) return;
    dragRef.current = {
      startX: ev.clientX,
      startY: ev.clientY,
      startTx: transformRef.current.x,
      startTy: transformRef.current.y,
      moved: false,
    };
  }, []);

  useEffect(() => {
    function onMove(ev: MouseEvent) {
      const drag = dragRef.current;
      if (!drag) return;
      const dx = ev.clientX - drag.startX;
      const dy = ev.clientY - drag.startY;
      drag.moved = true;
      setTransform((prev) => ({
        x: drag.startTx + dx,
        y: drag.startTy + dy,
        scale: prev.scale,
      }));
    }
    function onUp() {
      dragRef.current = null;
    }
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  if (error) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <Splash variant="anvil" caption="The forge sputtered" sub={error.message} />
      </div>
    );
  }

  if (loading && !content) {
    return (
      <div className="flex flex-1 items-center justify-center">
        <Splash variant="scroll" caption="Reading the scroll…" sub={relPath} />
      </div>
    );
  }

  return (
    <div className="relative flex h-full min-h-0 flex-1 flex-col">
      <div className="flex h-9 items-center gap-3 border-b border-rule bg-vellum px-3 text-[11px] text-ink-3">
        <span className="font-mono text-ink">{relPath}</span>
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
        {renderError ? (
          <pre className="absolute inset-0 overflow-auto p-4 text-xs text-oxblood">
            {escapeHtml(renderError)}
          </pre>
        ) : (
          <div
            ref={ref}
            className="absolute left-0 top-0"
            style={{
              transform: `translate(${transform.x}px, ${transform.y}px)`,
              transformOrigin: "0 0",
              willChange: "transform",
            }}
          />
        )}
      </div>
    </div>
  );
}
