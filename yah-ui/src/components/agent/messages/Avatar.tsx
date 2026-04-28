interface AvatarProps {
  kind: "user" | "agent";
  muted?: boolean;
}

export function Avatar({ kind, muted = false }: AvatarProps) {
  const ch = kind === "user" ? "Y" : "✦";
  const tone =
    muted
      ? "bg-ink-4/30 text-ink-2"
      : kind === "user"
      ? "bg-midnight/90 text-vellum"
      : "bg-accent/90 text-vellum";
  return (
    <div
      className={`flex h-7 w-7 shrink-0 items-center justify-center rounded font-display text-[14px] font-medium shadow-[0_1px_0_rgba(0,0,0,0.1)] ${tone}`}
    >
      {ch}
    </div>
  );
}
