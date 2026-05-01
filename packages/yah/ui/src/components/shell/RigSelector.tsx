//! @yah:ticket(R019-T5, "Wire 'Connect remote rig…' menu — modal + lazy SshRpcClient")
//! @yah:assignee(agent:claude)
//! @yah:status(handoff)
//! @yah:phase(P4)
//! @yah:parent(R019)
//! @yah:next("Modal: host, optional port, user, optional key path, remote workspace path")
//! @yah:next("rig_attach for remote stores the spec only; SshRpcClient constructs lazily on first activation")
//! @yah:next("Unblocks once R019-F2 (SshRpcClient) lands. ConnectionStrip's heartbeat already handles transport-failure red state — no extra plumbing.")
//! @arch:see(.yah/arch/authored/rig-backend-dispatch.md)
//! @yah:handoff("Connect-remote-rig modal + spec persistence landed end-to-end; activation guard in place until R019-F2 ships. Backend (app/tauri/src/state.rs:108-180): Rig grew optional host/port/user/keyPath fields (workspace path reuses Rig.path so path_for() stays uniform); RigDto mirrors them and skip_serializing_if=None keeps local rigs JSON-clean. New RemoteRigSpec deserialize struct + RigId::from_remote(user,host,port,workspace) — port defaults to 22 in the hash so blank-vs-22 collide on the same id (test rig_id_from_remote_treats_port_default_as_22). AppState::attach_remote_rig is the same pattern as attach_rig: idempotent, keeps a placeholder KgService so RigEntry shape stays uniform until R019-F3 swaps it for RigBackend dispatch. AppState::kind_for(rig_id) added so commands.rs::arch_open_rig can short-circuit RigKind::Remote with a clear 'Remote rig activation isn\\u00e2\\u20ac\\u2122t wired yet (waiting on R019-F2)' string error rather than walking a remote path that doesn\\u00e2\\u20ac\\u2122t exist on the host. Tauri command rig_attach_remote(spec) registered in lib.rs invoke_handler; boot_registry now match-dispatches local vs remote when reattaching from rigs.json (older entries missing host/user are warn-skipped, not aborted). Renderer (yah-ui): WireRigDto + WireRemoteRigSpec in env/types.ts, rigAttachRemote on Rpc (tauri.ts invokes 'rig_attach_remote', browser stub returns a mock remote dto with reachable=false). Rig type in src/types.ts gained host/port/user/keyPath. App.tsx threads onAttachRemoteRig through TitleBar -> RigSelector. ConnectRemoteRigModal in RigSelector.tsx is a fixed-position overlay form: required host/user/workspacePath, optional port/keyPath/name; Esc cancels; backdrop click cancels; submit awaits rpc.rigAttachRemote then refreshes rigs and selects the new rig. Pill + menu rows now render remote rigs as user@host[:port] via formatRemoteAddress (port omitted when default 22). Verify: cargo build -p yah-tauri green, cargo test -p yah-tauri --lib 7/7 (4 new RigId::from_remote tests), cd yah-ui && bun run typecheck clean, bun run build 1677 modules / 3.70MB.")
//! @yah:next("R019-F2 (SshRpcClient transport) is the unblocker — once it lands, replace the arch_open_rig early-return in app/tauri/src/commands.rs with the lazy-construct path (build SshRpcClient from Rig.host/user/port/key_path on first activation, cache per RigEntry). The placeholder KgService in attach_remote_rig becomes irrelevant at that point — R019-F3 (RigBackend enum) is the deeper refactor that swaps RigEntry.svc for RigBackend so every arch_* command dispatches Local vs Remote uniformly.")
//! @yah:next("Polish (low priority): the modal has no 'Test connection' affordance — once SshRpcClient exists, add a Test button that opens an ephemeral session and checks 'yah serve --stdio' is reachable on the workspace path. Today the user only finds out it works when they activate the rig.")
//! @yah:next("Polish: 'Edit remote rig…' menu item that prefills the modal from the existing rig's spec — RigDto already exposes host/port/user/keyPath, so it's just wiring an edit-mode prop on the modal that calls a yet-to-exist rig_update_remote command. Skip until users actually ask for it.")
//! @yah:verify("cargo build -p yah-tauri")
//! @yah:verify("cargo test -p yah-tauri --lib")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")
//! @yah:gotcha("RigId::from_remote treats port=None and port=Some(22) as the same id (intentional — see test). If you ever change the default port, update the hash too or you'll mint duplicate ids for the same user.")
//! @yah:gotcha("attach_remote_rig still creates a real Arc<KgService> placeholder so RigEntry shape is uniform with local rigs. That's harmless today (arch_open_rig refuses before booting) but R019-F3 will replace svc with RigBackend — when it does, walk the remote attach path to drop the placeholder, otherwise you'll have an unused KgService per remote rig forever.")
//!
//! @yah:ticket(R034-F5, "Rig card identity row + ranked picker; replaces keyPath in remote-rig modal")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P4)
//! @yah:parent(R034)
//! @arch:see(.yah/arch/authored/yah-identities.md)

