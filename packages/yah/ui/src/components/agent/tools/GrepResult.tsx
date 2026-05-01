interface GrepHit {
  file: string;
  line: number;
  text: string;
}

interface GrepResultProps {
  pattern: string;
  glob?: string;
  result?: GrepHit[];
  onJumpToFile?: (fileColon: string) => void;
}

export function GrepResult({ result, onJumpToFile }: GrepResultProps) {
  if (!result || result.length === 0) {
    return (
      <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
        No matches.
      </div>
    );
  }
  return (
    <div className="py-1">
      {result.map((hit, i) => {
        const target = `${hit.file}:${hit.line}`;
        return (
          <button
            key={i}
            type="button"
            onClick={() => onJumpToFile?.(target)}
            className="flex w-full items-baseline gap-2.5 px-3 py-1 text-left hover:bg-vellum-2/60"
          >
            <span className="border-b border-dashed border-current font-mono text-[11px] text-accent">
              {hit.file}
            </span>
            <span className="font-mono text-[11px] text-ink-4">:{hit.line}</span>
            <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-ink-2">
              {hit.text}
            </span>
          </button>
        );
      })}
    </div>
  );
}
