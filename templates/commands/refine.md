# /refine — Refine phases into relays + tickets on the hack-board

You just described an implementation plan with phases. Refine them into hack-board items so the work is trackable and forkable.

## Model

- **Relay** — a thread of work. One agent owns it. Carries the baton across context resets.
- **Ticket** — an incremental work unit *inside* a relay. Usually session-sized — claim, work, archive. IDs look like `R007-T1`, `R007-T2` when scoped under a relay, or bare (`T03`, `F02`, `B01`) for one-off standalone work with no parent.
- **Epic** — a relay declared with `@hack:kind(epic)`, or *inferred* when one or more other **bare-R relays** declare `@hack:parent(RXXX)` pointing at it. Coordination-of-relays, not coordination-of-tickets — sub-tickets never promote their parent relay to epic.
- **Phase** — ordering tag. "These items ship together." `@hack:phase(P1)`
- **Parent** — hierarchy pointer. `@hack:parent(R007)` belongs to R007. For compound IDs the parent is inferred from the prefix.

## Process

### Step 0: Scan what's already in flight

Before you plan anything, run:

```bash
rs-hack board inflight
```

This prints every Open / Active / Handoff relay and ticket with its one-line purpose and arch-doc ref. Five agents refining in parallel will independently plan the same problem unless they look first — R10. Read the list and decide:

- **This problem is already a live relay** → don't refine. Claim it (`rs-hack board claim <ID>` if it's Open, or `rs-hack board move <ID> active` if it's Handoff) and continue its plan rather than starting over.
- **It partially overlaps an existing relay** → open your next steps as sub-tickets under that relay (`rs-hack board open --kind task --parent R<n>`, per R8) instead of a new relay.
- **It's genuinely independent** → proceed below. When you write the arch doc, reference any adjacent relays so the next picker sees the relationship.

### Step 1: Write the architecture doc

Create `architecture/{topic}.md` with the full plan. Keep the prose — it's context future agents need.

### Step 2: Create the relay (or relay chain)

**Never pick IDs yourself.** Use `rs-hack board open` when refining a plan — it scans source under a file lock, picks the next unused ID for the requested kind, and writes the annotation block straight into the **Open** column (unclaimed, no assignee). That's the only safe way to avoid ID collisions with another agent working in parallel, and `open` makes the intent explicit: these are inbox items waiting for someone to take them on.

```bash
# The overall effort as a relay (capture the printed ID — it's R-something)
RELAY=$(rs-hack board open \
  --kind relay \
  --file src/module_central_to_the_work.rs \
  --title "ProcessBlock Unification" \
  --see architecture/processblock_unification.md)
echo "$RELAY"   # e.g. R012
```

For epics (multiple independent tracks), open a parent relay first, then open children with `--parent $RELAY`:

```bash
R_EPIC=$(rs-hack board open --kind relay --file src/lib.rs --title "ProcessBlock epic")
rs-hack board open --kind relay --file src/phase4.rs --title "Phase 4 migration" --parent $R_EPIC --phase P1
rs-hack board open --kind relay --file src/cv_bridge.rs --title "CV Port Bridge" --parent $R_EPIC --phase P2
```

### Step 3: Create tickets under the relay

Each concrete sub-step becomes a **ticket inside** the relay. Use `rs-hack board open --parent $RELAY`; the kind (feature/bug/task) becomes a `@hack:kind(...)` tag. The ID is allocated as a compound sub-ticket.

```bash
TID=$(rs-hack board open \
  --kind task \
  --file src/rbj_biquad_node.rs \
  --title "Add cv_to_hz to RbjBiquadNode" \
  --parent $RELAY \
  --phase P1)
echo "$TID"   # e.g. R012-T1 (first sub-ticket under R012)
```

Sub-tickets under `$RELAY` get IDs like `R012-T1`, `R012-T2`, … regardless of `--kind`. The `-T` segment is always `T`; the feature/bug/task distinction survives as the `@hack:kind(...)` tag (and as the badge letter on the card).

Standalone one-off tickets (`--kind task` with no `--parent`) still get bare IDs: `T03`, `F02`, `B01`. Use only when the work is self-contained and has no coordinating relay.

Use `--json` for `{id, file, line}` if you're chaining commands.

### Step 4: Post a summary

```
hack_summary(
  text: "Created CV Port Bridge plan: R012 with 8 tickets across 3 phases.",
  author: "agent:claude"
)
```

### Step 5: Confirm

Tell the user:
- The architecture doc path
- The relay IDs and what they map to
- The ticket IDs created, grouped by phase
- Which phase is ready to start

## Example

```
Created:
  architecture/cv_port_bridge.md — full plan

  R012: CV Port Bridging (parent: R010)

  P1 — hardcoded fix (ready to start):
    R012-T1: V/Oct params produce wrong Hz           (kind: bug)     [open]
    R012-T2: Add cv_to_hz to RbjBiquadNode.process   (kind: task)    [open]
    R012-T3: Add cv_to_hz to CascadeFilter.process   (kind: task)    [open]
    R012-T4: Add cv_to_hz to LFO.process             (kind: task)    [open]

  P2 — infrastructure:
    R012-T5: CVMapping::pull() method                (kind: feature) [open]
    R012-T6: Wire prebaked fn pointer per CV input                   [open]
    R012-T7: Replace hardcoded cv_to_hz with pull()                  [open]

  P3 — cleanup:
    R012-T8: Remove dead n2v_scale/n2v_offset                        [open]
    R012-T9: Update stale doc comments                               [open]
```

## Tips

- Don't over-ticket. "Delete some dead code" is one task, not one per file.
- If the agent said "Want me to start?" — after creating tickets, run `rs-hack board claim <first-P1-ID>` to flip that ticket into Active.
- Phases can run in parallel if independent. The relay owner decides.
