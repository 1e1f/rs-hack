// Wire-shape types for the arch.* RPC surface served by yah-tauri.
// Mirrors yah-kg/src/{ids,kind,edge,anno,event,rpc}.rs serde derives.
//
// Keep this file in sync with the contract crate. When the daemon's
// type changes break the wire format, the typecheck here is the first
// thing that catches it on the frontend side.

// ---------- identity ----------

export type NodeId = string; // 32-char hex (16-byte blake3)
export type EdgeId = string; // 32-char hex

export interface Span {
  start_line: number;
  start_col: number;
  end_line: number;
  end_col: number;
}

// ---------- taxonomy ----------

export type Lang = "rust" | "ts" | "yaml" | "json" | "koda";

export type CommonKind =
  | "directory"
  | "file"
  | "module"
  | "type"
  | "function"
  | "method"
  | "field"
  | "variant"
  | "constant"
  | "document"
  | "tag";

export type RustKind =
  | { rust_kind: "trait" }
  | { rust_kind: "impl" }
  | { rust_kind: "assoc_type" }
  | { rust_kind: "assoc_const" }
  | { rust_kind: "macro_decl"; "0": MacroFlavor }
  | { rust_kind: "lifetime" };

export type MacroFlavor = "rules" | "proc_derive" | "proc_attr" | "proc_fn";

export type TsKind =
  | { ts_kind: "interface" }
  | { ts_kind: "type_alias" }
  | { ts_kind: "enum" }
  | { ts_kind: "decorator" }
  | { ts_kind: "jsx_component" };

export type DocKind =
  | { doc_kind: "anchor" }
  | { doc_kind: "property" }
  | { doc_kind: "schema_ref" };

export type KodaKind = { koda_kind: "placeholder" };

export type NodeKind =
  | { lang: "common"; kind: CommonKind }
  | { lang: "rust"; kind: RustKind }
  | { lang: "ts"; kind: TsKind }
  | { lang: "doc"; kind: DocKind }
  | { lang: "koda"; kind: KodaKind };

// ---------- edges ----------

export type EdgeKind =
  // universal structural
  | { edge: "contains" }
  | { edge: "defines" }
  | { edge: "imports" }
  | { edge: "re_exports" }
  | { edge: "calls" }
  | { edge: "references" }
  | { edge: "implements" }
  // rust-specific
  | { edge: "impl_for" }
  | { edge: "impl_of_trait" }
  | { edge: "macro_invokes" }
  | { edge: "derived_by" }
  | { edge: "attributed_by" }
  | { edge: "bounds" }
  | { edge: "generated_by" }
  // ts-specific
  | { edge: "extends" }
  | { edge: "decorated_by" }
  // doc-specific
  | { edge: "refers_to" }
  | { edge: "conforms_to" }
  // annotation overlay
  | { edge: "tag" }
  | { edge: "flow" }
  // koda
  | { edge: "koda"; extra: { koda_edge: "placeholder" } };

export interface EdgeOut {
  id: EdgeId;
  from: NodeId;
  to: NodeId;
  kind: EdgeKind;
  annotations?: string[];
}

// ---------- annotations ----------

export interface TagRef {
  namespace?: string;
  name: string;
}

export type AnnotationKind =
  | { anno: "tag"; "0": TagRef }
  | { anno: "flow"; to_qualified: string; reason?: string }
  | { anno: "rule"; rule_kind: string; args?: string[] };

export interface AnnotationRef {
  anchor: NodeId;
  source_file: string;
  source_line: number;
  kind: AnnotationKind;
}

// ---------- node payloads ----------

export interface NodeRef {
  id: NodeId;
  lang: Lang;
  kind: NodeKind;
  label: string;
  qualified: string;
  file: string;
  span: Span;
  synthetic?: boolean;
}

export interface NodeFull extends NodeRef {
  doc?: string;
  properties?: Record<string, string>;
  annotations?: AnnotationRef[];
}

// ---------- RPC params/results ----------