import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { Menu, MenuItem } from "../shared/Menu";
import type { ConnectionState } from "../../env/hooks";
import type {
  HetznerServer,
  WireIdentity,
  WireRemoteRigSpec,
} from "../../env/types";
import { getEnv } from "../../env";
import {
  findIdentityByKeyPath,
  privateKeyPathForIdentity,
  rankIdentities,
  type IdentityTarget,
  type RankedIdentity,
} from "../../lib/identity-ranking";
import type { Rig } from "../../types";

interface RigSelectorProps {
  rigs: Rig[];
  activeId: string;
  onChange: (id: string) => void;
  /** Live state of the active rig's backend connection. Drives the dot in
   *  the closed pill (green = ok, brass = idle, oxblood = error). Menu rows
   *  for inactive rigs still use the static `reachable` flag. */
  connectionState?: ConnectionState;
  /** Open the native folder picker, attach the chosen path as a new rig,
   *  and select it. App.tsx owns the registry-refresh side effect. */
  onAttachLocal?: () => Promise<void> | void;
  /** Submit a Connect-remote-rig modal payload. App.tsx round-trips
   *  through `rpc.rigAttachRemote` and refreshes the rig list. Throwing
   *  surfaces the error inline in the modal — App.tsx should let
   *  daemon errors propagate so the user sees them. */
  onAttachRemote?: (spec: WireRemoteRigSpec) => Promise<void> | void;
  /** Rigs pinned to the title bar (rendered as quick-switch chips beside
   *  the rig pill). The menu row for each rig gets a pin toggle that flips
   *  membership in this list. */
  pinnedRigIds?: string[];
  onTogglePin?: (id: string) => void;
  maxPinned?: number;
}

/* Render a remote rig as `user@host:port` (port omitted when default).
   Local rigs show their `path` instead, so this is remote-only. */
function formatRemoteAddress(rig: Rig): string {
  if (!rig.host) return "";
  const userPart = rig.user ? `${rig.user}@` : "";
  const portPart = rig.port && rig.port !== 22 ? `:${rig.port}` : "";
  return `${userPart}${rig.host}${portPart}`;
}

/* Rig pill in TitleBar — vellum-tinted button with a candle-pulse status dot
   (forest/brass/oxblood matching the footer ConnectionStrip). Clicking opens
   an anchored Menu listing all known rigs plus connect/open footer items. The
   toolbar dot grows an oxblood pip when *any* attached rig has handoff work
   waiting, and each menu row carries a brass pill with that rig's count. */
