
import { useEffect, useRef, useState } from "react";
import { useDraggable } from "@dnd-kit/core";
import { CSS } from "@dnd-kit/utilities";
import { KindBadge, StatusPill } from "../shared/Pill";
import { Glyph, Icon } from "../shared/Glyph";
import { CardExpanded } from "./CardExpanded";
import { CountChip } from "./CountChip";
import { buildPickupPrompt, buildReviewPrompt } from "./prompt";
import { getEnv } from "../../env";
import type { WireViolation } from "../../env/types";
import type { Ticket } from "../../types";

interface TicketCardProps {
  /* Rig the prompt should be rendered against. Threaded down from the
     Board so the clipboard-copy click can call `arch.ticket_prompt` —
     the daemon-side renderer that stays byte-equal with
     `yah board show <id> --prompt`. */
  rigId: string;
  ticket: Ticket;
  columnEyebrow?: string;
  /* Rule violations whose anchor file matches this ticket's anchor file —
     surfaced as a small badge in the header so a failing rule reads as a
     status line on the relay/ticket that owns it (per R017-F2). */
  violations?: WireViolation[];
}

/* Mapping from raw status to the column-eyebrow string. Used to suppress the
   StatusPill when it would just echo the column header — a "Handoff" pill in
   the Ready-to-review column is noise; a "Claimed" pill in In-flight still
   adds info, so we keep it. */
const STATUS_LABEL: Record<string, string> = {
  open: "Awaiting pickup",
  claimed: "In flight",
  "in-progress": "In flight",
  handoff: "Ready to review",
  review: "Validated",
  done: "Validated",
};

