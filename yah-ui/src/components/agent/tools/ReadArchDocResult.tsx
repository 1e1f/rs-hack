/* Body for `read_arch_doc` — `{ rel_path, content, bytes }`. Renders a
   compact summary; the full content goes via the JSON the model sees. */

interface ReadArchDocResultProps {
  relPath: string;
  result?: {
    rel_path?: string;
    content?: string;
    bytes?: number;
  };
}

export function ReadArchDocResult({ relPath, result }: ReadArchDocResultProps) {
  if (!result) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        Reading <span className="font-mono not-italic">{relPath}</span>…
      </div>
    );
  }
  const lines = result.content ? result.content.split("\n").length : 0;
  return (
    <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
      Read{" "}
      <span className="font-mono not-italic text-ink-2">
        {result.rel_path ?? relPath}
      </span>
      {lines > 0 && <> — {lines} lines</>}
      {result.bytes != null && <span className="text-ink-4"> · {result.bytes}B</span>}
    </div>
  );
}
