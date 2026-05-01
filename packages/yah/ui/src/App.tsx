//! @yah:ticket(R027-F5, "Infra tab gate on api_key_has('hetzner') + nudge-to-Settings empty state (replaces COMING_SOON splash)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P3)
//! @yah:parent(R027)
//!
//! @yah:ticket(R034-F4, "Settings \\u2192 Identities section: list/generate/import/delete UI + probe results")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P4)
//! @yah:parent(R034)
//! @arch:see(.yah/arch/authored/yah-identities.md)
//! @yah:handoff("Settings \\u2192 Identities lands. New env surface: WireIdentity / WireIdentitySource / WireAuthorization / WireProbeReport / WireProbeOutcome / WireSingleProbeResult in yah-ui/src/env/types.ts mirroring identities::* (camelCase + tagged kind). IdentityRpc on Rpc with list/create/import/remove/probeAll/probeHetzner/probeGithub/authorizeHetzner/deauthorizeHetzner/authorizeGithub/deauthorizeGithub. tauri.ts invokes the matching identity_* commands; browser.ts returns two fixed mock identities (one yah-managed + one imported, with Hetzner+GitHub auth on the first) so the section is inspectable via dev-server, mutations reject loudly. New component yah-ui/src/components/shell/IdentitiesSection.tsx renders one row per identity (name + source badge + algo + fingerprint + path), Generate (identity_create) / Import existing (identity_import with optional name override) inline forms, per-row Delete with two-click confirm (label switches to 'Confirm + delete keyfile' for yahGenerated to flag the disk side effect), per-row authorizedAt list with describeAuthorization() for hetzner/github/gitlab/sshHost variants + relative last-seen, top-bar Refresh probes button calling identity_probe_all with a banner that shows each provider distinctly (ok matches count / skipped reason / error reason), Browser preview banner when envKind === 'browser'. SettingsModal grew an 'identities' section in SECTIONS + a wrapper that pulls envKind from the api-keys context. Verify: cd yah-ui && bun run typecheck (clean); bun run build (1699 modules, 4.23MB); cargo build -p yah-tauri (clean).")
//! @yah:next("Per-row 'Authorize Hetzner' / 'Authorize GitHub' buttons exposing identity_authorize_* (P3 surface already wired through IdentityRpc). Out of F4 scope but a small follow-up — current design surfaces the authorized state but not the button to fix it.")
//! @yah:next("Per-row 'Re-check' that calls identity_probe_hetzner / identity_probe_github for that one identity. Useful when one stale lastSeen matters more than a full fan-out probe; both are already on the IdentityRpc surface.")
//! @yah:next("File-picker integration: the Import form takes a typed path today. Wire a Tauri file-picker so the user can browse to ~/.ssh/id_*.pub instead of pasting; browser preview keeps the typed-path fallback.")
//! @yah:gotcha("identity_authorize_hetzner / identity_authorize_github commands are wired through to the renderer but no UI button calls them yet \\u2014 the section intentionally surfaces only the read view of authorizations, not the write. F5 (rig card) or a follow-up adds the button.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")
//! @yah:verify("cargo build -p yah-tauri")

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { TitleBar } from "./components/shell/TitleBar";
import { TabStrip, TAB_ORDER } from "./components/shell/TabStrip";
import { ConnectionStrip } from "./components/shell/ConnectionStrip";
import {
  ApiKeysProvider,
  useApiKeys,
  type ApiKeyProvider,
} from "./components/shell/api-keys-context";
import { SettingsView } from "./components/shell/SettingsView";
import { useConnectionStatus, useValidate } from "./env/hooks";
import { Board } from "./components/board/Board";
import { ArchView } from "./components/arch/ArchView";
import { AgentView } from "./components/agent/AgentView";
import { InfraView } from "./components/infra/InfraView";
import { TerminalView } from "./components/terminal/TerminalView";
import { terminalStore } from "./components/terminal/terminal-store";
import { FilesView } from "./components/files/FilesView";
import type { HetznerServer } from "./env/types";
import { Splash, type SplashVariant } from "./components/shared/Splash";
import { getEnv } from "./env";
import { workItemToTicket } from "./env/mapper";
import type { WireRemoteRigSpec, WireRigDto } from "./env/types";
import { withDerivedRelayFields } from "./lib/relay-status";
import { mockTickets } from "./mock";
import type { Rig, Tab, Theme, Ticket } from "./types";

