import { expect, test } from "bun:test";
import { pairTools } from "./pairTools";
import type { SessionEvent, ToolKind } from "../../types";

function userEvent(id: string, content = "hi"): SessionEvent {
  return { id, t: 0, role: "user", content };
}

function toolUseEvent(
  id: string,
  tool: ToolKind,
  toolCallId?: string,
): SessionEvent {
  return {
    id,
    t: 0,
    role: "assistant",
    type: "tool_use",
    tool,
    toolCallId,
    args: {},
  };
}

function toolResultEvent(
  id: string,
  tool: ToolKind,
  toolCallId?: string,
  result: unknown = {},
): SessionEvent {
  return { id, t: 0, role: "tool", tool, toolCallId, result };
}

test("pairTools pairs adjacent tool_use + tool by tool kind (legacy)", () => {
  const items = pairTools([
    userEvent("u1"),
    toolUseEvent("a1", "read"),
    toolResultEvent("t1", "read"),
  ]);
  expect(items).toHaveLength(2);
  expect(items[0].event.id).toBe("u1");
  expect(items[1].event.id).toBe("a1");
  expect(items[1].result?.id).toBe("t1");
});

test("pairTools pairs by toolCallId across out-of-order results", () => {
  // Two parallel tool calls; results land in reverse order. Adjacency
  // pairing would mis-attach; id pairing must follow the call_id.
  const items = pairTools([
    toolUseEvent("a1", "read", "call_a"),
    toolUseEvent("a2", "grep", "call_b"),
    toolResultEvent("t2", "grep", "call_b"),
    toolResultEvent("t1", "read", "call_a"),
  ]);
  expect(items).toHaveLength(2);
  expect(items[0].event.id).toBe("a1");
  expect(items[0].result?.id).toBe("t1");
  expect(items[1].event.id).toBe("a2");
  expect(items[1].result?.id).toBe("t2");
});

test("pairTools drops standalone tool results with no preceding tool_use", () => {
  const items = pairTools([
    userEvent("u1"),
    toolResultEvent("t1", "read", "call_orphan"),
  ]);
  expect(items).toHaveLength(1);
  expect(items[0].event.id).toBe("u1");
});

test("pairTools leaves tool_use unpaired when no result has landed", () => {
  const items = pairTools([toolUseEvent("a1", "list_dir", "call_a")]);
  expect(items).toHaveLength(1);
  expect(items[0].event.id).toBe("a1");
  expect(items[0].result).toBeUndefined();
});
