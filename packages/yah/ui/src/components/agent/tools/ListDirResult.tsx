/* Body for the host's `list_dir` tool. The wire shape mirrors
   `app/tauri/src/agent_tools.rs::ListDir::execute`:
   `{ path, entries: [{ name, kind, bytes? }], total, truncated }`. */

interface ListDirEntry {
  name: string;
  kind: "dir" | "file" | "symlink" | "other";
  bytes?: number | null;
}

interface ListDirResultProps {
  path: string;
  result?: {
    entries?: ListDirEntry[];
    total?: number;
    truncated?: boolean;
  };
}

export function ListDirResult({ path, result }: ListDirResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Listing <span className="font-mono not-italic">{path || "."}</span>…
      </div>
    );
  }
  const entries = result.entries ?? [];
  if (entries.length === 0) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Empty directory.
      </div>
    );
  }
  return (
    <div className="py-1">
      {entries.map((entry, i) => (
        <div
          key={i}
          className="flex items-baseline gap-2.5 px-3 py-0.5 font-mono text-[11px]"
        >
          <span className="text-ink-4">
            {entry.kind === "dir" ? "📁" : entry.kind === "symlink" ? "↪" : "·"}
          </span>
          <span className="min-w-0 flex-1 truncate text-ink-2">
            {entry.name}
            {entry.kind === "dir" ? "/" : ""}
          </span>
          {entry.bytes != null && entry.kind === "file" && (
            <span className="text-ink-4">{formatBytes(entry.bytes)}</span>
          )}
        </div>
      ))}
      {result.truncated && (
        <div className="px-3 py-1 font-display text-[11.5px] italic text-ink-4">
          truncated · {result.total ?? entries.length} total
        </div>
      )}
    </div>
  );
}

function formatBytes(b: number): string {
  if (b < 1024) return `${b}B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)}K`;
  return `${(b / 1024 / 1024).toFixed(1)}M`;
}
