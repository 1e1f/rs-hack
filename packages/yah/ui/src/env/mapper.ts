// RPC `WorkItem` → UI `Ticket` mapper. Lives alongside the env adapter
// because the wire shape (yah-kg/src/anno.rs WorkItemAnno + rpc.rs
// WorkItem) is what gets imported, and the UI Ticket is the consumer.

import type {
  ArchEdge,
  ArchNode,
  ArchSubgraph,
  EdgeKind as ArchEdgeKind,
  Ticket,
  TicketKind,
} from "../types";
import type { EdgeOut, NodeRef, Subgraph, WireWorkItem } from "./types";

const TICKET_KINDS: ReadonlySet<string> = new Set([
  "feature",
  "bug",
  "task",
  "epic",
]);

function asTicketKind(kind: string | undefined): TicketKind | undefined {
  return kind && TICKET_KINDS.has(kind) ? (kind as TicketKind) : undefined;
}

// ---------- Subgraph wire → ArchSubgraph (UI) ----------
//
// The wire `Subgraph` from `arch_subgraph` carries the raw KG node/edge
// payloads (yah-kg/src/{ids,edge}.rs); the GraphPane consumes the
// UI-flavoured `ArchSubgraph` shape. The bulk of this mapper is the
// edge-kind collapse: yah-kg has ~20 edge variants but the canvas
// renders only six. The grouping is deliberate — it preserves the
// "this is structure / this is a call / this is a flow / this is a
// trait impl" distinction the user actually filters on while folding
// the rest down. When new edge kinds land server-side, add them here
// rather than expanding the UI palette.

/** Lower the wire's tagged-union `EdgeKind` to the six visual buckets
 *  GraphPane knows about. Returns `null` when the kind is intentionally
 *  hidden (currently `koda`, which is a placeholder type). */
function mapEdgeKind(kind: EdgeOut["kind"]): ArchEdgeKind | null {
  switch (kind.edge) {
    case "calls":
    case "imports":
    case "references":
    case "macro_invokes":
    case "derived_by":
    case "attributed_by":
    case "bounds":
    case "generated_by":
    case "extends":
    case "decorated_by":
      return "depends_on";
    case "implements":
    case "impl_for":
    case "impl_of_trait":
    case "conforms_to":
      return "implements";
    case "contains":
    case "defines":
    case "re_exports":
    case "refers_to":
      return "context";
    case "flow":
      return "message_flow";
    case "tag":
      return "data_flow";
    case "koda":
      return null;
  }
}

/** Pull the UI-friendly `layer` for a node from its wire `qualified`
 *  name. The KG doesn't yet surface `@arch:layer(...)` annotations as a
 *  first-class field, so we fall back to the leading qualified-name
 *  segment (e.g. `yah_kg::store::index` → `yah_kg`). When the KG starts
 *  emitting layer hints the lookup table here is the natural
 *  consolidation point. */
function deriveLayer(node: NodeRef): string | undefined {
  const head = node.qualified.split(/::|\./)[0]?.trim();
  return head && head.length > 0 ? head : undefined;
}

function nodeRefToArchNode(node: NodeRef): ArchNode {
  return {
    id: node.id,
    shortName: node.label || node.qualified || node.id,
    layer: deriveLayer(node),
    roles: [],
    file: node.file,
    line: node.span.start_line,
  };
}

/** Map the wire `Subgraph` to the UI's `ArchSubgraph`.
 *
 *  Edges whose kind doesn't map to a visual bucket are dropped (see
 *  `mapEdgeKind`). Edges whose endpoints aren't in the returned
 *  `nodes` list are also dropped — GraphPane's mermaid renderer
 *  silently breaks on dangling edges, and the daemon can return them
 *  when an edge sits on the depth boundary. */
export function subgraphToArchSubgraph(
  wire: Subgraph,
  depth: number,
): ArchSubgraph {
  const nodes: ArchNode[] = wire.nodes.map(nodeRefToArchNode);
  const idSet = new Set(nodes.map((n) => n.id));
  const edges: ArchEdge[] = [];
  for (const edge of wire.edges) {
    if (!idSet.has(edge.from) || !idSet.has(edge.to)) continue;
    const kind = mapEdgeKind(edge.kind);
    if (kind === null) continue;
    edges.push({ from: edge.from, to: edge.to, kind });
  }
  return {
    rootId: wire.root,
    depth,
    nodes,
    edges,
  };
}

export function workItemToTicket(wi: WireWorkItem): Ticket {
  const anchor = wi.anchors[0];
  const kind = asTicketKind(wi.anno.kind);
  return {
    id: wi.id,
    title: wi.anno.title,
    itemType: wi.item_type,
    kind,
    // Source omits @yah:status — board treats that as `open`.
    status: wi.anno.status ?? "open",
    assignee: wi.anno.assignee,
    parent: wi.anno.parent,
    phase: wi.anno.phase,
    handoff: wi.anno.handoff,
    nextSteps: wi.anno.next_steps,
    gotchas: wi.anno.gotchas,
    verify: wi.anno.verify,
    seeAlso: wi.anno.see_also,
    file: anchor?.file ?? "",
    line: anchor?.line ?? 0,
    // Inferred epics (relays with bare-R children) aren't flagged on the
    // wire today — only the explicit kind(epic) case lands in Zones.
    isZone: kind === "epic",
    lastModifiedTs: wi.last_modified_ts,
  };
}
