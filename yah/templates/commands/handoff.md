# /handoff — Create or continue a relay for the next agent

You are completing a chunk of work and handing off to the next agent (which may be yourself in a new context). Generate or update the relay so the next agent has everything they need.

## Two flows — decide first

**Continuing the same line of work (most common):** You're moving an existing relay forward. Update the *existing* `@yah:relay(RXXX, ...)` block — **reuse the same R-number**. Use `yah board move RXXX handoff --handoff '…' --next '…'` to rewrite the status and append your payload in place. This is the default when you picked up a relay, did a phase of work, and now need the next agent (or a fresh you) to keep going. A relay in the `handoff` column is a baton waiting to be picked up; a human should never hand the same active relay to two agents at once.

**Spawning a parallel or independent track:** A *new* line of effort that should run alongside (not continue) the original. Use `yah board claim --kind relay` — it allocates the next R-number atomically under the ID lock so two parallel agents can't collide, and writes the annotation straight into the **Active** column with you as assignee. Then `yah board move RYYY handoff --handoff '…' --next '…'` when you're ready to pass the baton. Pass `--parent RXXX` if it's a child relay of an epic.

**Adding a ticket under the current relay (not a new relay):** If you're mid-relay and the next concrete step is a self-contained chunk, claim a *sub-ticket* instead of a new relay: `yah board claim --kind task --parent R012` → prints `R012-T3` (or whatever the next sub-number is). Sub-tickets archive eagerly; the relay persists.

**Declaring an epic:** If what you're creating is a *coordination point* that will own multiple child relays rather than a thread of work itself, pass `--kind epic` to `yah board claim`. The tool emits `@yah:relay(...)` plus `@yah:kind(epic)`. Epics live in their own column on the board, their status is computed from their children (`active` / `closed`), and they can't be archived while children are still live. Example:

```bash
EPIC=$(yah board claim --kind epic --file src/lib.rs --title "ProcessBlock unification")
# EPIC is an R-number, e.g. R010. Subsequent child relays point at it:
yah board claim --kind relay --file src/phase4.rs --title "Phase 4 migration" --parent $EPIC --phase P1
```

When in doubt, choose same-relay — renumbering is churn.

## Same-relay flow

1. **Locate the existing relay** with `yah board tickets --prompt RXXX` if you don't remember where its annotation block lives.

2. **Run `yah board move`** to transition the relay into the handoff column and append your payload in one shot:

```bash
yah board move RXXX handoff \
  --handoff "What YOU just finished in this context." \
  --next "First concrete next step" \
  --next "Second next step" \
  --gotcha "Pre-existing breakage next agent must not chase" \
  --assumes "Thing you baked in but didn't actually verify" \
  --cleanup "Any tech debt you noticed" \
  --verify "cargo test -p my-crate"
```

`move` keeps the same R-number and rewrites the existing annotation block in source — the old `@yah:status(...)` line is replaced, and the new `@yah:handoff(...)`, `@yah:next(...)`, etc. lines are appended to the existing block. Enforces the allowed-transitions matrix (open → active → handoff → review → handoff), so you can't accidentally skip a column.

3. **Run `yah board tickets`** to confirm the relay is in the `handoff` column.

## New-relay flow

**Never pick an R-number yourself.** Two agents running concurrently will collide. Use `yah board claim` — it takes a file lock, scans source for the highest existing R-number, and writes the annotation atomically. It prints the claimed ID.

Two-step: `claim` creates the relay in the **Active** column under your assignee; `move` then transitions it to `handoff` with the payload for the next agent. Splitting these out keeps intent clear and matches the three-verb surface (`open` / `claim` / `move`).

```bash
RID=$(yah board claim \
  --kind relay \
  --file src/module_central_to_the_work.rs \
  --title "Short title of the new track" \
  --phase P1 \
  --parent RXXX \
  --see architecture/path/to/doc.md)

yah board move $RID handoff \
  --handoff "What was already done that this track depends on." \
  --next "First concrete step for the new track" \
  --next "Second step" \
  --verify "cargo test -p my-crate"
```

`claim` stdout is the ID (e.g. `R008`). `--json` gives `{id, file, line}`. The annotation is appended to an existing `//!` doc-block at the top of the file, or prepended if there isn't one. `claim` defaults assignee to the current agent (`$CLAUDE_AGENT` env var if set, otherwise `agent:claude`). Pass `--assignee` to override.

Confirm the new relay appears on the board. `--parent` is only set when the new relay is a child of an epic — leave it off for standalone tracks.

## Field guidelines

- **handoff**: what was completed in this context — files, APIs, what compiles, what tests pass.
- **next**: each `@yah:next(...)` is one actionable item.
- **gotcha**: pre-existing breakage or traps the next agent needs to know *up front*. These render above the context block in the pickup prompt. Use them for things like "`cargo test -p foo --lib` has unrelated compile errors — don't try to fix, that's another dev's WIP." Repeatable.
- **assumes**: claims you baked into the handoff but didn't actually verify. These render as a risks section in the pickup prompt. Use when you believe something works but haven't tested it end-to-end — the next agent can confirm or challenge rather than take it on faith. Repeatable.
- **cleanup**: non-blocking tech debt discovered along the way.
- **verify**: test commands or manual checks that prove this phase is done.
- **parent**: only for new-relay flow — names the epic this is a child of.
- **phase**: when multiple tickets or handoffs need to run in order.

## After writing

Tell the user:
- Whether you updated an existing relay or created a new one
- The R-number and file:line
- That the next agent can pick up with: `yah board tickets --prompt RXXX`
