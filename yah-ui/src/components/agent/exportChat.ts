import type { SessionEvent } from "../../types";

/* Three transcript export shapes. All operate on the post-processed
   `SessionEvent[]` the chat pane already holds — no extra round-trip
   through the daemon. The link mode just hands back a path; the on-disk
   jsonl is the authoritative copy for downstream tools that need the
   raw stream (deltas, turn_ended usage, etc.) we don't keep client-side. */

export interface ExportContext {
  sessionId: string | null;
  /* Engine label as the daemon reports it back (e.g. `claude:claude-opus-4-7`,
     `openai:qwen3.5`). Used as the model header in manifest mode. */
  engine: string | null;
  /* Wallclock anchor for the session — the first event we have. Manifest
     mode uses it to render relative offsets when no per-event timestamps
     exist. Defaults to `Date.now()` if the array is empty. */
  startedAt?: number;
}

/* Relative path to the on-disk jsonl. The daemon writes
   `<rig>/.yah/sessions/<sessionId>.jsonl` — same convention since R031,
   confirmed against existing files in `.yah/sessions/`. The `session:`
   prefix is part of the id, not a separator. */
export function formatLink(sessionId: string | null): string {
  if (!sessionId) return ".yah/sessions/<no active session>";
  return `.yah/sessions/${sessionId}.jsonl`;
}

export function formatMarkdown(events: SessionEvent[], ctx: ExportContext): string {
  const out: string[] = [];
  out.push(`# Chat transcript`);
  out.push("");
  if (ctx.sessionId) out.push(`- **session**: \`${ctx.sessionId}\``);
  if (ctx.engine) out.push(`- **engine**: \`${ctx.engine}\``);
  out.push(`- **events**: ${events.length}`);
  out.push("");
  out.push("---");
  out.push("");

  let lastRole: "user" | "agent" | null = null;
  for (const ev of events) {
    if (ev.role === "user") {
      if (lastRole !== null) out.push("---");
      out.push(`## You`);
      out.push("");
      out.push(ev.content);
      out.push("");
      lastRole = "user";
      continue;
    }
    if (ev.role === "assistant" && ev.type === "text") {
      if (lastRole !== "agent") {
        if (lastRole !== null) out.push("---");
        out.push(`## Agent${ctx.engine ? ` · ${ctx.engine}` : ""}`);
        out.push("");
      }
      out.push(ev.content);
      out.push("");
      lastRole = "agent";
      continue;
    }
    if (ev.role === "assistant" && ev.type === "thinking") {
      if (lastRole !== "agent") {
        if (lastRole !== null) out.push("---");
        out.push(`## Agent${ctx.engine ? ` · ${ctx.engine}` : ""}`);
        out.push("");
      }
      out.push("<details><summary>thinking</summary>");
      out.push("");
      out.push(ev.content);
      out.push("");
      out.push("</details>");
      out.push("");
      lastRole = "agent";
      continue;
    }
    if (ev.role === "assistant" && ev.type === "tool_use") {
      if (lastRole !== "agent") {
        if (lastRole !== null) out.push("---");
        out.push(`## Agent${ctx.engine ? ` · ${ctx.engine}` : ""}`);
        out.push("");
      }
      const result = findResult(events, ev.id, ev.toolCallId);
      const ok = result?.ok;
      const status = ok === false ? "FAIL" : ok === true ? "ok" : "pending";
      const argLine = stringifyArgs(ev.args);
      const smell = readSmell(result);
      out.push(`<details><summary>🔧 \`${ev.tool}\` · ${status}${smell ? ` — ${smell}` : ""}</summary>`);
      out.push("");
      out.push("```json");
      out.push(`// args`);
      out.push(argLine);
      if (result) {
        out.push(`// result`);
        out.push(JSON.stringify(result.result, null, 2));
      }
      out.push("```");
      out.push("");
      out.push("</details>");
      out.push("");
      lastRole = "agent";
      continue;
    }
    if (ev.role === "approval") {
      if (lastRole !== null) out.push("---");
      out.push(`## Approval prompt — \`${ev.toolName}\` · ${ev.status}${ev.decision ? ` (${ev.decision})` : ""}`);
      out.push("");
      out.push("```json");
      out.push(stringifyArgs(ev.args));
      out.push("```");
      out.push("");
      lastRole = null;
      continue;
    }
  }
  return out.join("\n");
}

