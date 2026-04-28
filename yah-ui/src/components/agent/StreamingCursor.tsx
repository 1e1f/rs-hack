export function StreamingCursor() {
  return (
    <div className="flex items-center gap-3 text-ink-3">
      <div className="w-7 shrink-0" aria-hidden />
      <span className="font-display text-[13px] italic">agent is working</span>
      <span className="candle inline-block h-1.5 w-1.5 rounded-full bg-accent" />
    </div>
  );
}
