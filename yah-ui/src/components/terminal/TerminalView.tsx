import { useEffect, useMemo, useRef } from "react";
import { Splash } from "../shared/Splash";
import { terminalStore, useTerminalStore } from "./terminal-store";
import type { TerminalSession } from "./terminal-store";

import "@xterm/xterm/css/xterm.css";

interface TerminalViewProps {
  /** Cmd-clicking a recognized file path in any pane routes here. */
  onJumpToFile: (fileColon: string) => void;
  /** Working directory new local shells should spawn into. Threaded
   *  in from App so the active rig's `path` becomes the default cwd —
   *  saves the operator a `cd` on every new shell. Falls back to the
   *  shell's own default ($HOME) when null/undefined. */
  defaultCwd?: string | null;
  /** Currently focused rig. Sessions are scoped per-rig: the rail and
   *  visible panes only show sessions whose `session.rigId` matches.
   *  Switching rigs hides (but doesn't dispose) the old rig's
   *  sessions; switching back surfaces them with their scrollback
   *  intact. New sessions opened from this view inherit this rigId. */
  rigId?: string | null;
}

/* In-app SSH terminal pane.
 *
 * Layout: a left rail listing live sessions, and a single visible
 * xterm pane on the right driven by the active session. Switching
 * sessions re-attaches the existing Terminal to the host element —
 * the Terminal itself outlives React mounts, so scrollback survives
 * tab flips.
 *
 * No "open" button here yet; sessions land via the Hetzner server
 * list (R030-T3) or any future entry point that calls
 * `terminalStore.open(...)`. */
export function TerminalView({
  onJumpToFile,
  defaultCwd,
  rigId,
}: TerminalViewProps) {
  const { sessions, activeId } = useTerminalStore();
  const ownerRig = rigId ?? null;

  /* Per-rig active-session memory. When the operator switches rigs,
     restore the session that was last active for the new rig (or
     null if none). Without this every rig switch would clobber the
     other rig's selection. Stored in a ref so it survives renders
     without triggering them. */
  const lastActiveByRigRef = useRef<Map<string | "null", string | null>>(new Map());

  /* On rigId change: remember the current activeId for the previous
     rig (only if it actually belonged to that rig), then restore the
     last-active session for the new rig. */
  const prevRigKeyRef = useRef<string | "null">(ownerRig ?? "null");
  useEffect(() => {
    const newKey = ownerRig ?? "null";
    const prevKey = prevRigKeyRef.current;
    if (prevKey !== newKey) {
      // Persist the previous rig's selection before switching.
      const currentActive = activeId
        ? sessions.find((s) => s.id === activeId)
        : null;
      if (currentActive && (currentActive.rigId ?? "null") === prevKey) {
        lastActiveByRigRef.current.set(prevKey, currentActive.id);
      }
      // Restore (or clear) for the new rig.
      const restore = lastActiveByRigRef.current.get(newKey) ?? null;
      const restoreSession = restore
        ? sessions.find((s) => s.id === restore)
        : null;
      terminalStore.setActive(restoreSession ? restore : null);
      prevRigKeyRef.current = newKey;
    }
  }, [ownerRig, activeId, sessions]);

  /* Sessions belonging to the current rig — what the rail + visible
     panes render against. Sessions from other rigs stay alive (still
     consuming PTY bytes into scrollback) and just sit hidden in the
     stack of layered panes. */
  const visibleSessions = useMemo(
    () => sessions.filter((s) => (s.rigId ?? null) === ownerRig),
    [sessions, ownerRig],
  );

  /* Wire the cmd-click router so the store knows where to send file
     targets. Doing it here (not in App.tsx) keeps the dependency
     direction one-way: TerminalView → store, never the reverse. */
  useEffect(() => {
    terminalStore.setLinkHandler(onJumpToFile);
    return () => {
      terminalStore.setLinkHandler(null);
    };
  }, [onJumpToFile]);

  return (
    <div className="flex h-full">
      <SessionRail
        sessions={visibleSessions}
        activeId={activeId}
        defaultCwd={defaultCwd}
        rigId={ownerRig}
      />
      {/* All sessions (including hidden, other-rig ones) render into
          their own dedicated hosts stacked in the same area. The
          per-pane `visible` flag combines `s.id === activeId` AND
          rig match so other-rig sessions stay opacity:0/zIndex:0
          when this rig is active — they keep consuming PTY bytes in
          the background so a switch back surfaces fresh state. */}
      <div className="relative min-w-0 flex-1">
        {visibleSessions.length === 0 && (
          <EmptyPane defaultCwd={defaultCwd} rigId={ownerRig} />
        )}
        {sessions.map((s) => (
          <SessionPane
            key={s.id}
            session={s}
            visible={
              s.id === activeId && (s.rigId ?? null) === ownerRig
            }
          />
        ))}
      </div>
    </div>
  );
}