/* WireRigDto → renderer Rig. The wire shape carries `path` + `lastActiveAt`
   that the selector renders directly; the runtime `Rig` type also makes
   `host` optional (only populated for remote rigs, which the daemon
   doesn't yet emit — RigKind::Remote is reserved for SSH-RPC). */
function wireToRig(w: WireRigDto): Rig {
  return {
    id: w.id,
    name: w.name,
    kind: w.kind,
    path: w.path,
    host: w.host,
    port: w.port,
    user: w.user,
    keyPath: w.keyPath,
    reachable: w.reachable,
    lastActiveAt: w.lastActiveAt ?? undefined,
  };
}

/* Synthetic rig that surfaces the bundled mock data — only added when the
   user has explicitly opted in via the localStorage flag below. Selecting
   it short-circuits the backend fetch and feeds <Board> from `mockTickets`
   so the UI can be exercised without a daemon. To enable from devtools:
     localStorage.setItem("yah-ui:enable-example-rig", "1"); location.reload(); */
const EXAMPLE_RIG_ID = "__example__";
const EXAMPLE_FLAG_KEY = "yah-ui:enable-example-rig";
const EXAMPLE_RIG: Rig = {
  id: EXAMPLE_RIG_ID,
  name: "example rig",
  kind: "local",
  path: "(bundled demo data)",
  reachable: true,
};

function exampleRigEnabled(): boolean {
  try {
    return (
      typeof localStorage !== "undefined" &&
      localStorage.getItem(EXAMPLE_FLAG_KEY) === "1"
    );
  } catch {
    return false;
  }
}

/* Pinned rigs surface as quick-switch chips in the title bar. Up to 3 fit
   alongside the active-rig pill before the layout starts feeling crowded;
   the menu remains the source-of-truth for rigs beyond that. Persisted so
   pins survive across launches. */
const PINNED_RIGS_KEY = "yah-ui:pinned-rigs";
const MAX_PINNED_RIGS = 3;

function loadPinnedRigs(): string[] {
  try {
    if (typeof localStorage === "undefined") return [];
    const raw = localStorage.getItem(PINNED_RIGS_KEY);
    if (!raw) return [];
    const arr = JSON.parse(raw);
    return Array.isArray(arr)
      ? arr
          .filter((x): x is string => typeof x === "string")
          .slice(0, MAX_PINNED_RIGS)
      : [];
  } catch {
    return [];
  }
}

interface SettingsRequest {
  section: "general" | "api-keys";
  focus?: ApiKeyProvider;
}