export interface SubgraphParams {
  root: NodeId;
  depth: number;
  edges?: EdgeKind[];
  kinds?: NodeKind[];
  langs?: Lang[];
  node_limit?: number;
}

export interface Subgraph {
  root: NodeId;
  nodes: NodeRef[];
  edges: EdgeOut[];
  truncated: boolean;
}

export interface LookupParams {
  file: string;
  line?: number;
  col?: number;
}

export interface LookupResult {
  ids: NodeId[];
}

export type Direction = "in" | "out" | "both";

export interface NeighborsParams {
  id: NodeId;
  dir: Direction;
  edges?: EdgeKind[];
}

export interface NeighborsResult {
  edges: EdgeOut[];
}

export interface RootsParams {
  lang?: Lang;
  kind?: NodeKind;
}

export interface RootsResult {
  roots: NodeRef[];
}

export interface StatsResult {
  node_count: number;
  edge_count: number;
  by_lang: Record<string, number>;
  by_kind: Record<string, number>;
  last_index_ms?: number;
}

// ---------- work items (relays + tickets) ----------
//
// Mirrors yah-kg/src/anno.rs WorkItemAnno/WorkItemType/TicketStatus and
// yah-kg/src/rpc.rs WorkItemAnchor/WorkItem. Wire-shape only — the UI
// converts these to the flatter `Ticket` type via workItemToTicket().

export type WireTicketStatus =
  | "open"
  | "claimed"
  | "in-progress"
  | "handoff"
  | "review"
  | "done";

export type WireWorkItemType = "relay" | "ticket";

export interface WireWorkItemAnno {
  id: string;
  title: string;
  kind?: string;
  status?: WireTicketStatus;
  assignee?: string;
  parent?: string;
  phase?: string;
  severity?: string;
  handoff?: string[];
  next_steps?: string[];
  gotchas?: string[];
  assumes?: string[];
  verify?: string[];
  cleanup?: string[];
  /** `@arch:see(path)` — repeatable. Architecture-doc references the
   *  pickup prompt surfaces under "Reference" and the board renders as
   *  yah://arch/doc/<rel> click-throughs on the ticket card. */
  see_also?: string[];
}

export interface WireWorkItemAnchor {
  node: NodeId;
  /** Rig-relative path. */
  file: string;
  /** 1-based. */
  line: number;
  /**
   * The annotation payload as parsed at this anchor. With one anchor per
   * work-item this matches `WireWorkItem.anno`; with multiple anchors the
   * per-anchor copies are what the board-recompute layer compares to
   * detect field-level conflicts.
   */
  anno: WireWorkItemAnno;
}

export interface WireWorkItem {
  id: string;
  node: NodeId;
  item_type: WireWorkItemType;
  anno: WireWorkItemAnno;
  anchors: WireWorkItemAnchor[];
  /**
   * Unix seconds of the most recent event for this id in
   * `.yah/events/<shard>.jsonl` (status moves, scans, archive). Daemon
   * falls back to the source file's mtime when no shard exists, and to
   * `0` when neither is resolvable. Renderer can use this to sort each
   * board column by recency without an extra RPC.
   */
  last_modified_ts: number;
}

/** `arch.list_tickets` and `arch.list_relays` accept no params today. */
export interface ListTicketsParams {}
export interface ListTicketsResult {
  tickets: WireWorkItem[];
}
export interface ListRelaysParams {}
export interface ListRelaysResult {
  relays: WireWorkItem[];
}
export interface GetTicketParams {
  id: string;
}
export interface GetTicketResult {
  ticket: WireWorkItem | null;
}

// ---------- arch.validate ----------
//
// Mirrors yah-kg/src/validate.rs `Scope`, `Severity`, `Violation` and
// yah-kg/src/rpc.rs `ValidateParams` / `ValidateResult`. `Scope` uses
// tagged-struct variants so the JSON shape is self-describing; default
// is `{ scope: "all" }` when params are omitted.

