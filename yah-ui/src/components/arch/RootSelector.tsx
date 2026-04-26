interface RootSelectorProps {
  value: string;
  onChange: (id: string) => void;
}

export function RootSelector({ value, onChange }: RootSelectorProps) {
  return (
    <section>
      <div className="mb-1.5 text-[10px] font-medium uppercase tracking-wider text-text-muted">
        Root
      </div>
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="file or symbol…"
        className="w-full rounded border border-border bg-elevated px-2 py-1.5 font-mono text-[11px] text-text outline-none focus:border-blue"
      />
      <div className="mt-2 flex flex-col gap-1 text-[11px]">
        <button className="rounded px-2 py-1 text-left text-text-dim hover:bg-elevated">
          From current ticket
        </button>
        <button className="rounded px-2 py-1 text-left text-text-dim hover:bg-elevated">
          From current file in agent
        </button>
        <button className="rounded px-2 py-1 text-left text-text-dim hover:bg-elevated">
          Browse files…
        </button>
      </div>
    </section>
  );
}
