import { useEffect, useState } from "react";
import { getEnv } from "../../env";
import type { HetznerServer } from "../../env/types";
import { Splash } from "../shared/Splash";

interface HetznerServerListProps {
  /** Open an SSH terminal session against `server` and switch to the
   *  Terminal tab. Hidden when the server has no IPv4 to connect to. */
  onOpenTerminal?: (server: HetznerServer) => void;
}

/* Renders the operator's Hetzner project as a flat list. Fetched server-side
   via `hetzner_list_servers` so the API token never reaches the renderer.
   Refetch on demand via the header reload button — the list is small (low
   tens of servers) and the upstream rate limit (3600 req/h) is generous
   enough that polling later if we want to show live status flips is cheap.

   Status colour conventions match the upstream UI hint:
   running → mint, off/stopped → ink-3, anything in-flight (initializing,
   migrating, rebuilding) → accent. Unknown statuses fall through to ink-3
   so a future Hetzner string addition stays readable rather than throwing. */

type LoadState =
  | { kind: "loading" }
  | { kind: "ok"; servers: HetznerServer[] }
  | { kind: "error"; message: string };

const TRANSITIONAL = new Set([
  "initializing",
  "starting",
  "stopping",
  "migrating",
  "rebuilding",
]);

function statusClass(status: string): string {
  if (status === "running") return "text-mint";
  if (TRANSITIONAL.has(status)) return "text-accent";
  return "text-ink-3";
}

export function HetznerServerList({ onOpenTerminal }: HetznerServerListProps) {
  const [state, setState] = useState<LoadState>({ kind: "loading" });
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      setState({ kind: "loading" });
      try {
        const env = await getEnv();
        const servers = await env.rpc.hetzner.listServers();
        if (!cancelled) setState({ kind: "ok", servers });
      } catch (err) {
        if (!cancelled) {
          setState({
            kind: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [generation]);

  const reload = () => setGeneration((g) => g + 1);

  return (
    <div className="flex h-full flex-col">
      <header className="flex items-baseline justify-between border-b border-line px-6 py-3">
        <div className="flex items-baseline gap-3">
          <div className="font-display text-[18px] text-ink-2 [font-variant-caps:all-small-caps]">
            Hetzner servers
          </div>
          {state.kind === "ok" && (
            <div className="text-[12px] text-ink-3">
              {state.servers.length} {state.servers.length === 1 ? "server" : "servers"}
            </div>
          )}
        </div>
        <button
          onClick={reload}
          disabled={state.kind === "loading"}
          className="rounded border border-line px-2 py-1 text-[12px] text-ink-2 hover:bg-paper-2 disabled:opacity-50"
        >
          Reload
        </button>
      </header>

      <div className="flex-1 overflow-auto">
        {state.kind === "loading" && <LoadingPane />}
        {state.kind === "error" && (
          <ErrorPane message={state.message} onRetry={reload} />
        )}
        {state.kind === "ok" && state.servers.length === 0 && <EmptyPane />}
        {state.kind === "ok" && state.servers.length > 0 && (
          <ServerTable servers={state.servers} onOpenTerminal={onOpenTerminal} />
        )}
      </div>
    </div>
  );
}

function ServerTable({
  servers,
  onOpenTerminal,
}: {
  servers: HetznerServer[];
  onOpenTerminal?: (server: HetznerServer) => void;
}) {
  return (
    <table className="w-full border-collapse text-[13px]">
      <thead className="sticky top-0 bg-paper text-left text-[11px] uppercase tracking-wide text-ink-3">
        <tr>
          <th className="px-6 py-2 font-medium">Name</th>
          <th className="px-3 py-2 font-medium">Status</th>
          <th className="px-3 py-2 font-medium">Type</th>
          <th className="px-3 py-2 font-medium">Location</th>
          <th className="px-3 py-2 font-medium">IPv4</th>
          <th className="px-3 py-2 font-medium" />
        </tr>
      </thead>
      <tbody>
        {servers.map((s) => (
          <tr key={s.id} className="border-t border-line hover:bg-paper-2">
            <td className="px-6 py-2 font-medium text-ink">{s.name}</td>
            <td className={`px-3 py-2 ${statusClass(s.status)}`}>{s.status}</td>
            <td className="px-3 py-2 text-ink-2">{s.server_type}</td>
            <td className="px-3 py-2 text-ink-2">{s.location}</td>
            <td className="px-3 py-2 font-mono text-[12px] text-ink-2">
              {s.ipv4 ?? "—"}
            </td>
            <td className="px-3 py-2 text-right">
              {s.ipv4 && onOpenTerminal && s.status === "running" && (
                <button
                  onClick={() => onOpenTerminal(s)}
                  className="rounded border border-line px-2 py-0.5 text-[11px] text-ink-2 hover:border-accent hover:text-accent"
                  title={`ssh root@${s.ipv4}`}
                >
                  ▸ ssh
                </button>
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function LoadingPane() {
  return (
    <div className="flex h-full items-center justify-center text-[13px] text-ink-3">
      Reaching Hetzner…
    </div>
  );
}

function EmptyPane() {
  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-3">
        <Splash
          variant="node"
          caption="No servers in this project"
          sub="Token works — your Hetzner project just doesn't have any servers yet. Provision your first machine to see it here."
        />
      </div>
    </div>
  );
}

function ErrorPane({
  message,
  onRetry,
}: {
  message: string;
  onRetry: () => void;
}) {
  return (
    <div className="flex h-full items-center justify-center px-6">
      <div className="flex max-w-md flex-col items-center gap-3 text-center">
        <Splash
          variant="architecture"
          caption="Couldn't reach Hetzner"
          sub={message}
        />
        <button
          onClick={onRetry}
          className="rounded bg-accent px-3 py-1.5 text-[12px] font-medium text-paper-2 hover:bg-accent-2"
        >
          Try again
        </button>
      </div>
    </div>
  );
}
