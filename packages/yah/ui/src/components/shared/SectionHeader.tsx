import type { ReactNode } from "react";

interface SectionHeaderProps {
  /** Label text. The first character is rendered as the illuminated drop-cap;
      the remainder follows in small-caps eyebrow type. */
  children: string;
  /** Optional trailing slot — typically a count badge or action button. */
  right?: ReactNode;
  /** Trailing horizontal rule that fills remaining width. Default true. */
  rule?: boolean;
  /** Tightens the bottom margin for use inside dense card stacks. */
  dense?: boolean;
  className?: string;
}

/* Illuminated drop-cap eyebrow — used for column headers, side-rail sections,
   and any place the design wants a printed-page section break. The first
   character renders via the `.illum-cap` accent style; the remainder is
   the standard `.eyebrow` small-caps treatment. */
export function SectionHeader({
  children,
  right,
  rule = true,
  dense = false,
  className = "",
}: SectionHeaderProps) {
  const head = children.charAt(0);
  const tail = children.slice(1);
  return (
    <div
      className={`flex items-center gap-2 ${dense ? "mb-1" : "mb-2"} ${className}`}
    >
      <span className="inline-flex items-baseline">
        <span className="illum-cap">{head}</span>
        <span className="eyebrow">{tail}</span>
      </span>
      {rule && <span className="h-px flex-1 bg-rule/50" />}
      {right && <span className="shrink-0">{right}</span>}
    </div>
  );
}
