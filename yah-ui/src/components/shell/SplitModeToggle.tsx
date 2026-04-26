import { useRef, useState } from "react";
import { Glyph, Icon } from "../shared/Glyph";
import type { GlyphName } from "../shared/Glyph";
import { Menu } from "../shared/Menu";
import type { Tab, TabGroup } from "../../types";

interface SplitModeToggleProps {
  activeTab: Tab;
  value: Tab | null;
  onChange: (partner: Tab | null) => void;
}

interface PartnerSpec {
  id: Tab;
  label: string;
  glyph: GlyphName;
  group: TabGroup;
}

const PARTNERS: PartnerSpec[] = [
  { id: "board", label: "Board", glyph: "g-board", group: "design" },
  { id: "arch", label: "Architecture", glyph: "g-arch", group: "design" },
  { id: "agent", label: "Agent", glyph: "g-talk", group: "design" },
  { id: "terminal", label: "Terminal", glyph: "g-pc", group: "run" },
  { id: "preview", label: "Preview", glyph: "g-preview", group: "run" },
  { id: "files", label: "Files", glyph: "g-files", group: "run" },
  { id: "services", label: "Services", glyph: "g-services", group: "run" },
];

/* Right-of-TitleBar split-pane control. Closed: shows split/single icon plus
   the partner tab when active. Open: a menu offering "Single pane" plus a
   list of partner tabs (cross-cluster pairings listed first since the design
   favors design+run pairings over same-cluster). */
export function SplitModeToggle({
  activeTab,
  value,
  onChange,
}: SplitModeToggleProps) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);

  const active = PARTNERS.find((p) => p.id === activeTab);
  const partner = value ? PARTNERS.find((p) => p.id === value) : null;
  const activeGroup = active?.group;
  const others = PARTNERS.filter((p) => p.id !== activeTab);
  const cross = others.filter((p) => p.group !== activeGroup);
  const same = others.filter((p) => p.group === activeGroup);

  return (
    <div className="relative">
      <button
        ref={ref}
        onClick={() => setOpen((o) => !o)}
        title="Split pane"
        className={`flex items-center gap-1 rounded-[5px] border px-2 py-1 ${
          value
            ? "border-rule bg-vellum"
            : "border-transparent hover:bg-vellum/55"
        }`}
      >
        <Icon name={value ? "split" : "single"} size={13} className="text-ink-2" />
        {partner && (
          <>
            <span className="mx-0.5 h-2.5 w-px bg-rule/50" />
            <Glyph name={partner.glyph} size={11} className="text-ink-3" />
            <span className="text-[11px] text-ink-3">{partner.label}</span>
          </>
        )}
        <Icon name="chevron-down" size={11} className="ml-0.5 text-ink-3" />
      </button>
      <Menu open={open} onClose={() => setOpen(false)} anchorRef={ref} align="right" width={240}>
        <button
          onClick={() => {
            onChange(null);
            setOpen(false);
          }}
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:bg-vellum-2 ${
            !value ? "text-ink" : "text-ink-3"
          }`}
        >
          <Icon name="single" size={13} />
          <span className="flex-1">Single pane</span>
          {!value && <Icon name="check" size={11} className="text-ink-3" />}
        </button>
        <div className="eyebrow px-2 pb-1 pt-2">Split with</div>
        {cross.map((p) => (
          <PartnerOption
            key={p.id}
            p={p}
            value={value}
            onChange={onChange}
            close={() => setOpen(false)}
          />
        ))}
        {same.length > 0 && <div className="my-1.5 mx-1 border-t border-rule/40" />}
        {same.map((p) => (
          <PartnerOption
            key={p.id}
            p={p}
            value={value}
            onChange={onChange}
            close={() => setOpen(false)}
          />
        ))}
      </Menu>
    </div>
  );
}

function PartnerOption({
  p,
  value,
  onChange,
  close,
}: {
  p: PartnerSpec;
  value: Tab | null;
  onChange: (t: Tab | null) => void;
  close: () => void;
}) {
  const isActive = value === p.id;
  return (
    <button
      onClick={() => {
        onChange(isActive ? null : p.id);
        close();
      }}
      className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:bg-vellum-2 ${
        isActive ? "bg-vellum text-ink" : "text-ink-3"
      }`}
    >
      <Glyph name={p.glyph} size={13} className={isActive ? "text-ink" : "text-ink-3"} />
      <span className="flex-1">{p.label}</span>
      <span className="text-[9px] tracking-[0.1em] text-ink-4 [font-variant-caps:all-small-caps]">
        {p.group}
      </span>
    </button>
  );
}
