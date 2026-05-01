import { AssistantMsg } from "./messages/AssistantMsg";
import { ThinkingMsg } from "./messages/ThinkingMsg";
import { UserMsg } from "./messages/UserMsg";
import { ToolFrame } from "./tools/ToolFrame";
import { ReadResult } from "./tools/ReadResult";
import { GrepResult } from "./tools/GrepResult";
import { EditDiff } from "./tools/EditDiff";
import { BashOutput } from "./tools/BashOutput";
import { ListDirResult } from "./tools/ListDirResult";
import { ArchNodeResult } from "./tools/ArchNodeResult";
import { ArchNeighborsResult } from "./tools/ArchNeighborsResult";
import { ArchSubgraphResult } from "./tools/ArchSubgraphResult";
import { ArchLookupResult } from "./tools/ArchLookupResult";
import { ReadArchDocResult } from "./tools/ReadArchDocResult";
import type { SessionEvent } from "../../types";

interface MessageProps {
  event: SessionEvent;
  /* Paired tool result for tool_use events. SessionPane walks the event list
     and forwards the next role:"tool" frame here so the tool surface renders
     args + result in a single card (T3). Tool result events themselves don't
     render — they're consumed via this prop. */
  result?: SessionEvent;
  onJumpToFile?: (fileColon: string) => void;
  /* yah:// link router for `[label](yah://...)` anchors that the agent
     emits in markdown output. App owns the mapping (file/line -> arch
     tab; arch/symbol -> arch graph re-root). */
  onYahLink?: (href: string) => void;
}

export function Message({
  event,
  result,
  onJumpToFile,
  onYahLink,
}: MessageProps) {
  if (event.role === "user") {
    return <UserMsg content={event.content} />;
  }

  if (event.role === "assistant") {
    if (event.type === "thinking") {
      return <ThinkingMsg content={event.content} />;
    }
    if (event.type === "text") {
      return (
        <AssistantMsg
          content={event.content}
          onJumpToFile={onJumpToFile}
          onYahLink={onYahLink}
        />
      );
    }
    if (event.type === "tool_use") {
      return renderToolCall(event, result, onJumpToFile);
    }
  }

  return null;
}

type ToolUse = Extract<SessionEvent, { role: "assistant"; type: "tool_use" }>;
type ToolResult = Extract<SessionEvent, { role: "tool" }>;