export type WireScope =
  | { scope: "all" }
  | { scope: "subtree"; root: NodeId }
  | { scope: "file"; path: string };

export type WireSeverity = "error" | "warning";

export interface WireViolation {
  /** `rule_kind` from the offending `@yah:rule(...)` directive. Unknown
   *  rule kinds parsed from source still emit a violation under their
   *  original kind string. */
  rule_kind: string;
  /** Structural node carrying the rule annotation. */
  anchor: NodeId;
  /** Rig-relative path to the source line where the rule is authored. */
  anchor_file: string;
  /** 1-based source line the rule annotation lives on. */
  anchor_line: number;
  /** Structural node that triggered the violation. `null` for parse /
   *  vocabulary errors that aren't node-specific. */
  offending?: NodeId | null;
  /** Convenience copy of the offending node's file (rig-relative). */
  offending_file?: string | null;
  /** 1-based start line of the offending node's source span. */
  offending_line?: number | null;
  /** Human-readable message; UI surfaces it as the tooltip. */
  message: string;
  severity: WireSeverity;
}

export interface ValidateParams {
  scope?: WireScope;
}

export interface ValidateResult {
  violations: WireViolation[];
}

// ---------- arch.ticket_prompt ----------
//
// Mirrors yah-kg/src/prompt.rs `PromptMode` and yah-kg/src/rpc.rs
// `TicketPromptParams` / `TicketPromptResult`. The TicketCard buttons
// route through this to keep clipboard payloads byte-equal with
// `yah board show <id> --prompt`.

export type WirePromptMode = "pickup" | "review";

export interface TicketPromptParams {
  id: string;
  mode?: WirePromptMode;
}

export interface TicketPromptResult {
  /** `null` when no work-item bears `params.id`. UI surfaces the miss
   *  rather than throwing. */
  markdown: string | null;
}

// ---------- rig registry (R024-T2) ----------
//
// Mirrors `state::RigDto` on the Tauri side. Used by `rig_list` and
// returned from `rig_attach` / `rig_set_active`. Path + lastActiveAt are
// extra over the renderer's existing `Rig` interface; the env adapter
// keeps them so future UI (rig selector recency sort) can use them.

export type WireRigKind = "local" | "remote";

export interface WireRigDto {
  id: string;
  name: string;
  path: string;
  kind: WireRigKind;
  reachable: boolean;
  lastActiveAt?: number | null;
  /** Remote-only: SSH host. Absent for local rigs. */
  host?: string;
  /** Remote-only: SSH port. `undefined` means "default 22". */
  port?: number;
  /** Remote-only: SSH user. */
  user?: string;
  /** Remote-only: explicit private key path (when set). */
  keyPath?: string;
}

/** Connect-remote-rig modal payload. Mirrors `state::RemoteRigSpec` on
 *  the Tauri side. `host`, `user`, and `workspacePath` are required;
 *  the rest fall back to ssh defaults at connection time. `name`
 *  defaults to `host` when blank. */
export interface WireRemoteRigSpec {
  host: string;
  user: string;
  workspacePath: string;
  port?: number;
  keyPath?: string;
  name?: string;
}

export interface WalkSummary {
  filesSeen: number;
  filesIndexed: number;
  filesSkipped: number;
  parseErrors: number;
}

export type IndexReason = "boot" | "file_watch" | "manual" | "agent_edit";

// ---------- arch.list_authored_files / arch.read_authored_file ----------
//
// Mirrors the daemon's sandboxed view of `<rig_root>/.yah/arch/authored/`.
// The renderer treats `rel_path` as opaque — it only ever round-trips back
// to `read_authored_file`. `name` is the picker-display string (basename
// without extension; nested subfolders surface as `subdir/foo`).

export interface AuthoredFile {
  rel_path: string;
  name: string;
  bytes: number;
}

export interface ListAuthoredFilesResult {
  files: AuthoredFile[];
}

export interface ReadAuthoredFileResult {
  rel_path: string;
  content: string;
  bytes: number;
}