export function TicketCard({ rigId, ticket: t, columnEyebrow, violations }: TicketCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [copyState, setCopyState] = useState<"idle" | "copied" | "error">(
    "idle",
  );
  const [archiveState, setArchiveState] = useState<
    "idle" | "armed" | "archiving" | "error"
  >("idle");
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const [pathCopied, setPathCopied] = useState(false);
  const copyResetRef = useRef<number | null>(null);
  const archiveResetRef = useRef<number | null>(null);
  const pathResetRef = useRef<number | null>(null);
  useEffect(
    () => () => {
      if (copyResetRef.current !== null)
        window.clearTimeout(copyResetRef.current);
      if (archiveResetRef.current !== null)
        window.clearTimeout(archiveResetRef.current);
      if (pathResetRef.current !== null)
        window.clearTimeout(pathResetRef.current);
    },
    [],
  );

  /* Click-to-copy on the file chip — full rig-relative path + line, since
     the chip itself only shows the last two path segments to keep the
     footer compact. */
  async function handleCopyPath(e: React.MouseEvent) {
    e.stopPropagation();
    const text = `${t.file}:${t.line}`;
    try {
      await navigator.clipboard.writeText(text);
      setPathCopied(true);
      if (pathResetRef.current !== null)
        window.clearTimeout(pathResetRef.current);
      pathResetRef.current = window.setTimeout(() => setPathCopied(false), 1500);
    } catch (err) {
      console.warn("[copy-path] clipboard write failed", err);
    }
  }

  /* Two-stage archive: first click arms (3s window), second commits.
     Mirrors the legacy hack-board behaviour the prompt copy still
     references. The card disappears on success once the watcher's
     reindex propagates; on failure we surface the daemon's error. */
  async function handleArchive(e: React.MouseEvent) {
    e.stopPropagation();
    if (archiveState === "armed") {
      if (archiveResetRef.current !== null) {
        window.clearTimeout(archiveResetRef.current);
        archiveResetRef.current = null;
      }
      setArchiveState("archiving");
      setArchiveError(null);
      try {
        const env = await getEnv();
        await env.rpc.archiveTicket(rigId, t.id);
        /* Card unmounts when the next refetch fires; if it doesn't (e.g.
           archive succeeded but reindex stalled), drop back to idle. */
        archiveResetRef.current = window.setTimeout(() => {
          setArchiveState("idle");
        }, 4000);
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        console.warn("[archive] failed", err);
        setArchiveError(msg);
        setArchiveState("error");
        archiveResetRef.current = window.setTimeout(() => {
          setArchiveState("idle");
          setArchiveError(null);
        }, 4000);
      }
      return;
    }
    setArchiveState("armed");
    if (archiveResetRef.current !== null)
      window.clearTimeout(archiveResetRef.current);
    archiveResetRef.current = window.setTimeout(() => {
      setArchiveState("idle");
    }, 3000);
  }
  const isRelay = t.itemType === "relay";
  const isZone = !!t.isZone;
  const isEpic = t.kind === "epic";
  const isReviewMode = t.status === "review" || t.status === "done";

  async function handleCopyPrompt(e: React.MouseEvent) {
    e.stopPropagation();
    /* Source-of-truth prompt rendering lives on the daemon
       (`arch.ticket_prompt` → yah_kg::prompt::render). The browser stub
       returns markdown:null because there's no daemon running; in that
       case we fall back to the local builder so dev-mode without Tauri
       still produces a clipboard payload. The Tauri path always returns
       markdown for live ids — null only when the id isn't on the board. */
    const mode = isReviewMode ? "review" : "pickup";
    let text: string | null = null;
    try {
      const env = await getEnv();
      const result = await env.rpc.ticketPrompt(rigId, { id: t.id, mode });
      text = result.markdown;
    } catch (err) {
      console.warn("[ticket-prompt] rpc failed, falling back to local builder", err);
    }
    if (text == null) {
      text = isReviewMode ? buildReviewPrompt(t) : buildPickupPrompt(t);
    }
    try {
      await navigator.clipboard.writeText(text);
      setCopyState("copied");
    } catch {
      setCopyState("error");
    }
    if (copyResetRef.current !== null)
      window.clearTimeout(copyResetRef.current);
    copyResetRef.current = window.setTimeout(() => {
      setCopyState("idle");
      copyResetRef.current = null;
    }, 1600);
  }
  const hasAgent =
    isRelay && typeof t.assignee === "string" && t.assignee.startsWith("agent:");
  /* "Live" = a relay an agent is currently driving. We don't animate per-ticket;
     ownership granularity is the relay. */
  const live =
    hasAgent &&
    (t.status === "in-progress" ||
      t.status === "claimed" ||
      t.status === "handoff");
  /* Schema-smell: a relay carrying both child relays AND child tickets.
     childCounts.{open,active,handoff} sums all live children, so a positive
     diff between that sum and relays count means there are loose tickets too. */
  const cc = t.childCounts;
  const totalLive = cc ? cc.open + cc.active + cc.handoff : 0;
  const relayCount = cc?.relays ?? 0;
  const mixed =
    isRelay && cc != null && relayCount > 0 && totalLive - relayCount > 0;
  /* Hide status pill when it just echoes the column eyebrow. */
  const echoesColumn =
    !!columnEyebrow && STATUS_LABEL[t.status] === columnEyebrow;

  const violationCount = violations?.length ?? 0;
  const violationHasError =
    violations?.some((v) => v.severity === "error") ?? false;
  const violationTooltip = violations
    ?.map(
      (v) =>
        `${v.severity === "error" ? "✗" : "⚠"} ${v.rule_kind}: ${v.message}`,
    )
    .join("\n");

  const { attributes, listeners, setNodeRef, transform, isDragging } =
    useDraggable({ id: t.id, disabled: isZone });

  const style = {
    transform: CSS.Translate.toString(transform),
    opacity: isDragging ? 0.55 : 1,
  };

  return (
    <article
      ref={setNodeRef}
      style={style}
      {...attributes}
      {...listeners}
      onClick={() => setExpanded((v) => !v)}
      className={`lift relative rounded-md border bg-vellum text-[12px] shadow-[0_1px_2px_rgba(70,45,20,0.08)] ${
        isRelay
          ? "border-[color-mix(in_oklab,var(--color-accent)_22%,var(--color-rule))] pl-3.5"
          : "border-rule/50 pl-3"
      } px-3 py-2.5 ${isZone ? "cursor-pointer" : "cursor-grab active:cursor-grabbing"} ${
        isDragging ? "dragging" : ""
      }`}
    >
      {isRelay && (
        <span
          aria-hidden
          className={`absolute left-0 top-0 bottom-0 w-1 rounded-[2px] ${live ? "candle-rail" : ""}`}
          style={{
            background:
              "linear-gradient(to bottom, transparent, var(--color-accent), transparent)",
            filter: live
              ? "drop-shadow(0 0 6px color-mix(in oklab, var(--color-accent) 65%, transparent))"
              : "none",
          }}
        />
      )}
      {isRelay && (
        <span
          aria-hidden
          className="absolute -top-px right-3"
          style={{
            width: isEpic ? 9 : 7,
            height: isEpic ? 16 : 12,
            background: isEpic
              ? "linear-gradient(to right, var(--color-accent-2) 0%, var(--color-accent) 35%, color-mix(in oklab, var(--color-accent) 50%, var(--color-vellum)) 50%, var(--color-accent) 65%, var(--color-accent-2) 100%)"
              : "var(--color-accent)",
            clipPath:
              "polygon(0 0, 100% 0, 100% 100%, 50% calc(100% - 4px), 0 100%)",
            filter: live
              ? "drop-shadow(0 1px 2px color-mix(in oklab, var(--color-accent) 60%, transparent))"
              : "drop-shadow(0 1px 1px rgba(0,0,0,0.1))",
          }}
        />
      )}

      <header className="flex items-start gap-2">
        <button
          onClick={(e) => {
            e.stopPropagation();
            setExpanded((v) => !v);
          }}
          onPointerDown={(e) => e.stopPropagation()}
          className="mt-0.5 text-ink-3 hover:text-ink-2"
          title={expanded ? "Collapse" : "Expand"}
        >
          <Icon name={expanded ? "chevron-down" : "chevron-right"} size={12} />
        </button>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <KindBadge kind={t.kind} itemType={t.itemType} isZone={isZone} />
            <span className="whitespace-nowrap font-mono text-[11px] text-ink-3">
              {t.id}
              {t.parent && !t.id.startsWith(`${t.parent}-`) && (
                <span className="opacity-55">
                  {" "}
                  ↪ {t.parent.replace(/^R/, "")}
                </span>
              )}
            </span>
            {!echoesColumn && !isZone && <StatusPill status={t.status} />}
            {violationCount > 0 && (
              <span
                title={violationTooltip}
                className={`inline-flex cursor-help items-center gap-0.5 ${
                  violationHasError ? "text-st-bug" : "text-st-handoff"
                }`}
              >
                <Icon name="bug" size={12} />
                <span className="font-mono text-[10px]">{violationCount}</span>
              </span>
            )}
            {mixed && (
              <span
                title="Mixed children — this relay holds both child relays and loose tickets. Move tickets into a child relay."
                className="inline-flex cursor-help items-center text-st-handoff"
              >
                <Icon name="bug" size={12} />
              </span>
            )}
            {t.phase && (
              <span className="font-mono text-[11px] text-ink-3">
                {t.phase}
              </span>
            )}
          </div>
          <h3 className="mt-1 font-display text-[15px] font-medium leading-[1.3] text-ink">
            {t.title}
          </h3>

          {t.assignee && (
            <div className="mt-1 flex items-center gap-1.5 text-[11px] text-ink-3">
              <span className="h-1.5 w-1.5 rounded-full bg-[color-mix(in_oklab,var(--color-midnight)_70%,var(--color-ink))]" />
              <span className="font-mono">{t.assignee}</span>
            </div>
          )}

          {isZone && t.childCounts && (
            <ZoneChildCounts counts={t.childCounts} />
          )}
        </div>
        {/* Right-aligned freshness pill. mr-2 on relays so it doesn't sit
            under the absolute bookmark notch at right-3. */}
        <div className={`shrink-0 ${isRelay ? "mr-2" : ""} mt-0.5`}>
          <RelativeTime ts={t.lastModifiedTs} />
        </div>
      </header>

      {expanded && <CardExpanded ticket={t} />}

      <footer className="mt-2 flex items-center justify-end gap-1 border-t border-rule/50 pt-1.5">
        <button
          onClick={handleCopyPath}
          onPointerDown={(e) => e.stopPropagation()}
          title={
            pathCopied
              ? "Copied"
              : `Click to copy: ${t.file}:${t.line}`
          }
          className={`min-w-0 flex-1 truncate rounded px-1 text-left font-mono text-[11px] hover:bg-vellum-2 ${
            pathCopied ? "text-st-review" : "text-ink-3 hover:text-ink-2"
          }`}
        >
          {pathCopied ? "copied" : `${shortPath(t.file)}:${t.line}`}
        </button>
        {expanded && (
          <CardButton title="Open in Architecture">
            <Glyph name="g-arch" size={12} /> graph
          </CardButton>
        )}
        <CardButton title="Open in Agent">
          <Glyph name="g-agent" size={12} /> agent
        </CardButton>
        <CardButton
          title={
            copyState === "copied"
              ? "Prompt copied to clipboard"
              : copyState === "error"
                ? "Clipboard write failed — check browser permissions"
                : isReviewMode
                  ? "Copy review prompt (verify + approve/send-back)"
                  : "Copy pickup prompt"
          }
          onClick={handleCopyPrompt}
          tone={
            copyState === "copied"
              ? "success"
              : copyState === "error"
                ? "danger"
                : "default"
          }
        >
          <Icon name={copyState === "copied" ? "check" : "copy"} size={11} />{" "}
          {copyState === "copied"
            ? "copied"
            : copyState === "error"
              ? "failed"
              : isReviewMode
                ? "review"
                : "prompt"}
        </CardButton>
        {isReviewMode && (
          <CardButton
            title={
              archiveState === "armed"
                ? "Click again to commit — strips @yah: lines from source"
                : archiveState === "error"
                  ? archiveError ?? "Archive failed"
                  : archiveState === "archiving"
                    ? "Archiving…"
                    : "Archive ticket (two-stage: arms first, then commits)"
            }
            onClick={handleArchive}
            tone={
              archiveState === "armed" || archiveState === "error"
                ? "danger"
                : "default"
            }
          >
            <Icon name="archive" size={11} />{" "}
            {archiveState === "armed"
              ? "confirm"
              : archiveState === "archiving"
                ? "…"
                : archiveState === "error"
                  ? "failed"
                  : "archive"}
          </CardButton>
        )}
      </footer>
    </article>
  );
}