export function RigSelector({
  rigs,
  activeId,
  onChange,
  connectionState,
  onAttachLocal,
  onAttachRemote,
  pinnedRigIds,
  onTogglePin,
  maxPinned = 3,
}: RigSelectorProps) {
  const pinnedSet = new Set(pinnedRigIds ?? []);
  const pinSlotsFull = pinnedSet.size >= maxPinned;
  const [open, setOpen] = useState(false);
  const [remoteOpen, setRemoteOpen] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);
  const active = rigs.find((r) => r.id === activeId);

  /* Identity registry — loaded lazily on first menu open so the chrome
     doesn't pay the daemon round-trip on every render. The dropdown
     uses this to label remote rigs with their bound identity (matched
     by keyPath until R034-T6 swaps to rigs.identityId). */
  const [identities, setIdentities] = useState<WireIdentity[]>([]);
  const [identitiesLoaded, setIdentitiesLoaded] = useState(false);
  useEffect(() => {
    if (!open || identitiesLoaded) return;
    let cancelled = false;
    void (async () => {
      const env = await getEnv();
      const ids = await env.rpc.identity.list().catch(() => [] as WireIdentity[]);
      if (cancelled) return;
      setIdentities(ids);
      setIdentitiesLoaded(true);
    })();
    return () => {
      cancelled = true;
    };
  }, [open, identitiesLoaded]);
  const anyAttention = rigs.some((r) => (r.needsAttention ?? 0) > 0);
  const activeState: ConnectionState = active?.reachable === false
    ? "error"
    : connectionState ?? "idle";

  return (
    <div className="relative">
      <button
        ref={ref}
        onClick={() => setOpen((v) => !v)}
        className="flex items-center gap-2 rounded-[5px] bg-vellum/55 px-2 py-1 hover:bg-vellum"
      >
        <RigStateDot state={activeState} pip={anyAttention} />
        <span className="flex items-baseline gap-1">
          <span className="font-display text-[14px] font-medium text-ink">
            {active?.name ?? "no rig"}
          </span>
          {active?.kind === "remote" && active.host && (
            <span className="font-mono text-[10px] text-ink-3">
              {formatRemoteAddress(active)}
            </span>
          )}
        </span>
        <Icon name="chevron-down" size={12} className="text-ink-3" />
      </button>
      <Menu
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={ref}
        width={300}
      >
        <div className="eyebrow px-2 pb-1.5 pt-0.5">Rigs</div>
        {rigs.map((r) => {
          const isPinned = pinnedSet.has(r.id);
          const canPin = isPinned || !pinSlotsFull;
          const boundIdentity =
            r.kind === "remote" ? findIdentityByKeyPath(identities, r.keyPath) : null;
          return (
            <div
              key={r.id}
              className={`group flex w-full items-center gap-2 rounded px-2 py-[7px] ${
                r.id === activeId ? "bg-vellum-2" : "hover:bg-vellum-2/60"
              }`}
            >
              <button
                onClick={() => {
                  onChange(r.id);
                  setOpen(false);
                }}
                className="flex min-w-0 flex-1 items-center gap-2 text-left"
              >
                <RigDot reachable={r.reachable} />
                <div className="min-w-0 flex-1">
                  <div className="font-display text-[13px] font-medium text-ink">
                    {r.name}
                  </div>
                  {(r.host || r.kind === "local") && (
                    <div className="truncate font-mono text-[11px] text-ink-3">
                      {r.kind === "local"
                        ? r.path ?? "local filesystem"
                        : formatRemoteAddress(r)}
                    </div>
                  )}
                  {boundIdentity && (
                    <div className="truncate text-[10px] text-ink-3/80">
                      <span className="[font-variant-caps:all-small-caps]">
                        identity
                      </span>{" "}
                      <span className="font-display">{boundIdentity.name}</span>
                      <span className="text-ink-4"> · {boundIdentity.algorithm}</span>
                    </div>
                  )}
                </div>
                {(r.needsAttention ?? 0) > 0 && (
                  <AttentionPill count={r.needsAttention!} />
                )}
                <span className="text-[10px] text-ink-3 [font-variant-caps:all-small-caps]">
                  {r.kind}
                </span>
              </button>
              {onTogglePin && (
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    if (!isPinned && pinSlotsFull) return;
                    onTogglePin(r.id);
                  }}
                  disabled={!canPin}
                  title={
                    isPinned
                      ? "Unpin from title bar"
                      : pinSlotsFull
                        ? `Pin slots full (max ${maxPinned})`
                        : "Pin to title bar"
                  }
                  className={`shrink-0 rounded p-1 ${
                    isPinned
                      ? "text-accent"
                      : "text-ink-4 opacity-0 group-hover:opacity-100 hover:text-ink-2"
                  } disabled:cursor-not-allowed disabled:opacity-30`}
                >
                  <Icon name="pin" size={11} />
                </button>
              )}
            </div>
          );
        })}
        <div className="my-1.5 border-t border-rule/40" />
        <MenuItem
          leading={<Icon name="plus" size={12} />}
          disabled={!onAttachRemote}
          onClick={() => {
            setOpen(false);
            setRemoteOpen(true);
          }}
        >
          Connect remote rig…
        </MenuItem>
        <MenuItem
          leading={<Icon name="folder" size={12} />}
          onClick={() => {
            setOpen(false);
            void onAttachLocal?.();
          }}
        >
          Open local folder…
        </MenuItem>
      </Menu>
      {remoteOpen && onAttachRemote && (
        <ConnectRemoteRigModal
          onCancel={() => setRemoteOpen(false)}
          onSubmit={async (spec) => {
            await onAttachRemote(spec);
            setRemoteOpen(false);
          }}
        />
      )}
    </div>
  );
}