// ---------- key-value store ----------
//
// Async, JSON-shaped persistence. The browser adapter backs this with
// `window.localStorage`; the Tauri adapter wraps `@tauri-apps/plugin-store`.
// Hooks reach for it through `env().kv` to seed UI state from the previous
// session before the daemon's first `index_finished` lands.

export interface KVStore {
  get<T = unknown>(key: string): Promise<T | null>;
  set<T = unknown>(key: string, value: T): Promise<void>;
  remove(key: string): Promise<void>;
  /** Enumerate every key currently in the store. Used for LRU eviction. */
  keys(): Promise<string[]>;
}

// ---------- infra (R027-F6) ----------
//
// Mirrors `app/tauri/src/hetzner.rs::HetznerServer`. Fetched server-side
// from `GET /v1/servers` so the token never reaches the renderer; the
// Infra tab gets back only the parsed list. Subset of Hetzner's upstream
// `Server` schema — extra fields land here as the UI grows.

export interface HetznerServer {
  id: number;
  name: string;
  status: string;
  server_type: string;
  location: string;
  ipv4?: string | null;
  created: string;
}

/** A key already known to the Hetzner project. Wired into a new server's
 *  authorized_keys via `ssh_keys: [id]` on POST /v1/servers. */
export interface HetznerSshKey {
  id: number;
  name: string;
  fingerprint: string;
  public_key: string;
  created: string;
}

/** A key found in (or just written to) `~/.ssh/`. The provision form
 *  surfaces these so the operator can pick a local key + upload it to
 *  Hetzner without manually copy-pasting its public half. */
export interface LocalSshKey {
  name: string;
  public_key_path: string;
  public_key: string;
  fingerprint: string;
  algorithm: string;
  has_private: boolean;
}

/** Renderer-supplied spec for `POST /v1/servers`. Field names mirror the
 *  upstream Hetzner JSON. `ssh_keys` is a list of project-scoped key ids
 *  returned by `listSshKeys` / `uploadSshKey`. */
export interface HetznerCreateServerSpec {
  name: string;
  server_type: string;
  location: string;
  image: string;
  ssh_keys: number[];
}

/** Per-location price for a server type. Strings are decimal EUR — the
 *  upstream API returns them stringified to dodge float rounding. Net
 *  matches the figures on hetzner.com/cloud; gross is incl-VAT. */
export interface HetznerServerTypePrice {
  location: string;
  price_monthly_net: string;
  price_monthly_gross: string;
}

/** Catalogue entry from `GET /v1/server_types`. The provision form
 *  drives its Type dropdown from this list, filtered by whether
 *  `prices[].location` includes the chosen location id. */
export interface HetznerServerType {
  id: number;
  name: string;
  description: string;
  cores: number;
  memory: number;
  disk: number;
  architecture: string;
  cpu_type: string;
  deprecated: boolean;
  prices: HetznerServerTypePrice[];
}

export interface HetznerLocation {
  id: number;
  name: string;
  description: string;
  country: string;
  city: string;
  network_zone: string;
}

/** System image (no snapshots/backups). The API publishes one record
 *  per (name, architecture); the form dedupes by name when picking,
 *  because `POST /v1/servers` matches by name and auto-selects the
 *  variant for the chosen server type's architecture. */
export interface HetznerImage {
  id: number;
  name: string;
  description: string;
  os_flavor: string;
  os_version: string | null;
  architecture: string;
  deprecated: boolean;
}

// ---------- terminal (R030) ----------
//
// Mirrors `app/tauri/src/terminal.rs`. The Tauri side runs the russh
// SSH client and pumps PTY bytes through `terminal:event` payloads;
// the renderer feeds keystrokes back via `terminal_input` and resizes
// via `terminal_resize`.

