import { Glyph } from "../shared/Glyph";
import type { GlyphName } from "../shared/Glyph";
import type { Tab, TabGroup } from "../../types";

interface TabSpec {
  id: Tab;
  label: string;
  glyph: GlyphName;
  group: TabGroup;
  hint: string;
}

const TABS: TabSpec[] = [
  { id: "board", label: "Board", glyph: "g-board", group: "design", hint: "1" },
  { id: "agent", label: "Agent", glyph: "g-talk", group: "design", hint: "2" },
  { id: "arch", label: "Architecture", glyph: "g-arch", group: "design", hint: "3" },
  { id: "files", label: "Files", glyph: "g-files", group: "test", hint: "4" },
  { id: "terminal", label: "Terminal", glyph: "g-pc", group: "test", hint: "5" },
  { id: "preview", label: "Preview", glyph: "g-preview", group: "test", hint: "6" },
  { id: "infra", label: "Infra", glyph: "g-pc", group: "host", hint: "7" },
  { id: "services", label: "Services", glyph: "g-services", group: "host", hint: "8" },
  { id: "analytics", label: "Analytics", glyph: "g-scry", group: "host", hint: "9" },
];

export const TAB_ORDER: Tab[] = TABS.map((t) => t.id);

interface TabStripProps {
  active: Tab;
  onChange: (t: Tab) => void;
}

/* Tab strip below TitleBar. Three clusters (Design / Test / Host) separated
   by thin dividers; right-aligned keyboard hints. Active tab gets an accent
   underline and accent-tinted glyph. */
export function TabStrip({ active, onChange }: TabStripProps) {
  const designTabs = TABS.filter((t) => t.group === "design");
  const testTabs = TABS.filter((t) => t.group === "test");
  const hostTabs = TABS.filter((t) => t.group === "host");
  return (
    <div className="relative flex items-stretch border-b border-rule/50 bg-paper-2/30 pl-1.5">
      <ClusterLabel>Design</ClusterLabel>
      {designTabs.map((t) => (
        <TabButton key={t.id} t={t} active={active} onChange={onChange} />
      ))}
      <ClusterDivider />
      <ClusterLabel>Test</ClusterLabel>
      {testTabs.map((t) => (
        <TabButton key={t.id} t={t} active={active} onChange={onChange} />
      ))}
      <ClusterDivider />
      <ClusterLabel>Host</ClusterLabel>
      {hostTabs.map((t) => (
        <TabButton key={t.id} t={t} active={active} onChange={onChange} />
      ))}
      <div className="flex-1" />
      <KbdHint />
    </div>
  );
}

function TabButton({
  t,
  active,
  onChange,
}: {
  t: TabSpec;
  active: Tab;
  onChange: (t: Tab) => void;
}) {
  const isActive = active === t.id;
  return (
    <button
      onClick={() => onChange(t.id)}
      className={`group relative flex items-center gap-1.5 px-3.5 py-2.5 font-display text-[15px] tracking-[0.02em] ${
        isActive ? "text-ink" : "text-ink-3 hover:text-ink-2"
      }`}
    >
      <Glyph
        name={t.glyph}
        size={16}
        className={`mr-0.5 ${isActive ? "text-accent" : "text-ink-3"}`}
      />
      <span>{t.label}</span>
      <span className="ml-1 text-[10px] text-ink-4 opacity-0 transition-opacity group-hover:opacity-100">
        {t.hint}
      </span>
      {isActive && (
        <span className="absolute inset-x-2 -bottom-px h-px bg-accent" />
      )}
    </button>
  );
}

function ClusterLabel({ children }: { children: string }) {
  return (
    <span className="flex items-center px-2 font-display text-[10px] font-medium tracking-[0.18em] text-ink-4 [font-variant-caps:all-small-caps]">
      {children}
    </span>
  );
}

function ClusterDivider() {
  return (
    <span
      aria-hidden
      className="mx-1.5 h-5 w-px self-center bg-gradient-to-b from-transparent via-rule/60 to-transparent"
    />
  );
}

function KbdHint() {
  return (
    <div className="flex items-center gap-2 pr-2 text-[11px] text-ink-3">
      <span>
        <kbd>1–9</kbd> tabs
      </span>
      <span className="h-3 w-px bg-rule/50" />
      <span>
        <kbd>⌘K</kbd> relay
      </span>
      <span className="h-3 w-px bg-rule/50" />
      <span>
        <kbd>⌘\</kbd> split
      </span>
    </div>
  );
}
