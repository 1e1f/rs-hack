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
}

export interface WalkSummary {
  filesSeen: number;
  filesIndexed: number;
  filesSkipped: number;
  parseErrors: number;
}

export type IndexReason = "boot" | "file_watch" | "manual" | "agent_edit";

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