/** Spec for `terminal_open_ssh`. The Tauri side connects,
 *  authenticates publickey using **only** `keyPath`, allocates a PTY,
 *  and starts a shell. Auth failures resolve as a rejected Promise;
 *  transport errors after auth surface as `error` events on the
 *  stream. yah never auto-discovers `~/.ssh/id_*` and never speaks to
 *  ssh-agent — the renderer must pin a key the operator authorized
 *  for this server (see `App.openTerminalForServer` for the
 *  fingerprint-intersection pick). */
export interface TerminalOpenSpec {
  host: string;
  user?: string;
  port?: number;
  /** Required. Absolute or `~`-relative path to the private key file
   *  yah is authorized to use against `host`. */
  keyPath: string;
  /** Initial PTY size; subsequent resizes go through `terminal_resize`. */
  cols?: number;
  rows?: number;
  /** Display label for the session rail. Defaults to `user@host`. */
  label?: string;
}

/** Spec for `terminal_open_local`. Spawns the user's shell in a local
 *  PTY (no SSH) — useful for isolating renderer issues from
 *  remote-shell quirks (MOTDs, prompt scripts, alt-charset bleed).
 *  Defaults: shell=$SHELL || /bin/bash, cwd=$HOME. */
export interface TerminalOpenLocalSpec {
  shell?: string;
  cwd?: string;
  cols?: number;
  rows?: number;
  label?: string;
}

/** Mirror of `TerminalSessionSummary` (camelCase on the wire). */
export interface WireTerminalSessionSummary {
  sessionId: string;
  host: string;
  user: string;
  label: string;
  createdAtMs: number;
}

/** Discriminated stream from `terminal:event`. Field names follow the
 *  Rust struct's snake_case (the variant fields aren't renamed). */
export type WireTerminalEvent =
  | { kind: "ready"; session_id: string }
  | { kind: "host_key"; session_id: string; fingerprint: string }
  | { kind: "data"; session_id: string; bytes_b64: string }
  | { kind: "closed"; session_id: string; reason: string }
  | { kind: "error"; session_id: string; message: string };

// ---------- events ----------

export type ChangedField =
  | "span"
  | "label"
  | "qualified"
  | "file"
  | "doc"
  | "properties"
  | "annotations";

export type IndexScope =
  | { scope: "all" }
  | { scope: "files"; paths: string[] }
  | { scope: "subtree"; root: string };

export type ArchEvent =
  | { event: "index_started"; reason: IndexReason; scope: IndexScope }
  | {
      event: "index_finished";
      duration_ms: number;
      nodes_added: number;
      nodes_changed: number;
      nodes_removed: number;
      edges_added: number;
      edges_removed: number;
    }
  | { event: "node_added"; node: NodeRef }
  | { event: "node_changed"; id: NodeId; fields: ChangedField[] }
  | { event: "node_removed"; id: NodeId }
  | { event: "edge_added"; edge: EdgeOut }
  | { event: "edge_removed"; id: EdgeId }
  | {
      event: "agent_touch";
      ids: NodeId[];
      tool: string;
      relay: string;
      ts: number;
    };

// ---------- subprocess + service liveness probes ----------
//
// Backs the AgentProvidersPanel's "Claude (PVd)" card and the Ollama
// "Local serve" subline. Mirrors `app/tauri/src/claude_cli.rs` —
// camelCase on the wire matches the `serde(rename_all = "camelCase")`
// derive on the Rust structs.

export interface ClaudeCliProbe {
  /** True iff `claude --version` exited successfully within the host timeout. */
  installed: boolean;
  /** First line of stdout from `claude --version` — verbatim. */
  version?: string | null;
  /** Resolved path from `which claude`. */
  path?: string | null;
  /** Failure reason for spawn errors / timeouts / non-zero exits. */
  error?: string | null;
}

export interface OllamaServeProbe {
  /** True iff `localhost:11434/api/tags` answered 2xx within the host timeout. */
  running: boolean;
  /** Populated only when the upstream answered a non-success status; the
   *  common "nothing on 11434" case leaves this `null` so the UI can
   *  render "not running" without a noisy error blob. */
  error?: string | null;
}

