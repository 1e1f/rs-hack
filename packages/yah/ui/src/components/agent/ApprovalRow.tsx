//! @yah:ticket(R031-F5, "UI: inline write-tool approval row")
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)

import { useState } from "react";
import { Icon } from "../shared/Glyph";
import type {
  WireApprovalChoice,
  WireApprovalRule,
  WireArgPattern,
} from "../../env/types";
import type { SessionEvent } from "../../types";

type ApprovalEvent = Extract<SessionEvent, { role: "approval" }>;

interface ApprovalRowProps {
  event: ApprovalEvent;
  /* Posts the user's reply through `agent.approval.decide`. The chat
     hook resolves the matching `approval_resolved` back into the
     SessionEvent, so this component never updates `event.status`
     locally — re-render rides on prop change. */
  onDecide: (requestId: string, choice: WireApprovalChoice) => Promise<void>;
}

/* Inline write-tool approval prompt (R031-F5). Three buttons:
   - **Apply** — run this call once, no rule persisted.
   - **Skip** — refuse this call. Surfaces as a `ToolOutcome::fail` so
     the LLM can adjust.
   - **Always allow** — run this call AND persist a rule matching its
     shape. For bash, the rule is a `BashCmdPattern` with `Exact` for
     every arg; for non-bash, a plain `Tool { name }` rule.

   Once the user clicks, we call `onDecide` and dim the row. The
   matching `approval_resolved` event from the gate flips
   `event.status` to `"resolved"` — the row stays on screen with a
   summary of which choice landed, so the chat history stays
   coherent. */
export function ApprovalRow({ event, onDecide }: ApprovalRowProps) {
  const [submitting, setSubmitting] = useState(false);
  const resolved = event.status === "resolved";

  async function decide(choice: WireApprovalChoice) {
    if (submitting || resolved) return;
    setSubmitting(true);
    try {
      await onDecide(event.requestId, choice);
    } finally {
      setSubmitting(false);
    }
  }

  const alwaysAllowRule = ruleFromEvent(event);

  return (
    <div className="flex gap-3">
      <div className="flex w-7 shrink-0 justify-center pt-1.5">
        <span
          className={`h-3.5 w-3.5 rounded-full ${
            resolved ? "bg-ink-3 opacity-50" : "bg-accent candle"
          }`}
          aria-hidden
        />
      </div>
      <div
        className={`min-w-0 flex-1 rounded border bg-vellum/60 ${
          resolved ? "border-rule/40 opacity-70" : "border-accent/45"
        }`}
      >
        <div className="flex items-center gap-2 border-b border-rule/40 px-2.5 py-1.5">
          <Icon name="warning" size={12} className="text-accent" />
          <span className="font-mono text-[12px] font-medium text-ink">
            {resolved ? "approved" : "approval"}
          </span>
          <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-ink-3">
            {event.toolName}
            {event.bash && (
              <>
                {" · "}
                {event.bash.cmd}
              </>
            )}
          </span>
          {resolved && event.decision && (
            <span className="font-mono text-[11px] text-ink-3">
              · {event.decision}
            </span>
          )}
        </div>

        <div className="px-2.5 py-2 font-mono text-[11.5px] text-ink-2">
          {event.bash ? (
            <BashSummary bash={event.bash} />
          ) : (
            <ArgsSummary args={event.args} />
          )}
        </div>

        {!resolved && (
          <div className="flex items-center justify-end gap-1.5 border-t border-rule/40 bg-paper-2/40 px-2.5 py-1.5">
            <button
              type="button"
              onClick={() => void decide({ kind: "skip" })}
              disabled={submitting}
              className="rounded border border-rule/50 bg-paper-2 px-2 py-0.5 text-[11px] text-ink-2 hover:bg-vellum/55 disabled:pointer-events-none disabled:opacity-40"
            >
              Skip
            </button>
            {alwaysAllowRule && (
              <button
                type="button"
                onClick={() =>
                  void decide({ kind: "always-allow", rule: alwaysAllowRule })
                }
                disabled={submitting}
                className="rounded border border-rule/50 bg-paper-2 px-2 py-0.5 text-[11px] text-ink-2 hover:bg-vellum/55 disabled:pointer-events-none disabled:opacity-40"
                title="Run this call and persist a rule matching its shape"
              >
                Always allow
              </button>
            )}
            <button
              type="button"
              onClick={() => void decide({ kind: "apply" })}
              disabled={submitting}
              className="rounded bg-accent px-2 py-0.5 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
            >
              Apply
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function BashSummary({
  bash,
}: {
  bash: { env: Record<string, string>; cmd: string; args: string[] };
}) {
  const envEntries = Object.entries(bash.env);
  return (
    <div className="flex flex-col gap-1">
      {envEntries.length > 0 && (
        <div className="text-ink-3">
          {envEntries
            .map(([k, v]) => `${k}=${quoteIfNeeded(v)}`)
            .join(" ")}
        </div>
      )}
      <div>
        <span className="text-ink">{bash.cmd}</span>
        {bash.args.map((a, i) => (
          <span key={i} className="text-ink-2">
            {" "}
            {quoteIfNeeded(a)}
          </span>
        ))}
      </div>
    </div>
  );
}

function ArgsSummary({ args }: { args: Record<string, any> }) {
  const keys = Object.keys(args);
  if (keys.length === 0) {
    return <span className="text-ink-3 italic">no args</span>;
  }
  return (
    <div className="flex flex-col gap-0.5">
      {keys.map((k) => (
        <div key={k}>
          <span className="text-ink-3">{k}=</span>
          <span className="text-ink-2">{stringify(args[k])}</span>
        </div>
      ))}
    </div>
  );
}

function stringify(v: unknown): string {
  if (typeof v === "string") return v;
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

function quoteIfNeeded(s: string): string {
  return /[\s"'\\$`]/.test(s) ? JSON.stringify(s) : s;
}

/** Build the rule pre-filled into the AlwaysAllow choice. For bash we
 *  emit a `BashCmdPattern` with `Exact` matchers for each arg — the
 *  user can edit later from Settings. For other tools, a plain
 *  `Tool { name }` rule. Returns `null` when we can't safely shape a
 *  rule (e.g. an unknown payload). */
function ruleFromEvent(event: ApprovalEvent): WireApprovalRule | null {
  if (event.bash) {
    const args: WireArgPattern[] = event.bash.args.map((a) => ({
      kind: "exact",
      value: a,
    }));
    return { kind: "bash_cmd_pattern", cmd: event.bash.cmd, args };
  }
  if (event.toolName) {
    return { kind: "tool", name: event.toolName };
  }
  return null;
}
