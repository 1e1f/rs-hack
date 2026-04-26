import { Glyph, Icon } from "../shared/Glyph";
import { RigSelector } from "./RigSelector";
import { RelaySelector } from "./RelaySelector";
import { SplitModeToggle } from "./SplitModeToggle";
import type { Rig, Tab, Theme, Ticket } from "../../types";

interface TitleBarProps {
  rigs: Rig[];
  activeRigId: string;
  onRigChange: (id: string) => void;
  relays: Ticket[];
  activeRelayId: string | null;
  onRelayChange: (id: string | null) => void;
  theme: Theme;
  onThemeChange: (t: Theme) => void;
  activeTab: Tab;
  splitMode: Tab | null;
  onSplitModeChange: (t: Tab | null) => void;
}

/* Top chrome bar: traffic-light spacer (78px), wordmark, rig › relay
   selectors, then right-side cluster of split toggle / theme / settings.
   Uses parchment palette and serif wordmark to match the design. */
export function TitleBar({
  rigs,
  activeRigId,
  onRigChange,
  relays,
  activeRelayId,
  onRelayChange,
  theme,
  onThemeChange,
  activeTab,
  splitMode,
  onSplitModeChange,
}: TitleBarProps) {
  return (
    <header className="relative flex h-11 items-center gap-2.5 border-b border-rule/50 bg-paper-2/60 pl-[78px] pr-2.5">
      <TrafficLights />

      <div className="mr-1 flex items-center gap-1.5">
        <YahMark />
        <span className="font-display text-[18px] font-medium tracking-[0.5px] text-ink">
          yah
        </span>
      </div>

      <span className="mx-1 h-5 w-px bg-rule/50" />

      <RigSelector
        rigs={rigs}
        activeId={activeRigId}
        onChange={onRigChange}
      />

      <span className="mx-0.5 font-display text-[14px] text-ink-3">›</span>

      <RelaySelector
        relays={relays}
        activeId={activeRelayId}
        onChange={onRelayChange}
      />

      <div className="flex-1" />

      <SplitModeToggle
        activeTab={activeTab}
        value={splitMode}
        onChange={onSplitModeChange}
      />

      <button
        onClick={() => onThemeChange(theme === "light" ? "dark" : "light")}
        title="Toggle theme"
        className="flex items-center justify-center rounded p-1.5 text-ink-2 hover:bg-vellum/55"
      >
        <Glyph name={theme === "light" ? "g-moon" : "g-sun"} size={17} />
      </button>
      <button
        title="Settings"
        className="flex items-center justify-center rounded p-1.5 text-ink-2 hover:bg-vellum/55"
      >
        <Icon name="settings" size={16} />
      </button>
    </header>
  );
}

/* macOS traffic-light placeholder pills — Tauri overlays the real ones in
   prod; in browser dev these stand in so the wordmark sits at the right inset. */
function TrafficLights() {
  return (
    <div className="absolute left-3 top-3.5 flex gap-2" aria-hidden>
      <span className="h-3 w-3 rounded-full bg-[#e96a64]" />
      <span className="h-3 w-3 rounded-full bg-[#e1ad3d]" />
      <span className="h-3 w-3 rounded-full bg-[#62c167]" />
    </div>
  );
}

function YahMark() {
  return (
    <svg width="22" height="22" viewBox="0 0 24 24" aria-hidden>
      <defs>
        <linearGradient id="yahmark-grad" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0%" stopColor="var(--color-accent)" />
          <stop offset="100%" stopColor="var(--color-accent-2)" />
        </linearGradient>
      </defs>
      <path
        d="M12 2 L21 6 L21 13 C21 18 17 21.5 12 22.5 C7 21.5 3 18 3 13 L3 6 Z"
        fill="url(#yahmark-grad)"
        opacity="0.9"
      />
      <path
        d="M8 9 L12 13 L16 9 M12 13 L12 17"
        stroke="#fff7e3"
        strokeWidth="1.6"
        fill="none"
        strokeLinecap="round"
        strokeLinejoin="round"
        opacity="0.9"
      />
    </svg>
  );
}
