interface ReadResultProps {
  path: string;
  range?: [number, number];
  /* From the paired tool result. Optional — until the result lands the frame
     just shows "reading…". Mock shape: { lines: number; summary: string }. */
  result?: { lines?: number; summary?: string };
}

export function ReadResult({ path, range, result }: ReadResultProps) {
  const span = range ? `lines ${range[0]}–${range[1]}` : "file";
  return (
    <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
      {result ? (
        <>
          Read {result.lines != null ? `${result.lines} lines` : span} from{" "}
          <span className="font-mono not-italic text-ink-2">{path}</span>
          {result.summary && <> — {result.summary}</>}
        </>
      ) : (
        <>Reading {span} from <span className="font-mono not-italic">{path}</span>…</>
      )}
    </div>
  );
}