function renderToolCall(
  use: ToolUse,
  result: SessionEvent | undefined,
  onJumpToFile?: (fileColon: string) => void,
) {
  /* SessionPane pairs by `toolCallId` when both events carry it (the runner
     mints one per provider tool_call); else it falls back to next-with-
     matching-tool. By the time we get here, `result` is already the right
     pairing — so we only re-check `role` for the type narrow. */
  const paired =
    result && result.role === "tool" ? (result as ToolResult) : undefined;
  const args = use.args ?? {};
  const ok = paired?.ok;
  /* `_smell` is stamped onto every tool result by the host
     (`agent_tools::stamp_smell`) — a one-line summary like
     `read_file path · 4.6KB · ok`. Surface it under the headline so the
     user (and the model when it recounts) has a stable line to quote. */
  const smell =
    paired?.result && typeof paired.result === "object" && !Array.isArray(paired.result)
      ? (paired.result as Record<string, unknown>)._smell as string | undefined
      : undefined;

  if (use.tool === "read") {
    /* read_file (host) maps to the `read` visual surface. The host's range
       is split as `start_line` + `end_line`; legacy mocks still pass a
       tuple `range: [from, to]`. Normalise here so ReadResult only sees
       the tuple form. */
    const range: [number, number] | undefined = Array.isArray(args.range)
      ? (args.range as [number, number])
      : args.start_line != null && args.end_line != null
        ? [args.start_line as number, args.end_line as number]
        : undefined;
    const readBody = paired?.result as
      | { lines?: number; summary?: string; bytes?: number; truncated?: boolean }
      | undefined;
    const summary =
      readBody?.summary ??
      (readBody?.truncated ? "truncated" : undefined);
    return (
      <ToolFrame
        tool="read"
        ok={ok}
        smell={smell}
        headline={
          <>
            {args.path}
            {range && (
              <span className="text-ink-4">
                {" "}
                :{range[0]}–{range[1]}
              </span>
            )}
          </>
        }
      >
        <ReadResult
          path={args.path}
          range={range}
          result={readBody && { lines: readBody.lines, summary }}
        />
      </ToolFrame>
    );
  }

  if (use.tool === "grep") {
    /* The host returns `{ pattern, hits: [...], truncated }`; mocks pass a
       bare array. Accept both. */
    const grepBody = paired?.result as
      | { hits?: unknown[]; truncated?: boolean }
      | unknown[]
      | undefined;
    const hits = Array.isArray(grepBody)
      ? grepBody
      : Array.isArray(grepBody?.hits)
        ? grepBody.hits
        : undefined;
    return (
      <ToolFrame
        tool="grep"
        ok={ok}
        smell={smell}
        headline={
          <>
            <span className="text-ink-4">"</span>
            {args.pattern}
            <span className="text-ink-4">"</span>
            {args.glob && <span className="text-ink-4"> in {args.glob}</span>}
          </>
        }
      >
        <GrepResult
          pattern={args.pattern}
          glob={args.glob}
          result={hits as any}
          onJumpToFile={onJumpToFile}
        />
      </ToolFrame>
    );
  }

  if (use.tool === "edit" || use.tool === "write") {
    const editResult = paired?.result as
      | { additions?: number; deletions?: number }
      | undefined;
    return (
      <ToolFrame
        tool={use.tool}
        ok={ok}
        smell={smell}
        headline={args.path}
        additions={editResult?.additions}
        deletions={editResult?.deletions}
      >
        {typeof args.diff === "string" ? (
          <EditDiff diff={args.diff} />
        ) : (
          <div className="px-3 py-2 font-display text-[12.5px] italic text-ink-3">
            (no diff)
          </div>
        )}
      </ToolFrame>
    );
  }

  if (use.tool === "bash") {
    const bashResult = paired?.result as
      | { stdout?: string; stderr?: string; exit?: number }
      | undefined;
    return (
      <ToolFrame
        tool="bash"
        ok={ok}
        smell={smell}
        headline={args.cmd}
        exit={bashResult?.exit}
      >
        <BashOutput cmd={args.cmd} result={bashResult} />
      </ToolFrame>
    );
  }

  if (use.tool === "list_dir") {
    return (
      <ToolFrame tool="list_dir" ok={ok} smell={smell} headline={args.path || "."}>
        <ListDirResult path={args.path ?? ""} result={paired?.result} />
      </ToolFrame>
    );
  }

  if (use.tool === "arch_node") {
    return (
      <ToolFrame tool="arch_node" ok={ok} smell={smell} headline={args.id}>
        <ArchNodeResult result={paired?.result} onJumpToFile={onJumpToFile} />
      </ToolFrame>
    );
  }

  if (use.tool === "arch_neighbors") {
    return (
      <ToolFrame
        tool="arch_neighbors"
        ok={ok}
        smell={smell}
        headline={
          <>
            {args.id}
            {args.dir && <span className="text-ink-4"> · {args.dir}</span>}
          </>
        }
      >
        <ArchNeighborsResult result={paired?.result} />
      </ToolFrame>
    );
  }

  if (use.tool === "arch_subgraph") {
    return (
      <ToolFrame
        tool="arch_subgraph"
        ok={ok}
        smell={smell}
        headline={
          <>
            {args.root}
            <span className="text-ink-4"> · depth {args.depth ?? 2}</span>
          </>
        }
      >
        <ArchSubgraphResult result={paired?.result} />
      </ToolFrame>
    );
  }

  if (use.tool === "arch_lookup") {
    return (
      <ToolFrame
        tool="arch_lookup"
        ok={ok}
        smell={smell}
        headline={
          <>
            {args.file}
            {args.line != null && (
              <span className="text-ink-4">:{args.line}</span>
            )}
          </>
        }
      >
        <ArchLookupResult result={paired?.result} />
      </ToolFrame>
    );
  }

  if (use.tool === "read_arch_doc") {
    return (
      <ToolFrame tool="read_arch_doc" ok={ok} smell={smell} headline={args.rel_path}>
        <ReadArchDocResult relPath={args.rel_path ?? ""} result={paired?.result} />
      </ToolFrame>
    );
  }

  return null;
}
