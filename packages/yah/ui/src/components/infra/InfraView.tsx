import { useEffect, useState } from "react";
import { getEnv } from "../../env";
import type { HetznerServer } from "../../env/types";
import { HetznerServerList } from "./HetznerServerList";
import { ProvisionForm } from "./ProvisionForm";

interface InfraViewProps {
  /** Open an SSH terminal session for the given server and switch to
   *  the Terminal tab. Owned by App so the tab-state flip and the
   *  module-level terminalStore.open call happen in one place. */
  onOpenTerminal?: (server: HetznerServer) => void;
}

/* Sub-tab strip inside the Infra tab. Servers shows the existing list
   view; Provision is the new server-create form (R029-T3). The default
   sub-tab is decided at mount: empty Hetzner project → land on Provision
   so a fresh operator's first action is "create your first machine"
   instead of staring at an empty list. */

type SubTab = "servers" | "provision";

export function InfraView({ onOpenTerminal }: InfraViewProps) {
  /* `null` while we don't yet know whether the project has servers — the
     mount-time list call drives the choice. We render a thin placeholder
     instead of guessing, so the form doesn't flash before flipping to
     Servers when servers exist. */
  const [subTab, setSubTab] = useState<SubTab | null>(null);
  /* Bumped after a successful create_server. Passed to HetznerServerList
     as `key` so the panel remounts (re-fetches) when we flip to Servers. */
  const [serversRefreshKey, setServersRefreshKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const servers = await env.rpc.hetzner.listServers();
        if (cancelled) return;
        setSubTab(servers.length === 0 ? "provision" : "servers");
      } catch {
        /* The list might fail (token rejected, transport down). Still
           land on Servers — its own error pane explains the failure
           with a Retry. The Provision form would just hit the same
           error a moment later when fetching ssh_keys / on submit. */
        if (!cancelled) setSubTab("servers");
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  if (subTab === null) {
    return (
      <div className="flex h-full items-center justify-center text-[13px] text-ink-3">
        Reaching Hetzner…
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      <SubTabStrip active={subTab} onChange={setSubTab} />
      <div className="min-h-0 flex-1">
        {subTab === "servers" ? (
          <HetznerServerList
            key={serversRefreshKey}
            onOpenTerminal={onOpenTerminal}
          />
        ) : (
          <ProvisionForm
            onCreated={() => {
              setServersRefreshKey((n) => n + 1);
              setSubTab("servers");
            }}
          />
        )}
      </div>
    </div>
  );
}

function SubTabStrip({
  active,
  onChange,
}: {
  active: SubTab;
  onChange: (t: SubTab) => void;
}) {
  return (
    <div className="flex items-stretch border-b border-rule/50 bg-paper-2/20 pl-3">
      <SubTabButton id="servers" label="Servers" active={active} onChange={onChange} />
      <SubTabButton id="provision" label="Provision" active={active} onChange={onChange} />
    </div>
  );
}

function SubTabButton({
  id,
  label,
  active,
  onChange,
}: {
  id: SubTab;
  label: string;
  active: SubTab;
  onChange: (t: SubTab) => void;
}) {
  const isActive = active === id;
  return (
    <button
      onClick={() => onChange(id)}
      className={`relative px-4 py-2 font-display text-[13px] tracking-[0.02em] ${
        isActive ? "text-ink" : "text-ink-3 hover:text-ink-2"
      }`}
    >
      {label}
      {isActive && <span className="absolute inset-x-2 -bottom-px h-px bg-accent" />}
    </button>
  );
}
