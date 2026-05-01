//!
//!
//!
//!
//!
//!
//! @yah:ticket(R027-F4, "env adapter apiKey RPC surface + browser stub no-ops + remove P1 'not yet persisted' banner")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R027)
//! @yah:handoff("env adapter apiKey RPC surface landed. Added ApiKeyRpc { set, has, delete } to Rpc trait in yah-ui/src/env/index.ts; tauri.ts wires through invoke('api_key_set'|'api_key_has'|'api_key_delete'); browser.ts no-ops set/delete and returns false from has. api-keys-context.tsx swapped from in-memory Map to env().rpc.apiKey calls — has(provider) reads a React state map populated on mount by probing each known provider, set/remove are now async and update the cache after the RPC resolves. Context exposes envKind so SettingsModal can render the 'Browser preview — keys not persisted' banner only under dev-server (the 'Tokens not yet persisted securely' banner is gone). test() returns 'Verify pending Rust-side' for both providers under Tauri (browser-side Hetzner fetch removed since the token no longer lives in the renderer); real verify lands with R027-F6 for Hetzner and a follow-up for Cloudflare. SettingsModal saveAdd/deleteToken now await and surface RPC errors via testResult.")
//! @yah:verify("cd yah-ui && bun run typecheck")
//! @yah:verify("cd yah-ui && bun run build")

// Host-environment adapter.
//
// The same React bundle runs under two hosts:
// * Tauri desktop (primary): commands go through Tauri's IPC, events
//   come off the window event bus.
// * Browser dev (secondary): no Tauri APIs available; the adapter
//   serves mock data so screens are still inspectable.
//
// Components must NEVER import `@tauri-apps/api/*` directly. They go
// through `env` here so the browser bundle stays tree-shakeable
// (`@tauri-apps/api` is loaded lazily only when running under Tauri).

import type {
  ArchEvent,
  ClaudeCliProbe,
  GetTicketParams,
  GetTicketResult,
  HetznerCreateServerSpec,
  HetznerImage,
  HetznerLocation,
  HetznerServer,
  HetznerServerType,
  HetznerSshKey,
  KVStore,
  LocalSshKey,
  ListAuthoredFilesResult,
  ListRelaysParams,
  ListRelaysResult,
  ListTicketsParams,
  ListTicketsResult,
  LookupParams,
  LookupResult,
  NeighborsParams,
  NeighborsResult,
  NodeFull,
  NodeId,
  OllamaServeProbe,
  ReadAuthoredFileResult,
  RootsParams,
  RootsResult,
  SessionId,
  StatsResult,
  Subgraph,
  SubgraphParams,
  TerminalOpenSpec,
  TerminalOpenLocalSpec,
  TicketPromptParams,
  TicketPromptResult,
  ValidateResult,
  WalkSummary,
  WireAgentSettings,
  WireApprovalChoice,
  WireApprovalRule,
  WireApprovalRuleset,
  WireAuthorization,
  WireDirListResult,
  WireFileReadRange,
  WireFileReadResult,
  WireIdentity,
  WireProbeReport,
  WireRemoteRigSpec,
  WireRigAgentEvent,
  WireRigDto,
  WireRigFileEvent,
  WireScope,
  WireWatchId,
  WireSessionHistoryRow,
  WireSessionMeta,
  WireSessionSummary,
  WireSingleProbeResult,
  WireStartSessionResult,
  WireTerminalEvent,
  WireTerminalSessionSummary,
  IndexReason,
  Lang,
} from "./types";

export type ArchEventListener = (event: ArchEvent) => void;
export type AgentEventListener = (event: WireRigAgentEvent) => void;
export type TerminalEventListener = (event: WireTerminalEvent) => void;
export type FileEventListener = (event: WireRigFileEvent) => void;
export type Unlisten = () => void;

/** OS-keychain-backed API token storage. Tokens never travel back through
 *  the renderer after first set — `get` is intentionally Rust-only (provider
 *  clients read it from the Tauri side, hit the upstream API, and return only
 *  the parsed result). The renderer's only credential affordance is
 *  `has(provider)` for UI gating. See .yah/arch/authored/settings-api-keys.md. */
export interface ApiKeyRpc {
  /** Write `token` to secure storage under `provider`. Overwrites silently. */
  set(provider: string, token: string): Promise<void>;
  /** Whether secure storage currently holds a token for `provider`. */
  has(provider: string): Promise<boolean>;
  /** Clear `provider`'s token. Resolves true if a token was removed. */
  delete(provider: string): Promise<boolean>;
  /** Import an OpenAI API key stored by `codex login --api-key` into yah's
   *  `openai` keychain slot. Browser-based Codex OAuth is reported as an error. */
  importCodexOpenAi(): Promise<boolean>;
}

