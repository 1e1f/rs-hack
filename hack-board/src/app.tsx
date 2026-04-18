/**
 * hack-board UI
 *
 * Kanban board that displays @hack: tickets from Rust source.
 * Connects to SSE for live updates.
 */

import { useState, useEffect, useCallback, useMemo } from "react";
import { createRoot } from "react-dom/client";

// ── Types ───────────────────────────────────────────────────────────────

interface Ticket {
  id: string;
  title: string;
  item_type: "ticket" | "relay";
  kind?: string; // feature | bug | task | epic
  status: string;
  assignee?: string;
  phase?: string;
  parent?: string;
  severity?: string;
  handoff?: string[];
  next_steps?: string[];
  cleanup?: string[];
  verify?: string[];
  depends_on: string[];
  see_also: string[];
  file: string;
  line: number;
  is_epic?: boolean;
  epic_status?: "active" | "closed";
}

/**
 * Coerce legacy server payloads where `handoff` was a single string into the
 * current `string[]` shape. Lets a newer UI run against an older rs-hack
 * binary during upgrades without throwing `.join is not a function`.
 */
function normalizeTicket(t: any): Ticket {
  if (typeof t.handoff === "string") {
    t.handoff = t.handoff.length > 0 ? [t.handoff] : [];
  } else if (t.handoff == null) {
    t.handoff = [];
  }
  return t as Ticket;
}

interface Summary {
  id: string;
  ticket?: string;
  author?: string;
  timestamp: number;
  text: string;
  file: string;
  promoted: boolean;
  relay_id?: string;
  relay_title?: string;
}

type TodoRefMode = "reference" | "refine" | "implement" | "refactor";

interface TodoRef {
  path: string;
  mode: TodoRefMode;
}

interface Todo {
  id: string;
  text: string;
  kind?: string;
  stage?: string;
  see?: TodoRef[];
}

const KINDS = ["feature", "bug", "task"] as const;
const STAGES = ["fresh", "research", "refine", "split", "ready"] as const;
const REF_MODES: TodoRefMode[] = [
  "reference",
  "refine",
  "implement",
  "refactor",
];

const REF_MODE_COLOR: Record<TodoRefMode, { bg: string; fg: string }> = {
  reference: { bg: "#1e3a4f", fg: "#89dceb" },
  refine: { bg: "#3c3c1e", fg: "#f9e2af" },
  implement: { bg: "#1e3a2f", fg: "#a6e3a1" },
  refactor: { bg: "#45291e", fg: "#fab387" },
};

const KIND_COLOR: Record<string, { bg: string; fg: string }> = {
  feature: { bg: "#1e3a5f", fg: "#89b4fa" },
  bug: { bg: "#45171e", fg: "#f38ba8" },
  task: { bg: "#3c3c1e", fg: "#f9e2af" },
};

const STAGE_COLOR: Record<string, { bg: string; fg: string }> = {
  fresh: { bg: "#45475a", fg: "#cdd6f4" },
  research: { bg: "#1e3a5f", fg: "#89dceb" },
  refine: { bg: "#3c3c1e", fg: "#f9e2af" },
  split: { bg: "#45291e", fg: "#fab387" },
  ready: { bg: "#1e3a2f", fg: "#a6e3a1" },
};

// ── Path helpers ────────────────────────────────────────────────────────

function shortPath(full: string): string {
  const parts = full.split("/").filter(Boolean);
  if (parts.length < 2) return full;
  return parts.slice(-2).join("/");
}

function fileName(full: string): string {
  const parts = full.split("/").filter(Boolean);
  return parts[parts.length - 1] || full;
}

// ── Copy helpers ────────────────────────────────────────────────────────

function CopyBtn({
  text,
  label,
  title,
}: {
  text: string;
  label?: string;
  title?: string;
}) {
  const [state, setState] = useState<"idle" | "copied">("idle");
  return (
    <button
      title={title || `Copy: ${text}`}
      onClick={async (e) => {
        e.stopPropagation();
        try {
          await navigator.clipboard.writeText(text);
          setState("copied");
          setTimeout(() => setState("idle"), 1200);
        } catch {}
      }}
      style={{
        background: state === "copied" ? "#a6e3a1" : "#313244",
        color: state === "copied" ? "#11111b" : "#a6adc8",
        border: "none",
        borderRadius: 3,
        padding: "1px 6px",
        fontSize: 10,
        fontFamily: "monospace",
        cursor: "pointer",
        lineHeight: 1.3,
      }}
    >
      {state === "copied" ? "✓" : label || "copy"}
    </button>
  );
}

function PathRow({ file, line }: { file: string; line?: number }) {
  const short = shortPath(file);
  const full = line != null ? `${file}:${line}` : file;
  const display = line != null ? `${short}:${line}` : short;
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        fontSize: 11,
        color: "#585b70",
        fontFamily: "monospace",
      }}
    >
      <span title={full}>{display}</span>
      <CopyBtn text={full} title={`Copy full path: ${full}`} />
    </span>
  );
}

function SeeRow({ see }: { see: string }) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        fontSize: 11,
        color: "#94e2d5",
        fontFamily: "monospace",
      }}
    >
      <span title={see}>see: {fileName(see)}</span>
      <CopyBtn text={see} title={`Copy full path: ${see}`} />
    </span>
  );
}

// ── ArchiveButton ───────────────────────────────────────────────────────

function ArchiveButton({ ticket }: { ticket: Ticket }) {
  const [confirm, setConfirm] = useState(false);
  const [state, setState] = useState<"idle" | "archived" | "error">("idle");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const verify = ticket.verify || [];

  const btn = (
    <button
      title={
        confirm
          ? "Click again to confirm archive (removes @hack: annotations from source)"
          : "Archive this ticket"
      }
      onClick={async (e) => {
        e.stopPropagation();
        if (!confirm) {
          setConfirm(true);
          setErrorMsg(null);
          setTimeout(() => setConfirm(false), 4000);
          return;
        }
        try {
          const res = await fetch(
            `/api/archive/${encodeURIComponent(ticket.id)}`,
            { method: "POST" }
          );
          if (res.ok) {
            setState("archived");
            setErrorMsg(null);
          } else {
            setState("error");
            const body = await res.json().catch(() => ({}));
            if (res.status === 409 && body.blockingChildren) {
              const names = body.blockingChildren
                .map((c: any) => `${c.id} (${c.status})`)
                .join(", ");
              setErrorMsg(`blocked by: ${names}`);
            } else if (body.error) {
              setErrorMsg(body.error);
            } else {
              setErrorMsg(`HTTP ${res.status}`);
            }
          }
        } catch (err: any) {
          setState("error");
          setErrorMsg(err?.message || "network error");
        }
      }}
      style={{
        background: confirm
          ? "#f38ba8"
          : state === "archived"
            ? "#a6e3a1"
            : state === "error"
              ? "#f38ba8"
              : "#313244",
        color: confirm || state !== "idle" ? "#11111b" : "#a6adc8",
        border: "none",
        borderRadius: 3,
        padding: "1px 6px",
        fontSize: 10,
        fontFamily: "monospace",
        cursor: "pointer",
        lineHeight: 1.3,
      }}
    >
      {state === "archived"
        ? "archived"
        : state === "error"
          ? "error"
          : confirm
            ? verify.length > 0
              ? "confirm?"
              : "confirm archive"
            : "archive"}
    </button>
  );

  const errBox = errorMsg ? (
    <div
      style={{
        fontSize: 10,
        color: "#f38ba8",
        fontFamily: "monospace",
        background: "#45171e",
        padding: "4px 6px",
        borderRadius: 4,
        maxWidth: 260,
        lineHeight: 1.3,
        wordBreak: "break-word",
      }}
      title={errorMsg}
    >
      {errorMsg}
    </div>
  ) : null;

  // When armed and verify commands exist, surface them so the user can
  // run them before committing to archive.
  if (confirm && verify.length > 0) {
    return (
      <span style={{ display: "inline-flex", flexDirection: "column", gap: 4 }}>
        <div
          style={{
            fontSize: 10,
            color: "#f9e2af",
            fontFamily: "monospace",
            background: "#45291e",
            padding: "4px 6px",
            borderRadius: 4,
            maxWidth: 260,
          }}
        >
          <div style={{ marginBottom: 3, color: "#fab387" }}>
            verify before archive:
          </div>
          {verify.map((v, i) => (
            <div
              key={i}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                lineHeight: 1.4,
              }}
            >
              <span style={{ flex: 1, wordBreak: "break-all" }}>• {v}</span>
              <CopyBtn text={v} label="copy" />
            </div>
          ))}
        </div>
        {btn}
        {errBox}
      </span>
    );
  }
  if (errBox) {
    return (
      <span style={{ display: "inline-flex", flexDirection: "column", gap: 4 }}>
        {btn}
        {errBox}
      </span>
    );
  }
  return btn;
}

