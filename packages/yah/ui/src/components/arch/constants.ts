import type { EdgeKind } from "../../types";

export type EdgeStroke = "solid" | "dashed" | "dotted";

export interface EdgeKindMeta {
  id: EdgeKind;
  label: string;
  stroke: EdgeStroke;
  /** CSS color token; consumed by GraphPane for arrow tint and by EdgeKindFilters
      for the stroke sample. */
  color: string;
}

export const EDGE_KINDS: EdgeKindMeta[] = [
  { id: "depends_on",   label: "depends on",   stroke: "solid",  color: "var(--color-ink-2)" },
  { id: "message_flow", label: "message flow", stroke: "dashed", color: "var(--color-accent)" },
  { id: "data_flow",    label: "data flow",    stroke: "solid",  color: "var(--color-midnight)" },
  { id: "context",      label: "context",      stroke: "dotted", color: "var(--color-ink-3)" },
  { id: "bridge",       label: "bridge",       stroke: "dashed", color: "var(--color-brass)" },
  { id: "implements",   label: "implements",   stroke: "solid",  color: "var(--color-forest)" },
];

export const ALL_EDGE_KINDS: EdgeKind[] = EDGE_KINDS.map((k) => k.id);

/* Layer hues — keep keys in sync with GraphPane's classDef pass.
   Legend reads the same map so swatches match the rendered graph. */
export const LAYER_HUES: Record<string, string> = {
  audio:    "var(--color-midnight)",
  dispatch: "var(--color-brass)",
  io:       "var(--color-forest)",
  state:    "var(--color-plum)",
  core:     "var(--color-accent)",
  view:     "var(--color-midnight)",
};

export function strokeDasharray(s: EdgeStroke): string | undefined {
  if (s === "dashed") return "4 3";
  if (s === "dotted") return "1 2";
  return undefined;
}