/** SSH-key identity registry. Backs the Settings → Identities section
 *  and (later) the rig-card identity row. The renderer never sees a
 *  private key — only paths and fingerprints flow through `WireIdentity`. */
export interface IdentityRpc {
  /** Snapshot of every registered identity. */
  list(): Promise<WireIdentity[]>;
  /** Generate a fresh ed25519 yah-managed identity under
   *  `$YAH_HOME/keys/<name>`. Refuses to clobber an existing keyfile. */
  create(name: string): Promise<WireIdentity>;
  /** Reference an existing public key file (typically under `~/.ssh/`).
   *  yah never copies the private bytes — only path + fingerprint. */
  import(publicKeyPath: string, name?: string): Promise<WireIdentity>;
  /** Drop one identity from the registry by id. yah-generated keyfiles
   *  are deleted; imported keys stay where the user put them. Resolves
   *  `false` when the id wasn't registered. */
  remove(id: string): Promise<boolean>;
  /** Run every probe and reconcile the registry — local files, Hetzner,
   *  GitHub. Per-provider outcomes ride out independently so a missing
   *  PAT doesn't suppress a successful Hetzner probe. */
  probeAll(): Promise<WireProbeReport>;
  /** Re-check just this identity against Hetzner. */
  probeHetzner(id: string): Promise<WireSingleProbeResult>;
  /** Re-check just this identity against the GitHub account in the PAT. */
  probeGithub(id: string): Promise<WireSingleProbeResult>;
  /** Register this identity at the operator's Hetzner project. */
  authorizeHetzner(id: string, name: string): Promise<WireAuthorization>;
  /** Drop this identity from Hetzner. Resolves `false` when the
   *  identity has no Hetzner authorization recorded. */
  deauthorizeHetzner(id: string): Promise<boolean>;
  /** Register this identity at the operator's GitHub account.
   *  PAT must have `admin:public_key` scope. */
  authorizeGithub(id: string, title: string): Promise<WireAuthorization>;
  /** Drop this identity from GitHub. */
  deauthorizeGithub(id: string): Promise<boolean>;
}

/** Surface every component talks to. Implementations in tauri.ts / browser.ts.
 *
 *  Every arch-scoped read/mutation takes a `rigId` because the daemon
 *  multiplexes one KgService per attached rig (see
 *  app/tauri/src/commands.rs `svc_by_id`). The renderer tracks the
 *  active rigId in <App> and threads it down. */
export interface Rpc {
  /** Boot a rig that's already attached; idempotent — a second call rebinds. */
  openRig(rigId: string): Promise<WalkSummary>;
  closeRig(rigId: string): Promise<void>;

  // Read queries — all serializable; refer to ./types for shapes.
  subgraph(rigId: string, params: SubgraphParams): Promise<Subgraph>;
  lookup(rigId: string, params: LookupParams): Promise<LookupResult>;
  node(rigId: string, id: NodeId): Promise<NodeFull | null>;
  neighbors(rigId: string, params: NeighborsParams): Promise<NeighborsResult>;
  roots(rigId: string, params: RootsParams): Promise<RootsResult>;
  stats(rigId: string): Promise<StatsResult>;
  languages(rigId: string): Promise<Lang[]>;

  // Rig registry — Board needs the active rigId to scope its fetches.
  rigList(): Promise<WireRigDto[]>;
  /** Add a folder to the rig registry. Idempotent: re-attaching the same
   *  path returns the existing rig with its `name` refreshed. Does not
   *  boot indexing — the daemon stays cold until the user activates the
   *  rig (lazy boot keeps cold-start cheap). */
  rigAttach(path: string, name: string): Promise<WireRigDto>;
  /** Add a remote (SSH) rig. Stores the spec only — no SSH session is
   *  opened here; the lazy `SshRpcClient` construction lands with
   *  R019-F2. Until then, activating a remote rig surfaces a clear
   *  "not yet wired" error. Idempotent on `(user, host, port,
   *  workspacePath)`. */
  rigAttachRemote(spec: WireRemoteRigSpec): Promise<WireRigDto>;
  /** Drop a rig from the registry. Returns false if the id wasn't there. */
  rigDetach(rigId: string): Promise<boolean>;
  /** Mark a rig as the focused one. Persists `lastActiveAt` server-side
   *  so the next session opens on the same rig. Returns `false` if the
   *  id isn't attached. */
  rigSetActive(rigId: string): Promise<boolean>;