/* Connect-remote-rig modal — renders as a fixed-position overlay with a
   single form. The renderer never opens the SSH session itself; submit
   round-trips through `rpc.rigAttachRemote`, which stores the spec
   on the daemon. The lazy `SshRpcClient` (R019-F2) is what actually
   connects on first activation, so this modal is intentionally pure
   form: no host probing, no key validation. The "remote rig not yet
   wired" copy at the bottom is the user-facing version of the
   commands.rs::arch_open_rig guard. */
interface ConnectRemoteRigModalProps {
  onCancel: () => void;
  onSubmit: (spec: WireRemoteRigSpec) => Promise<void>;
}

function ConnectRemoteRigModal({ onCancel, onSubmit }: ConnectRemoteRigModalProps) {
  const [host, setHost] = useState("");
  const [user, setUser] = useState("");
  const [port, setPort] = useState("");
  const [workspacePath, setWorkspacePath] = useState("");
  const [name, setName] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const hostInputRef = useRef<HTMLInputElement>(null);

  /* Hetzner inventory: optional shortcut. Picking a server fills
     host / user / name in one click and biases the identity ranking
     toward Hetzner-authorized keys. listServers rejects when the
     Hetzner API key isn't stored — we treat that as "no inventory"
     rather than an error and hide the selector. */
  const [inventory, setInventory] = useState<HetznerServer[]>([]);
  const [selectedServerId, setSelectedServerId] = useState<string>("");

  /* Identity registry: drives the picker that replaces the old
     free-text key-path field. Loaded best-effort — under the browser
     env stub the call returns []. The picker collapses to a single
     "Generate yah-managed key" affordance when the registry is empty,
     so first-time users have one decision instead of three.
     `identityChosenManually` flips on the first explicit pick so
     subsequent inventory changes don't yank the selection out from
     under the user. */
  const [identities, setIdentities] = useState<WireIdentity[]>([]);
  const [selectedIdentityId, setSelectedIdentityId] = useState<string>("");
  const [identityChosenManually, setIdentityChosenManually] = useState(false);

  // Esc cancels; autofocus the first field so users can type immediately.
  useEffect(() => {
    hostInputRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onCancel();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  // Fetch Hetzner inventory + identity registry. Both calls are best-
  // effort: any failure leaves the corresponding picker empty and the
  // form falls back to manual entry / no identity binding.
  const [identitiesLoaded, setIdentitiesLoaded] = useState(false);
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const env = await getEnv();
      const [servers, ids] = await Promise.all([
        env.rpc.hetzner.listServers().catch(() => []),
        env.rpc.identity.list().catch(() => [] as WireIdentity[]),
      ]);
      if (cancelled) return;
      setInventory(servers.filter((s) => s.status === "running" && s.ipv4));
      setIdentities(ids);
      setIdentitiesLoaded(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /* Targets the identity picker should optimize for. Selecting a
     Hetzner server biases ranking toward Hetzner-authorized keys; with
     no server selected, ranking falls back to "yah-generated >
     imported, recency wins" so the registry's preferred key floats up
     even on a hand-typed host. */
  const targets = useMemo<IdentityTarget[]>(
    () => (selectedServerId ? [{ kind: "hetzner" }] : []),
    [selectedServerId],
  );
  const ranked = useMemo<RankedIdentity[]>(
    () => rankIdentities(identities, targets),
    [identities, targets],
  );

  /* Auto-select the top-ranked identity until the user makes their own
     pick. Cleared selections (`""`) re-engage auto-pick because the
     user is signaling "no identity" intentionally; that's a valid
     terminal state, so we don't fight it. */
  useEffect(() => {
    if (identityChosenManually) return;
    if (ranked.length === 0) return;
    const topId = ranked[0].identity.id;
    setSelectedIdentityId((curr) => (curr ? curr : topId));
  }, [ranked, identityChosenManually]);

  function pickIdentity(id: string) {
    setSelectedIdentityId(id);
    setIdentityChosenManually(true);
  }

  function applyServerToForm(serverId: string) {
    setSelectedServerId(serverId);
    if (!serverId) return;
    const server = inventory.find((s) => String(s.id) === serverId);
    if (!server || !server.ipv4) return;
    setHost(server.ipv4);
    setName(server.name);
    setUser((u) => u || "root"); // Hetzner's default; let user override
  }

  async function reloadIdentities(): Promise<WireIdentity[]> {
    const env = await getEnv();
    const ids = await env.rpc.identity.list().catch(() => [] as WireIdentity[]);
    setIdentities(ids);
    return ids;
  }

  async function generateIdentity(label: string): Promise<void> {
    const env = await getEnv();
    const fresh = await env.rpc.identity.create(label);
    await reloadIdentities();
    pickIdentity(fresh.id);
  }

  const selectedIdentity = identities.find((i) => i.id === selectedIdentityId);
  const resolvedKeyPath = selectedIdentity
    ? privateKeyPathForIdentity(selectedIdentity)
    : null;

  const portTrim = port.trim();
  const portNum = portTrim ? Number(portTrim) : null;
  const portInvalid =
    portTrim !== "" && (portNum === null || !Number.isInteger(portNum) || portNum < 1 || portNum > 65535);
  const canSubmit =
    !!host.trim() && !!user.trim() && !!workspacePath.trim() && !portInvalid && !submitting;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      const spec: WireRemoteRigSpec = {
        host: host.trim(),
        user: user.trim(),
        workspacePath: workspacePath.trim(),
      };
      if (portNum !== null) spec.port = portNum;
      // The picker is the one source for keyPath — manual override
      // landed in T6's plan as a follow-up. With no identity selected,
      // we omit keyPath entirely and let ssh-agent / `~/.ssh/id_*`
      // pick it up, mirroring the previous default.
      if (resolvedKeyPath) spec.keyPath = resolvedKeyPath;
      if (name.trim()) spec.name = name.trim();
      await onSubmit(spec);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      setSubmitting(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      onMouseDown={onCancel}
    >
      <form
        onMouseDown={(e) => e.stopPropagation()}
        onSubmit={submit}
        className="w-[420px] rounded-[6px] border border-rule/50 bg-paper-2 p-5 shadow-[0_18px_60px_-12px_rgba(70,45,20,0.4)]"
      >
        <div className="mb-1 font-display text-[15px] font-medium text-ink">
          Connect remote rig
        </div>
        <div className="mb-3 text-[11px] text-ink-3">
          The rig is saved locally; the SSH session opens on first activation.
        </div>
        <div className="flex flex-col gap-2.5">
          {inventory.length > 0 && (
            <Field label="From Hetzner inventory" hint="optional — auto-fills host / key">
              <select
                value={selectedServerId}
                onChange={(e) => applyServerToForm(e.target.value)}
                className="rounded border border-rule/40 bg-vellum/35 px-2 py-1 font-mono text-[12px] text-ink focus:border-accent focus:bg-vellum focus:outline-none"
              >
                <option value="">— Custom (manual entry)</option>
                {inventory.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.name} ({s.ipv4 ?? "—"}) · {s.server_type} · {s.location}
                  </option>
                ))}
              </select>
            </Field>
          )}
          <Field label="Host" required>
            <Input
              inputRef={hostInputRef}
              value={host}
              onChange={setHost}
              placeholder="server.example.com"
            />
          </Field>
          <Field label="User" required>
            <Input value={user} onChange={setUser} placeholder="agent" />
          </Field>
          <div className="grid grid-cols-2 gap-2.5">
            <Field label="Port" hint="default 22">
              <Input
                value={port}
                onChange={setPort}
                placeholder="22"
                inputMode="numeric"
              />
            </Field>
            <Field label="Display name" hint="defaults to host">
              <Input value={name} onChange={setName} placeholder="" />
            </Field>
          </div>
          <IdentityPicker
            ranked={ranked}
            selectedId={selectedIdentityId}
            onPick={pickIdentity}
            onGenerate={generateIdentity}
            ready={identitiesLoaded}
          />
          <Field label="Workspace path" required hint="absolute path on the remote">
            <Input
              value={workspacePath}
              onChange={setWorkspacePath}
              placeholder="/home/agent/projects/foo"
            />
          </Field>
        </div>
        {portInvalid && (
          <div className="mt-2 text-[11px] text-oxblood">
            Port must be a number between 1 and 65535.
          </div>
        )}
        {error && <div className="mt-2 text-[11px] text-oxblood">{error}</div>}
        <div className="mt-4 flex items-center justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={submitting}
            className="rounded px-2.5 py-1 text-[12px] text-ink-2 hover:bg-vellum disabled:pointer-events-none disabled:opacity-40"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!canSubmit}
            className="rounded bg-accent px-2.5 py-1 text-[12px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
          >
            {submitting ? "Saving…" : "Save rig"}
          </button>
        </div>
      </form>
    </div>
  );
}

