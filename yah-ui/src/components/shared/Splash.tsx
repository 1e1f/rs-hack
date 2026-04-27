import { useEffect, useState } from "react";

/* Empty-state composite: wayfarer linocut + caption + subline.
   Surfaces in EmptyColumn / FillerSplash (board), no-session (agent),
   no-graph (arch), and the run-cluster coming-soon panes.

   Five base variants map 1:1 to a PNG pair (light + dark) under
   `/illustrations/wayfarer-<column>-<theme>.png`. Legacy aliases (scroll,
   lantern, camp, anvil, empty, signpost) survive from the design return so
   surfaces outside the board can pick a column-flavoured illustration
   without inventing new artwork. */

export type SplashVariant =
  | "zones"
  | "open"
  | "active"
  | "handoff"
  | "review"
  | "scroll"
  | "lantern"
  | "camp"
  | "anvil"
  | "empty"
  | "signpost";

type ColumnSlug = "zones" | "open" | "active" | "handoff" | "review";

const VARIANT_TO_COLUMN: Record<SplashVariant, ColumnSlug> = {
  zones: "zones",
  open: "open",
  active: "active",
  handoff: "handoff",
  review: "review",
  scroll: "zones",
  lantern: "open",
  camp: "handoff",
  anvil: "review",
  empty: "open",
  signpost: "open",
};

interface SplashProps {
  variant?: SplashVariant;
  width?: number;
  caption?: string;
  sub?: string;
}

/* Track <html data-theme="..."> so a theme flip swaps light/dark variants
   without remounting. Mirrors GraphPane's MutationObserver pattern. */
function useTheme(): "light" | "dark" {
  const get = (): "light" | "dark" => {
    if (typeof document === "undefined") return "light";
    return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
  };
  const [theme, setTheme] = useState<"light" | "dark">(get);
  useEffect(() => {
    const obs = new MutationObserver(() => setTheme(get()));
    obs.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => obs.disconnect();
  }, []);
  return theme;
}

export function Splash({
  variant = "open",
  width = 220,
  caption,
  sub,
}: SplashProps) {
  const col = VARIANT_TO_COLUMN[variant] ?? "open";
  const theme = useTheme();
  const src = `/illustrations/wayfarer-${col}-${theme}.png`;

  return (
    <div className="mx-auto flex max-w-[360px] flex-col items-center gap-3.5 px-4 py-5 text-center">
      <img
        src={src}
        alt=""
        draggable={false}
        style={{ width, height: "auto", maxWidth: "100%" }}
        className="opacity-90"
      />
      {caption && (
        <div className="font-display text-[16px] italic leading-[1.4] text-ink-2">
          {caption}
        </div>
      )}
      {sub && (
        <div className="max-w-[280px] text-[12px] leading-[1.5] text-ink-3">
          {sub}
        </div>
      )}
    </div>
  );
}