  // Work items (relays + tickets) — feed the Board tab.
  listTickets(rigId: string, params?: ListTicketsParams): Promise<ListTicketsResult>;
  listRelays(rigId: string, params?: ListRelaysParams): Promise<ListRelaysResult>;
  getTicket(rigId: string, params: GetTicketParams): Promise<GetTicketResult>;

  /** Run the `@yah:rule(...)` validator. Omit `scope` (or pass nothing)
   *  to validate the whole rig. */
  validate(rigId: string, scope?: WireScope): Promise<ValidateResult>;

  /** Archive a ticket from review/done. Strips its `@yah:` annotation
   *  block from source and appends an `archived` event to the rig's
   *  shard log. Errors surface as toast text (e.g. "Epic 'R007' has 2
   *  live child relay(s)…"). Currently shells out to the `yah` CLI on
   *  the Tauri side — see `arch_archive_ticket` for the migration plan. */
  archiveTicket(rigId: string, id: string): Promise<void>;

  /** Enumerate authored mermaid files under `<rig>/.yah/arch/authored/`.
   *  Returns `{ files: [] }` when the directory doesn't exist yet — a
   *  fresh rig with no diagrams is a normal empty state, not an error. */
  listAuthoredFiles(rigId: string): Promise<ListAuthoredFilesResult>;

  /** Read one authored mermaid file by rig-relative path. Daemon enforces
   *  the sandbox: paths that escape `.yah/arch/authored/` (via `..`,
   *  symlink, or absolute prefix) reject as a Conflict toast. */
  readAuthoredFile(rigId: string, relPath: string): Promise<ReadAuthoredFileResult>;

  /** One-shot listing of `path` under the rig root (immediate children
   *  only). Empty/`.` lists the rig root. Daemon canonicalizes — paths
   *  that escape via `..`/symlink reject as Conflict. Backs `<FileTree>`
   *  lazy-expand under the Files tab. Mirrors `rpc::DirListParams`. */
  dirList(rigId: string, path: string): Promise<WireDirListResult>;

  /** Read a file under the rig root. `path` is rig-relative (POSIX).
   *  Without `range`, the daemon clips at a 5MB soft cap and sets
   *  `truncated`; with `range`, the cap doesn't apply. UTF-8 files come
   *  back with `encoding: "utf8"`; non-UTF-8 bytes round-trip as base64.
   *  The daemon canonicalizes `path` and rejects escapes outside the
   *  rig root. Mirrors `rpc::FileReadParams` / `FileReadResult`. */
  fileRead(
    rigId: string,
    path: string,
    range?: WireFileReadRange,
  ): Promise<WireFileReadResult>;

  /** Subscribe to filesystem changes under `path` (recursive — every
   *  descendant emits). Empty `path` watches the rig root. Returns a
   *  watch id; stream the events via [`onFileEvent`] and pass the id to
   *  [`fileUnwatch`] when done. Per-rig-process stable; resubscribe
   *  after a daemon restart. Mirrors `rpc::DirWatchParams`. */
  dirWatch(rigId: string, path: string): Promise<WireWatchId>;

  /** Drop a watch handle previously returned by [`dirWatch`] /
   *  `file.watch`. Idempotent — unknown ids resolve cleanly. */
  fileUnwatch(rigId: string, id: WireWatchId): Promise<void>;

  /** Subscribe to the daemon's `file.event` stream (debounced
   *  `notify`-backed). The listener fires for every active watch; route
   *  by `event.watch_id` against the ids you got from [`dirWatch`] /
   *  `file.watch`. Returns a teardown fn. */
  onFileEvent(listener: FileEventListener): Promise<Unlisten>;

  /** Render the canonical pickup or review markdown for a work-item id.
   *  Both the CLI's `yah board show <id> --prompt` and this RPC flow
   *  through the same renderer (`yah_kg::prompt::render`) so the output
   *  is byte-identical for the same id+mode. `result.markdown` is `null`
   *  when the id isn't on the board. */
  ticketPrompt(rigId: string, params: TicketPromptParams): Promise<TicketPromptResult>;

  // Mutations.
  reindexPath(rigId: string, path: string, reason: IndexReason): Promise<void>;
  touch(rigId: string, paths: string[], tool: string, relay: string): Promise<void>;

