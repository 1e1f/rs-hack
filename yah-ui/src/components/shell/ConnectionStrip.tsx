//! @yah:ticket(R015-F4, "ConnectionStrip: Live/Stale/Offline derived from time since last IndexFinished")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R015)
//! @yah:next("Track t_last_index in a hook listening on arch:event index_finished")
//! @yah:next("Live <5s, Stale 5-30s, Offline >30s or no events ever")
//! @yah:handoff("ConnectionStrip landed end-to-end. New file yah-ui/src/components/shell/ConnectionStrip.tsx hosts the @yah ticket and renders a slim chrome bar (h-7 border-b under TitleBar) that returns null on 'live' and slides in for 'stale'/'offline'. New useConnectionStatus() hook in yah-ui/src/env/hooks.ts stamps t_last_index from useArchEvents(_, 'index_finished'); a 1s setInterval re-derives age so the badge demotes Live→Stale→Offline without waiting on a fresh event. Thresholds match the ticket: <5s live, 5-30s stale, >=30s or null offline. State dot uses --color-brass for stale and --color-oxblood for offline; copy reads 'last sync 12s ago' (or 'rig unreachable — no index events received yet' on null). App.tsx mounts <ConnectionStrip /> between TitleBar and TabStrip. Ticket annotation moved off ArchView.tsx onto the new component file. typecheck clean (only pre-existing serve.ts errors under R015-T6); bun build src/main.tsx --target=browser clean (1675 modules, 7.32MB).")
//! @yah:next("End-to-end smoke under Tauri: YAH_RIG_ROOT=/Users/leif/ss/rs-hack cargo run -p yah-tauri — strip should render 'offline' on cold boot, flip to 'live' after first index_finished, demote to 'stale' after 5s of idle, return to 'offline' after 30s.")
//! @yah:next("Browser dev (bun run dev): browser env adapter never emits index_finished, so strip stays in 'offline'. Confirm copy reads 'rig unreachable — no index events received yet' and the strip doesn't crowd the layout.")
//! @yah:next("Consider piping a manual reindex retry button into the offline state — design doc shows '[retry]' affordance at architecture/yah-ui-implementation-guide.md:377.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun build src/main.tsx --outdir /tmp/_yahui_R015_F4 --target=browser")
//! @yah:gotcha("useConnectionStatus uses a 1s interval to re-derive state from t_last_index — fine for a single instance under TitleBar, but don't blanket the tree with this hook (each instance ticks independently).")
//! @yah:assumes("ArchEvent stream is the right truth-source for 'connection healthy'. If a future SSE-only error path (e.g. transport disconnect with no events) needs to flip the dot independently of t_last_index, this hook needs an explicit error channel from env.rpc.onEvent.")

import type { ConnectionState, ConnectionStatus } from "../../env/hooks";

/* Always-mounted footer status tray. The leftmost dot surfaces backend
   reachability (green/yellow/red); the rest of the bar is reserved for
   future agent/status widgets. Mounts unconditionally so the layout above
   it doesn't reflow when state flips. Status is lifted to <App> so the
   title-bar rig dot and this footer share one heartbeat. */
export function ConnectionStrip({ status }: { status: ConnectionStatus }) {
  return (
    <div
      className="flex h-7 shrink-0 items-center gap-2 border-t border-rule/50 bg-paper-2/40 px-3 text-[11px] text-ink-2"
      role="status"
      aria-live="polite"
    >
      <StateDot state={status.state} title={describe(status)} />
    </div>
  );
}

function StateDot({ state, title }: { state: ConnectionState; title: string }) {
  const color =
    state === "ok"
      ? "var(--color-forest)"
      : state === "idle"
      ? "var(--color-brass)"
      : "var(--color-oxblood)";
  return (
    <span
      className="inline-block h-2 w-2 rounded-full"
      style={{ background: color }}
      title={title}
      aria-label={title}
    />
  );
}

function describe(status: ConnectionStatus): string {
  if (status.state === "error") return "connection error";
  if (status.lastOkAt === null) return "connecting…";
  const seconds = Math.max(0, Math.floor((Date.now() - status.lastOkAt) / 1000));
  return status.state === "ok"
    ? `synced ${formatAge(seconds)} ago`
    : `idle ${formatAge(seconds)}`;
}

function formatAge(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h`;
}
