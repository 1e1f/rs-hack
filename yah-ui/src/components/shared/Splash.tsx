import type { ReactElement } from "react";

/* Empty-state composite: ornament + caption + subline.
   Surfaces in EmptyColumn / FillerSplash (board), no-session (agent),
   no-graph (arch), and the run-cluster coming-soon panes.

   v1 ships a vector ornament placeholder in lieu of the linocut PNGs
   (R011 / Phase 5 wires those in via two themed sets — wayfarer + arcane —
   keyed by `<html data-illo-set="…">` and `<html data-theme="…">`). The
   prop signature here mirrors the design return so P5 swaps the inner
   SVG for an `<img>` without touching callers. */

export type SplashVariant =
  | "zones"
  | "open"
  | "active"
  | "handoff"
  | "review"
  /* Legacy aliases used by run-tabs / arch / agent surfaces — map to a
     column slug so the same illustration set covers all four panes. */
  | "scroll"
  | "lantern"
  | "camp"
  | "anvil"
  | "empty"
  | "signpost";

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

type ColumnSlug = "zones" | "open" | "active" | "handoff" | "review";

interface SplashProps {
  variant?: SplashVariant;
  width?: number;
  caption?: string;
  sub?: string;
}

export function Splash({
  variant = "open",
  width = 220,
  caption,
  sub,
}: SplashProps) {
  const col = VARIANT_TO_COLUMN[variant] ?? "open";
  return (
    <div className="mx-auto flex max-w-[360px] flex-col items-center gap-3.5 px-4 py-5 text-center">
      <Ornament slug={col} width={width} />
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

/* Per-column placeholder ornament. Each slug gets a distinct silhouette
   so empty columns read differently at a glance — the linocut PNGs that
   replace these in P5 follow the same one-per-column pattern. All shapes
   render in `currentColor` against the ink-4 inheritance so they sit
   quietly inside the dashed parchment frame. */
function Ornament({
  slug,
  width,
}: {
  slug: ColumnSlug;
  width: number;
}): ReactElement {
  const path = ORNAMENTS[slug];
  return (
    <svg
      width={width}
      height={width * 0.62}
      viewBox="0 0 220 136"
      aria-hidden
      className="text-ink-4 opacity-80"
    >
      <path d={path} fill="currentColor" />
    </svg>
  );
}

/* Heraldic placeholder silhouettes — book-stack (zones), wayfarer with
   staff (open), shop sign (active), cookpot (handoff), bottles (review).
   Hand-tuned to the 220×136 viewBox so the dashed frame around them feels
   intentional, not arbitrary. */
const ORNAMENTS: Record<ColumnSlug, string> = {
  zones:
    "M30 96 H190 V104 H30 Z M40 88 H180 V96 H40 Z M50 60 H170 V88 H50 Z M58 52 H162 V60 H58 Z M68 28 H152 V52 H68 Z M76 22 H144 V28 H76 Z",
  open:
    "M110 28 a8 8 0 1 1 0.01 0 Z M104 40 H116 V72 H104 Z M104 72 L94 104 H102 L110 80 L118 104 H126 L116 72 Z M118 48 L142 60 L142 66 L118 56 Z",
  active:
    "M44 36 H176 V44 H44 Z M52 44 H168 V92 H52 Z M58 52 H162 V84 H58 Z M104 92 H116 V108 H104 Z M40 108 H180 V112 H40 Z",
  handoff:
    "M70 56 H150 a14 14 0 0 1 14 14 V92 a14 14 0 0 1 -14 14 H70 a14 14 0 0 1 -14 -14 V70 a14 14 0 0 1 14 -14 Z M82 40 H92 V56 H82 Z M128 40 H138 V56 H128 Z M50 108 H170 V112 H50 Z",
  review:
    "M68 36 H78 V52 H68 Z M68 52 H78 V100 H68 Z M96 28 H106 V52 H96 Z M96 52 H106 V100 H96 Z M124 32 H134 V52 H124 Z M124 52 H134 V100 H124 Z M152 24 H162 V52 H152 Z M152 52 H162 V100 H152 Z M50 100 H180 V108 H50 Z",
};