  /** Subscribe to the daemon's ArchEvent stream. Returns a teardown fn. */
  onEvent(listener: ArchEventListener): Promise<Unlisten>;

  /** Agent runtime — start chat or relay-anchored sessions, send turns,
   *  abort. Streaming events arrive on the `agent:event` channel via
   *  [`AgentRpc.onEvent`]. */
  agent: AgentRpc;

  /** Per-provider secret storage. See [`ApiKeyRpc`]. */
  apiKey: ApiKeyRpc;

  /** SSH-key registry — first-class identities (yah-generated +
   *  imported) plus cross-target authorization probes. See
   *  [`IdentityRpc`] and .yah/arch/authored/yah-identities.md. */
  identity: IdentityRpc;

  /** Subprocess + service liveness probes — backs the AgentProvidersPanel
   *  bootstrap UI for paths that don't authenticate via a paste-key
   *  affordance (today: `claude` PVd preset; ollama local-serve). See
   *  [`ProbeRpc`]. */
  probe: ProbeRpc;

  /** Cloud-provider read surfaces. The Tauri side hits the upstream API
   *  using the token from `apiKey` storage and returns only the parsed
   *  result — the renderer never sees the credential. */
  hetzner: HetznerRpc;

  /** Local SSH key discovery + ed25519 keypair generation. Backs the
   *  R029 provision flow's "use existing local key" + "generate new
   *  yah key" paths. Private-key contents never reach the renderer. */
  ssh: SshRpc;

  /** SSH-backed terminal sessions. Each session is a russh-managed PTY
   *  on the Tauri side, streaming bytes to the renderer via the
   *  `terminal:event` channel. See `app/tauri/src/terminal.rs`. */
  terminal: TerminalRpc;
}

/** Liveness probes for subprocess + service backends that don't fit the
 *  paste-key model. The `claude` (PVd) preset has no API key — Claude
 *  Code manages its own login — so the AgentProvidersPanel reaches for
 *  these probes instead of `apiKey.has(...)` to decide whether to show
 *  a green or grey status pill. */
export interface ProbeRpc {
  /** Run `claude --version` Rust-side. Resolves with a probe report
   *  even on spawn failure (the renderer reads `installed: false` +
   *  `error` to decide what to render). */
  claudeCli(): Promise<ClaudeCliProbe>;
  /** Hit `localhost:11434/api/tags` Rust-side. The fallback path that
   *  `agent.rs` uses when no Ollama Cloud key is set is silent today;
   *  this probe gives the panel a "Local serve detected" pill. */
  ollamaServe(): Promise<OllamaServeProbe>;
}

/** Agent runtime surface. Backs AgentView's session pane and the chat
 *  CTAs from NoSession. Sessions split into two flavors today:
 *
 *  - **Relay-anchored** (`startSession`): prelude built from a ticket's
 *    annotations (handoff/next/verify/gotcha/parent chain + KG slice).
 *  - **Unanchored chat** (`startChatSession`): minimal prelude — just a
 *    "you are working in this rig" header. No ticket lookup.
 *
 *  Both shapes converge on the same `agent:event` stream and the same
 *  `send`/`stop`/`listSessions` operations. KG-anchored arch-doc
 *  sessions (Phase 2 of the chat work) will land as a third start
 *  method on the same surface. */
export interface AgentRpc {
  /** Open a relay-anchored session. Errors when the rig isn't attached
   *  or the ticket isn't on the board. */
  startSession(
    rigId: string,
    ticketId: string,
  ): Promise<WireStartSessionResult>;
  /** Open an unanchored chat. `engine` accepts `"openai"` /
   *  `"openai:gpt-4o"` / `"claude"` / `"ollama"`. Optional `model`
   *  overrides the model embedded in the engine string. */
  startChatSession(
    rigId: string,
    engine: string,
    model?: string,
  ): Promise<WireStartSessionResult>;
  /** Append a user turn. Streaming reply rides `agent:event`. */
  send(sessionId: SessionId, text: string): Promise<void>;
  /** Abort any in-flight turn and drop the session. Returns false when
   *  the session id is already gone. */
  stop(sessionId: SessionId): Promise<boolean>;
  /** Snapshot of every live session — relay and chat together. */
  listSessions(): Promise<WireSessionSummary[]>;
  /** Catalogue of model ids the provider currently serves. Hits the
   *  upstream `/v1/models` endpoint using the keychain key for that
   *  provider. Empty array is a legitimate state (e.g. a fresh local
   *  Ollama with no models pulled yet); rejection means the upstream
   *  errored or the keychain slot is empty. */
  listModels(provider: string): Promise<string[]>;
  /** Subscribe to the agent event stream. Each event is rig-tagged so
   *  one listener can fan out across multiple rigs / sessions. */
  onEvent(listener: AgentEventListener): Promise<Unlisten>;
  /** Approval-rules + inline-prompt RPCs (R031-F5). The chat pane
   *  posts the user's reply to a pending approval through
   *  `decide`; the Settings UI lists/adds/removes persisted rules
   *  via `rules*`. */
  approval: ApprovalRpc;
  /** Per-rig agent runtime settings — today, the writer-tools opt-in
   *  flag (R031-F5 production flip). */
  settings: AgentSettingsRpc;
  /** Past sessions on disk + sidecar metadata. List is cheap (stat the
   *  jsonl directory + read each `<id>.meta.json` if present); reindex
   *  is strictly user-click and writes a fresh sidecar. */
  history: SessionHistoryRpc;
}