export function App() {
  const [tab, setTab] = useState<Tab>("board");
  const [theme, setTheme] = useState<Theme>("light");
  /* Settings is a regular tab (rendered inside <main> like Board/Arch/etc.),
     opened via the title-bar gear or contextual nudges (e.g. Infra empty
     state pre-targeting api-keys + the Hetzner row). The request carries
     section + focus so deep-link callers route through the same path as the
     plain "open Settings" action. */
  const [settingsRequest, setSettingsRequest] = useState<SettingsRequest>({
    section: "general",
  });
  const openSettings = useCallback(
    (req: SettingsRequest = { section: "general" }) => {
      setSettingsRequest(req);
      setTab("settings");
    },
    [],
  );
  /* Cold boot: empty rig list + empty board. Real rigs land via the
     rig-list effect below (Tauri only); under browser dev the user opts in
     to the synthetic example rig via `EXAMPLE_FLAG_KEY` to populate
     `mockTickets`. Picking a rig calls `setRigId`, which the board fetch
     effect listens to. */
  const initialRigs: Rig[] = exampleRigEnabled() ? [EXAMPLE_RIG] : [];
  const [rigs, setRigs] = useState<Rig[]>(initialRigs);
  const [rigId, setRigId] = useState<string>(initialRigs[0]?.id ?? "");
  const [relayId, setRelayId] = useState<string | null>(null);
  const [splitMode, setSplitMode] = useState<Tab | null>(null);
  const [rawTickets, setTickets] = useState<Ticket[]>([]);
  /* Relays' displayed status is derived from their children — see
     lib/relay-status.ts. We hold the raw list (source statuses) in
     state so refetches and drag-drop optimistic updates round-trip
     cleanly, then re-derive for every consumer. */
  const tickets = useMemo(
    () => withDerivedRelayFields(rawTickets),
    [rawTickets],
  );
  /* Lifted out of ArchView so cross-tab nav (jumpToFile / NodeActionMenu's
     "Open in agent") can re-root the graph from anywhere. The empty-string
     seed produces an empty splash on first render — the user picks a real
     root via the toolbar (or jumps in from a file chip) once tickets land. */
  const [archRoot, setArchRoot] = useState<string>("");
  const [archDepth, setArchDepth] = useState<number>(2);
  const openTerminalTab = useCallback(() => {
    setTab("terminal");
  }, []);
  /* Authored arch-doc selection (rig-relative `.yah/arch/authored/<rel>`
     or null = JIT graph). Lifted to App so a yah://arch/doc/... click on
     a ticket card can flip the arch tab to a doc view from anywhere.
     Resets per-rig in ArchView's effect — this state intentionally lives
     across rig switches because the router fires before the rig-id
     change settles. */
  const [authoredMmd, setAuthoredMmd] = useState<string | null>(null);

  /* Theme is a CSS-variable swap on [data-theme=...] — see globals.css. */
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  /* Plain digit keys 1–9 switch tabs while no input has focus. We skip the
     handler when the user is typing into a text field, has a modifier
     pressed (⌘/⌃/⌥), or is mid-IME composition — so this never collides
     with a relay search, agent prompt, etc. */
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return;
      const target = e.target as HTMLElement | null;
      if (target) {
        const tag = target.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
        if (target.isContentEditable) return;
      }
      const idx = "123456789".indexOf(e.key);
      if (idx === -1 || idx >= TAB_ORDER.length) return;
      e.preventDefault();
      setTab(TAB_ORDER[idx]);
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /* Seed the rig registry from the daemon (Tauri only; browser stub
     returns []). Last-active wins as the initial selection; first
     attached is the fallback. Runs once on mount — adding rigs at runtime
     (via the not-yet-built attach UI) will need to refresh this manually.
     The example rig (when enabled) is appended so it stays selectable
     alongside real rigs but ranks last in the lastActive sort. */
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const list = await env.rpc.rigList();
        if (cancelled) return;
        const real = list.map(wireToRig);
        /* Empty rigs.json on the backend → fall back to the example rig as
           a first-run welcome surface. The explicit flag still works on top
           of real rigs so devs can flip into demo mode any time. */
        const showExample = real.length === 0 || exampleRigEnabled();
        const next = showExample ? [...real, EXAMPLE_RIG] : real;
        if (next.length === 0) return;
        setRigs(next);
        const active = real
          .slice()
          .sort((a, b) => (b.lastActiveAt ?? 0) - (a.lastActiveAt ?? 0))[0];
        if (active) setRigId(active.id);
        else setRigId(EXAMPLE_RIG_ID);
      } catch (err) {
        console.warn("[rigs] rig_list failed", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  /* Tracks which rigs we've already booted in this session. boot_registry
     attaches every rig from rigs.json but doesn't index any of them — that
     happens lazily on first use. arch_open_rig wipes-and-rewalks each time
     it's called, so we guard with this set so re-selecting a hot rig doesn't
     trigger an unnecessary reindex. */
  const openedRigsRef = useRef<Set<string>>(new Set());

  /* While arch_open_rig is in flight on a fresh rig, we hide the tab content
     behind a Splash overlay (random column variant + scanning copy) so the
     user sees activity instead of a stale or empty board. Once openRig
     resolves, the rig joins openedRigsRef and the overlay clears. */
  const [scanState, setScanState] = useState<
    { rigId: string; variant: SplashVariant } | null
  >(null);

  /* Pinned rigs render as quick-switch chips next to the rig pill in the
     title bar. Capped at MAX_PINNED_RIGS to keep the bar from getting
     crowded; the menu is still the canonical view of every attached rig. */
  const [pinnedRigIds, setPinnedRigIds] = useState<string[]>(loadPinnedRigs);
  useEffect(() => {
    try {
      localStorage.setItem(PINNED_RIGS_KEY, JSON.stringify(pinnedRigIds));
    } catch {
      /* localStorage may be unavailable (private mode, sandboxed) — pins
         degrade to per-session memory; nothing else breaks. */
    }
  }, [pinnedRigIds]);
  const togglePinRig = useCallback((id: string) => {
    setPinnedRigIds((prev) => {
      if (prev.includes(id)) return prev.filter((x) => x !== id);
      if (prev.length >= MAX_PINNED_RIGS) return prev;
      return [...prev, id];
    });
  }, []);

  /* Board fetch + index_finished subscription, scoped to the active rig.
     Re-runs whenever `rigId` changes so picking from the selector retargets
     the daemon. The synthetic example rig short-circuits to mock data; an
     empty rigId clears the board. First touch of a real rig triggers
     arch_open_rig to boot the daemon (otherwise listTickets returns empty
     until the user happens to edit a file). */
  useEffect(() => {
    if (!rigId) {
      setTickets([]);
      return;
    }
    if (rigId === EXAMPLE_RIG_ID) {
      setTickets(mockTickets);
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    async function refetch() {
      try {
        const env = await getEnv();
        const [t, r] = await Promise.all([
          env.rpc.listTickets(rigId),
          env.rpc.listRelays(rigId),
        ]);
        if (cancelled) return;
        const merged = [...r.relays, ...t.tickets].map(workItemToTicket);
        setTickets(merged);
      } catch (err) {
        console.warn("[board] tickets fetch failed", err);
      }
    }

    void (async () => {
      const env = await getEnv();
      try {
        await env.rpc.rigSetActive(rigId);
      } catch {
        /* unattached id during dev; ignore */
      }
      if (cancelled) return;

      if (!openedRigsRef.current.has(rigId)) {
        setScanState({ rigId, variant: pickScanVariant() });
        try {
          await env.rpc.openRig(rigId);
          if (!cancelled) openedRigsRef.current.add(rigId);
        } catch (err) {
          console.warn("[board] open_rig failed", err);
        }
        if (!cancelled) setScanState(null);
      }
      if (cancelled) return;

      await refetch();
      if (cancelled) return;
      const off = await env.rpc.onEvent((e) => {
        if (e.event !== "index_finished") return;
        const wrapped = e as { rig_id?: string };
        if (wrapped.rig_id && wrapped.rig_id !== rigId) return;
        void refetch();
      });
      if (cancelled) off();
      else unlisten = off;
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [rigId]);

  /* Cross-tab nav contract (impl-guide §6). Each handler is one named action
     callable from any tab; we pass them down explicitly rather than via an
     event bus. */
  const jumpToFile = useCallback((fileColon: string) => {
    /* `path:line` → for v1 we re-root the graph to the file path's basename
       sans extension (closest stand-in for an arch node id until the
       backend serves a real path→node lookup). */
    const path = fileColon.split(":")[0] ?? fileColon;
    const base = path.split("/").pop() ?? path;
    const stem = base.replace(/\.[^.]+$/, "");
    setArchRoot(stem);
    setTab("arch");
  }, []);
  /* yah:// link router. Agent output emits `[label](yah://...)` anchors
     that the markdown renderer turns into clickable buttons; this is
     where the click lands. Three shapes today:
       yah://file/<path>[#L<line>]   -> jumpToFile (Arch graph re-root)
       yah://arch/symbol/<name>      -> Arch graph re-root by symbol id
       yah://arch/<name>             -> alias for arch/symbol/<name>
       yah://arch/doc/<rel-path>     -> open authored arch doc in arch tab
     Anything else logs and is ignored — we'd rather skip a malformed
     anchor than throw. */
  const routeYahLink = useCallback(
    (href: string) => {
      const m = /^yah:\/\/([^/]+)\/(.+)$/.exec(href);
      if (!m) {
        console.warn("[yah-link] unparseable", href);
        return;
      }
      const [, kind, rest] = m;
      if (kind === "file") {
        const [path, frag] = rest.split("#");
        const lineMatch = frag ? /^L(\d+)$/.exec(frag) : null;
        const fileColon = lineMatch ? `${path}:${lineMatch[1]}` : path;
        jumpToFile(fileColon);
        return;
      }
      if (kind === "arch") {
        if (rest.startsWith("doc/")) {
          /* `doc/` prefix flips the arch canvas to AuthoredMmdPane /
             AuthoredMdPane (chosen by extension). The rel-path round-
             trips through the daemon's sandbox check, so a malformed
             path lands on the splash rather than a silent failure. */
          setAuthoredMmd(rest.slice("doc/".length));
          setTab("arch");
          return;
        }
        // yah://arch/symbol/<name> or yah://arch/<name>
        const symbol = rest.replace(/^symbol\//, "");
        setAuthoredMmd(null);
        setArchRoot(symbol);
        setTab("arch");
        return;
      }
      console.warn("[yah-link] unknown scheme", href);
    },
    [jumpToFile],
  );

  const openInAgent = useCallback((target: string) => {
    /* Target may be an arch node id or a relay id; the agent view uses
       relayId to pick the session, so we set it directly. Mock data has no
       node→relay map, so for a node-id target the agent view falls back to
       the no-session pane — that's expected for v1. */
    setRelayId(target);
    setTab("agent");
  }, []);

  /* "Open local folder…" in the rig selector. Pops the native folder
     picker, attaches the chosen path, and selects it. The backend persists
     ~/.yah/rigs.json, so the new rig sticks across restarts. Idempotent on
     paths already attached — the daemon refreshes the name and returns the
     existing entry. */
  const onAttachLocalRig = useCallback(async () => {
    try {
      const env = await getEnv();
      const path = await env.pickFolder();
      if (!path) return;
      const name = path.split("/").filter(Boolean).pop() ?? path;
      const dto = await env.rpc.rigAttach(path, name);
      const list = await env.rpc.rigList();
      setRigs(list.map(wireToRig));
      setRigId(dto.id);
    } catch (err) {
      console.warn("[rigs] attach failed", err);
    }
  }, []);

  /* "Connect remote rig…" in the rig selector. Stores the SSH spec
     server-side; the daemon doesn't open the SSH session yet (the lazy
     `SshRpcClient` lands with R019-F2). Activation surfaces a clear
     "remote rig not yet wired" error until then — the rig stays on
     the board so the user can come back to it once the transport
     ships. The modal owns the form; this callback owns the round-trip. */
  const onAttachRemoteRig = useCallback(async (spec: WireRemoteRigSpec) => {
    const env = await getEnv();
    const dto = await env.rpc.rigAttachRemote(spec);
    const list = await env.rpc.rigList();
    setRigs(list.map(wireToRig));
    setRigId(dto.id);
  }, []);

  /* Open an SSH terminal session for a Hetzner server and switch to
     the Terminal tab. Resolves the private key by intersecting local
     `~/.ssh/*.pub` entries (with a present private half) against the
     public keys registered in the operator's Hetzner project — yah
     never falls back to `~/.ssh/id_*` blind, since that would let a
     stranger key (e.g. a passphrase-protected `id_rsa`) into the
     auth attempt against a server it wasn't authorized for.
     Comparison is on the canonical "<algo> <base64>" prefix (comment
     dropped) because the ssh-key crate emits SHA256:… fingerprints
     while Hetzner returns MD5 colon-hex — formats can't be matched
     directly, but the underlying OpenSSH line is byte-for-byte the
     one yah uploaded. Yah-generated keys (comment `…@yah`) are
     preferred over imported ones; the tab only flips on a successful
     resolve so the user doesn't land on an empty Terminal pane after
     a silent failure. */
  const openTerminalForServer = useCallback(async (server: HetznerServer) => {
    if (!server.ipv4) return;
    try {
      const env = await getEnv();
      const [localKeys, projectKeys] = await Promise.all([
        env.rpc.ssh.listLocal(),
        env.rpc.hetzner.listSshKeys(),
      ]);
      const canon = (line: string) => {
        const parts = line.trim().split(/\s+/);
        return parts.length >= 2 ? `${parts[0]} ${parts[1]}` : "";
      };
      const projectKeyBlobs = new Set(
        projectKeys.map((k) => canon(k.public_key)).filter(Boolean),
      );
      const candidates = localKeys.filter(
        (k) => k.has_private && projectKeyBlobs.has(canon(k.public_key)),
      );
      if (candidates.length === 0) {
        console.warn(
          `[terminal] no yah-authorized SSH key found for ${server.name}: upload a key via the Provision form first`,
        );
        return;
      }
      const picked =
        candidates.find((k) => k.public_key.trimEnd().endsWith("@yah")) ?? candidates[0];
      const keyPath = picked.public_key_path.replace(/\.pub$/, "");
      await terminalStore.open(
        {
          host: server.ipv4,
          user: "root",
          keyPath,
          label: `${server.name} (${server.ipv4})`,
        },
        { rigId: rigId || null },
      );
      setTab("terminal");
    } catch (err) {
      console.warn("[terminal] open failed", err);
    }
  }, [rigId]);

  /* Attention badge derivation (R024-T3): once tickets carry a rigId, this
     groups handoff count per rig. Today tickets are rig-less, so the live
     count attributes to the active rig only and other rigs use the seeded
     mock value. The fallback chain keeps the UI exercisable without
     waiting on the backend rigId column. */
  const rigsWithAttention = useMemo(() => {
    const liveActiveCount = tickets.filter((t) => t.status === "handoff").length;
    return rigs.map((r) =>
      r.id === rigId
        ? { ...r, needsAttention: liveActiveCount || r.needsAttention }
        : r,
    );
  }, [tickets, rigId, rigs]);

  /* Lifted so the title-bar rig dot and the footer ConnectionStrip share a
     single heartbeat instead of double-probing the daemon. */
  const connectionStatus = useConnectionStatus(rigId);

  /* Rig-wide rule validation. The hook refetches on every index_finished
     so violations stay in sync with code edits. Browser stub returns
     `{ violations: [] }`, so this is a no-op outside Tauri. Both the
     Architecture and Board tabs read from the same array. */
  const { violations } = useValidate(rigId);

  return (
    <ApiKeysProvider>
    <div className="flex h-full flex-col bg-paper text-ink">
      <TitleBar
        rigs={rigsWithAttention}
        activeRigId={rigId}
        onRigChange={setRigId}
        onAttachLocalRig={onAttachLocalRig}
        onAttachRemoteRig={onAttachRemoteRig}
        pinnedRigIds={pinnedRigIds}
        onTogglePinRig={togglePinRig}
        maxPinnedRigs={MAX_PINNED_RIGS}
        connectionState={connectionStatus.state}
        relays={tickets.filter((t) => t.itemType === "relay" || t.parent)}
        activeRelayId={relayId}
        onRelayChange={setRelayId}
        theme={theme}
        onThemeChange={setTheme}
        activeTab={tab}
        splitMode={splitMode}
        onSplitModeChange={setSplitMode}
        onOpenSettings={() => openSettings()}
      />
      <TabStrip active={tab} onChange={setTab} />
      <main className="relative min-h-0 flex-1 overflow-hidden">
        {scanState && scanState.rigId === rigId ? (
          <ScanningPane variant={scanState.variant} />
        ) : tab === "terminal" || tab === "agent" ? null : (
          <TabPane
            tab={tab}
            rigId={rigId}
            relayId={relayId}
            setRelayId={setRelayId}
            tickets={tickets}
            setTickets={setTickets}
            archRoot={archRoot}
            setArchRoot={setArchRoot}
            archDepth={archDepth}
            setArchDepth={setArchDepth}
            authoredMmd={authoredMmd}
            setAuthoredMmd={setAuthoredMmd}
            jumpToFile={jumpToFile}
            openInAgent={openInAgent}
            routeYahLink={routeYahLink}
            openTerminalForServer={openTerminalForServer}
            openTerminalTab={openTerminalTab}
            violations={violations}
            onOpenSettings={openSettings}
            settingsRequest={settingsRequest}
          />
        )}
        {/* Persistent terminal layer. xterm 6's `Terminal.open()` is
            single-shot — re-attaching to a fresh host on tab return
            leaves the renderer blank — so we keep TerminalView mounted
            for the whole app lifetime and just toggle visibility +
            pointer-events when the tab swaps. visibility:hidden (not
            display:none) preserves layout sizing so the xterm hosts
            keep their measured cols/rows across tab changes. */}
        <div
          className="absolute inset-0"
          style={{
            visibility:
              tab === "terminal" && !(scanState && scanState.rigId === rigId)
                ? "visible"
                : "hidden",
            pointerEvents:
              tab === "terminal" && !(scanState && scanState.rigId === rigId)
                ? "auto"
                : "none",
          }}
          aria-hidden={tab !== "terminal"}
        >
          <TerminalView
            onJumpToFile={jumpToFile}
            defaultCwd={rigs.find((r) => r.id === rigId)?.path ?? null}
            rigId={rigId || null}
          />
        </div>
        {/* Agent tab is also persistent so per-chat state (engine
            picker, useChatSession events, scroll position) survives
            tab switches. AgentView itself filters chats by rigId so
            switching rigs surfaces only that rig's chats. */}
        <div
          className="absolute inset-0"
          style={{
            visibility:
              tab === "agent" && !(scanState && scanState.rigId === rigId)
                ? "visible"
                : "hidden",
            pointerEvents:
              tab === "agent" && !(scanState && scanState.rigId === rigId)
                ? "auto"
                : "none",
          }}
          aria-hidden={tab !== "agent"}
        >
          <AgentView
            rigId={rigId || null}
            relayId={relayId}
            onSelectRelay={setRelayId}
            onJumpToFile={jumpToFile}
            onOpenTerminalTab={openTerminalTab}
            onYahLink={routeYahLink}
          />
        </div>
      </main>
      <ConnectionStrip status={connectionStatus} />
    </div>
    </ApiKeysProvider>
  );
}

function TabPane({
  tab,
  rigId,
  relayId,
  setRelayId,
  tickets,
  setTickets,
  archRoot,
  setArchRoot,
  archDepth,
  setArchDepth,
  authoredMmd,
  setAuthoredMmd,
  jumpToFile,
  openInAgent,
  routeYahLink,
  openTerminalForServer,
  openTerminalTab,
  violations,
  onOpenSettings,
  settingsRequest,
}: {
  tab: Tab;
  rigId: string;
  relayId: string | null;
  setRelayId: (id: string | null) => void;
  tickets: Ticket[];
  setTickets: (t: Ticket[]) => void;
  archRoot: string;
  setArchRoot: (s: string) => void;
  archDepth: number;
  setArchDepth: (n: number) => void;
  authoredMmd: string | null;
  setAuthoredMmd: (s: string | null) => void;
  jumpToFile: (fileColon: string) => void;
  openInAgent: (target: string) => void;
  routeYahLink: (href: string) => void;
  openTerminalForServer: (server: HetznerServer) => void;
  openTerminalTab: () => void;
  violations: import("./env/types").WireViolation[];
  onOpenSettings: (req?: SettingsRequest) => void;
  settingsRequest: SettingsRequest;
}) {
  switch (tab) {
    case "board":
      return (
        <Board
          rigId={rigId}
          tickets={tickets}
          onTicketsChange={setTickets}
          activeRelayId={relayId}
          onClearRelayFilter={() => setRelayId(null)}
          onYahLink={routeYahLink}
          violations={violations}
        />
      );
    case "arch":
      return (
        <ArchView
          rigId={rigId}
          rootId={archRoot}
          onRootChange={setArchRoot}
          depth={archDepth}
          onDepthChange={setArchDepth}
          authoredMmd={authoredMmd}
          onAuthoredMmdChange={setAuthoredMmd}
          onJumpToFile={jumpToFile}
          onOpenInAgent={openInAgent}
          onYahLink={routeYahLink}
          violations={violations}
        />
      );
    case "agent":
      /* AgentView is rendered persistently in the parent <main>
         (alongside TerminalView) so per-chat state survives tab
         switches. This case never fires because the parent
         short-circuits before constructing TabPane for tab ===
         "agent". */
      return null;
    case "infra":
      return (
        <InfraTab
          onOpenSettings={onOpenSettings}
          onOpenTerminal={openTerminalForServer}
        />
      );
    case "terminal":
      /* TerminalView is rendered persistently in the parent <main>
         (above this switch) so xterm hosts survive tab switches —
         this case never fires because the parent short-circuits
         before constructing TabPane for tab === "terminal". */
      return null;
    case "files":
      return <FilesView rigId={rigId} />;
    case "settings":
      return (
        <SettingsView
          onOpenTerminalTab={openTerminalTab}
          initialSection={settingsRequest.section}
          initialFocus={settingsRequest.focus}
          activeRigId={rigId || undefined}
        />
      );
    case "preview":
    case "services":
    case "analytics":
      return <ComingSoon tab={tab} />;
  }
}

/* Test- and Host-cluster tabs ship as splash placeholders in v1. Each tab
   gets an illustration + caption so the empty state reads as deliberate,
   not unfinished. Host-cluster placements lean on the new dedicated assets:
   `node` (bathhouse plans) for Infra (per-machine provisioning); `mirror`
   (looking-glass onto an encampment) for Analytics (telemetry view of the
   running mirror); `architecture` (civic-plumbing plans) is reserved for
   the Arch view itself.

   `infra` was here too until R027-F5 split it into its own component with
   the api_key_has('hetzner') gate — see InfraTab below. */
const COMING_SOON_SPLASH: Record<
  "terminal" | "preview" | "services" | "analytics",
  { variant: SplashVariant; caption: string; sub: string }
> = {
  terminal: {
    variant: "camp",
    caption: "Campfire not yet lit",
    sub: "A scrollback terminal with cross-linked file/grep results lands in v2 — for now, run commands in your usual shell.",
  },
  preview: {
    variant: "mirror",
    caption: "Pages still being scribed",
    sub: "Live preview of the rig's web output (dev server mirror) lands in v2. The agent can't drive a browser yet.",
  },
  services: {
    variant: "node",
    caption: "The forge stands quiet",
    sub: "Services running on the current mirror — db, queues, workers, their logs and lifecycle. Lands with the PaaS milestone.",
  },
  analytics: {
    variant: "mirror",
    caption: "The looking-glass is dark",
    sub: "Telemetry from the current mirror — node heartbeats, request rates, the side-effects of a live encampment. Lands once a mirror is running.",
  },
};

/* Scanning splash — shown while arch_open_rig walks a freshly-attached or
   previously-cold rig. One copy line per illustration so the metaphor stays
   honest with whichever variant got picked. */
const SCAN_COPY: Record<
  "zones" | "open" | "active" | "handoff" | "review",
  { caption: string; sub: string }
> = {
  zones: {
    caption: "Charting the territory…",
    sub: "Walking the rig and laying out the campaign map. First scans take a moment.",
  },
  open: {
    caption: "Counting wares…",
    sub: "Tallying every annotation in the rig. First scans take a moment.",
  },
  active: {
    caption: "Pitching the camp…",
    sub: "Setting up watchers and indexing the source tree. First scans take a moment.",
  },
  handoff: {
    caption: "Calling the messenger…",
    sub: "Reaching out to the rig and gathering its signals. First scans take a moment.",
  },
  review: {
    caption: "Decanting the work…",
    sub: "Distilling tickets and relays from the source. First scans take a moment.",
  },
};

const SCAN_VARIANTS: ReadonlyArray<keyof typeof SCAN_COPY> = [
  "zones",
  "open",
  "active",
  "handoff",
  "review",
];

function pickScanVariant(): SplashVariant {
  return SCAN_VARIANTS[Math.floor(Math.random() * SCAN_VARIANTS.length)];
}

function ScanningPane({ variant }: { variant: SplashVariant }) {
  const copy =
    variant in SCAN_COPY
      ? SCAN_COPY[variant as keyof typeof SCAN_COPY]
      : SCAN_COPY.open;
  return (
    <div className="flex h-full items-center justify-center">
      <div className="animate-pulse">
        <Splash variant={variant} caption={copy.caption} sub={copy.sub} />
      </div>
    </div>
  );
}

function ComingSoon({
  tab,
}: {
  tab: "terminal" | "preview" | "services" | "analytics";
}) {
  const cfg = COMING_SOON_SPLASH[tab];
  return (
    <div className="flex h-full items-center justify-center">
      <div className="flex flex-col items-center gap-4">
        <div className="font-display text-[24px] text-ink-2 [font-variant-caps:all-small-caps]">
          {tab}
        </div>
        <Splash variant={cfg.variant} caption={cfg.caption} sub={cfg.sub} />
      </div>
    </div>
  );
}

/* Infra tab gate (R027-F5/F6). When no Hetzner token is stored, surface an
   empty-state nudge that opens the Settings modal pre-targeted at the
   Hetzner row (F5). When a token exists, render the server-list view
   backed by the Rust-side Hetzner client (F6). The fetch happens
   server-side via `env.rpc.hetzner.listServers` so the token never
   reaches the renderer — the threat-model boundary holds. */
function InfraTab({
  onOpenSettings,
  onOpenTerminal,
}: {
  onOpenSettings: (req?: SettingsRequest) => void;
  onOpenTerminal: (server: HetznerServer) => void;
}) {
  const apiKeys = useApiKeys();
  const hasHetzner = apiKeys.has("hetzner");

  if (!hasHetzner) {
    return (
      <div className="flex h-full items-center justify-center">
        <div className="flex flex-col items-center gap-4">
          <div className="font-display text-[24px] text-ink-2 [font-variant-caps:all-small-caps]">
            infra
          </div>
          <Splash
            variant="architecture"
            caption="No keys to the kingdom yet"
            sub="Drop a Hetzner API token in Settings to provision and inspect the machines that host this rig's mirrors."
          />
          <button
            onClick={() =>
              onOpenSettings({ section: "api-keys", focus: "hetzner" })
            }
            className="rounded bg-accent px-3 py-1.5 text-[12px] font-medium text-paper-2 hover:bg-accent-2"
          >
            Configure Hetzner token
          </button>
        </div>
      </div>
    );
  }

  return <InfraView onOpenTerminal={onOpenTerminal} />;
}
