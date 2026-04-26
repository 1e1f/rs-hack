import { useState } from "react";
import type { SessionEvent } from "../../types";

type ToolUseEvent = Extract<
  SessionEvent,
  { role: "assistant"; type: "tool_use" }
>;

interface ToolCallProps {
  use: ToolUseEvent;
}

const TOOL_LABEL: Record<string, string> = {
  read: "read",
  edit: "edit",
  bash: "bash",
  grep: "grep",
  write: "write",
};

export function ToolCall({ use }: ToolCallProps) {
  const [open, setOpen] = useState(use.tool === "edit");
  const args = use.args;

  return (
    <div className="rounded border border-border bg-surface/60 font-mono text-[11px]">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2 py-1.5 text-left hover:bg-elevated"
      >
        <span className="text-text-muted">{open ? "▾" : "▸"}</span>
        <span className="rounded bg-border px-1.5 py-0.5 text-[10px] uppercase tracking-wider text-text-dim">
          {TOOL_LABEL[use.tool] ?? use.tool}
        </span>
        <ToolHeadline tool={use.tool} args={args} />
      </button>
      {open && (
        <div className="border-t border-border/60 px-3 py-2">
          <ToolBody tool={use.tool} args={args} />
        </div>
      )}
    </div>
  );
}

function ToolHeadline({
  tool,
  args,
}: {
  tool: string;
  args: Record<string, any>;
}) {
  if (tool === "read") {
    return (
      <span className="truncate text-text-dim">
        {args.path}
        {args.range && (
          <span className="text-text-muted">
            {" "}
            :{args.range[0]}-{args.range[1]}
          </span>
        )}
      </span>
    );
  }
  if (tool === "edit") {
    return <span className="truncate text-text-dim">{args.path}</span>;
  }
  if (tool === "bash") {
    return (
      <span className="truncate text-text-dim">
        <span className="text-green">$</span> {args.cmd}
      </span>
    );
  }
  if (tool === "grep") {
    return (
      <span className="truncate text-text-dim">
        <span className="text-text-muted">/</span>
        {args.pattern}
        <span className="text-text-muted">/ {args.glob}</span>
      </span>
    );
  }
  return (
    <span className="text-text-dim">{JSON.stringify(args).slice(0, 80)}</span>
  );
}

function ToolBody({
  tool,
  args,
}: {
  tool: string;
  args: Record<string, any>;
}) {
  if (tool === "edit" && typeof args.diff === "string") {
    return <DiffBlock diff={args.diff} />;
  }
  if (tool === "bash") {
    return (
      <div>
        <div className="mb-1 text-text-dim">
          <span className="text-green">$</span> {args.cmd}
        </div>
        <pre className="whitespace-pre-wrap text-text-muted">
          (output rendered when result arrives)
        </pre>
      </div>
    );
  }
  if (tool === "grep") {
    return (
      <div className="text-text-dim">
        pattern <span className="text-yellow">{args.pattern}</span> in{" "}
        <span className="text-cyan">{args.glob}</span>
      </div>
    );
  }
  if (tool === "read") {
    return (
      <div className="text-text-dim">
        <FileLink path={args.path} line={args.range?.[0]} />
      </div>
    );
  }
  return <pre className="text-text-muted">{JSON.stringify(args, null, 2)}</pre>;
}

function DiffBlock({ diff }: { diff: string }) {
  const lines = diff.split("\n");
  return (
    <pre className="overflow-x-auto rounded bg-base/60 p-2 text-[10.5px] leading-relaxed">
      {lines.map((line, i) => {
        const tone =
          line.startsWith("+")
            ? "text-green"
            : line.startsWith("-")
            ? "text-red"
            : "text-text-muted";
        return (
          <div key={i} className={tone}>
            {line || "\u00A0"}
          </div>
        );
      })}
    </pre>
  );
}

function FileLink({ path, line }: { path: string; line?: number }) {
  return (
    <button className="font-mono text-cyan hover:underline">
      {path}
      {line != null && `:${line}`}
    </button>
  );
}