/* Picker that replaces the old free-text Key path field. Surfaces the
   top three ranked identities (per yah-identities.md §UX) plus a
   "Generate yah-managed key…" affordance that calls identity_create
   inline — one decision for first-time users who have nothing in the
   registry yet. The picker derives the on-disk key path from the
   selected identity's source (`privateKeyPathForIdentity`), so the
   submit payload still carries `keyPath` until R034-T6 swaps the
   field for `identityId`. */
interface IdentityPickerProps {
  ranked: RankedIdentity[];
  selectedId: string;
  onPick: (id: string) => void;
  onGenerate: (name: string) => Promise<void>;
  /** False until the first identity_list resolves. The picker shows a
   *  faint "Loading…" placeholder until then so a slow daemon roundtrip
   *  doesn't flash the empty-state copy. */
  ready: boolean;
}

function IdentityPicker({ ranked, selectedId, onPick, onGenerate, ready }: IdentityPickerProps) {
  const [showAll, setShowAll] = useState(false);
  const [generateOpen, setGenerateOpen] = useState(false);
  const [generateName, setGenerateName] = useState("");
  const [generating, setGenerating] = useState(false);
  const [generateError, setGenerateError] = useState<string | null>(null);

  const top = ranked.slice(0, 3);
  const overflow = Math.max(0, ranked.length - top.length);
  const visible = showAll ? ranked : top;

  async function submitGenerate() {
    const trimmed = generateName.trim();
    if (!trimmed) return;
    setGenerating(true);
    setGenerateError(null);
    try {
      await onGenerate(trimmed);
      setGenerateOpen(false);
      setGenerateName("");
    } catch (err) {
      setGenerateError(err instanceof Error ? err.message : String(err));
    } finally {
      setGenerating(false);
    }
  }

  return (
    <Field
      label="Identity"
      hint={
        ranked.length === 0
          ? "ssh-agent / ~/.ssh/id_* unless you generate one"
          : "ranked by coverage of the selected target"
      }
    >
      <div className="flex flex-col gap-1">
        {!ready && (
          <div className="text-[11px] text-ink-3">Loading identities…</div>
        )}
        {ready && ranked.length === 0 && !generateOpen && (
          <div className="rounded border border-rule/40 bg-vellum/40 px-2 py-1.5 text-[11px] text-ink-3">
            No identities registered yet. Generate one below or skip — yah
            will fall back to ssh-agent / <code>~/.ssh/id_*</code>.
          </div>
        )}
        {visible.map((r) => (
          <IdentityPickerRow
            key={r.identity.id}
            ranked={r}
            selected={r.identity.id === selectedId}
            onPick={() => onPick(r.identity.id)}
          />
        ))}
        {!showAll && overflow > 0 && (
          <button
            type="button"
            onClick={() => setShowAll(true)}
            className="self-start rounded px-1 py-0.5 text-[11px] text-ink-3 hover:bg-vellum/60 hover:text-ink-2"
          >
            Show {overflow} more…
          </button>
        )}
        {selectedId && (
          <button
            type="button"
            onClick={() => onPick("")}
            className="self-start rounded px-1 py-0.5 text-[10px] text-ink-4 hover:text-ink-3"
          >
            Use ssh-agent / ~/.ssh/id_* instead
          </button>
        )}
        {!generateOpen && (
          <button
            type="button"
            onClick={() => setGenerateOpen(true)}
            className="flex items-center gap-1 self-start rounded px-1 py-0.5 text-[11px] text-accent hover:bg-vellum/60"
          >
            <Icon name="plus" size={10} />
            Generate yah-managed key…
          </button>
        )}
        {generateOpen && (
          <div className="rounded border border-rule/40 bg-vellum/40 px-2 py-1.5">
            <div className="mb-1 text-[10px] text-ink-3">
              Creates an ed25519 keypair under{" "}
              <code className="font-mono">~/.yah/keys/&lt;name&gt;</code> and
              auto-selects it.
            </div>
            <div className="flex items-center gap-1.5">
              <input
                autoFocus
                value={generateName}
                onChange={(e) => setGenerateName(e.target.value)}
                placeholder="e.g. yah-personal"
                spellCheck={false}
                disabled={generating}
                className="min-w-0 flex-1 rounded border border-rule/40 bg-paper-2 px-1.5 py-0.5 font-mono text-[11px] text-ink outline-none focus:border-accent/60 disabled:opacity-50"
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    void submitGenerate();
                  }
                }}
              />
              <button
                type="button"
                onClick={() => {
                  setGenerateOpen(false);
                  setGenerateName("");
                  setGenerateError(null);
                }}
                disabled={generating}
                className="rounded px-1 py-0.5 text-[11px] text-ink-3 hover:text-ink-2 disabled:opacity-40"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void submitGenerate()}
                disabled={!generateName.trim() || generating}
                className="rounded bg-accent px-1.5 py-0.5 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
              >
                {generating ? "Generating…" : "Generate"}
              </button>
            </div>
            {generateError && (
              <div className="mt-1 text-[10px] text-oxblood">✗ {generateError}</div>
            )}
          </div>
        )}
      </div>
    </Field>
  );
}

