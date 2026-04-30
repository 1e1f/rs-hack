//! @yah:ticket(R031-F3, "UI: render ToolCall/ToolResult in ChatPane via existing Message tool_use scaffolding")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R031)
//! @yah:next("Replace the no-op tool_call/tool_result switch cases in useChatSession with SessionEvent emits (assistant/tool_use + tool/<name> roles)")
//! @yah:next("Reuse Message + ToolFrame for read/grep/edit/bash; add bodies for arch_node, arch_neighbors, arch_subgraph, list_dir, read_arch_doc")
//! @yah:next("SessionPane already pairs tool_use with the next role:tool — confirm the pairing key works with our tool_call_id (may need a small tweak)")
//! @arch:see(architecture/agent-tool-calls.md)

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getEnv } from "../../env";
import type {
  SessionId,
  WireApprovalChoice,
  WireRigAgentEvent,
} from "../../env/types";
import type { SessionEvent, ToolKind } from "../../types";

/* Live agent chat session — orchestrates start/send/stop and folds the
   `agent:event` stream back into the existing `SessionEvent` shape so the
   chat pane can reuse Message + UserMsg + AssistantMsg.

   This hook deliberately does NOT subsume the existing mock-driven
   AgentView for relay sessions yet; relay-anchored start lands as a
   follow-up. It's enough to validate the trait shape end-to-end against
   a real provider (Ollama / OpenAI / Claude). */

export type ChatStatus = "idle" | "streaming" | "error";

export interface UseChatSessionApi {
  sessionId: SessionId | null;
  /* Engine label as the daemon reports it back (e.g. "openai:gpt-4o").
     Set by `start` (chat) and `startRelay` (relay-anchored). */
  engine: string | null;
  status: ChatStatus;
  events: SessionEvent[];
  error: string | null;
  start: (rigId: string, engine: string, model?: string) => Promise<void>;
  /* Relay-anchored start. Goes through `agent_start_session` —
     daemon-side prelude::assemble fetches the ticket's annotations
     and builds the prelude; the engine + model returned in the
     start result come from the ticket's `@yah:engine(...)` (or the
     workspace default). The chat-pane header reads `engine` to label
     the session; AgentView keeps `ticketId` on the chat record so a
     re-mount knows which ticket to anchor against. */
  startRelay: (rigId: string, ticketId: string) => Promise<void>;
  send: (text: string) => Promise<void>;
  stop: () => Promise<void>;
  /* Resolve a pending write-tool approval prompt (R031-F5). The chat
     pane's inline `ApprovalRow` calls this with the user's choice; we
     post it through `agent.approval.decide`. The matching
     `approval_resolved` event flips the `SessionEvent` to its resolved
     state — UI doesn't need to update locally first. */
  decideApproval: (
    requestId: string,
    choice: WireApprovalChoice,
  ) => Promise<void>;
}