/** Session-history surface (R031-F8). Each rig's `.yah/sessions/`
 *  directory holds the on-disk jsonl per session; `<id>.meta.json` is
 *  a synthesized sidecar (title / summary / tags) regenerated on user
 *  click via `reindex`. */
export interface SessionHistoryRpc {
  list(rigId: string): Promise<WireSessionHistoryRow[]>;
  reindex(rigId: string, sessionId: SessionId): Promise<WireSessionMeta>;
}

/** R031-F5: approval-prompt + rule-store surface. The Tauri commands
 *  on the other end of these calls are `agent_approval_*` and
 *  `agent_approval_rules_*`. */
export interface ApprovalRpc {
  /** Resolve a pending approval. Resolves `true` when the request was
   *  still pending; `false` when it had already been resolved (a
   *  double-click). */
  decide(
    rigId: string,
    sessionId: SessionId,
    requestId: string,
    choice: WireApprovalChoice,
  ): Promise<boolean>;
  /** Snapshot of the persisted ruleset for `rigId`. A fresh rig with
   *  no file resolves to an empty V1 envelope. */
  rulesList(rigId: string): Promise<WireApprovalRuleset>;
  /** Append `rule`. Idempotent — the store de-dupes equal rules.
   *  Returns the updated ruleset. */
  rulesAdd(rigId: string, rule: WireApprovalRule): Promise<WireApprovalRuleset>;
  /** Remove the rule at `index`. Out-of-range is a no-op. Returns the
   *  updated ruleset. */
  rulesRemove(rigId: string, index: number): Promise<WireApprovalRuleset>;
}

/** R031-F5: per-rig agent runtime settings. Today: the experimental
 *  writer-tools opt-in flag. The flag takes effect on the next session
 *  start — already-running sessions keep the registry shape they were
 *  minted with. */
export interface AgentSettingsRpc {
  get(rigId: string): Promise<WireAgentSettings>;
  set(rigId: string, settings: WireAgentSettings): Promise<WireAgentSettings>;
}

/** Hetzner Cloud client. Backs the Infra tab's server-list view (R027-F6)
 *  and the provision flow's SSH-key plumbing (R029-T1). Mirrors
 *  `app/tauri/src/hetzner.rs`. */
export interface HetznerRpc {
  /** Enumerate every server in the operator's Hetzner project. Rejects
   *  with a descriptive error string when no token is stored or the
   *  upstream API rejects the credential. */
  listServers(): Promise<HetznerServer[]>;
  /** Enumerate every SSH key already known to the Hetzner project. */
  listSshKeys(): Promise<HetznerSshKey[]>;
  /** Upload a public key to the Hetzner project. The new key's id can
   *  be passed straight to a future `createServer` call. Hetzner
   *  rejects duplicate names with a 422; the error string surfaces
   *  unchanged. */
  uploadSshKey(name: string, publicKey: string): Promise<HetznerSshKey>;
  /** Provision a new server. Returns the parsed `HetznerServer` so the
   *  Servers list can update without a second round-trip. Hetzner's
   *  422 reason strings (unknown server_type, image, location, name
   *  taken) surface unchanged via the rejection. */
  createServer(spec: HetznerCreateServerSpec): Promise<HetznerServer>;
  /** Live catalogue endpoints for the provision form. Each entry's
   *  `prices[].location` array is the source of truth for "is this
   *  type buildable in location X?" — hardcoded matrices silently
   *  422 when Hetzner adds/removes regions. */
  listServerTypes(): Promise<HetznerServerType[]>;
  listLocations(): Promise<HetznerLocation[]>;
  /** System images (no snapshots/backups). One record per
   *  (name, architecture); the renderer dedupes by name. */
  listImages(): Promise<HetznerImage[]>;
}

