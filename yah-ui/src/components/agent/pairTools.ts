import type { SessionEvent } from "../../types";

export interface RenderedItem {
  event: SessionEvent;
  result?: SessionEvent;
}

/* Pair each `tool_use` with its matching `tool` result and drop the
   standalone result event from the rendered list — the tool surface
   (ToolFrame + per-tool body) renders args + result as a single card.

   Pairing prefers `toolCallId` (the runner mints one per provider
   tool_call) and falls back to next-with-matching-tool for legacy
   fixtures and for the OpenAI streaming path before the host has
   adopted explicit ids. Parallel tool calls — where the model issues
   several at once and they resolve out of order — only round-trip
   correctly under the id-based path. */
export function pairTools(events: SessionEvent[]): RenderedItem[] {
  const out: RenderedItem[] = [];
  const consumed = new Set<string>();

  for (let i = 0; i < events.length; i++) {
    const e = events[i];
    if (consumed.has(e.id)) continue;
    if (e.role === "tool") {
      // Stray tool result with no preceding tool_use — drop it. The
      // model still saw the JSON; the renderer would have nothing to
      // pair the chip onto.
      continue;
    }
    if (e.role === "assistant" && e.type === "tool_use") {
      const result = findResult(events, i, e);
      if (result) consumed.add(result.id);
      out.push({ event: e, result });
      continue;
    }
    out.push({ event: e });
  }
  return out;
}

function findResult(
  events: SessionEvent[],
  fromIdx: number,
  use: Extract<SessionEvent, { role: "assistant"; type: "tool_use" }>,
): SessionEvent | undefined {
  // Id-based match first (parallel-safe).
  if (use.toolCallId) {
    for (let j = fromIdx + 1; j < events.length; j++) {
      const cand = events[j];
      if (cand.role === "tool" && cand.toolCallId === use.toolCallId) {
        return cand;
      }
    }
    // No matching id — fall through to adjacency match. Mocks don't set
    // toolCallId; mid-migration fixtures might mix the two shapes.
  }
  const next = events[fromIdx + 1];
  if (next && next.role === "tool" && next.tool === use.tool) {
    return next;
  }
  return undefined;
}