function ZoneChildCounts({
  counts,
}: {
  counts: { open: number; active: number; handoff: number };
}) {
  return (
    <div className="mt-1.5 flex items-baseline">
      <CountChip n={counts.open} label="open" hue="open" />
      <CountChip n={counts.active} label="active" hue="active" />
      <CountChip n={counts.handoff} label="handoff" hue="handoff" />
    </div>
  );
}

/* Module-level minute ticker — every mounted RelativeTime subscribes to one
   shared interval so we don't fan out fifty setIntervals across a busy
   board. The interval starts on first subscriber and stops when the last
   unsubscribes. */
const tickSubs = new Set<() => void>();
let tickHandle: number | null = null;

function ensureTicker() {
  if (tickHandle === null && tickSubs.size > 0) {
    tickHandle = window.setInterval(() => {
      tickSubs.forEach((fn) => fn());
    }, 60_000);
  }
}

function useMinuteTick(): void {
  const [, setN] = useState(0);
  useEffect(() => {
    const fn = () => setN((n) => n + 1);
    tickSubs.add(fn);
    ensureTicker();
    return () => {
      tickSubs.delete(fn);
      if (tickSubs.size === 0 && tickHandle !== null) {
        window.clearInterval(tickHandle);
        tickHandle = null;
      }
    };
  }, []);
}