function IdentityPickerRow({
  ranked,
  selected,
  onPick,
}: {
  ranked: RankedIdentity;
  selected: boolean;
  onPick: () => void;
}) {
  const { identity, tier, coveredCount } = ranked;
  const sourceLabel = identity.source.kind === "yahGenerated" ? "yah-managed" : "imported";
  const tierBadge =
    tier === 1
      ? { text: "covers all", className: "bg-forest/20 text-forest" }
      : tier === 2
      ? { text: `covers ${coveredCount}`, className: "bg-brass/25 text-ink-2" }
      : tier === 3
      ? { text: "greenfield", className: "bg-accent/15 text-ink-2" }
      : { text: "imported", className: "bg-vellum/60 text-ink-3" };
  return (
    <button
      type="button"
      onClick={onPick}
      className={`flex items-center gap-2 rounded border px-2 py-1 text-left transition-colors ${
        selected
          ? "border-accent bg-vellum"
          : "border-rule/40 bg-paper-2 hover:bg-vellum/60"
      }`}
    >
      <span
        className={`inline-flex h-3 w-3 shrink-0 items-center justify-center rounded-full border ${
          selected ? "border-accent bg-accent" : "border-ink-3/40"
        }`}
      >
        {selected && <span className="h-1.5 w-1.5 rounded-full bg-paper-2" />}
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex items-baseline gap-1.5">
          <span className="truncate font-display text-[12px] text-ink">
            {identity.name}
          </span>
          <span className="text-[10px] text-ink-3">{sourceLabel}</span>
        </span>
        <span className="block truncate font-mono text-[10px] text-ink-3">
          {identity.algorithm} · {identity.id}
        </span>
      </span>
      <span
        className={`shrink-0 rounded px-1 py-0.5 text-[9px] uppercase tracking-wide ${tierBadge.className}`}
      >
        {tierBadge.text}
      </span>
    </button>
  );
}