// ── PromptButton (top-right) / SummaryPromptButton (inline on summary) ──

/**
 * Small top-right button that copies a continuation prompt to the clipboard.
 * Label varies by the ticket's current column:
 *   - open / handoff → "prompt" (pickup prompt from /api/prompt/:id)
 *   - review/done    → "review" (review-mode prompt — same endpoint, server
 *                      branches on status and synthesizes review copy)
 * Not shown for active/in-progress — an agent is already on it.
 *
 * Eventually this becomes an actual agent-harness launch; today it's just a
 * clipboard copy that the user pastes into Claude Code.
 */
function PromptButton({ ticket }: { ticket: Ticket }) {
  const [state, setState] = useState<"idle" | "copied" | "error">("idle");
  const isReview = ticket.status === "review" || ticket.status === "done";
  const label = isReview ? "review" : "prompt";
  return (
    <button
      title={
        isReview
          ? "Copy a review-mode prompt (run verify, approve or send back)"
          : "Copy a continuation prompt (pickup)"
      }
      onClick={async (e) => {
        e.stopPropagation();
        try {
          const res = await fetch(
            `/api/prompt/${encodeURIComponent(ticket.id)}`
          );
          if (!res.ok) {
            setState("error");
            return;
          }
          await navigator.clipboard.writeText(await res.text());
          setState("copied");
          setTimeout(() => setState("idle"), 2000);
        } catch {
          setState("error");
        }
      }}
      style={{
        background:
          state === "copied"
            ? "#a6e3a1"
            : state === "error"
              ? "#f38ba8"
              : "#cba6f7",
        color: "#11111b",
        border: "none",
        borderRadius: 3,
        padding: "1px 6px",
        fontSize: 10,
        fontFamily: "monospace",
        fontWeight: 600,
        cursor: "pointer",
        lineHeight: 1.3,
      }}
    >
      {state === "copied" ? "✓" : state === "error" ? "error" : label}
    </button>
  );
}

/**
 * Per-summary "prompt from this comment" button. Uses the summary text
 * (not the ticket metadata), which is a distinct fork point — useful when
 * an agent left a mid-session note you want to continue *from that note*
 * rather than from the ticket's current handoff.
 */
function SummaryPromptButton({
  summaryId,
}: {
  summaryId: string;
}) {
  const [state, setState] = useState<"idle" | "copied">("idle");
  return (
    <button
      onClick={async (e) => {
        e.stopPropagation();
        try {
          const res = await fetch("/api/summaries");
          const summaries = await res.json();
          const s = summaries.find((x: any) => x.id === summaryId);
          if (s) {
            const prompt = [
              `# Fork from: ${s.ticket || summaryId}`,
              s.author ? `\nFrom: ${s.author}` : "",
              `\n---\n\n${s.text}`,
            ].join("");
            await navigator.clipboard.writeText(prompt);
          }
          setState("copied");
          setTimeout(() => setState("idle"), 2000);
        } catch (e) {
          console.error("Prompt-from-summary failed:", e);
        }
      }}
      style={{
        marginTop: 6,
        width: "100%",
        padding: "5px 10px",
        background: state === "copied" ? "#a6e3a1" : "#cba6f7",
        color: "#11111b",
        border: "none",
        borderRadius: 6,
        fontSize: 11,
        fontWeight: 600,
        cursor: "pointer",
      }}
    >
      {state === "copied" ? "Copied!" : "prompt from this comment"}
    </button>
  );
}

// ── Columns (claimed + in-progress collapsed into "Active") ─────────────

interface ColumnDef {
  key: string;
  label: string;
  color: string;
  statuses: string[];
}

const COLUMNS: ColumnDef[] = [
  {
    key: "active",
    label: "Active",
    color: "#3b82f6",
    statuses: ["claimed", "in-progress"],
  },
  { key: "handoff", label: "Handoff", color: "#8b5cf6", statuses: ["handoff"] },
  {
    key: "review",
    label: "Review",
    color: "#06b6d4",
    // 'done' lingers here as a terminal state until archived
    statuses: ["review", "done"],
  },
];

// Color lookup for a ticket's current column — used by epic child dots so the
// visual maps directly to what column the child is in.
const OPEN_COLOR = "#f9e2af";
function columnColorForStatus(status: string): string {
  for (const col of COLUMNS) {
    if (col.statuses.includes(status)) return col.color;
  }
  if (status === "open") return OPEN_COLOR;
  return "#585b70";
}

const EPIC_STATUS_COLOR: Record<string, { bg: string; fg: string }> = {
  active: { bg: "#45291e", fg: "#fab387" },
  closed: { bg: "#1e3a2f", fg: "#a6e3a1" },
};

// Mirrors the server-side transition matrix. Kept here so the UI can paint
// valid drop targets during a drag without a round-trip.
const STATUS_TO_BUCKET: Record<string, string> = {
  open: "open",
  claimed: "active",
  "in-progress": "active",
  handoff: "handoff",
  review: "review",
  done: "review",
};
const TRANSITIONS: Record<string, string[]> = {
  open: ["active"],
  active: ["open", "handoff", "review"],
  handoff: ["active", "review"],
  review: ["handoff"],
};
function canTransition(from: string, toBucket: string): boolean {
  const fromBucket = STATUS_TO_BUCKET[from] || from;
  return TRANSITIONS[fromBucket]?.includes(toBucket) ?? false;
}
const DRAG_MIME = "application/x-hackboard-ticket";

// ── TicketCard ──────────────────────────────────────────────────────────