export function useChatSession(): UseChatSessionApi {
  const [sessionId, setSessionId] = useState<SessionId | null>(null);
  const [engine, setEngine] = useState<string | null>(null);
  const [events, setEvents] = useState<SessionEvent[]>([]);
  const [status, setStatus] = useState<ChatStatus>("idle");
  const [error, setError] = useState<string | null>(null);

  /* Mutable refs so the agent:event listener (registered once on mount)
     sees the freshest values without retriggering useEffect every time
     a turn rolls in. */
  const sessionIdRef = useRef<SessionId | null>(null);
  const rigIdRef = useRef<string | null>(null);
  const streamingEventIdRef = useRef<string | null>(null);
  const eventCounterRef = useRef(0);
  /* Map provider tool_call_id → ToolKind so the matching tool_result can
     adopt the correct visual surface (the wire shape only carries the
     id on the result side). Lives on a ref because it's listener-state
     that mustn't trigger re-renders. */
  const toolByCallIdRef = useRef<Map<string, ToolKind>>(new Map());
  /* Map approval requestId → SessionEvent.id so `approval_resolved` can
     reach back into `events` and flip the row to its resolved state
     without scanning the array on every event. */
  const approvalEventIdsRef = useRef<Map<string, string>>(new Map());

  const nextEventId = useCallback(() => `chat:${eventCounterRef.current++}`, []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const env = await getEnv();
      const off = await env.rpc.agent.onEvent((ev: WireRigAgentEvent) => {
        const current = sessionIdRef.current;
        if (!current || ev.sessionId !== current) return;
        applyEvent(ev);
      });
      if (cancelled) {
        off();
      } else {
        unlisten = off;
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, []);

  function applyEvent(ev: WireRigAgentEvent) {
    switch (ev.kind) {
      case "session_started":
        // The host emits SessionStarted right after start() resolves. We
        // don't surface it as a chat bubble — engine/cache metadata is
        // the SessionHeader's job. Reset error in case a previous turn
        // left one set.
        setError(null);
        return;
      case "turn_started": {
        // Don't open a bubble eagerly — message_delta opens one lazily on
        // first text chunk. A turn that begins with a tool_call (no
        // preamble text) would otherwise leave an empty assistant bubble
        // hanging above the tool surface.
        streamingEventIdRef.current = null;
        setStatus("streaming");
        return;
      }
      case "message_delta": {
        let id = streamingEventIdRef.current;
        if (!id) {
          // First delta of a (sub-)iteration. After a tool_call mid-turn,
          // the runner re-issues the request and resumes streaming — we
          // open a fresh assistant text bubble below the tool surface.
          id = nextEventId();
          streamingEventIdRef.current = id;
          const newId = id;
          setEvents((prev) => [
            ...prev,
            {
              id: newId,
              t: Date.now(),
              role: "assistant",
              type: "text",
              content: ev.text,
            },
          ]);
          return;
        }
        const targetId = id;
        setEvents((prev) =>
          prev.map((e) =>
            e.id === targetId && e.role === "assistant" && e.type === "text"
              ? { ...e, content: e.content + ev.text }
              : e,
          ),
        );
        return;
      }
      case "turn_ended": {
        // The runner's TurnEnded.text mirrors the *last* iteration's
        // assistant text — i.e. the trailing reply after the final tool
        // round-trip. Replace the active streaming bubble (if any) with
        // it as a self-correction in case deltas rounded off mid-utf8.
        // No streaming bubble means the turn ended on a tool_call with
        // no trailing text — leave the prior tool surfaces alone.
        const id = streamingEventIdRef.current;
        if (id && ev.text) {
          setEvents((prev) =>
            prev.map((e) =>
              e.id === id && e.role === "assistant" && e.type === "text"
                ? { ...e, content: ev.text }
                : e,
            ),
          );
        }
        streamingEventIdRef.current = null;
        setStatus("idle");
        return;
      }
      case "turn_failed": {
        // Symmetric to turn_ended: the runner accumulated text up to
        // the failure and flushed it here. Patch the streaming bubble
        // (if any) so the user sees what the model produced before
        // crashing, then surface the failure on the error banner.
        const id = streamingEventIdRef.current;
        if (id && ev.text) {
          setEvents((prev) =>
            prev.map((e) =>
              e.id === id && e.role === "assistant" && e.type === "text"
                ? { ...e, content: ev.text }
                : e,
            ),
          );
        }
        streamingEventIdRef.current = null;
        setStatus("error");
        setError(ev.message);
        console.error("[agent turn failed]", ev.sessionId, ev.message);
        return;
      }
      case "session_ended":
        sessionIdRef.current = null;
        rigIdRef.current = null;
        toolByCallIdRef.current.clear();
        approvalEventIdsRef.current.clear();
        setSessionId(null);
        setStatus("idle");
        return;
      case "error":
        // Synchronous, command-shaped failure (no in-flight turn) —
        // keychain-missing, runner-not-configured, etc. Distinct from
        // turn_failed (which carries partial accumulated text); this
        // path has no bubble to patch.
        streamingEventIdRef.current = null;
        setStatus("error");
        setError(ev.message);
        // Mirror to the WebKit inspector so devs running with
        // Cmd+Option+I open get the same payload they see in the
        // ChatPane error banner — easier to copy-paste into a bug
        // report than scraping the rendered red box.
        console.error("[agent error]", ev.sessionId, ev.message);
        return;
      case "tool_call": {
        // Mid-turn tool call closes the current streaming bubble — the
        // next message_delta opens a fresh bubble below the tool
        // surface (post-tool-result text from the model).
        streamingEventIdRef.current = null;
        const tool = mapWireToolName(ev.toolName);
        toolByCallIdRef.current.set(ev.toolCallId, tool);
        setEvents((prev) => [
          ...prev,
          {
            id: nextEventId(),
            t: Date.now(),
            role: "assistant",
            type: "tool_use",
            tool,
            toolCallId: ev.toolCallId,
            args: normalizeArgs(ev.args),
          },
        ]);
        return;
      }
      case "approval_requested": {
        // Mid-turn write-tool prompt closes the active streaming bubble
        // (the model paused on a tool_use); the inline approval row
        // takes its place. Once the user clicks, the gate either
        // dispatches the tool (Apply / AlwaysAllow → tool_call /
        // tool_result follow) or fails it with `approval_skipped` (Skip
        // → tool_result with ok=false), and the runner re-issues with a
        // fresh streaming bubble below.
        streamingEventIdRef.current = null;
        const eventId = nextEventId();
        approvalEventIdsRef.current.set(ev.requestId, eventId);
        const bash = ev.bash
          ? {
              env: ev.bash.env ?? {},
              cmd: ev.bash.cmd,
              args: ev.bash.args ?? [],
            }
          : undefined;
        setEvents((prev) => [
          ...prev,
          {
            id: eventId,
            t: Date.now(),
            role: "approval",
            requestId: ev.requestId,
            toolName: ev.toolName,
            args: normalizeArgs(ev.args),
            bash,
            status: "pending",
          },
        ]);
        return;
      }
      case "approval_resolved": {
        const targetId = approvalEventIdsRef.current.get(ev.requestId);
        approvalEventIdsRef.current.delete(ev.requestId);
        if (!targetId) return;
        setEvents((prev) =>
          prev.map((e) =>
            e.id === targetId && e.role === "approval"
              ? { ...e, status: "resolved", decision: ev.decision }
              : e,
          ),
        );
        return;
      }
      case "tool_result": {
        // The wire result side carries only `toolCallId, ok, result` —
        // the visual ToolKind comes from the matching tool_call we
        // recorded above. Default to "read" if the call event was
        // somehow missed (e.g. listener attached mid-turn) so the
        // render path doesn't fall off a cliff.
        const tool =
          toolByCallIdRef.current.get(ev.toolCallId) ?? "read";
        setEvents((prev) => [
          ...prev,
          {
            id: nextEventId(),
            t: Date.now(),
            role: "tool",
            tool,
            toolCallId: ev.toolCallId,
            ok: ev.ok,
            result: ev.result,
          },
        ]);
        return;
      }
    }
  }

  const start = useCallback(
    async (rigId: string, engineSpec: string, model?: string) => {
      setError(null);
      setStatus("idle");
      setEvents([]);
      toolByCallIdRef.current.clear();
      approvalEventIdsRef.current.clear();
      streamingEventIdRef.current = null;
      rigIdRef.current = rigId;
      const env = await getEnv();
      try {
        const result = await env.rpc.agent.startChatSession(
          rigId,
          engineSpec,
          model,
        );
        sessionIdRef.current = result.sessionId;
        setSessionId(result.sessionId);
        setEngine(result.engine);
      } catch (e) {
        // The host returns Tauri-side `Err(String)` for bad upstream
        // responses (404 model-not-found, 401 missing-key, etc).
        // Surface it on the same `status === "error"` branch the
        // mid-turn AgentEvent::Error events use, so ChatPane's error
        // banner is the single error rendering point.
        const msg = e instanceof Error ? e.message : String(e);
        setStatus("error");
        setError(msg);
        console.error("[agent start error]", rigId, engineSpec, msg);
      }
    },
    [],
  );

  const startRelay = useCallback(
    async (rigId: string, ticketId: string) => {
      setError(null);
      setStatus("idle");
      setEvents([]);
      toolByCallIdRef.current.clear();
      approvalEventIdsRef.current.clear();
      streamingEventIdRef.current = null;
      rigIdRef.current = rigId;
      const env = await getEnv();
      try {
        const result = await env.rpc.agent.startSession(rigId, ticketId);
        sessionIdRef.current = result.sessionId;
        setSessionId(result.sessionId);
        setEngine(result.engine);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setStatus("error");
        setError(msg);
        console.error("[agent startRelay error]", rigId, ticketId, msg);
      }
    },
    [],
  );

  const send = useCallback(
    async (text: string) => {
      const sid = sessionIdRef.current;
      if (!sid) {
        setError("No active session — start a chat first.");
        return;
      }
      const trimmed = text.trim();
      if (!trimmed) return;
      setEvents((prev) => [
        ...prev,
        {
          id: nextEventId(),
          t: Date.now(),
          role: "user",
          content: trimmed,
        },
      ]);
      const env = await getEnv();
      try {
        await env.rpc.agent.send(sid, trimmed);
      } catch (e) {
        setStatus("error");
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [nextEventId],
  );

  const decideApproval = useCallback(
    async (requestId: string, choice: WireApprovalChoice) => {
      const sid = sessionIdRef.current;
      const rid = rigIdRef.current;
      if (!sid || !rid) return;
      const env = await getEnv();
      try {
        await env.rpc.agent.approval.decide(rid, sid, requestId, choice);
      } catch (e) {
        // Surface on the same banner as other RPC failures. The pending
        // event stays pending — the user can retry by clicking again.
        setStatus("error");
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [],
  );

  const stop = useCallback(async () => {
    const sid = sessionIdRef.current;
    if (!sid) return;
    const env = await getEnv();
    try {
      await env.rpc.agent.stop(sid);
    } catch {
      // best-effort — the renderer-side state still clears.
    }
    sessionIdRef.current = null;
    setSessionId(null);
    setStatus("idle");
  }, []);

  return useMemo(
    () => ({
      sessionId,
      engine,
      status,
      events,
      error,
      start,
      startRelay,
      send,
      stop,
      decideApproval,
    }),
    [
      sessionId,
      engine,
      status,
      events,
      error,
      start,
      startRelay,
      send,
      stop,
      decideApproval,
    ],
  );
}

/* Map a host-registry tool name (read_file, list_dir, arch_node, …) onto
   the renderer's closed `ToolKind` set. Unknown names fall through to
   `read` so the surface still draws something — the model's tool
   descriptions ride in the schema, not in the visual chrome. */
function mapWireToolName(name: string): ToolKind {
  switch (name) {
    case "read_file":
      return "read";
    case "list_dir":
      return "list_dir";
    case "grep":
      return "grep";
    case "arch_node":
      return "arch_node";
    case "arch_neighbors":
      return "arch_neighbors";
    case "arch_subgraph":
      return "arch_subgraph";
    case "arch_lookup":
      return "arch_lookup";
    case "read_arch_doc":
      return "read_arch_doc";
    default:
      return "read";
  }
}

function normalizeArgs(args: unknown): Record<string, any> {
  return args && typeof args === "object" && !Array.isArray(args)
    ? (args as Record<string, any>)
    : {};
}
