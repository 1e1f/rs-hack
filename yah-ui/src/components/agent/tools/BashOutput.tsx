interface BashOutputProps {
  cmd: string;
  result?: { stdout?: string; stderr?: string; exit?: number };
}

export function BashOutput({ cmd, result }: BashOutputProps) {
  const stdout = result?.stdout?.trimEnd() ?? "";
  const stderr = result?.stderr?.trimEnd() ?? "";
  return (
    <div className="px-3 py-2">
      <div className="mb-1 font-mono text-[11.5px] text-ink-3">
        <span className="text-st-review">$</span> {cmd}
      </div>
      {result ? (
        <pre className="max-h-56 overflow-auto rounded bg-paper-3/40 px-3 py-2 font-mono text-[11.5px] leading-relaxed text-ink-2 whitespace-pre-wrap">
          {stdout}
          {stderr && (
            <span className="text-st-bug">
              {stdout && "\n"}
              {stderr}
            </span>
          )}
        </pre>
      ) : (
        <div className="font-display text-[12.5px] italic text-ink-3">
          Running…
        </div>
      )}
    </div>
  );
}