// ---------- agent runtime ----------

/** Stable id for an agent session. Format: `session:<8 hex>` (Tauri host
 *  mints these). Other runners may pick another scheme — the contract is
 *  "stable string, unique within the host process". */
export type SessionId = string;

/** One streamed event from an agent session. Mirrors
 *  `yah_kg::agent::AgentEvent` (see yah-kg/src/agent.rs). The Tauri seam
 *  prepends `rigId` for renderer routing — see [`WireRigAgentEvent`]. */
export type WireAgentEvent =
  | {
      kind: "session_started";
      sessionId: SessionId;
      ticketId: string;
      engine: string;
      cacheKey: string;
      estimatedTokens: number;
      ringDepth: number;
    }
  | { kind: "turn_started"; sessionId: SessionId }
  | { kind: "message_delta"; sessionId: SessionId; text: string }
  | {
      kind: "turn_ended";
      sessionId: SessionId;
      text: string;
      stopReason?: string;
    }
  | {
      kind: "turn_failed";
      sessionId: SessionId;
      text: string;
      message: string;
    }
  | { kind: "session_ended"; sessionId: SessionId }
  | { kind: "error"; sessionId: SessionId; message: string }
  | {
      kind: "tool_call";
      sessionId: SessionId;
      toolCallId: string;
      toolName: string;
      args: unknown;
    }
  | {
      kind: "tool_result";
      sessionId: SessionId;
      toolCallId: string;
      ok: boolean;
      result: unknown;
    }
  /** Write tool needs user approval before it runs (R031-F5). The chat
   *  pane renders an inline approval row keyed to `requestId`; the user
   *  posts their reply through `agent.approval.decide`. The gate awaits
   *  that reply before dispatching the tool — no `tool_call` /
   *  `tool_result` is emitted until/unless the user clicks Apply or
   *  AlwaysAllow. `bash` is set when the call is the bash tool — it
   *  carries the parsed `{ env, cmd, args[] }` so the renderer can show
   *  a structured row and pre-fill the AlwaysAllow rule. */
  | {
      kind: "approval_requested";
      sessionId: SessionId;
      requestId: string;
      toolName: string;
      args: unknown;
      bash?: WireBashCall;
    }
  /** User resolved a pending approval. Mostly informational — the gate
   *  reacts internally; this variant lets every chat pane drop the
   *  inline approval row even when the click happened on a different
   *  surface (e.g. a future "approve all" affordance). */
  | {
      kind: "approval_resolved";
      sessionId: SessionId;
      requestId: string;
      decision: "apply" | "skip" | "always-allow";
    };

/** Parsed bash invocation. Mirrors `agent_approval::BashCall` —
 *  `env: { KEY: VAL }`, `cmd`, `args[]`. The agent never sees the
 *  re-synthesized command line; the gate rebuilds it from the
 *  *approved* fields so an attacker can't sneak unapproved env or args
 *  past a regex match. */
export interface WireBashCall {
  env: Record<string, string>;
  cmd: string;
  args: string[];
}

/** One approval rule. Matches the *parsed* tool call, never a rendered
 *  string. Mirrors `agent_approval::ApprovalRule`. */
export type WireApprovalRule =
  | { kind: "tool"; name: string }
  | { kind: "tool_path"; name: string; glob: string }
  | { kind: "bash_cmd"; cmd: string }
  | { kind: "bash_cmd_pattern"; cmd: string; args: WireArgPattern[] };

/** Per-arg matcher inside `bash_cmd_pattern`. */
export type WireArgPattern =
  | { kind: "exact"; value: string }
  | { kind: "any" };

/** User's reply to an inline approval prompt. */
export type WireApprovalChoice =
  | { kind: "apply" }
  | { kind: "skip" }
  | { kind: "always-allow"; rule: WireApprovalRule };

/** Versioned envelope for the persisted ruleset (matches
 *  `agent_approval::ApprovalRuleset`). The Settings UI ignores
 *  unknown `version` tags — those are produced by a future client
 *  this build doesn't know how to render. */