interface FieldProps {
  label: string;
  required?: boolean;
  hint?: string;
  children: React.ReactNode;
}

function Field({ label, required, hint, children }: FieldProps) {
  return (
    <label className="flex flex-col gap-1">
      <span className="flex items-baseline gap-1.5 text-[11px] text-ink-3">
        <span className="[font-variant-caps:all-small-caps]">{label}</span>
        {required && <span className="text-oxblood">*</span>}
        {hint && <span className="text-[10px] text-ink-4">— {hint}</span>}
      </span>
      {children}
    </label>
  );
}

interface InputProps {
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  inputMode?: React.HTMLAttributes<HTMLInputElement>["inputMode"];
  inputRef?: React.RefObject<HTMLInputElement | null>;
}

function Input({ value, onChange, placeholder, inputMode, inputRef }: InputProps) {
  return (
    <input
      ref={inputRef}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      inputMode={inputMode}
      className="rounded border border-rule/40 bg-vellum/35 px-2 py-1 font-mono text-[12px] text-ink placeholder:text-ink-4 focus:border-accent focus:bg-vellum focus:outline-none"
    />
  );
}

function RigDot({ reachable, pip = false }: { reachable: boolean; pip?: boolean }) {
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      <span
        className={`h-2 w-2 rounded-full ${
          reachable
            ? "bg-forest shadow-[0_0_0_2px_color-mix(in_oklab,var(--color-forest)_25%,transparent)] candle"
            : "bg-oxblood"
        }`}
      />
      {pip && (
        <span
          aria-label="rigs with attention"
          className="absolute -right-0.5 -top-0.5 h-[6px] w-[6px] rounded-full bg-oxblood shadow-[0_0_0_1.5px_var(--color-paper-2)]"
        />
      )}
    </span>
  );
}

