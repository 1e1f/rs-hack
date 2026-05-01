interface EditDiffProps {
  /* Mock shape is a flat string with \n-joined lines, each prefixed `+`, `-`,
     or ` `. Backend will likely send the same shape. Hunk headers (`@@…@@`)
     pass through as context rows. */
  diff: string;
}

type Tone = "add" | "del" | "ctx";

export function EditDiff({ diff }: EditDiffProps) {
  const lines = diff.split("\n");
  return (
    <pre className="max-h-72 overflow-auto py-2 font-mono text-[11.5px] leading-relaxed">
      {lines.map((line, i) => {
        const tone: Tone =
          line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
        const text = tone === "ctx" ? line : line.slice(1);
        return (
          <div
            key={i}
            className={`flex px-3 ${
              tone === "add"
                ? "bg-[color-mix(in_oklab,var(--color-st-review)_14%,transparent)]"
                : tone === "del"
                ? "bg-[color-mix(in_oklab,var(--color-st-bug)_14%,transparent)]"
                : ""
            }`}
          >
            <span
              className={`w-4 shrink-0 ${
                tone === "add"
                  ? "text-st-review"
                  : tone === "del"
                  ? "text-st-bug"
                  : "text-ink-4"
              }`}
            >
              {tone === "add" ? "+" : tone === "del" ? "−" : " "}
            </span>
            <span
              className={`flex-1 whitespace-pre-wrap ${
                tone === "ctx" ? "text-ink-3" : "text-ink"
              }`}
            >
              {text || "\u00A0"}
            </span>
          </div>
        );
      })}
    </pre>
  );
}