function TicketCard({
  ticket,
  summaries,
  onDragStart,
  onDragEnd,
}: {
  ticket: Ticket;
  summaries: Summary[];
  onDragStart?: (status: string) => void;
  onDragEnd?: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const badge =
    ticket.item_type === "relay"
      ? "R"
      : ticket.kind === "bug"
        ? "B"
        : ticket.kind === "feature"
          ? "F"
          : "T";
  const badgeColor =
    ticket.item_type === "relay"
      ? "#cba6f7"
      : ticket.kind === "bug"
        ? "#ef4444"
        : ticket.kind === "feature"
          ? "#3b82f6"
          : "#f9e2af";

  // Count hidden content so the toggle can tell the user what's folded.
  const hiddenCounts: string[] = [];
  if (ticket.next_steps && ticket.next_steps.length > 0) {
    hiddenCounts.push(`${ticket.next_steps.length} next`);
  }
  if (ticket.verify && ticket.verify.length > 0) {
    hiddenCounts.push(`${ticket.verify.length} verify`);
  }
  if (ticket.cleanup && ticket.cleanup.length > 0) {
    hiddenCounts.push(`${ticket.cleanup.length} cleanup`);
  }
  if (ticket.see_also.length > 0) {
    hiddenCounts.push(`${ticket.see_also.length} ref${ticket.see_also.length > 1 ? "s" : ""}`);
  }
  if (summaries.length > 0) {
    hiddenCounts.push(
      `${summaries.length} summar${summaries.length > 1 ? "ies" : "y"}`
    );
  }
  // Handoff is a list; the card only collapses it when the *joined* body is
  // long enough to need truncating, or when there are multiple bullets
  // (which always benefit from expand-to-read).
  const handoffJoined =
    ticket.handoff && ticket.handoff.length > 0
      ? ticket.handoff.join("\n\n")
      : "";
  const handoffLong =
    (ticket.handoff?.length ?? 0) > 1 || handoffJoined.length > 80;
  const hasMore = hiddenCounts.length > 0 || handoffLong;

  return (
    <div
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData(DRAG_MIME, ticket.id);
        e.dataTransfer.setData("text/plain", ticket.id);
        e.dataTransfer.effectAllowed = "move";
        onDragStart?.(ticket.status);
      }}
      onDragEnd={() => onDragEnd?.()}
      style={{
        background: "#1e1e2e",
        border: "1px solid #313244",
        borderRadius: 8,
        padding: "10px 12px",
        marginBottom: 8,
        cursor: "grab",
        transition: "border-color 0.15s, opacity 0.15s",
      }}
      onMouseEnter={(e) => (e.currentTarget.style.borderColor = "#585b70")}
      onMouseLeave={(e) => (e.currentTarget.style.borderColor = "#313244")}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            background: badgeColor,
            color: "#fff",
            borderRadius: 4,
            padding: "1px 6px",
            fontSize: 11,
            fontWeight: 700,
            fontFamily: "monospace",
          }}
        >
          {badge}
        </span>
        <TicketIdBadge id={ticket.id} />
        {hasMore && (
          <button
            onClick={() => setExpanded((x) => !x)}
            title={expanded ? "Collapse" : "Expand"}
            style={{
              background: "transparent",
              color: "#585b70",
              border: "none",
              cursor: "pointer",
              fontSize: 11,
              fontFamily: "monospace",
              padding: 0,
              marginLeft: 2,
            }}
          >
            {expanded ? "▾" : "▸"}
          </button>
        )}
        <span
          style={{
            marginLeft: "auto",
            display: "inline-flex",
            gap: 4,
            alignItems: "center",
          }}
        >
          {(ticket.status === "open" ||
            ticket.status === "handoff" ||
            ticket.status === "review" ||
            ticket.status === "done") && (
            <PromptButton ticket={ticket} />
          )}
          <ArchiveButton ticket={ticket} />
        </span>
      </div>

      <div
        style={{
          color: "#cdd6f4",
          fontSize: 13,
          fontWeight: 500,
          marginBottom: 8,
          lineHeight: 1.4,
        }}
      >
        {ticket.title}
      </div>

      {/* Metadata pills */}
      <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
        {ticket.assignee && (
          <Pill label={ticket.assignee} color="#45475a" textColor="#b4befe" />
        )}
        {ticket.phase && (
          <Pill label={ticket.phase} color="#45475a" textColor="#f9e2af" />
        )}
        {ticket.parent && (
          <Pill label={ticket.parent} color="#1e3a2f" textColor="#a6e3a1" />
        )}
        {ticket.severity && (
          <Pill
            label={ticket.severity}
            color={
              ticket.severity === "critical"
                ? "#45171e"
                : ticket.severity === "high"
                  ? "#45291e"
                  : "#45475a"
            }
            textColor={
              ticket.severity === "critical"
                ? "#f38ba8"
                : ticket.severity === "high"
                  ? "#fab387"
                  : "#a6adc8"
            }
          />
        )}
      </div>

      {/* Handoff — compact: show just first line truncated; expanded:
          single entry as paragraph, multi as bullets. */}
      {ticket.handoff && ticket.handoff.length > 0 && (
        <div
          style={{
            marginTop: 8,
            padding: "6px 8px",
            background: "#313244",
            borderRadius: 4,
            borderLeft: "3px solid #cba6f7",
            fontSize: 12,
            color: "#bac2de",
            lineHeight: 1.4,
            ...(expanded
              ? {}
              : {
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }),
          }}
          title={!expanded ? handoffJoined : undefined}
        >
          {expanded ? (
            ticket.handoff.length === 1 ? (
              <div style={{ whiteSpace: "pre-wrap" }}>{ticket.handoff[0]}</div>
            ) : (
              <ul
                style={{
                  margin: 0,
                  paddingLeft: 18,
                  display: "flex",
                  flexDirection: "column",
                  gap: 4,
                }}
              >
                {ticket.handoff.map((h, i) => (
                  <li key={i} style={{ whiteSpace: "pre-wrap" }}>
                    {h}
                  </li>
                ))}
              </ul>
            )
          ) : (
            // Compact: show just the first bullet (truncated by the parent
            // container's ellipsis styling). If there's more than one, append
            // a subtle "+N" suffix so the reader knows to expand.
            <>
              {ticket.handoff[0]}
              {ticket.handoff.length > 1 && (
                <span style={{ color: "#585b70" }}>
                  {" "}
                  +{ticket.handoff.length - 1}
                </span>
              )}
            </>
          )}
        </div>
      )}

      {/* depends_on — always visible; short and important */}
      {ticket.depends_on.length > 0 && (
        <div
          style={{
            marginTop: 8,
            display: "flex",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 4,
            fontSize: 11,
          }}
        >
          <span style={{ color: "#585b70" }}>needs:</span>
          {ticket.depends_on.map((dep) => (
            <Pill
              key={dep}
              label={dep}
              color="#45171e"
              textColor="#f38ba8"
            />
          ))}
        </div>
      )}

      {/* Expanded-only sections */}
      {expanded && ticket.next_steps && ticket.next_steps.length > 0 && (
        <div style={{ marginTop: 8, fontSize: 12, color: "#a6adc8" }}>
          <div style={{ color: "#585b70", marginBottom: 2 }}>Next:</div>
          {ticket.next_steps.map((step: string, i: number) => (
            <div key={i} style={{ paddingLeft: 8, lineHeight: 1.4 }}>
              - {step}
            </div>
          ))}
        </div>
      )}

      {expanded && ticket.verify && ticket.verify.length > 0 && (
        <div style={{ marginTop: 8, fontSize: 12, color: "#a6adc8" }}>
          <div style={{ color: "#585b70", marginBottom: 2 }}>Verify:</div>
          {ticket.verify.map((v: string, i: number) => (
            <div
              key={i}
              style={{
                paddingLeft: 8,
                lineHeight: 1.4,
                fontFamily: "monospace",
              }}
            >
              - {v}
            </div>
          ))}
        </div>
      )}

      {expanded && ticket.cleanup && ticket.cleanup.length > 0 && (
        <div style={{ marginTop: 8, fontSize: 12, color: "#a6adc8" }}>
          <div style={{ color: "#585b70", marginBottom: 2 }}>Cleanup:</div>
          {ticket.cleanup.map((c: string, i: number) => (
            <div key={i} style={{ paddingLeft: 8, lineHeight: 1.4 }}>
              - {c}
            </div>
          ))}
        </div>
      )}

      {expanded && ticket.see_also.length > 0 && (
        <div
          style={{
            marginTop: 8,
            display: "flex",
            flexDirection: "column",
            gap: 3,
          }}
        >
          {ticket.see_also.map((s, i) => (
            <SeeRow key={i} see={s} />
          ))}
        </div>
      )}

      {expanded &&
        summaries.length > 0 &&
        (() => {
          const latestUnpromoted = summaries.find((s) => !s.promoted);
          return (
            <div style={{ marginTop: 8 }}>
              <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
                {summaries.length} summar{summaries.length > 1 ? "ies" : "y"}
              </div>
              {summaries.map((s) => (
                <div
                  key={s.id}
                  style={{
                    padding: "6px 8px",
                    background: "#313244",
                    borderRadius: 4,
                    borderLeft: `3px solid ${s.promoted ? "#45475a" : "#89b4fa"}`,
                    fontSize: 12,
                    color: s.promoted ? "#585b70" : "#bac2de",
                    lineHeight: 1.4,
                    marginBottom: 4,
                    whiteSpace: "pre-wrap",
                  }}
                >
                  {s.author && (
                    <div
                      style={{
                        fontSize: 10,
                        color: "#585b70",
                        marginBottom: 2,
                      }}
                    >
                      {s.author}
                    </div>
                  )}
                  {s.text.length > 200 ? s.text.slice(0, 200) + "..." : s.text}
                  {s === latestUnpromoted && (
                    <SummaryPromptButton summaryId={s.id} />
                  )}
                </div>
              ))}
            </div>
          );
        })()}

      {/* Collapsed summary / expand affordance */}
      {!expanded && hiddenCounts.length > 0 && (
        <button
          onClick={() => setExpanded(true)}
          style={{
            marginTop: 8,
            background: "transparent",
            color: "#585b70",
            border: "none",
            padding: 0,
            fontSize: 11,
            fontFamily: "monospace",
            cursor: "pointer",
            textAlign: "left",
          }}
        >
          + {hiddenCounts.join(" · ")}
        </button>
      )}

      {/* Source path (short + copy full) */}
      <div style={{ marginTop: 8 }}>
        <PathRow file={ticket.file} line={ticket.line} />
      </div>
    </div>
  );
}

