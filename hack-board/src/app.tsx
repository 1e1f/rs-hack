/**
 * hack-board UI
 *
 * Kanban board that displays @hack: tickets from Rust source.
 * Connects to SSE for live updates.
 */

import { useState, useEffect, useCallback } from "react";
import { createRoot } from "react-dom/client";

// ── Types ───────────────────────────────────────────────────────────────

interface Ticket {
  id: string;
  title: string;
  item_type: "ticket" | "relay";
  kind?: string;  // "feature", "bug", "task"
  status: string;
  assignee?: string;
  phase?: string;
  parent?: string;
  severity?: string;
  handoff?: string;
  next_steps?: string[];
  cleanup?: string[];
  verify?: string[];
  depends_on: string[];
  see_also: string[];
  file: string;
  line: number;
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

function ForkButton({
  summaryId,
  ticketId,
}: {
  summaryId: string;
  ticketId?: string;
}) {
  const [state, setState] = useState<"idle" | "copied">("idle");

  return (
    <button
      onClick={async () => {
        try {
          // If this is a ticket (relay/handoff), use the prompt endpoint
          if (ticketId) {
            const res = await fetch(`/api/prompt/${ticketId}`);
            if (res.ok) {
              await navigator.clipboard.writeText(await res.text());
              setState("copied");
              setTimeout(() => setState("idle"), 2000);
              return;
            }
          }
          // Otherwise, grab the summary text directly
          const res = await fetch("/api/summaries");
          const summaries = await res.json();
          const s = summaries.find((s: any) => s.id === summaryId);
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
          console.error("Fork failed:", e);
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
      {state === "copied" ? "Copied!" : "Fork"}
    </button>
  );
}

const COLUMNS = [
  { key: "open", label: "Open", color: "#6b7280" },
  { key: "claimed", label: "Claimed", color: "#3b82f6" },
  { key: "in-progress", label: "In Progress", color: "#f59e0b" },
  { key: "handoff", label: "Handoff", color: "#8b5cf6" },
  { key: "review", label: "Review", color: "#06b6d4" },
  { key: "done", label: "Done", color: "#10b981" },
];

// ── Card Component ──────────────────────────────────────────────────────

function TicketCard({
  ticket,
  summaries,
}: {
  ticket: Ticket;
  summaries: Summary[];
}) {
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

  return (
    <div
      style={{
        background: "#1e1e2e",
        border: "1px solid #313244",
        borderRadius: 8,
        padding: "12px 14px",
        marginBottom: 8,
        cursor: "default",
        transition: "border-color 0.15s",
      }}
      onMouseEnter={(e) =>
        (e.currentTarget.style.borderColor = "#585b70")
      }
      onMouseLeave={(e) =>
        (e.currentTarget.style.borderColor = "#313244")
      }
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
        <span
          style={{
            color: "#a6adc8",
            fontSize: 12,
            fontFamily: "monospace",
          }}
        >
          {ticket.id}
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
          <Pill
            label={ticket.assignee}
            color="#45475a"
            textColor="#b4befe"
          />
        )}
        {ticket.phase && (
          <Pill
            label={ticket.phase}
            color="#45475a"
            textColor="#f9e2af"
          />
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

      {/* Handoff message */}
      {ticket.handoff && (
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
          }}
        >
          {ticket.handoff}
        </div>
      )}

      {/* Next steps (for relays) */}
      {ticket.next_steps && ticket.next_steps.length > 0 && (
        <div style={{ marginTop: 8, fontSize: 12, color: "#a6adc8" }}>
          <div style={{ color: "#585b70", marginBottom: 2 }}>Next:</div>
          {ticket.next_steps.map((step: string, i: number) => (
            <div key={i} style={{ paddingLeft: 8, lineHeight: 1.4 }}>
              - {step}
            </div>
          ))}
        </div>
      )}

      {/* Summaries (agent comments) */}
      {summaries.length > 0 && (() => {
        // Only the most recent unpromoted summary gets the Continue button
        const latestUnpromoted = summaries.find((s) => !s.promoted);
        return (
          <div style={{ marginTop: 8 }}>
            <div style={{ fontSize: 11, color: "#585b70", marginBottom: 4 }}>
              {summaries.length} summary{summaries.length > 1 ? "ies" : ""}
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
                  <ForkButton summaryId={s.id} ticketId={s.ticket} />
                )}
              </div>
            ))}
          </div>
        );
      })()}

      {/* Fork button for relays/handoffs with no summaries */}
      {summaries.length === 0 &&
        (ticket.status === "handoff" || ticket.item_type === "relay") && (
          <ForkButton summaryId={ticket.id} ticketId={ticket.id} />
        )}

      {/* Source link */}
      <div
        style={{
          marginTop: 8,
          fontSize: 11,
          color: "#585b70",
          fontFamily: "monospace",
        }}
      >
        {ticket.file}:{ticket.line}
      </div>
    </div>
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
  tickets,
  summaries,
}: {
  label: string;
  color: string;
  tickets: Ticket[];
  summaries: Summary[];
}) {
  return (
    <div
      style={{
        flex: "1 1 0",
        minWidth: 240,
        maxWidth: 340,
      }}
    >
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
        <span
          style={{
            color: "#cdd6f4",
            fontSize: 14,
            fontWeight: 600,
          }}
        >
          {label}
        </span>
        <span
          style={{
            color: "#585b70",
            fontSize: 12,
            marginLeft: "auto",
          }}
        >
          {tickets.length}
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
        {tickets.map((t) => (
          <TicketCard
            key={t.id}
            ticket={t}
            summaries={summaries.filter((s) => s.ticket === t.id)}
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

// ── Board Component ─────────────────────────────────────────────────────

function Board() {
  const [tickets, setTickets] = useState<Ticket[]>([]);
  const [summaries, setSummaries] = useState<Summary[]>([]);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const evtSource = new EventSource("/api/events");

    evtSource.onmessage = (event) => {
      try {
        const data = JSON.parse(event.data);
        // Handle both old format (array) and new format ({tickets, summaries})
        if (Array.isArray(data)) {
          setTickets(data);
        } else {
          setTickets(data.tickets || []);
          setSummaries(data.summaries || []);
        }
        setConnected(true);
      } catch (e) {
        console.error("Failed to parse SSE data:", e);
      }
    };

    evtSource.onerror = () => {
      setConnected(false);
    };

    return () => evtSource.close();
  }, []);

  const ticketsByStatus = useCallback(
    (status: string) => tickets.filter((t) => t.status === status),
    [tickets]
  );

  const totalDone = ticketsByStatus("done").length;
  const orphanSummaries = summaries.filter(
    (s) => !s.ticket && !s.promoted
  );

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
      {/* Header */}
      <div
        style={{
          padding: "16px 24px",
          borderBottom: "1px solid #313244",
          display: "flex",
          alignItems: "center",
          gap: 12,
        }}
      >
        <span style={{ fontSize: 18, fontWeight: 700 }}>hack-board</span>
        <span style={{ color: "#585b70", fontSize: 13 }}>
          {tickets.length} tickets | {totalDone} done
          {summaries.length > 0 && ` | ${summaries.length} summaries`}
        </span>
        <span
          style={{
            marginLeft: "auto",
            width: 8,
            height: 8,
            borderRadius: "50%",
            background: connected ? "#a6e3a1" : "#f38ba8",
          }}
        />
      </div>

      {/* Columns */}
      <div
        style={{
          display: "flex",
          gap: 12,
          padding: 20,
          overflowX: "auto",
        }}
      >
        {/* Inbox column for orphan summaries */}
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
                      <ForkButton summaryId={s.id} ticketId={s.ticket} />
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
            tickets={ticketsByStatus(col.key)}
            summaries={summaries}
          />
        ))}
      </div>
    </div>
  );
}

// ── Mount ───────────────────────────────────────────────────────────────

const root = createRoot(document.getElementById("root")!);
root.render(<Board />);