function SessionRail({
  sessions,
  activeId,
  defaultCwd,
  rigId,
}: {
  sessions: TerminalSession[];
  activeId: string | null;
  defaultCwd?: string | null;
  rigId?: string | null;
}) {
  return (
    <div className="flex w-56 flex-col border-r border-rule/50 bg-paper-2/20">
      <div className="flex items-center justify-between border-b border-rule/50 px-3 py-2">
        <span className="font-display text-[11px] font-medium uppercase tracking-wide text-ink-3">
          Sessions
        </span>
        <button
          onClick={() => {
            void terminalStore
              .openLocal({ cwd: defaultCwd ?? undefined }, { rigId })
              .catch((err) => {
                console.warn("[terminal] openLocal failed", err);
              });
          }}
          className="rounded border border-rule/50 px-1.5 py-0.5 text-[11px] text-ink-3 transition-colors hover:border-accent hover:text-accent"
          title={defaultCwd ? `New local shell in ${defaultCwd}` : "New local shell"}
        >
          + shell
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        {sessions.length === 0 ? (
          <div className="px-3 py-3 text-[12px] text-ink-3">
            No sessions yet — open one from the Infra tab, or click
            <span className="mx-1 font-mono">+ shell</span>
            above for a local PTY.
          </div>
        ) : (
          sessions.map((s) => (
            <SessionRow key={s.id} session={s} active={s.id === activeId} />
          ))
        )}
      </div>
    </div>
  );
}

function SessionRow({
  session,
  active,
}: {
  session: TerminalSession;
  active: boolean;
}) {
  return (
    <div
      onClick={() => terminalStore.setActive(session.id)}
      className={`group flex cursor-pointer items-center gap-2 border-b border-rule/30 px-3 py-2 text-[12px] ${
        active ? "bg-paper-2/60 text-ink" : "text-ink-2 hover:bg-paper-2/30"
      }`}
    >
      <StatusDot status={session.status} />
      <div className="min-w-0 flex-1">
        <div className="truncate font-medium">{session.label}</div>
        <div className="truncate text-[10px] text-ink-3">
          {statusText(session)}
        </div>
      </div>
      <button
        onClick={(e) => {
          e.stopPropagation();
          void terminalStore.close(session.id);
        }}
        className="opacity-0 transition-opacity group-hover:opacity-100"
        title="Close session"
      >
        <span className="text-[14px] text-ink-3 hover:text-oxblood">×</span>
      </button>
    </div>
  );
}

function StatusDot({ status }: { status: TerminalSession["status"] }) {
  const cls =
    status === "ready"
      ? "bg-mint"
      : status === "connecting"
        ? "bg-accent animate-pulse"
        : status === "error"
          ? "bg-oxblood"
          : "bg-ink-3";
  return <span className={`h-2 w-2 rounded-full ${cls}`} />;
}

function statusText(s: TerminalSession): string {
  if (s.status === "ready") return s.host;
  if (s.status === "connecting") return "connecting…";
  if (s.status === "error") return `error: ${s.statusDetail ?? "unknown"}`;
  return `closed (${s.statusDetail ?? "—"})`;
}

