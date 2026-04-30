import { useEffect, useRef, useState } from "react";
import mermaid from "mermaid";
import { useAuthoredFileContent } from "../../env/hooks";
import { Markdown } from "../agent/messages/Markdown";
import { Splash } from "../shared/Splash";

/* Renders an authored `.md` arch doc fetched via `arch.read_authored_file`.
   Reuses Markdown.tsx for prose / code / lists / tables and specializes
   ```mermaid fences into live diagrams. The .mmd pane is a stage
   (mermaid all the way down); this pane is a scroll: paragraphs flow
   top-to-bottom, diagrams render inline at container width. */

interface AuthoredMdPaneProps {
  rigId: string;
  relPath: string;
  onJumpToFile?: (fileColon: string) => void;
  onYahLink?: (href: string) => void;
}

export function AuthoredMdPane({
  rigId,
  relPath,
  onJumpToFile,
  onYahLink,
}: AuthoredMdPaneProps) {
  const { content, loading, error } = useAuthoredFileContent(rigId, relPath);

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
      </div>
      <div className="min-h-0 flex-1 overflow-auto bg-paper">
        <div className="mx-auto max-w-3xl px-6 py-6">
          <Markdown
            source={content ?? ""}
            onJumpToFile={onJumpToFile}
            onYahLink={onYahLink}
            renderFence={(lang, body) =>
              lang === "mermaid" ? <MermaidFence source={body} /> : null
            }
          />
        </div>
      </div>
    </div>
  );
}

/* Inline mermaid renderer for ```mermaid fences inside a markdown doc.
   Unlike the AuthoredMmdPane stage there's no pan/zoom — the diagram
   sits in the document flow at its natural size, capped to container
   width. Re-renders on theme flip via the same `data-theme` mutation
   observer the .mmd pane uses. Each instance gets a unique id so
   mermaid's defs cache doesn't cross-contaminate styling. */
function MermaidFence({ source }: { source: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [error, setError] = useState<string | null>(null);
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
    setError(null);
    const id = `md-mermaid-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    mermaid
      .render(id, source)
      .then(({ svg }) => {
        if (cancelled || !ref.current) return;
        ref.current.innerHTML = svg;
        const svgEl = ref.current.querySelector<SVGSVGElement>("svg");
        if (svgEl) {
          svgEl.style.maxWidth = "100%";
          svgEl.style.height = "auto";
          svgEl.removeAttribute("width");
          svgEl.removeAttribute("height");
          svgEl.style.display = "block";
        }
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [source, themeTick]);

  if (error) {
    return (
      <pre className="my-2 overflow-x-auto rounded border border-rule/40 bg-paper-3/40 px-3 py-2 font-mono text-[12.5px] text-oxblood">
        {error}
      </pre>
    );
  }
  return (
    <div
      ref={ref}
      className="my-3 flex justify-center rounded border border-rule/40 bg-vellum px-3 py-3"
    />
  );
}

/* Mermaid global config — kept local to this file for the same reason
   AuthoredMmdPane keeps its copy: a future divergence in palette is one
   local edit away. Initialized lazily; mermaid is fine being initialized
   multiple times. */
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

function cssVar(name: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement)
    .getPropertyValue(name)
    .trim();
  return v || fallback;
}