export interface WireApprovalRulesetV1 {
  version: "1";
  rules: WireApprovalRule[];
}
export type WireApprovalRuleset = WireApprovalRulesetV1;

/** Versioned per-rig agent runtime settings (R031-F5 production flip).
 *  Today: just `agentWritersEnabled`. Mirrors `agent_settings::AgentSettings`. */
export interface WireAgentSettingsV1 {
  version: "1";
  agentWritersEnabled: boolean;
}
export type WireAgentSettings = WireAgentSettingsV1;

/** Wire shape on the `agent:event` channel. Rig-tagged envelope around
 *  the runtime AgentEvent so the renderer can fan one listener out
 *  across multiple rigs. */
export type WireRigAgentEvent = WireAgentEvent & { rigId: string };

/** Carries the prelude's metadata so the renderer can show the budget
 *  gauge / engine pill without a follow-up RPC. Mirrors
 *  `app/tauri/src/agent.rs::StartSessionResult`. */
export interface WireStartSessionResult {
  sessionId: SessionId;
  ticketId: string;
  engine: string;
  model: string;
  cacheKey: string;
  estimatedTokens: number;
  ringDepth: number;
  truncated: boolean;
}

/** Snapshot of one live session for the AgentView session-list rail. */
export interface WireSessionSummary {
  sessionId: SessionId;
  rigId: string;
  ticketId: string;
  engine: string;
  turns: number;
  running: boolean;
}

/** Sidecar metadata produced by the (re)indexer for one historical
 *  session. Mirrors `session_history::SessionMeta`. Tags are populated
 *  by the future LLM-backed indexer; the stub leaves them empty. */
export interface WireSessionMeta {
  title: string;
  summary: string;
  tags: string[];
  ticketId?: string;
  engine?: string;
  indexedAt: number;
  turnCount: number;
  toolCallCount: number;
  toolFailCount: number;
  firstEventAt?: number;
  lastEventAt?: number;
}

/** One row from `agent.history.list` — pairs the on-disk jsonl with
 *  its sidecar meta (if any). `stale` is true when the meta is missing
 *  or `indexedAt < jsonlMtime` — surfaces as the "needs re-index" dot
 *  in the rail. Mirror of `session_history::SessionHistoryRow`. */
export interface WireSessionHistoryRow {
  sessionId: SessionId;
  jsonlMtime: number;
  bytes: number;
  meta?: WireSessionMeta;
  stale: boolean;
}

/** Where the private half of an Identity lives. Mirror of
 *  `identities::IdentitySource` — tagged on `kind`. */
export type WireIdentitySource =
  | { kind: "yahGenerated"; privateKeyPath: string }
  | {
      kind: "imported";
      privateKeyPath?: string | null;
      publicKeyPath: string;
    };

/** One "this identity is registered at <target>" record. Cache with
 *  `lastSeen` timestamps — probes reconcile against ground truth. */
export type WireAuthorization =
  | {
      kind: "hetzner";
      projectId: string;
      keyIdInHetzner: number;
      name: string;
      lastSeen: number;
    }
  | {
      kind: "github";
      account: string;
      keyId: number;
      title: string;
      lastSeen: number;
    }
  | {
      kind: "gitlab";
      instance: string;
      account: string;
      keyId: number;
      title: string;
      lastSeen: number;
    }
  | {
      kind: "sshHost";
      userAtHost: string;
      lastSeen: number;
    };

/** Single SSH keypair record. `id` = SHA256 fingerprint of the public
 *  key. Mirror of `identities::Identity`. */
export interface WireIdentity {
  id: string;
  name: string;
  algorithm: string;
  publicKey: string;
  source: WireIdentitySource;
  authorizedAt: WireAuthorization[];
  createdAt: number;
  lastUsedAt?: number | null;
}

