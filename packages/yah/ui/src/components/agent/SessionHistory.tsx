import { useCallback, useEffect, useState } from "react";
import { getEnv } from "../../env";
import type { WireSessionHistoryRow } from "../../env/types";
import { Icon } from "../shared/Glyph";
import { SectionHeader } from "../shared/SectionHeader";

/* Past chat sessions on disk + sidecar metadata. Mounts under
   `SessionList` in the rail aside; clicking a row expands it inline
   (no read-only viewer yet — the link mode in ChatPane's export menu is
   the bridge for now). The ↻ button kicks off a strictly-user-click
   reindex; an LLM-backed indexer will swap into that call site once we
   know the existing tag set is rich enough to bias toward. */

interface SessionHistoryProps {
  rigId: string | null;
  /* Bump from the parent to force a re-list — fires after a chat's
     post-Q+A auto-index lands so the row's new title appears without
     the operator clicking refresh. */
  refreshKey?: number;
}

export function SessionHistory({ rigId, refreshKey }: SessionHistoryProps) {
  const [rows, setRows] = useState<WireSessionHistoryRow[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [reindexing, setReindexing] = useState<Set<string>>(new Set());

  const refresh = useCallback(async () => {
    if (!rigId) {
      setRows([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const env = await getEnv();
      const next = await env.rpc.agent.history.list(rigId);
      setRows(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [rigId]);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshKey]);

  const reindex = useCallback(
    async (sessionId: string) => {
      if (!rigId) return;
      setReindexing((prev) => {
        const next = new Set(prev);
        next.add(sessionId);
        return next;
      });
      try {
        const env = await getEnv();
        const meta = await env.rpc.agent.history.reindex(rigId, sessionId);
        setRows((prev) =>
          prev.map((r) =>
            r.sessionId === sessionId
              ? { ...r, meta, stale: false }
              : r,
          ),
        );
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setReindexing((prev) => {
          const next = new Set(prev);
          next.delete(sessionId);
          return next;
        });
      }
    },
    [rigId],
  );

  return (
    <div className="mt-3 flex flex-col gap-1">
      <div className="mb-1 flex items-center justify-between px-1">
        <SectionHeader>History</SectionHeader>
        <button
          onClick={() => void refresh()}
          disabled={loading || !rigId}
          className="rounded p-1 text-ink-3 hover:bg-vellum/60 disabled:opacity-40"
          title="Refresh history"
        >
          <Icon name="refresh" size={11} />
        </button>
      </div>
      {error && (
        <div className="mx-1 rounded border border-oxblood/50 bg-oxblood/10 px-2 py-1 font-mono text-[10.5px] text-oxblood">
          {error}
        </div>
      )}
      {!loading && rows.length === 0 && !error && (
        <div className="px-2 py-1 font-display text-[11.5px] italic text-ink-4">
          {rigId ? "No past sessions yet." : "Attach a rig to see history."}
        </div>
      )}
      {rows.map((row) => {
        const isExpanded = expanded === row.sessionId;
        const isReindexing = reindexing.has(row.sessionId);
        const title = row.meta?.title ?? "(unindexed)";
        const ticket = row.meta?.ticketId ?? null;
        return (
          <div
            key={row.sessionId}
            className="rounded border border-rule/40 bg-vellum/40"
          >
            <button
              onClick={() => setExpanded(isExpanded ? null : row.sessionId)}
              className="flex w-full flex-col items-stretch px-2 py-1.5 text-left hover:bg-vellum/60"
            >
              <div className="mb-0.5 flex items-center gap-1.5">
                <span
                  className={`h-[6px] w-[6px] shrink-0 rounded-full ${
                    row.stale ? "bg-oxblood/70" : "bg-ink-4"
                  }`}
                  title={row.stale ? "needs re-index" : "fresh"}
                />
                <span className="min-w-0 flex-1 truncate font-display text-[12px] text-ink">
                  {title}
                </span>
                <span className="font-mono text-[10px] text-ink-4">
                  {relativeTime(row.jsonlMtime)}
                </span>
              </div>
              <div className="flex items-center gap-1.5 truncate font-mono text-[10px] text-ink-4">
                {ticket && (
                  <span className="rounded bg-vellum px-1 text-ink-3">
                    {ticket}
                  </span>
                )}
                {row.meta?.engine && (
                  <span className="truncate">{row.meta.engine}</span>
                )}
                {!row.meta && <span className="italic">unindexed</span>}
              </div>
            </button>
            {isExpanded && (
              <div className="border-t border-rule/40 px-2 py-1.5">
                {row.meta?.summary && (
                  <div className="mb-1 font-display text-[11.5px] text-ink-2">
                    {row.meta.summary}
                  </div>
                )}
                <MetaStats row={row} />
                <div className="mt-1.5 flex items-center gap-1.5">
                  <button
                    onClick={() => void reindex(row.sessionId)}
                    disabled={isReindexing}
                    className="flex items-center gap-1 rounded border border-rule/50 bg-paper-2 px-1.5 py-0.5 font-mono text-[10px] text-ink-2 hover:border-rule disabled:opacity-50"
                    title="Re-index summary + tags"
                  >
                    <Icon name="refresh" size={10} />
                    {row.meta ? "re-index" : "index"}
                  </button>
                  <button
                    onClick={() => void copyLink(row.sessionId)}
                    className="flex items-center gap-1 rounded border border-rule/50 bg-paper-2 px-1.5 py-0.5 font-mono text-[10px] text-ink-2 hover:border-rule"
                    title="Copy relative path to .jsonl"
                  >
                    <Icon name="copy" size={10} />
                    link
                  </button>
                </div>
                <div className="mt-1 truncate font-mono text-[9.5px] text-ink-4">
                  {row.sessionId}
                </div>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function MetaStats({ row }: { row: WireSessionHistoryRow }) {
  const m = row.meta;
  const parts: string[] = [];
  parts.push(`${row.bytes.toLocaleString()} bytes`);
  if (m) {
    parts.push(`${m.turnCount} turns`);
    if (m.toolCallCount > 0) {
      parts.push(
        m.toolFailCount > 0
          ? `${m.toolCallCount} tools (${m.toolFailCount} fail)`
          : `${m.toolCallCount} tools`,
      );
    }
    if (m.tags.length > 0) parts.push(`#${m.tags.join(" #")}`);
  }
  return (
    <div className="font-mono text-[10px] text-ink-4">
      {parts.join(" · ")}
    </div>
  );
}

async function copyLink(sessionId: string) {
  try {
    await navigator.clipboard.writeText(`.yah/sessions/${sessionId}.jsonl`);
  } catch {
    // No clipboard (browser preview / locked-down WebView). Silent —
    // users rarely retry.
  }
}

function relativeTime(ms: number): string {
  if (!ms) return "—";
  const diff = Date.now() - ms;
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "now";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 30) return `${d}d`;
  const mo = Math.floor(d / 30);
  return `${mo}mo`;
}

