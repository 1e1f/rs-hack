import { useEffect, useState } from "react";

/* Empty-state composite: wayfarer linocut + caption + subline.
   Surfaces in EmptyColumn / FillerSplash (board), no-session (agent),
   no-graph (arch), the test-cluster coming-soon panes, and the host-cluster
   coming-soon panes.

   Each "asset" maps 1:1 to a PNG pair (light + dark) under
   `/illustrations/solid-<asset>-<theme>.png`. Five board columns
   (zones/open/active/handoff/review) each have their own asset; three
   non-board assets (architecture, mirror, node) cover Design-arch, Host-services,
   and Host-infra respectively. Legacy aliases (scroll/lantern/camp/anvil/empty/
   signpost) survive from the design return and resolve through ALIAS_TO_ASSET
   so surfaces that already picked a column-flavoured illustration keep working. */

type AssetSlug =
  | "zones"
  | "open"
  | "active"
  | "handoff"
  | "review"
  | "architecture"
  | "mirror"
  | "node";

export type SplashVariant =
  | AssetSlug
  | "scroll"
  | "lantern"
  | "camp"
  | "anvil"
  | "empty"
  | "signpost";

const ALIAS_TO_ASSET: Record<
  "scroll" | "lantern" | "camp" | "anvil" | "empty" | "signpost",
  AssetSlug
> = {
  scroll: "zones",
  lantern: "open",
  camp: "handoff",
  anvil: "review",
  empty: "open",
  signpost: "open",
};

function resolveAsset(v: SplashVariant): AssetSlug {
  return v in ALIAS_TO_ASSET
    ? ALIAS_TO_ASSET[v as keyof typeof ALIAS_TO_ASSET]
    : (v as AssetSlug);
}

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
  const asset = resolveAsset(variant);
  const theme = useTheme();
  const src = `/illustrations/solid-${asset}-${theme}.png`;

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