/** Per-provider outcome inside a [`WireProbeReport`]. `ok` counts the
 *  identities that got a (new or refreshed) Authorization; `skipped` is
 *  the no-PAT path the UI surfaces as a configure-token nudge; `error`
 *  is a real upstream failure. */
export type WireProbeOutcome =
  | { kind: "ok"; matches: number }
  | { kind: "skipped"; reason: string }
  | { kind: "error"; reason: string };

/** Result of a fan-out probe pass — local discovery + per-provider
 *  outcomes. Mirror of `identities::ProbeReport`. */
export interface WireProbeReport {
  identitiesTotal: number;
  localAdded: number;
  hetzner: WireProbeOutcome;
  github: WireProbeOutcome;
}

/** Per-identity probe outcome for "re-check this row" actions. */
export type WireSingleProbeResult =
  | { kind: "found"; authorization: WireAuthorization }
  | { kind: "notFound" }
  | { kind: "skipped"; reason: string }
  | { kind: "error"; reason: string };

/* ---------- Files tab: dir.list / dir.watch / file.event wire types ---------- */

/** Mirror of `rpc::DirEntryKind`. Symlinks-to-targets are
 *  reported by the target's kind (`Dir`/`File`); broken symlinks come
 *  back as `Other` with `is_symlink: true`. */
export type WireDirEntryKind = "file" | "dir" | "other";

/** Mirror of `rpc::DirEntry`. `name` is the basename only — the
 *  parent path lives on the parent listing. */
export interface WireDirEntry {
  name: string;
  kind: WireDirEntryKind;
  /** File size in bytes for regular files; `0` for directories. */
  size: number;
  /** Modification time in milliseconds since the Unix epoch. `null` if
   *  the platform / filesystem can't report it. */
  mtime_ms?: number | null;
  /** True when the source entry is a symlink (kind reflects the
   *  target). */
  is_symlink?: boolean;
}

/** Mirror of `rpc::DirListResult`. `path` is the rig-relative
 *  parent path (empty when the rig root was listed). */
export interface WireDirListResult {
  path: string;
  entries: WireDirEntry[];
}

/** Watch handle id returned by `dir.watch` / `file.watch`. Per-rig-process
 *  stable until the daemon restarts; renderers re-watch on reconnect
 *  rather than caching ids across sessions. */
export type WireWatchId = number;

/** Mirror of `yah_kg::event::FileEventKind`. `notify` events are
 *  debounced and reclassified at emit; consumers should treat any event
 *  as "this path may have changed". */
export type WireFileEventKind = "created" | "modified" | "removed";

/* ---------- Files tab: file.read wire types (R033-T7) ---------- */

/** Mirror of `rpc::FileEncoding`. `utf8` means `content` is the file
 *  slice as a UTF-8 string; `base64` means `content` is base64-encoded
 *  bytes (renderer should fall back to the binary view). */
export type WireFileEncoding = "utf8" | "base64";

/** Mirror of `rpc::FileReadRange`. Used to page through files larger
 *  than the daemon's 5MB soft cap; omit the range for a one-shot read
 *  that may surface `truncated: true`. */
export interface WireFileReadRange {
  offset: number;
  len: number;
}

/** Mirror of `rpc::FileReadResult`. `bytes` is the size of the slice
 *  in `content`; `total_bytes` is the file size on disk. `truncated`
 *  fires only when the soft cap clipped a no-range read. */
export interface WireFileReadResult {
  path: string;
  content: string;
  encoding: WireFileEncoding;
  bytes: number;
  total_bytes: number;
  offset: number;
  eof: boolean;
  truncated: boolean;
}

/** Mirror of `yah_kg::event::FileEvent` after the Tauri side stamps
 *  rig_id (event_bridge.rs::RigFileEvent). Path is rig-relative,
 *  POSIX-separated. `mtime_ms` is `null` for `removed` events and on
 *  platforms that can't report it. */
export interface WireRigFileEvent {
  rig_id: string;
  watch_id: WireWatchId;
  kind: WireFileEventKind;
  path: string;
  mtime_ms?: number | null;
}
