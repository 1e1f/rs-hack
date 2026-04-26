import { useState } from "react";

export function PromptInput() {
  const [value, setValue] = useState("");

  return (
    <div className="border-t border-border bg-surface px-4 py-3">
      <div className="mx-auto flex max-w-3xl items-end gap-2 rounded border border-border bg-elevated p-2 focus-within:border-blue/50">
        <textarea
          value={value}
          onChange={(e) => setValue(e.target.value)}
          placeholder="Drive the agent on this relay…"
          rows={1}
          className="min-h-[20px] flex-1 resize-none bg-transparent text-[12px] leading-relaxed text-text outline-none placeholder:text-text-muted"
          onInput={(e) => {
            const el = e.currentTarget;
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
          }}
        />
        <button
          disabled={!value.trim()}
          className="self-end rounded bg-blue/20 px-3 py-1 text-[11px] text-blue hover:bg-blue/30 disabled:opacity-40"
        >
          send ⏎
        </button>
      </div>
      <div className="mx-auto mt-1.5 flex max-w-3xl items-center gap-3 text-[10px] text-text-muted">
        <span>⏎ send</span>
        <span>⇧⏎ newline</span>
        <span className="ml-auto">attach: ⌘.</span>
      </div>
    </div>
  );
}
