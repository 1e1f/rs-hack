// Domain types — shape mirrors what the yah Rust backend will serve.
// Keep these in sync with rs-hack-arch/src/ticket.rs, graph.rs, and the
// pi-mono session JSONL format.

export type TicketKind = "feature" | "bug" | "task" | "epic";
export type TicketStatus =
  | "open"
  | "claimed"
  | "in-progress"
  | "handoff"
  | "review"
  | "done";
export type ColumnKey = "zones" | "open" | "active" | "handoff" | "review";

export interface Ticket {
  id: string;
  title: string;
  itemType: "ticket" | "relay";
  kind?: TicketKind;
  status: TicketStatus;
  assignee?: string;
  parent?: string;
  phase?: string;
  handoff?: string[];
  nextSteps?: string[];
  gotchas?: string[];
  verify?: string[];
  file: string;
  line: number;
  // Zone-only: per-child counts. `relays` is the count of child *relays* —
  // when a relay has both child relays AND (open|active|handoff > relays)
  // worth of loose tickets, the card surfaces a "mixed children" smell.
  childCounts?: { open: number; active: number; handoff: number; relays?: number };
  isZone?: boolean;
  /* Unix seconds of the most recent event for this id (status moves, scans,
     archive). Sourced from `WireWorkItem.last_modified_ts`; daemon falls
     back to the source file's mtime when no shard exists. Used as the
     primary sort key per board column. */
  lastModifiedTs?: number;
}

export interface Rig {
  id: string;
  name: string;
  kind: "local" | "remote";
  host?: string; // remote rigs only
  /* For local rigs: filesystem path. For remote rigs: the *remote*
     workspace path the daemon will index — same field, different
     hosts. RigSelector formats accordingly. */
  path?: string;
  /* Remote-only spec, mirrored from RigDto so the selector can show
     `user@host:port` and a future "Edit remote rig…" affordance can
     prefill the modal without a fresh round-trip. `undefined` for
     locals. */
  port?: number;
  user?: string;
  keyPath?: string;
  reachable: boolean;
  /* Count of items wanting human attention on this rig — handoff tickets
     today, plus Col01 smell hits later. Surfaces as a brass pill in the
     RigSelector menu and an oxblood pip on the title-bar dot. */
  needsAttention?: number;
  /* Unix-ms of the last `set_active` for sort-by-recency in the picker. */
  lastActiveAt?: number;
}

// Architecture graph
export type EdgeKind =
  | "depends_on"
  | "message_flow"
  | "data_flow"
  | "bridge"
  | "context"
  | "implements";

export interface ArchNode {
  id: string;
  shortName: string;
  layer?: string;
  roles: string[];
  doc?: string;
  file: string;
  line: number;
}

export interface ArchEdge {
  from: string;
  to: string;
  kind: EdgeKind;
  reason?: string;
}

export interface ArchSubgraph {
  rootId: string;
  depth: number;
  nodes: ArchNode[];
  edges: ArchEdge[];
}

// Agent (pi-mono session)
//
// `ToolKind` is a closed set of *visual* surfaces the renderer knows how to
// draw. The wire layer passes the host registry's tool name (e.g.
// `read_file`, `arch_neighbors`) which maps onto one of these kinds via
// `mapWireToolName` in useChatSession — keeping the renderer ignorant of
// every host-side tool added in the future.
export type ToolKind =
  | "read"
  | "edit"
  | "bash"
  | "grep"
  | "write"
  | "list_dir"
  | "arch_node"
  | "arch_neighbors"
  | "arch_subgraph"
  | "arch_lookup"
  | "read_arch_doc";

export type SessionEvent =
  | { id: string; t: number; role: "user"; content: string }
  | { id: string; t: number; role: "assistant"; type: "text"; content: string }
  | { id: string; t: number; role: "assistant"; type: "thinking"; content: string }
  | {
      id: string;
      t: number;
      role: "assistant";
      type: "tool_use";
      tool: ToolKind;
      /* Provider-issued id for the call. Optional for back-compat with
         mocks that predate the runner's tool_call_id wire field; when
         present, the renderer pairs tool_use / tool by this rather than
         relying on event adjacency. */
      toolCallId?: string;
      args: Record<string, any>;
    }
  | {
      id: string;
      t: number;
      role: "tool";
      tool: ToolKind;
      toolCallId?: string;
      /* Mirrors `AgentEvent::ToolResult.ok` — `false` means the tool
         surfaced an in-band error (the model still sees it and adapts).
         Optional so legacy mocks rendering without an explicit ok still
         render. */
      ok?: boolean;
      result: any;
    }
  /* Inline write-tool approval prompt (R031-F5). useChatSession injects
     one of these on `approval_requested`; the matching `approval_resolved`
     flips `status` to `"resolved"` and stamps `decision`. The chat pane
     renders this via `ApprovalRow`, which posts the user's choice back
     through `agent.approval.decide`. `bash` is set when the call is the
     bash tool so the row shows env / cmd / args structurally and the
     AlwaysAllow rule can be pre-filled as `BashCmdPattern`. */
  | {
      id: string;
      t: number;
      role: "approval";
      requestId: string;
      toolName: string;
      args: Record<string, any>;
      bash?: { env: Record<string, string>; cmd: string; args: string[] };
      status: "pending" | "resolved";
      decision?: "apply" | "skip" | "always-allow";
    };

export interface Session {
  relayId: string;
  model: string;
  tokens: number;
  status: "idle" | "streaming" | "waiting" | "error";
  lastActive: number;
  events: SessionEvent[];
}

export type Tab =
  | "board"
  | "agent"
  | "arch"
  | "files"
  | "terminal"
  | "preview"
  | "infra"
  | "services"
  | "analytics";

export type TabGroup = "design" | "test" | "host";

export type Theme = "light" | "dark";
