import type { Ticket, TicketStatus } from "../types";

/* Relays follow their children. The user-typed `@yah:status(...)` on a
   relay in source is treated as a fallback for relays that have no
   children yet; once children exist, the relay's effective status is
   computed from them.

   Precedence:
     active   →  any child in claimed | in-progress
     handoff  →  else any child in handoff
                 OR (any review AND any open) — partial completion with
                 work outstanding reads as a checkpoint, not "not started"
     open     →  else any child in open (none reviewed yet either)
     review   →  else (all children in review | done)

   Epic relays (children are themselves relays) recurse — child relay
   statuses are computed first, then the parent uses those.

   The Rust daemon now applies the same derivation server-side in
   `arch.list_relays` (see `yah_kg::board::apply_derived_relay_fields`),
   so for the Tauri path this is a no-op — the relays already arrive
   pre-derived. Kept on the renderer as defense-in-depth (the algorithm
   is idempotent) and as the primary derivation for the browser-stub
   path, which short-circuits the daemon. */

type Bucket = "active" | "open" | "handoff" | "review";

function bucketOf(s: TicketStatus): Bucket {
  switch (s) {
    case "claimed":
    case "in-progress":
      return "active";
    case "open":
      return "open";
    case "handoff":
      return "handoff";
    case "review":
    case "done":
      return "review";
  }
}

function statusForBucket(b: Bucket): TicketStatus {
  switch (b) {
    case "active":
      return "in-progress";
    case "open":
      return "open";
    case "handoff":
      return "handoff";
    case "review":
      return "review";
  }
}

function deriveFromBuckets(seen: Set<Bucket>): Bucket {
  if (seen.has("active")) return "active";
  if (seen.has("handoff")) return "handoff";
  /* Partial-completion rule: some children reviewed AND some not yet
     picked up means the relay is at a checkpoint, not "open". Reads as
     handoff so reviewers know there's work waiting. */
  if (seen.has("review") && seen.has("open")) return "handoff";
  if (seen.has("open")) return "open";
  return "review";
}

/** For each relay in `tickets`, compute its derived status from its
 *  children. Returns a map keyed by relay id; relays without children
 *  are absent (caller keeps their source status). */
export function deriveRelayStatuses(
  tickets: Ticket[],
): Map<string, TicketStatus> {
  const byId = new Map<string, Ticket>();
  const childrenByParent = new Map<string, Ticket[]>();
  for (const t of tickets) {
    byId.set(t.id, t);
    if (t.parent) {
      const arr = childrenByParent.get(t.parent);
      if (arr) arr.push(t);
      else childrenByParent.set(t.parent, [t]);
    }
  }

  const memo = new Map<string, TicketStatus>();
  const visiting = new Set<string>();

  function effective(id: string): TicketStatus {
    const cached = memo.get(id);
    if (cached !== undefined) return cached;
    const item = byId.get(id);
    if (!item) return "open";
    /* Cycle guard — if we revisit, treat as the source status to break
       the loop. Cycles shouldn't happen in well-formed boards but the
       parent-pointer is user-written so it can be malformed. */
    if (visiting.has(id)) return item.status;

    if (item.itemType !== "relay") {
      memo.set(id, item.status);
      return item.status;
    }

    const children = childrenByParent.get(id);
    if (!children || children.length === 0) {
      memo.set(id, item.status);
      return item.status;
    }

    visiting.add(id);
    const seen = new Set<Bucket>();
    for (const c of children) {
      seen.add(bucketOf(effective(c.id)));
    }
    visiting.delete(id);

    const derived = statusForBucket(deriveFromBuckets(seen));
    memo.set(id, derived);
    return derived;
  }

  const out = new Map<string, TicketStatus>();
  for (const t of tickets) {
    if (t.itemType !== "relay") continue;
    if (!childrenByParent.has(t.id)) continue;
    out.set(t.id, effective(t.id));
  }
  return out;
}

/** For each relay with children, compute the most-recent
 *  `lastModifiedTs` across itself + every descendant (recursive through
 *  child relays). Lets the board column sort lift a relay to the top
 *  whenever any of its sub-tickets was just touched, even if the
 *  relay's own shard hasn't seen a write recently. */
export function aggregateRelayLastModified(
  tickets: Ticket[],
): Map<string, number> {
  const byId = new Map<string, Ticket>();
  const childrenByParent = new Map<string, Ticket[]>();
  for (const t of tickets) {
    byId.set(t.id, t);
    if (t.parent) {
      const arr = childrenByParent.get(t.parent);
      if (arr) arr.push(t);
      else childrenByParent.set(t.parent, [t]);
    }
  }

  const memo = new Map<string, number>();
  const visiting = new Set<string>();

  function effective(id: string): number {
    const cached = memo.get(id);
    if (cached !== undefined) return cached;
    const item = byId.get(id);
    if (!item) return 0;
    const own = item.lastModifiedTs ?? 0;
    if (visiting.has(id)) return own;
    if (item.itemType !== "relay") {
      memo.set(id, own);
      return own;
    }
    const children = childrenByParent.get(id);
    if (!children || children.length === 0) {
      memo.set(id, own);
      return own;
    }
    visiting.add(id);
    let max = own;
    for (const c of children) {
      const childTs = effective(c.id);
      if (childTs > max) max = childTs;
    }
    visiting.delete(id);
    memo.set(id, max);
    return max;
  }

  const out = new Map<string, number>();
  for (const t of tickets) {
    if (t.itemType !== "relay") continue;
    if (!childrenByParent.has(t.id)) continue;
    out.set(t.id, effective(t.id));
  }
  return out;
}

/** Apply both relay-derive passes (status + lastModifiedTs) and return a
 *  new array with the derived values substituted. Tickets and childless
 *  relays are passed through untouched (referentially equal). */
export function withDerivedRelayFields(tickets: Ticket[]): Ticket[] {
  const status = deriveRelayStatuses(tickets);
  const lastTs = aggregateRelayLastModified(tickets);
  if (status.size === 0 && lastTs.size === 0) return tickets;
  return tickets.map((t) => {
    const nextStatus = status.get(t.id);
    const nextTs = lastTs.get(t.id);
    const statusChanged = nextStatus !== undefined && nextStatus !== t.status;
    const tsChanged = nextTs !== undefined && nextTs !== t.lastModifiedTs;
    if (!statusChanged && !tsChanged) return t;
    return {
      ...t,
      ...(statusChanged ? { status: nextStatus! } : {}),
      ...(tsChanged ? { lastModifiedTs: nextTs! } : {}),
    };
  });
}

/** @deprecated Use `withDerivedRelayFields` — also rolls up
 *  `lastModifiedTs`. Kept as a thin alias so existing imports compile. */
export const withDerivedRelayStatuses = withDerivedRelayFields;