// ── EpicCard ────────────────────────────────────────────────────────────

function EpicCard({
  epic,
  childRelays,
}: {
  epic: Ticket;
  childRelays: Ticket[];
}) {
  const [expanded, setExpanded] = useState(false);
  const status = epic.epic_status || "active";
  const color = EPIC_STATUS_COLOR[status];
  return (
    <div
      style={{
        background: "#1e1e2e",
        border: `1px solid ${color.bg}`,
        borderRadius: 8,
        padding: "10px 12px",
        marginBottom: 8,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            background: "#cba6f7",
            color: "#11111b",
            borderRadius: 4,
            padding: "1px 6px",
            fontSize: 11,
            fontWeight: 700,
            fontFamily: "monospace",
          }}
          title="Epic"
        >
          E
        </span>
        <span
          style={{ color: "#a6adc8", fontSize: 12, fontFamily: "monospace" }}
        >
          {epic.id}
        </span>
        <Pill label={status} color={color.bg} textColor={color.fg} />
        <span style={{ marginLeft: "auto" }}>
          <ArchiveButton ticket={epic} />
        </span>
      </div>
      <div
        style={{
          color: "#cdd6f4",
          fontSize: 13,
          fontWeight: 500,
          marginBottom: 8,
          lineHeight: 1.4,
        }}
      >
        {epic.title}
      </div>

      {childRelays.length > 0 ? (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 3,
            marginBottom: 8,
          }}
        >
          <div style={{ fontSize: 11, color: "#585b70", marginBottom: 2 }}>
            {childRelays.length} child relay
            {childRelays.length > 1 ? "s" : ""}
          </div>
          {childRelays.map((c) => (
            <div
              key={c.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                lineHeight: 1.35,
              }}
              title={`${c.id} (${c.status}): ${c.title}`}
            >
              <span
                style={{
                  width: 8,
                  height: 8,
                  borderRadius: "50%",
                  background: columnColorForStatus(c.status),
                  flexShrink: 0,
                }}
              />
              <span
                style={{
                  color: "#a6adc8",
                  fontFamily: "monospace",
                  fontSize: 11,
                }}
              >
                {c.id}
              </span>
              <span
                style={{
                  color: "#cdd6f4",
                  flex: 1,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.title}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <div
          style={{
            fontSize: 11,
            color: "#585b70",
            marginBottom: 8,
            fontStyle: "italic",
          }}
        >
          no child relays yet — planning
        </div>
      )}

      {expanded && epic.handoff && epic.handoff.length > 0 && (
        <div
          style={{
            padding: "6px 8px",
            background: "#313244",
            borderRadius: 4,
            borderLeft: "3px solid #cba6f7",
            fontSize: 12,
            color: "#bac2de",
            lineHeight: 1.4,
            marginBottom: 8,
          }}
        >
          {epic.handoff.length === 1 ? (
            <div style={{ whiteSpace: "pre-wrap" }}>{epic.handoff[0]}</div>
          ) : (
            <ul
              style={{
                margin: 0,
                paddingLeft: 18,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              {epic.handoff.map((h, i) => (
                <li key={i} style={{ whiteSpace: "pre-wrap" }}>
                  {h}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {epic.handoff && epic.handoff.length > 0 && (
        <button
          onClick={() => setExpanded((x) => !x)}
          style={{
            marginTop: 0,
            marginBottom: 8,
            background: "transparent",
            color: "#585b70",
            border: "none",
            padding: 0,
            fontSize: 11,
            fontFamily: "monospace",
            cursor: "pointer",
          }}
        >
          {expanded ? "▾ hide description" : "▸ show description"}
        </button>
      )}

      <PathRow file={epic.file} line={epic.line} />
    </div>
  );
}

function EpicsColumn({
  epics,
  allTickets,
}: {
  epics: Ticket[];
  allTickets: Ticket[];
}) {
  return (
    <div style={{ flex: "1 1 0", minWidth: 260, maxWidth: 360 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 12,
          padding: "0 4px",
        }}
      >
        <div
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: "#cba6f7",
          }}
        />
        <span style={{ color: "#cdd6f4", fontSize: 14, fontWeight: 600 }}>
          Epics
        </span>
        <span style={{ color: "#585b70", fontSize: 12, marginLeft: "auto" }}>
          {epics.length}
        </span>
      </div>
      <div
        style={{
          background: "#181825",
          borderRadius: 8,
          padding: 8,
          minHeight: 100,
        }}
      >
        {epics.map((e) => (
          <EpicCard
            key={e.id}
            epic={e}
            childRelays={allTickets.filter((t) => t.parent === e.id)}
          />
        ))}
        {epics.length === 0 && (
          <div
            style={{
              color: "#45475a",
              fontSize: 12,
              textAlign: "center",
              padding: 20,
            }}
          >
            No epics
          </div>
        )}
      </div>
    </div>
  );
}

// ── TodoCard ────────────────────────────────────────────────────────────

function TodoCard({ todo }: { todo: Todo }) {
  const [promptState, setPromptState] = useState<"idle" | "copied">("idle");
  const [confirmDel, setConfirmDel] = useState(false);
  const kindColor = todo.kind && KIND_COLOR[todo.kind];
  const stageColor = todo.stage && STAGE_COLOR[todo.stage];

  return (
    <div
      style={{
        background: "#1e1e2e",
        border: "1px solid #313244",
        borderRadius: 8,
        padding: "12px 14px",
        marginBottom: 8,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 6,
        }}
      >
        <span
          style={{
            background: kindColor ? kindColor.bg : "#f9e2af",
            color: kindColor ? kindColor.fg : "#11111b",
            borderRadius: 4,
            padding: "1px 6px",
            fontSize: 11,
            fontWeight: 700,
            fontFamily: "monospace",
          }}
        >
          {todo.kind
            ? todo.kind === "bug"
              ? "B"
              : todo.kind === "feature"
                ? "F"
                : "T"
            : "?"}
        </span>
        <span
          style={{ color: "#a6adc8", fontSize: 12, fontFamily: "monospace" }}
        >
          {todo.id}
        </span>
        <button
          title={confirmDel ? "Click again to confirm delete" : "Delete todo"}
          onClick={async () => {
            if (!confirmDel) {
              setConfirmDel(true);
              setTimeout(() => setConfirmDel(false), 2000);
              return;
            }
            await fetch(`/api/todos/${encodeURIComponent(todo.id)}`, {
              method: "DELETE",
            });
          }}
          style={{
            marginLeft: "auto",
            background: confirmDel ? "#f38ba8" : "transparent",
            color: confirmDel ? "#11111b" : "#585b70",
            border: "none",
            cursor: "pointer",
            fontSize: 12,
            padding: "0 4px",
          }}
        >
          {confirmDel ? "confirm" : "×"}
        </button>
      </div>

      {/* Tag row */}
      {(todo.kind || todo.stage) && (
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            gap: 4,
            marginBottom: 6,
          }}
        >
          {todo.kind && kindColor && (
            <Pill
              label={todo.kind}
              color={kindColor.bg}
              textColor={kindColor.fg}
            />
          )}
          {todo.stage && stageColor && (
            <Pill
              label={todo.stage}
              color={stageColor.bg}
              textColor={stageColor.fg}
            />
          )}
        </div>
      )}

      <div
        style={{
          color: "#cdd6f4",
          fontSize: 13,
          lineHeight: 1.4,
          whiteSpace: "pre-wrap",
          marginBottom: 8,
        }}
      >
        {todo.text}
      </div>

      {todo.see && todo.see.length > 0 && (
        <div
          style={{
            marginBottom: 8,
            display: "flex",
            flexDirection: "column",
            gap: 4,
          }}
        >
          {todo.see.map((r, i) => {
            const color = REF_MODE_COLOR[r.mode];
            return (
              <div
                key={i}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 11,
                  fontFamily: "monospace",
                }}
              >
                <span
                  style={{
                    background: color.bg,
                    color: color.fg,
                    borderRadius: 3,
                    padding: "1px 5px",
                    fontSize: 10,
                    fontWeight: 600,
                  }}
                  title={`mode: ${r.mode}`}
                >
                  {r.mode}
                </span>
                <span
                  style={{
                    flex: 1,
                    color: "#94e2d5",
                    wordBreak: "break-all",
                  }}
                  title={r.path}
                >
                  {fileName(r.path)}
                </span>
                <CopyBtn text={r.path} title={`Copy full path: ${r.path}`} />
              </div>
            );
          })}
        </div>
      )}

      <button
        onClick={async () => {
          try {
            const res = await fetch(
              `/api/todo-prompt/${encodeURIComponent(todo.id)}`
            );
            if (res.ok) {
              await navigator.clipboard.writeText(await res.text());
              setPromptState("copied");
              setTimeout(() => setPromptState("idle"), 2000);
            }
          } catch (e) {
            console.error("Copy prompt failed:", e);
          }
        }}
        style={{
          width: "100%",
          padding: "5px 10px",
          background: promptState === "copied" ? "#a6e3a1" : "#89b4fa",
          color: "#11111b",
          border: "none",
          borderRadius: 6,
          fontSize: 11,
          fontWeight: 600,
          cursor: "pointer",
        }}
      >
        {promptState === "copied" ? "Copied!" : "Copy Prompt"}
      </button>
    </div>
  );
}

// ── Pill ────────────────────────────────────────────────────────────────

/**
 * Ticket ID badge. A bare ID (`T01`, `R007`) renders as a single monospace
 * pill. A compound ID (`R007-T1`) renders as two pills glued together:
 * the parent relay in its own muted lilac, then the sub-ticket segment
 * in cool blue. No space between — they read as one atom but the eye
 * parses the hierarchy at a glance.
 */
function TicketIdBadge({ id }: { id: string }) {
  const dashIdx = id.indexOf("-");
  if (dashIdx < 0) {
    return (
      <span
        style={{ color: "#a6adc8", fontSize: 12, fontFamily: "monospace" }}
      >
        {id}
      </span>
    );
  }
  const relay = id.slice(0, dashIdx);
  const sub = id.slice(dashIdx + 1);
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontSize: 11,
        fontFamily: "monospace",
        borderRadius: 3,
        overflow: "hidden",
      }}
      title={id}
    >
      <span
        style={{
          background: "#3c3c1e",
          color: "#cba6f7",
          padding: "1px 5px",
        }}
      >
        {relay}
      </span>
      <span
        style={{
          background: "#1e3a5f",
          color: "#89b4fa",
          padding: "1px 5px",
        }}
      >
        {sub}
      </span>
    </span>
  );
}

function Pill({
  label,
  color,
  textColor,
}: {
  label: string;
  color: string;
  textColor: string;
}) {
  return (
    <span
      style={{
        background: color,
        color: textColor,
        borderRadius: 4,
        padding: "1px 6px",
        fontSize: 11,
        fontFamily: "monospace",
      }}
    >
      {label}
    </span>
  );
}

// ── Column Component ────────────────────────────────────────────────────

function Column({
  label,
  color,
  bucket,
  tickets,
  summaries,
  draggedStatus,
  onDrop,
  onTicketDragStart,
  onTicketDragEnd,
}: {
  label: string;
  color: string;
  bucket: string;
  tickets: Ticket[];
  summaries: Summary[];
  draggedStatus: string | null;
  onDrop: (id: string, bucket: string) => void;
  onTicketDragStart: (status: string) => void;
  onTicketDragEnd: () => void;
}) {
  const dragging = draggedStatus !== null;
  const valid = dragging && canTransition(draggedStatus!, bucket);
  return (
    <div style={{ flex: "1 1 0", minWidth: 240, maxWidth: 340 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 12,
          padding: "0 4px",
        }}
      >
        <div
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: color,
          }}
        />
        <span style={{ color: "#cdd6f4", fontSize: 14, fontWeight: 600 }}>
          {label}
        </span>
        <span style={{ color: "#585b70", fontSize: 12, marginLeft: "auto" }}>
          {tickets.length}
        </span>
      </div>

      <div
        onDragOver={(e) => {
          if (valid) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
          }
        }}
        onDrop={(e) => {
          if (!valid) return;
          e.preventDefault();
          const id =
            e.dataTransfer.getData(DRAG_MIME) ||
            e.dataTransfer.getData("text/plain");
          if (id) onDrop(id, bucket);
        }}
        style={{
          background: "#181825",
          borderRadius: 8,
          padding: 8,
          minHeight: 100,
          outline: valid
            ? `2px dashed ${color}`
            : "2px dashed transparent",
          outlineOffset: valid ? -2 : 0,
          opacity: dragging && !valid ? 0.45 : 1,
          transition: "opacity 0.12s, outline-color 0.12s",
        }}
      >
        {tickets.map((t) => (
          <TicketCard
            key={t.id}
            ticket={t}
            summaries={summaries.filter((s) => s.ticket === t.id)}
            onDragStart={onTicketDragStart}
            onDragEnd={onTicketDragEnd}
          />
        ))}
        {tickets.length === 0 && (
          <div
            style={{
              color: "#45475a",
              fontSize: 12,
              textAlign: "center",
              padding: 20,
            }}
          >
            No tickets
          </div>
        )}
      </div>
    </div>
  );
}