/* Relative time pill — `lastModifiedTs` is unix seconds (not ms) so we
   convert before diffing. The daemon falls back to source mtime when no
   `.yah/events/<shard>.jsonl` exists, so brand-new rigs still get a
   sensible timestamp; a 0 ts means neither was resolvable, hide the pill. */
function RelativeTime({ ts }: { ts?: number }) {
  useMinuteTick();
  if (!ts) return null;
  const ms = ts * 1000;
  return (
    <span
      className="shrink-0 font-mono text-[11px] text-ink-3"
      title={new Date(ms).toLocaleString()}
    >
      {formatRelative(ms)}
    </span>
  );
}

/* Render a rig-relative path as the last two segments — short enough to
   fit in the footer without truncation in the common case, long enough to
   disambiguate (e.g. board/TicketCard.tsx vs shell/TicketCard.tsx). The
   click-handler still copies the full path. */
function shortPath(file: string): string {
  const parts = file.split("/").filter(Boolean);
  if (parts.length <= 2) return file;
  return parts.slice(-2).join("/");
}

function formatRelative(ms: number): string {
  const diff = Math.max(0, Date.now() - ms);
  const m = Math.floor(diff / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}

function CardButton({
  title,
  children,
  onClick,
  tone = "default",
}: {
  title: string;
  children: React.ReactNode;
  onClick?: (e: React.MouseEvent) => void;
  tone?: "default" | "success" | "danger";
}) {
  const toneClass =
    tone === "success"
      ? "text-st-review hover:bg-vellum-2"
      : tone === "danger"
        ? "text-st-bug hover:bg-vellum-2"
        : "text-ink-3 hover:bg-vellum-2 hover:text-ink-2";
  return (
    <button
      title={title}
      onClick={(e) => {
        e.stopPropagation();
        onClick?.(e);
      }}
      onPointerDown={(e) => e.stopPropagation()}
      className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] ${toneClass}`}
    >
      {children}
    </button>
  );
}