function SessionPane({
  session,
  visible,
}: {
  session: TerminalSession;
  visible: boolean;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  /* Attach this session's Terminal into our dedicated host once.
     Because each session has its own SessionPane (and therefore its
     own host element), `terminalStore.attach` is only called once
     per session — see the architecture note in TerminalView above. */
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    terminalStore.attach(session.id, host);

    /* ResizeObserver triggers a fit pass whenever the pane's
       bounding box changes — sidebar collapse, window resize,
       split-view drags. */
    const ro = new ResizeObserver(() => {
      terminalStore.fit(session.id);
    });
    ro.observe(host);

    return () => {
      ro.disconnect();
      terminalStore.detach(session.id);
    };
  }, [session.id]);

  /* On becoming visible: re-fit (host size may have drifted while
     hidden), force a full repaint (xterm's DOM renderer skips draw
     when it believes nothing changed, which leaves the pane blank
     after returning from an opacity:0 stretch), and focus so the
     user can type immediately. The rAF gate ensures the layout-
     changes-from-opacity-flip have settled before we measure +
     repaint. */
  useEffect(() => {
    if (!visible) return;
    let cancelled = false;
    requestAnimationFrame(() => {
      if (cancelled) return;
      terminalStore.fit(session.id);
      session.term.refresh(0, session.term.rows - 1);
      session.term.focus();
    });
    return () => {
      cancelled = true;
    };
  }, [visible, session]);

  return (
    <div
      className="absolute inset-0 flex flex-col bg-[#0f0f0f]"
      style={{
        opacity: visible ? 1 : 0,
        pointerEvents: visible ? "auto" : "none",
        zIndex: visible ? 1 : 0,
      }}
      aria-hidden={!visible}
    >
      <div className="flex items-center gap-3 border-b border-[#2a2a2a] bg-[#1a1a1a] px-3 py-1.5 text-[11px] text-[#a0a0a0]">
        <StatusDot status={session.status} />
        <span className="font-medium text-[#e6e6e6]">{session.label}</span>
        <span className="text-[#5a5a5a]">·</span>
        <span>{statusText(session)}</span>
        {session.hostKeyFingerprint && (
          <>
            <span className="text-[#5a5a5a]">·</span>
            <span
              className="font-mono"
              title={`server fingerprint: ${session.hostKeyFingerprint}`}
            >
              {shortFp(session.hostKeyFingerprint)}
            </span>
          </>
        )}
      </div>
      <div className="relative min-h-0 flex-1">
        <div ref={hostRef} className="absolute inset-0" />
        {session.status === "connecting" && (
          <ConnectingOverlay session={session} />
        )}
      </div>
    </div>
  );
}

/* Spinner over the (already-attached) xterm canvas while the backend
   handshake is in flight. Sits in the same stacking layer as the
   host so xterm's text doesn't bleed through; pointer-events stay
   off so the keystroke pump still works the moment we transition to
   ready (the operator can start typing into the still-overlaid pane
   if the cursor is already focused). */
function ConnectingOverlay({ session }: { session: TerminalSession }) {
  return (
    <div
      className="pointer-events-none absolute inset-0 flex items-center justify-center bg-[#0f0f0f]/85"
      aria-live="polite"
    >
      <div className="flex flex-col items-center gap-3">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-[#5a5a5a] border-t-[#d4a017]" />
        <div className="font-mono text-[12px] text-[#a0a0a0]">
          {session.statusDetail ?? "connecting…"}
        </div>
      </div>
    </div>
  );
}

function shortFp(fp: string): string {
  const colon = fp.indexOf(":");
  const tail = colon === -1 ? fp : fp.slice(colon + 1);
  return `…${tail.slice(-12)}`;
}

function EmptyPane({
  defaultCwd,
  rigId,
}: {
  defaultCwd?: string | null;
  rigId?: string | null;
}) {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="font-display text-[24px] text-ink-2 [font-variant-caps:all-small-caps]">
          terminal
        </div>
        <Splash
          variant="camp"
          caption="No fires lit"
          sub="Open a server from the Infra tab to start an SSH session here."
        />
        <button
          onClick={() => {
            void terminalStore
              .openLocal({ cwd: defaultCwd ?? undefined }, { rigId })
              .catch((err) => {
                console.warn("[terminal] openLocal failed", err);
              });
          }}
          className="rounded border border-rule/50 bg-paper-2/40 px-3 py-1.5 text-[12px] text-ink-2 transition-colors hover:bg-paper-2/70"
          title={defaultCwd ? `Spawn a local shell in ${defaultCwd}` : "Spawn a local shell PTY"}
        >
          + open local shell
        </button>
      </div>
    </div>
  );
}