// ── TodoColumn ──────────────────────────────────────────────────────────

function TodoColumn({
  todos,
  openTickets,
  summaries,
  onAdd,
  draggedStatus,
  onDrop,
  onTicketDragStart,
  onTicketDragEnd,
}: {
  todos: Todo[];
  openTickets: Ticket[];
  summaries: Summary[];
  onAdd: () => void;
  draggedStatus: string | null;
  onDrop: (id: string, bucket: string) => void;
  onTicketDragStart: (status: string) => void;
  onTicketDragEnd: () => void;
}) {
  const total = todos.length + openTickets.length;
  const dragging = draggedStatus !== null;
  const valid = dragging && canTransition(draggedStatus!, "open");
  return (
    <div style={{ flex: "1 1 0", minWidth: 240, maxWidth: 340 }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 12,
          padding: "0 4px",
        }}
      >
        <div
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: "#f9e2af",
          }}
        />
        <span style={{ color: "#cdd6f4", fontSize: 14, fontWeight: 600 }}>
          Open
        </span>
        <span style={{ color: "#585b70", fontSize: 12 }}>{total}</span>
        <button
          onClick={onAdd}
          title="Add todo"
          style={{
            marginLeft: "auto",
            background: "#f9e2af",
            color: "#11111b",
            border: "none",
            borderRadius: 4,
            padding: "2px 8px",
            fontSize: 12,
            fontWeight: 700,
            cursor: "pointer",
          }}
        >
          + Todo
        </button>
      </div>

      <div
        onDragOver={(e) => {
          if (valid) {
            e.preventDefault();
            e.dataTransfer.dropEffect = "move";
          }
        }}
        onDrop={(e) => {
          if (!valid) return;
          e.preventDefault();
          const id =
            e.dataTransfer.getData(DRAG_MIME) ||
            e.dataTransfer.getData("text/plain");
          if (id) onDrop(id, "open");
        }}
        style={{
          background: "#181825",
          borderRadius: 8,
          padding: 8,
          minHeight: 100,
          outline: valid ? "2px dashed #f9e2af" : "2px dashed transparent",
          outlineOffset: valid ? -2 : 0,
          opacity: dragging && !valid ? 0.45 : 1,
          transition: "opacity 0.12s, outline-color 0.12s",
        }}
      >
        {openTickets.map((t) => (
          <TicketCard
            key={t.id}
            ticket={t}
            summaries={summaries.filter((s) => s.ticket === t.id)}
            onDragStart={onTicketDragStart}
            onDragEnd={onTicketDragEnd}
          />
        ))}
        {todos.map((t) => (
          <TodoCard key={t.id} todo={t} />
        ))}
        {total === 0 && (
          <div
            style={{
              color: "#45475a",
              fontSize: 12,
              textAlign: "center",
              padding: 20,
            }}
          >
            Empty
          </div>
        )}
      </div>
    </div>
  );
}

// ── Board ───────────────────────────────────────────────────────────────

// ── FileSearch ──────────────────────────────────────────────────────────