/* Tri-state variant for the active-rig pill: green/brass/oxblood mirroring
   the footer ConnectionStrip. Only `ok` gets the candle-pulse halo; idle
   and error are static so they don't read as "live but tinted". */
function RigStateDot({
  state,
  pip = false,
}: {
  state: ConnectionState;
  pip?: boolean;
}) {
  const dotClass =
    state === "ok"
      ? "bg-forest shadow-[0_0_0_2px_color-mix(in_oklab,var(--color-forest)_25%,transparent)] candle"
      : state === "idle"
      ? "bg-brass"
      : "bg-oxblood";
  return (
    <span className="relative inline-flex h-2 w-2 shrink-0">
      <span className={`h-2 w-2 rounded-full ${dotClass}`} />
      {pip && (
        <span
          aria-label="rigs with attention"
          className="absolute -right-0.5 -top-0.5 h-[6px] w-[6px] rounded-full bg-oxblood shadow-[0_0_0_1.5px_var(--color-paper-2)]"
        />
      )}
    </span>
  );
}

/* Quick-switch chip for a pinned rig — sits next to the main rig pill in
   TitleBar. Smaller than the active pill (no chevron, monospace name) so
   the active rig still reads as primary. Hover surfaces an unpin X. */
export function PinnedRigChip({
  rig,
  onSelect,
  onUnpin,
}: {
  rig: Rig;
  onSelect: () => void;
  onUnpin: () => void;
}) {
  return (
    <div className="group relative flex items-center">
      <button
        onClick={onSelect}
        title={
          rig.kind === "local"
            ? (rig.path ?? rig.name)
            : (rig.host ?? rig.name)
        }
        className="flex items-center gap-1.5 rounded-l-[5px] bg-vellum/35 px-2 py-1 hover:bg-vellum/70"
      >
        <RigDot reachable={rig.reachable} />
        <span className="max-w-[120px] truncate font-display text-[12px] text-ink-2 group-hover:text-ink">
          {rig.name}
        </span>
        {(rig.needsAttention ?? 0) > 0 && (
          <AttentionPill count={rig.needsAttention!} />
        )}
      </button>
      <button
        onClick={(e) => {
          e.stopPropagation();
          onUnpin();
        }}
        title="Unpin from title bar"
        className="self-stretch rounded-r-[5px] border-l border-rule/30 bg-vellum/35 px-1 text-ink-4 opacity-0 hover:bg-vellum/70 hover:text-ink group-hover:opacity-100"
      >
        <Icon name="x" size={10} />
      </button>
    </div>
  );
}

function AttentionPill({ count }: { count: number }) {
  return (
    <span
      title={`${count} item${count === 1 ? "" : "s"} awaiting attention`}
      className="inline-flex h-[16px] min-w-[16px] items-center justify-center rounded-full bg-brass px-1 font-mono text-[10px] font-semibold leading-none text-paper-2"
    >
      {count}
    </span>
  );
}