export function formatManifest(events: SessionEvent[], ctx: ExportContext): string {
  const anchor = ctx.startedAt ?? events[0]?.t ?? Date.now();
  const out: string[] = [];
  out.push(`# manifest · ${ctx.sessionId ?? "(no session)"} · ${ctx.engine ?? "(no engine)"}`);
  out.push(`# events=${events.length}  anchor=${new Date(anchor).toISOString()}`);
  out.push("");

  let turn = 0;
  let lastToolCounts: Map<string, { ok: number; fail: number }> = new Map();

  const fmtOffset = (t: number) =>
    `+${((t - anchor) / 1000).toFixed(1)}s`.padStart(8);

  for (let i = 0; i < events.length; i++) {
    const ev = events[i];
    if (ev.role === "user") {
      turn += 1;
      lastToolCounts = new Map();
      out.push(
        `T${turn} user   · ${fmtOffset(ev.t)} · ${countTokensApprox(ev.content)} tok~ · ${oneLine(ev.content, 60)}`,
      );
      continue;
    }
    if (ev.role === "assistant" && ev.type === "text") {
      out.push(
        `T${turn} agent  · ${fmtOffset(ev.t)} · ${countTokensApprox(ev.content)} tok~ · ${oneLine(ev.content, 60)}`,
      );
      continue;
    }
    if (ev.role === "assistant" && ev.type === "thinking") {
      out.push(
        `T${turn} think  · ${fmtOffset(ev.t)} · ${countTokensApprox(ev.content)} tok~`,
      );
      continue;
    }
    if (ev.role === "assistant" && ev.type === "tool_use") {
      const result = findResult(events, ev.id, ev.toolCallId);
      const ok = result?.ok;
      const status = ok === false ? "FAIL" : ok === true ? "ok  " : "pend";
      const idShort = (ev.toolCallId ?? "—").slice(0, 12);
      const counts = lastToolCounts.get(ev.tool) ?? { ok: 0, fail: 0 };
      const isRetry =
        (ok === false && counts.fail > 0) ||
        (ok === true && counts.fail > 0);
      if (ok === true) counts.ok += 1;
      else if (ok === false) counts.fail += 1;
      lastToolCounts.set(ev.tool, counts);
      const smell = readSmell(result);
      const retryMark = isRetry ? "  ← retry" : "";
      out.push(
        `     ↳ ${ev.tool.padEnd(15)} ${idShort.padEnd(13)} · ${status} · ${smell || oneLine(stringifyArgs(ev.args), 50)}${retryMark}`,
      );
      continue;
    }
    if (ev.role === "approval") {
      out.push(
        `     ⚠ approval     ${ev.requestId.slice(0, 13).padEnd(13)} · ${ev.status.padEnd(4)} · ${ev.toolName}${ev.decision ? ` → ${ev.decision}` : ""}`,
      );
      continue;
    }
  }

  out.push("");
  out.push("# token counts are heuristic (chars/4) — full usage in jsonl");
  return out.join("\n");
}

function findResult(
  events: SessionEvent[],
  fromId: string,
  toolCallId?: string,
): Extract<SessionEvent, { role: "tool" }> | undefined {
  let started = false;
  for (const ev of events) {
    if (!started) {
      if (ev.id === fromId) started = true;
      continue;
    }
    if (ev.role === "tool") {
      if (toolCallId && ev.toolCallId === toolCallId) return ev;
      if (!toolCallId) return ev;
    }
  }
  return undefined;
}

function readSmell(
  result: Extract<SessionEvent, { role: "tool" }> | undefined,
): string {
  const r = result?.result;
  if (r && typeof r === "object" && !Array.isArray(r)) {
    const s = (r as Record<string, unknown>)._smell;
    return typeof s === "string" ? s : "";
  }
  return "";
}

function stringifyArgs(args: unknown): string {
  try {
    return JSON.stringify(args, null, 2);
  } catch {
    return String(args);
  }
}

function oneLine(s: string, max: number): string {
  const flat = s.replace(/\s+/g, " ").trim();
  return flat.length <= max ? flat : flat.slice(0, max - 1) + "…";
}

/* Heuristic token estimate — 4 chars per token is the rule of thumb the
   prelude assembler uses too. Real per-turn usage rides on
   `turn_ended.usage` in the jsonl. */
function countTokensApprox(s: string): number {
  return Math.ceil(s.length / 4);
}
