import { useRef, useState, type KeyboardEvent } from "react";
import { Icon } from "../shared/Glyph";

interface PromptBarProps {
  onSend?: (text: string) => void;
  onStop?: () => void;
  streaming?: boolean;
}

export function PromptBar({ onSend, onStop, streaming }: PromptBarProps) {
  const [value, setValue] = useState("");
  const taRef = useRef<HTMLTextAreaElement>(null);

  const send = () => {
    const text = value.trim();
    if (!text) return;
    onSend?.(text);
    setValue("");
    if (taRef.current) taRef.current.style.height = "auto";
  };

  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      send();
      return;
    }
    if (e.key === "." && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      if (streaming) onStop?.();
    }
  };

  return (
    <div className="border-t border-rule/60 bg-paper-2/60 px-6 py-2.5">
      <div className="flex items-end gap-2 rounded border border-rule/60 bg-vellum/70 px-3 py-2 focus-within:border-accent/60 focus-within:shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-accent)_25%,transparent)]">
        <Icon name="paperclip" size={14} className="mb-1 text-ink-3" />
        <textarea
          ref={taRef}
          value={value}
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={onKeyDown}
          onInput={(e) => {
            const el = e.currentTarget;
            el.style.height = "auto";
            el.style.height = `${Math.min(el.scrollHeight, 200)}px`;
          }}
          placeholder="message agent…  ⌘↵ to send"
          rows={2}
          className="min-h-[40px] flex-1 resize-none bg-transparent text-[13px] leading-[1.5] text-ink outline-none placeholder:text-ink-4"
        />
        {streaming ? (
          <button
            onClick={() => onStop?.()}
            className="flex items-center gap-1.5 self-end rounded border border-rule/60 bg-paper-2/60 px-3 py-1 text-[12px] text-ink-2 hover:border-accent/60 hover:text-accent"
          >
            <Icon name="stop" size={12} />
            stop
          </button>
        ) : (
          <button
            onClick={send}
            disabled={!value.trim()}
            className="flex items-center gap-1.5 self-end rounded bg-accent px-3 py-1 text-[12px] text-vellum hover:bg-accent-2 disabled:opacity-40 disabled:hover:bg-accent"
          >
            <Icon name="send" size={12} />
            send
          </button>
        )}
      </div>
      <div className="mt-1.5 flex justify-end gap-3 text-[10.5px] text-ink-3">
        <span>
          <kbd>⌘↵</kbd> send
        </span>
        <span>
          <kbd>⌘.</kbd> stop
        </span>
        <span>
          <kbd>⇧⏎</kbd> newline
        </span>
      </div>
    </div>
  );
}
