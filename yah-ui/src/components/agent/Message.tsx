import { AssistantMsg } from "./messages/AssistantMsg";
import { ThinkingMsg } from "./messages/ThinkingMsg";
import { UserMsg } from "./messages/UserMsg";
import { ToolFrame } from "./tools/ToolFrame";
import { ReadResult } from "./tools/ReadResult";
import { GrepResult } from "./tools/GrepResult";
import { EditDiff } from "./tools/EditDiff";
import { BashOutput } from "./tools/BashOutput";
import type { SessionEvent } from "../../types";

interface MessageProps {
  event: SessionEvent;
  /* Paired tool result for tool_use events. SessionPane walks the event list
     and forwards the next role:"tool" frame here so the tool surface renders
     args + result in a single card (T3). Tool result events themselves don't
     render — they're consumed via this prop. */
  result?: SessionEvent;
  onJumpToFile?: (fileColon: string) => void;
}

export function Message({ event, result, onJumpToFile }: MessageProps) {
  if (event.role === "user") {
    return <UserMsg content={event.content} />;
  }

  if (event.role === "assistant") {
    if (event.type === "thinking") {
      return <ThinkingMsg content={event.content} />;
    }
    if (event.type === "text") {
      return (
        <AssistantMsg content={event.content} onJumpToFile={onJumpToFile} />
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
  const paired =
    result && result.role === "tool" && result.tool === use.tool
      ? (result as ToolResult)
      : undefined;
  const args = use.args;

  if (use.tool === "read") {
    return (
      <ToolFrame
        tool="read"
        headline={
          <>
            {args.path}
            {Array.isArray(args.range) && (
              <span className="text-ink-4">
                {" "}
                :{args.range[0]}–{args.range[1]}
              </span>
            )}
          </>
        }
      >
        <ReadResult path={args.path} range={args.range} result={paired?.result} />
      </ToolFrame>
    );
  }

  if (use.tool === "grep") {
    const hits = Array.isArray(paired?.result) ? paired.result : undefined;
    return (
      <ToolFrame
        tool="grep"
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
          result={hits}
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
        headline={args.cmd}
        exit={bashResult?.exit}
      >
        <BashOutput cmd={args.cmd} result={bashResult} />
      </ToolFrame>
    );
  }

  return null;
}
