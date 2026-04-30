/* Body for the host's `arch_node` tool — mirrors
   `KgService::node` (`{ node: NodeFull | null }`). Only surfaces the
   load-bearing fields (qualified, label, file:line, kind/lang) — the full
   blob is in the JSON if the agent needs it later. */

interface ArchNodeResultProps {
  result?: {
    node?: {
      qualified?: string;
      label?: string;
      file?: string;
      span?: { start_line?: number };
      lang?: string;
      kind?: unknown;
    } | null;
  };
  onJumpToFile?: (fileColon: string) => void;
}

export function ArchNodeResult({ result, onJumpToFile }: ArchNodeResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Looking up node…
      </div>
    );
  }
  const node = result.node;
  if (!node) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Node not found.
      </div>
    );
  }
  const fileColon =
    node.file && node.span?.start_line
      ? `${node.file}:${node.span.start_line}`
      : node.file ?? null;
  return (
    <div className="px-3 py-2 font-mono text-[11.5px] text-ink-2">
      <div className="text-ink truncate">{node.qualified ?? node.label}</div>
      {fileColon && (
        <button
          type="button"
          onClick={() => onJumpToFile?.(fileColon)}
          disabled={!onJumpToFile}
          className="mt-1 truncate border-b border-dashed border-current text-[11px] text-accent disabled:cursor-default disabled:border-transparent disabled:text-ink-4"
        >
          {fileColon}
        </button>
      )}
      <div className="mt-1 text-[10.5px] text-ink-4">
        {node.lang ?? "?"} · {summarizeKind(node.kind)}
      </div>
    </div>
  );
}

function summarizeKind(kind: unknown): string {
  if (kind == null || typeof kind !== "object") return "?";
  const obj = kind as Record<string, unknown>;
  // Common kinds use { lang: "common", kind: "function" }; rust kinds use
  // { lang: "rust", kind: { rust_kind: "trait" } }. Pick whichever leaf is
  // a string — good enough for a one-line summary.
  const inner = obj.kind as unknown;
  if (typeof inner === "string") return inner;
  if (inner && typeof inner === "object") {
    const innerObj = inner as Record<string, unknown>;
    for (const v of Object.values(innerObj)) {
      if (typeof v === "string") return v;
    }
  }
  return "?";
}
