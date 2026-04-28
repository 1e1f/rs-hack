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
  path?: string; // local rigs (mirrors RigDto.path from app/tauri/src/state.rs)
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
export type ToolKind = "read" | "edit" | "bash" | "grep" | "write";

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
      args: Record<string, any>;
    }
  | {
      id: string;
      t: number;
      role: "tool";
      tool: ToolKind;
      result: any;
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
  | "arch"
  | "agent"
  | "terminal"
  | "preview"
  | "files"
  | "services";

export type TabGroup = "design" | "run";

export type Theme = "light" | "dark";