/** Local SSH key surface. Mirrors `app/tauri/src/ssh_keys.rs`. */
export interface SshRpc {
  /** Enumerate `~/.ssh/*.pub` entries that parse as OpenSSH public
   *  keys. Returns `[]` when `~/.ssh/` doesn't exist. */
  listLocal(): Promise<LocalSshKey[]>;
  /** Generate a fresh ed25519 keypair under `~/.ssh/<name>` (private,
   *  0600 on Unix) + `~/.ssh/<name>.pub`. Refuses to clobber an
   *  existing key. */
  generate(name: string): Promise<LocalSshKey>;
}

/** SSH terminal session surface. Mirrors `app/tauri/src/terminal.rs`.
 *  Each call to `openSsh` mints a session id; the renderer tracks
 *  per-session state (the xterm Terminal instance, scrollback) and
 *  uses the id to route keystrokes/resizes back through the IPC. */
export interface TerminalRpc {
  /** Resolves with the new `sessionId`. Auth failures resolve as
   *  rejections; transport errors after auth surface on the event
   *  stream as an `error` event. */
  openSsh(spec: TerminalOpenSpec): Promise<string>;
  /** Spawn a local shell PTY (no SSH). Diagnostic surface for
   *  isolating renderer issues from remote-shell behaviour. */
  openLocal(spec: TerminalOpenLocalSpec): Promise<string>;
  /** Send a keystroke buffer to the PTY. `bytesB64` is the keystroke
   *  bytes (UTF-8 typically) base64-encoded — the IPC seam dodges
   *  serde_json's number-per-byte expansion that way. */
  input(sessionId: string, bytesB64: string): Promise<void>;
  /** Inform sshd of the new PTY size when the xterm grid resizes. */
  resize(sessionId: string, cols: number, rows: number): Promise<void>;
  /** Drop the session. Resolves false if it was already gone. */
  close(sessionId: string): Promise<boolean>;
  /** Enumerate live sessions on the Tauri side. Used by the renderer
   *  to rehydrate its session rail after a tab remount. */
  listSessions(): Promise<WireTerminalSessionSummary[]>;
  /** Subscribe to the `terminal:event` stream. The listener fires
   *  for every session — consumers filter by `session_id`. */
  onEvent(listener: TerminalEventListener): Promise<Unlisten>;
}

export interface Env {
  kind: "tauri" | "browser";
  rpc: Rpc;
  /** Persistent JSON-shaped key-value store. See `./types#KVStore`. */
  kv: KVStore;
  /** Open a native folder picker. Returns the absolute path the user
   *  selected, or `null` if they cancelled. Browser stub returns null —
   *  attach flows fall back to the optional path arg there. */
  pickFolder(): Promise<string | null>;
}

/** Detect Tauri by checking for the IPC handle. Done via a runtime probe so
 *  the browser build never needs the Tauri imports at all. */
function isTauri(): boolean {
  // Tauri 2 exposes __TAURI_INTERNALS__ on the window. The check is in a
  // try/catch because some test environments stub `window` weirdly.
  try {
    return typeof window !== "undefined"
      // @ts-expect-error — runtime probe; no @tauri-apps types yet
      && typeof window.__TAURI_INTERNALS__ !== "undefined";
  } catch {
    return false;
  }
}

let cached: Env | null = null;

/** Lazy singleton. The first call detects the host and dynamic-imports the
 *  matching adapter; subsequent calls reuse it. */
export async function getEnv(): Promise<Env> {
  if (cached) return cached;
  if (isTauri()) {
    const mod = await import("./tauri");
    cached = {
      kind: "tauri",
      rpc: mod.rpc,
      kv: await mod.makeKv(),
      pickFolder: mod.pickFolder,
    };
  } else {
    const mod = await import("./browser");
    cached = {
      kind: "browser",
      rpc: mod.rpc,
      kv: mod.kv,
      pickFolder: mod.pickFolder,
    };
  }
  return cached;
}

/** Synchronous accessor for components that have already awaited `getEnv`
 *  during app boot. Throws if called before the first await. */
export function env(): Env {
  if (!cached) {
    throw new Error("env() called before getEnv() has resolved");
  }
  return cached;
}

export type { ArchEvent } from "./types";