function FileSearch({
  onPick,
  attachedPaths,
}: {
  onPick: (path: string) => void;
  attachedPaths: string[];
}) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<string[]>([]);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const t = setTimeout(async () => {
      try {
        const res = await fetch(
          `/api/files?ext=md&q=${encodeURIComponent(query)}&limit=20`
        );
        if (res.ok) {
          const list: string[] = await res.json();
          setResults(list.filter((p) => !attachedPaths.includes(p)));
        }
      } catch {}
    }, 120);
    return () => clearTimeout(t);
  }, [query, attachedPaths]);

  return (
    <div style={{ position: "relative" }}>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        onFocus={() => setOpen(true)}
        onBlur={() => setTimeout(() => setOpen(false), 120)}
        placeholder="Search .md files to attach…"
        style={{
          width: "100%",
          padding: "6px 10px",
          background: "#11111b",
          color: "#cdd6f4",
          border: "1px solid #313244",
          borderRadius: 6,
          fontSize: 12,
          fontFamily: "monospace",
          boxSizing: "border-box",
        }}
      />
      {open && (
        <div
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            right: 0,
            marginTop: 2,
            background: "#181825",
            border: "1px solid #313244",
            borderRadius: 6,
            maxHeight: 200,
            overflowY: "auto",
            zIndex: 10,
          }}
        >
          {results.length === 0 ? (
            <div
              style={{
                padding: "8px 10px",
                fontSize: 12,
                fontFamily: "monospace",
                color: "#585b70",
                fontStyle: "italic",
              }}
            >
              {query.trim()
                ? `no matches for "${query}"`
                : "type to search .md files"}
            </div>
          ) : (
            results.map((path) => (
              <div
                key={path}
                onMouseDown={(e) => {
                  e.preventDefault();
                  onPick(path);
                  setQuery("");
                }}
                style={{
                  padding: "6px 10px",
                  fontSize: 12,
                  fontFamily: "monospace",
                  color: "#cdd6f4",
                  cursor: "pointer",
                  borderBottom: "1px solid #313244",
                }}
                onMouseEnter={(e) =>
                  (e.currentTarget.style.background = "#313244")
                }
                onMouseLeave={(e) =>
                  (e.currentTarget.style.background = "transparent")
                }
              >
                {path}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}

// ── TodoForm modal ──────────────────────────────────────────────────────

function TodoForm({
  onClose,
  onSubmit,
}: {
  onClose: () => void;
  onSubmit: (todo: {
    text: string;
    kind?: string;
    stage?: string;
    see?: TodoRef[];
  }) => Promise<void>;
}) {
  const [text, setText] = useState("");
  const [kind, setKind] = useState<string | undefined>(undefined);
  const [stage, setStage] = useState<string | undefined>("fresh");
  const [see, setSee] = useState<TodoRef[]>([]);
  const [saving, setSaving] = useState(false);

  const canSubmit = text.trim().length > 0 && !saving;

  const submit = async () => {
    if (!canSubmit) return;
    setSaving(true);
    try {
      await onSubmit({
        text: text.trim(),
        kind,
        stage,
        see: see.length > 0 ? see : undefined,
      });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "rgba(0,0,0,0.6)",
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "center",
        paddingTop: 60,
        zIndex: 100,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) submit();
        }}
        style={{
          background: "#1e1e2e",
          border: "1px solid #313244",
          borderRadius: 10,
          padding: 20,
          width: "100%",
          maxWidth: 560,
          boxShadow: "0 10px 40px rgba(0,0,0,0.4)",
          fontFamily:
            '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
          color: "#cdd6f4",
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            marginBottom: 14,
          }}
        >
          <span style={{ fontSize: 15, fontWeight: 700 }}>New Todo</span>
          <span
            style={{
              marginLeft: 8,
              fontSize: 11,
              color: "#585b70",
              fontFamily: "monospace",
            }}
          >
            ⌘↵ to save · esc to cancel
          </span>
          <button
            onClick={onClose}
            style={{
              marginLeft: "auto",
              background: "transparent",
              border: "none",
              color: "#585b70",
              cursor: "pointer",
              fontSize: 16,
            }}
          >
            ×
          </button>
        </div>

        {/* Kind pills */}
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
            kind
          </div>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            {KINDS.map((k) => (
              <ChipButton
                key={k}
                label={k}
                active={kind === k}
                onClick={() => setKind(kind === k ? undefined : k)}
                color={KIND_COLOR[k]}
              />
            ))}
          </div>
        </div>

        {/* Stage pills */}
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
            stage
          </div>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            {STAGES.map((s) => (
              <ChipButton
                key={s}
                label={s}
                active={stage === s}
                onClick={() => setStage(stage === s ? undefined : s)}
                color={STAGE_COLOR[s]}
              />
            ))}
          </div>
        </div>

        {/* Text */}
        <div style={{ marginBottom: 10 }}>
          <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
            text (markdown)
          </div>
          <textarea
            autoFocus
            value={text}
            onChange={(e) => setText(e.target.value)}
            rows={6}
            placeholder="What needs to happen? Markdown is fine."
            style={{
              width: "100%",
              padding: "8px 10px",
              background: "#11111b",
              color: "#cdd6f4",
              border: "1px solid #313244",
              borderRadius: 6,
              fontSize: 13,
              fontFamily:
                '"SF Mono", Menlo, Consolas, "Liberation Mono", monospace',
              resize: "vertical",
              boxSizing: "border-box",
            }}
          />
        </div>

        {/* References */}
        <div style={{ marginBottom: 14 }}>
          <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
            references
          </div>
          <FileSearch
            attachedPaths={see.map((r) => r.path)}
            onPick={(p) =>
              setSee((xs) => [...xs, { path: p, mode: "reference" }])
            }
          />
          {see.length > 0 && (
            <div
              style={{
                marginTop: 6,
                display: "flex",
                flexDirection: "column",
                gap: 4,
              }}
            >
              {see.map((r, i) => {
                const color = REF_MODE_COLOR[r.mode];
                return (
                  <div
                    key={`${r.path}-${i}`}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      fontSize: 11,
                      fontFamily: "monospace",
                    }}
                  >
                    <select
                      value={r.mode}
                      onChange={(e) =>
                        setSee((xs) =>
                          xs.map((x, j) =>
                            j === i
                              ? { ...x, mode: e.target.value as TodoRefMode }
                              : x
                          )
                        )
                      }
                      style={{
                        background: color.bg,
                        color: color.fg,
                        border: "none",
                        borderRadius: 3,
                        padding: "2px 4px",
                        fontSize: 11,
                        fontFamily: "monospace",
                        cursor: "pointer",
                        minWidth: 90,
                      }}
                    >
                      {REF_MODES.map((m) => (
                        <option
                          key={m}
                          value={m}
                          style={{ background: "#1e1e2e", color: "#cdd6f4" }}
                        >
                          {m}
                        </option>
                      ))}
                    </select>
                    <span
                      style={{
                        flex: 1,
                        wordBreak: "break-all",
                        color: "#94e2d5",
                      }}
                    >
                      {r.path}
                    </span>
                    <button
                      onClick={() =>
                        setSee((xs) => xs.filter((_, j) => j !== i))
                      }
                      style={{
                        background: "transparent",
                        border: "none",
                        color: "#585b70",
                        cursor: "pointer",
                        fontSize: 14,
                        padding: 0,
                        lineHeight: 1,
                      }}
                      title="Remove"
                    >
                      ×
                    </button>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Buttons */}
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
          }}
        >
          <button
            onClick={onClose}
            style={{
              background: "#313244",
              color: "#cdd6f4",
              border: "none",
              borderRadius: 6,
              padding: "6px 14px",
              fontSize: 12,
              cursor: "pointer",
            }}
          >
            Cancel
          </button>
          <button
            onClick={submit}
            disabled={!canSubmit}
            style={{
              background: canSubmit ? "#f9e2af" : "#45475a",
              color: canSubmit ? "#11111b" : "#585b70",
              border: "none",
              borderRadius: 6,
              padding: "6px 14px",
              fontSize: 12,
              fontWeight: 700,
              cursor: canSubmit ? "pointer" : "not-allowed",
            }}
          >
            {saving ? "Saving…" : "Create Todo"}
          </button>
        </div>
      </div>
    </div>
  );
}

function ChipButton({
  label,
  active,
  onClick,
  color,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  color: { bg: string; fg: string };
}) {
  return (
    <button
      onClick={onClick}
      style={{
        background: active ? color.bg : "transparent",
        color: active ? color.fg : "#585b70",
        border: `1px solid ${active ? color.bg : "#313244"}`,
        borderRadius: 12,
        padding: "3px 10px",
        fontSize: 11,
        fontFamily: "monospace",
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}

interface ArchiveEntry {
  t: number;
  type: "archived";
  id: string;
  ticket: Ticket;
  sourceLines: string[];
  file: string;
  line: number;
}

function ArchiveView() {
  const [entries, setEntries] = useState<ArchiveEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetch("/api/archive")
      .then((r) => r.json())
      .then((data) => {
        setEntries(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div style={{ padding: 20, color: "#585b70" }}>Loading archive…</div>
    );
  }
  if (entries.length === 0) {
    return (
      <div style={{ padding: 20, color: "#585b70" }}>
        No archived tickets. Click the <code>archive</code> button on a ticket
        card to move it here.
      </div>
    );
  }
  return (
    <div style={{ padding: 20, display: "flex", flexDirection: "column", gap: 8 }}>
      {entries.map((e, i) => (
        <div
          key={`${e.ticket.id}-${e.t}-${i}`}
          style={{
            background: "#181825",
            border: "1px solid #313244",
            borderRadius: 8,
            padding: "12px 14px",
            maxWidth: 800,
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              marginBottom: 6,
            }}
          >
            <span
              style={{
                background: "#45475a",
                color: "#cdd6f4",
                borderRadius: 4,
                padding: "1px 6px",
                fontSize: 11,
                fontWeight: 700,
                fontFamily: "monospace",
              }}
            >
              {e.ticket.item_type === "relay" ? "R" : "T"}
            </span>
            <span
              style={{ color: "#a6adc8", fontSize: 12, fontFamily: "monospace" }}
            >
              {e.ticket.id}
            </span>
            <span
              style={{
                marginLeft: "auto",
                color: "#585b70",
                fontSize: 11,
                fontFamily: "monospace",
              }}
            >
              {new Date(e.t * 1000).toLocaleString()}
            </span>
          </div>
          <div
            style={{
              color: "#cdd6f4",
              fontSize: 13,
              fontWeight: 500,
              marginBottom: 6,
            }}
          >
            {e.ticket.title}
          </div>
          {e.ticket.handoff && e.ticket.handoff.length > 0 && (
            <div
              style={{
                fontSize: 12,
                color: "#bac2de",
                marginBottom: 6,
                lineHeight: 1.4,
                whiteSpace: "pre-wrap",
              }}
            >
              {e.ticket.handoff.length === 1
                ? e.ticket.handoff[0]
                : e.ticket.handoff.map((h, i) => (
                    <div key={i}>• {h}</div>
                  ))}
            </div>
          )}
          <div style={{ marginBottom: 6 }}>
            <PathRow file={e.file} line={e.line} />
          </div>
          <details style={{ fontSize: 11, color: "#585b70" }}>
            <summary style={{ cursor: "pointer" }}>
              original annotations ({e.sourceLines.length})
            </summary>
            <pre
              style={{
                background: "#11111b",
                padding: 8,
                borderRadius: 4,
                overflow: "auto",
                color: "#a6adc8",
                fontSize: 11,
              }}
            >
              {e.sourceLines.join("\n")}
            </pre>
          </details>
        </div>
      ))}
    </div>
  );
}

function Board() {
  const [tickets, setTickets] = useState<Ticket[]>([]);
  const [summaries, setSummaries] = useState<Summary[]>([]);
  const [todos, setTodos] = useState<Todo[]>([]);
  const [connected, setConnected] = useState(false);
  const [view, setView] = useState<"board" | "archive">("board");
  const [formOpen, setFormOpen] = useState(false);
  const [filterText, setFilterText] = useState("");
  const [filterRelay, setFilterRelay] = useState("");
  const [filterAssignee, setFilterAssignee] = useState("");
  const [draggedStatus, setDraggedStatus] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [workspace, setWorkspace] = useState<string>("");

  useEffect(() => {
    fetch("/api/status")
      .then((r) => r.json())
      .then((s) => setWorkspace(s.workspace || ""))
      .catch(() => {});
  }, []);

  const workspaceName = workspace
    ? workspace.split("/").filter(Boolean).pop() || workspace
    : "";

  useEffect(() => {
    document.title = workspaceName
      ? `hack-board — ${workspaceName}`
      : "hack-board";
  }, [workspaceName]);

  const handleStatusDrop = useCallback(
    async (id: string, bucket: string) => {
      setDraggedStatus(null);
      try {
        const res = await fetch(
          `/api/status/${encodeURIComponent(id)}`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ to: bucket }),
          }
        );
        if (!res.ok) {
          const body = await res.json().catch(() => ({} as any));
          const msg =
            body.error ||
            `Move failed (${res.status})` +
              (body.from && body.allowed
                ? ` — allowed from ${body.from}: ${body.allowed.join(", ")}`
                : "");
          setToast(msg);
          setTimeout(() => setToast(null), 3500);
        }
      } catch (e: any) {
        setToast(`Move failed: ${e.message || e}`);
        setTimeout(() => setToast(null), 3500);
      }
    },
    []
  );

  useEffect(() => {
    const evtSource = new EventSource("/api/events");

    evtSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        if (Array.isArray(data)) {
          setTickets(data.map(normalizeTicket));
        } else {
          setTickets((data.tickets || []).map(normalizeTicket));
          setSummaries(data.summaries || []);
          setTodos(data.todos || []);
        }
        setConnected(true);
      } catch (e) {
        console.error("Failed to parse SSE data:", e);
      }
    };

    evtSource.onerror = () => setConnected(false);
    return () => evtSource.close();
  }, []);

  const relayIds = useMemo(
    () =>
      tickets
        .filter((t) => t.item_type === "relay")
        .map((r) => r.id)
        .sort(),
    [tickets]
  );

  const assignees = useMemo(() => {
    const s = new Set<string>();
    for (const t of tickets) if (t.assignee) s.add(t.assignee);
    return Array.from(s).sort();
  }, [tickets]);

  const filterActive =
    filterText.trim() !== "" || filterRelay !== "" || filterAssignee !== "";

  const ticketMatchesFilter = useCallback(
    (t: Ticket) => {
      if (
        filterRelay &&
        t.id !== filterRelay &&
        t.parent !== filterRelay
      ) {
        return false;
      }
      if (filterAssignee && t.assignee !== filterAssignee) return false;
      if (filterText.trim()) {
        const q = filterText.trim().toLowerCase();
        if (
          !t.id.toLowerCase().includes(q) &&
          !t.title.toLowerCase().includes(q)
        ) {
          return false;
        }
      }
      return true;
    },
    [filterText, filterRelay, filterAssignee]
  );

  const filteredTickets = useMemo(
    () => tickets.filter(ticketMatchesFilter),
    [tickets, ticketMatchesFilter]
  );

  const ticketsInColumn = useCallback(
    (col: ColumnDef) =>
      filteredTickets.filter(
        (t) => !t.is_epic && col.statuses.includes(t.status)
      ),
    [filteredTickets]
  );

  const filteredEpics = useMemo(
    () => filteredTickets.filter((t) => t.is_epic),
    [filteredTickets]
  );

  const filteredTodos = useMemo(() => {
    // Relay/assignee filters have no meaning for todos — hide them when active.
    if (filterRelay || filterAssignee) return [];
    if (!filterText.trim()) return todos;
    const q = filterText.trim().toLowerCase();
    return todos.filter(
      (t) =>
        t.id.toLowerCase().includes(q) ||
        t.text.toLowerCase().includes(q) ||
        (t.kind && t.kind.toLowerCase().includes(q)) ||
        (t.stage && t.stage.toLowerCase().includes(q))
    );
  }, [todos, filterText, filterRelay, filterAssignee]);

  const orphanSummaries = summaries.filter((s) => !s.ticket && !s.promoted);

  const handleAddTodo = () => setFormOpen(true);

  const submitTodo = async (todo: {
    text: string;
    kind?: string;
    stage?: string;
    see?: TodoRef[];
  }) => {
    await fetch("/api/todos", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(todo),
    });
  };

  return (
    <div
      style={{
        fontFamily:
          '-apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
        background: "#11111b",
        minHeight: "100vh",
        color: "#cdd6f4",
      }}
    >
      <div
        style={{
          padding: "12px 24px",
          borderBottom: "1px solid #313244",
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexWrap: "wrap",
        }}
      >
        <span style={{ fontSize: 18, fontWeight: 700 }}>hack-board</span>
        {workspaceName && (
          <span
            title={workspace}
            style={{
              color: "#cba6f7",
              fontSize: 13,
              fontFamily: "monospace",
              fontWeight: 600,
            }}
          >
            {workspaceName}
          </span>
        )}
        <span style={{ color: "#585b70", fontSize: 13 }}>
          {filterActive
            ? `${filteredTickets.length}/${tickets.length}`
            : `${tickets.length}`}{" "}
          tickets
          {summaries.length > 0 && ` | ${summaries.length} summaries`}
          {todos.length > 0 && ` | ${todos.length} todos`}
        </span>

        {/* Filter strip */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            marginLeft: 8,
          }}
        >
          <input
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            placeholder="filter id/title…"
            style={{
              background: "#11111b",
              color: "#cdd6f4",
              border: "1px solid #313244",
              borderRadius: 6,
              padding: "4px 8px",
              fontSize: 12,
              fontFamily: "monospace",
              width: 160,
            }}
          />
          <select
            value={filterRelay}
            onChange={(e) => setFilterRelay(e.target.value)}
            title="Filter to a relay (shows relay + tickets with @hack:parent(RXXX))"
            style={{
              background: filterRelay ? "#cba6f7" : "#11111b",
              color: filterRelay ? "#11111b" : "#cdd6f4",
              border: "1px solid #313244",
              borderRadius: 6,
              padding: "4px 6px",
              fontSize: 12,
              fontFamily: "monospace",
              cursor: "pointer",
            }}
          >
            <option value="">all relays</option>
            {relayIds.map((id) => (
              <option key={id} value={id}>
                {id}
              </option>
            ))}
          </select>
          {assignees.length > 0 && (
            <select
              value={filterAssignee}
              onChange={(e) => setFilterAssignee(e.target.value)}
              title="Filter by assignee"
              style={{
                background: filterAssignee ? "#b4befe" : "#11111b",
                color: filterAssignee ? "#11111b" : "#cdd6f4",
                border: "1px solid #313244",
                borderRadius: 6,
                padding: "4px 6px",
                fontSize: 12,
                fontFamily: "monospace",
                cursor: "pointer",
              }}
            >
              <option value="">any assignee</option>
              {assignees.map((a) => (
                <option key={a} value={a}>
                  {a}
                </option>
              ))}
            </select>
          )}
          {filterActive && (
            <button
              onClick={() => {
                setFilterText("");
                setFilterRelay("");
                setFilterAssignee("");
              }}
              title="Clear filters"
              style={{
                background: "transparent",
                color: "#585b70",
                border: "1px solid #313244",
                borderRadius: 6,
                padding: "4px 8px",
                fontSize: 12,
                cursor: "pointer",
              }}
            >
              clear
            </button>
          )}
        </div>
        <button
          onClick={() => setView(view === "board" ? "archive" : "board")}
          style={{
            marginLeft: "auto",
            background: view === "archive" ? "#cba6f7" : "#313244",
            color: view === "archive" ? "#11111b" : "#a6adc8",
            border: "none",
            borderRadius: 6,
            padding: "4px 12px",
            fontSize: 12,
            fontWeight: 600,
            cursor: "pointer",
          }}
        >
          {view === "archive" ? "← Board" : "Archive"}
        </button>
        <button
          onClick={handleAddTodo}
          style={{
            background: "#f9e2af",
            color: "#11111b",
            border: "none",
            borderRadius: 6,
            padding: "4px 12px",
            fontSize: 12,
            fontWeight: 700,
            cursor: "pointer",
          }}
        >
          + Todo
        </button>
        <span
          style={{
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: connected ? "#a6e3a1" : "#f38ba8",
          }}
        />
      </div>

      {formOpen && (
        <TodoForm
          onClose={() => setFormOpen(false)}
          onSubmit={submitTodo}
        />
      )}

      {toast && (
        <div
          onClick={() => setToast(null)}
          style={{
            position: "fixed",
            bottom: 20,
            left: "50%",
            transform: "translateX(-50%)",
            background: "#45171e",
            color: "#f38ba8",
            border: "1px solid #f38ba8",
            borderRadius: 6,
            padding: "10px 16px",
            fontSize: 12,
            fontFamily: "monospace",
            maxWidth: 560,
            zIndex: 200,
            cursor: "pointer",
            boxShadow: "0 6px 24px rgba(0,0,0,0.5)",
          }}
        >
          {toast}
        </div>
      )}

      {view === "archive" ? (
        <ArchiveView />
      ) : (
      <div
        style={{
          display: "flex",
          gap: 12,
          padding: 20,
          overflowX: "auto",
        }}
      >
        <EpicsColumn epics={filteredEpics} allTickets={tickets} />

        <TodoColumn
          todos={filteredTodos}
          openTickets={filteredTickets.filter(
            (t) => !t.is_epic && t.status === "open"
          )}
          summaries={summaries}
          onAdd={handleAddTodo}
          draggedStatus={draggedStatus}
          onDrop={handleStatusDrop}
          onTicketDragStart={(s) => setDraggedStatus(s)}
          onTicketDragEnd={() => setDraggedStatus(null)}
        />

        {orphanSummaries.length > 0 && (
          <div style={{ flex: "1 1 0", minWidth: 240, maxWidth: 340 }}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                marginBottom: 12,
                padding: "0 4px",
              }}
            >
              <div
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: "50%",
                  background: "#f9e2af",
                }}
              />
              <span
                style={{ color: "#cdd6f4", fontSize: 14, fontWeight: 600 }}
              >
                Inbox
              </span>
              <span
                style={{ color: "#585b70", fontSize: 12, marginLeft: "auto" }}
              >
                {orphanSummaries.length}
              </span>
            </div>
            <div
              style={{
                background: "#181825",
                borderRadius: 8,
                padding: 8,
                minHeight: 100,
              }}
            >
              {(() => {
                const latestOrphan = orphanSummaries.find((s) => !s.promoted);
                return orphanSummaries.map((s) => (
                  <div
                    key={s.id}
                    style={{
                      background: "#1e1e2e",
                      border: "1px solid #313244",
                      borderRadius: 8,
                      padding: "12px 14px",
                      marginBottom: 8,
                      opacity: s.promoted ? 0.5 : 1,
                    }}
                  >
                    {s.author && (
                      <div
                        style={{
                          fontSize: 11,
                          color: "#585b70",
                          marginBottom: 4,
                        }}
                      >
                        {s.author}
                      </div>
                    )}
                    <div
                      style={{
                        color: "#cdd6f4",
                        fontSize: 13,
                        lineHeight: 1.4,
                        whiteSpace: "pre-wrap",
                      }}
                    >
                      {s.text.length > 300
                        ? s.text.slice(0, 300) + "..."
                        : s.text}
                    </div>
                    <div
                      style={{
                        marginTop: 8,
                        fontSize: 11,
                        color: "#585b70",
                        fontFamily: "monospace",
                      }}
                    >
                      {s.id}
                    </div>
                    {s === latestOrphan && (
                      <SummaryPromptButton summaryId={s.id} />
                    )}
                  </div>
                ));
              })()}
            </div>
          </div>
        )}

        {COLUMNS.map((col) => (
          <Column
            key={col.key}
            label={col.label}
            color={col.color}
            bucket={col.key}
            tickets={ticketsInColumn(col)}
            summaries={summaries}
            draggedStatus={draggedStatus}
            onDrop={handleStatusDrop}
            onTicketDragStart={(s) => setDraggedStatus(s)}
            onTicketDragEnd={() => setDraggedStatus(null)}
          />
        ))}
      </div>
      )}
    </div>
  );
}

// ── Mount ───────────────────────────────────────────────────────────────

const root = createRoot(document.getElementById("root")!);
root.render(<Board />);
