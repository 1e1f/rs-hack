<!-- rs-hack:hack-board:start -->
## hack-board — source-embedded tickets

Work items for this repo live as `@yah:` doc-comment annotations in Rust
source. There is **no separate issue tracker**. Launch the kanban UI with
`rs-hack board serve` (it auto-picks a port from the workspace path).

### Lifecycle

| Column | `@yah:status(...)` | Meaning |
|---|---|---|
| **Epics** | (derived) | Relays that coordinate child relays — see below |
| **Open** | `open` | Unclaimed — also holds `.hack/todo.md` entries (pre-ticket inbox) |
| **Active** | `claimed` or `in-progress` | Someone's working on it |
| **Handoff** | `handoff` | Ready for next agent — use `/handoff` |
| **Review** | `review` or `done` | Awaiting sign-off |

Tickets move between columns by editing their `@yah:status(...)` line in
source *or* by drag-and-drop on the UI (the server rewrites the status
line for you under the same transition matrix). Allowed transitions:

- `open → active`
- `active → open | handoff | review`   (`active → open` is the admin undo)
- `handoff → active | review`
- `review → handoff`

Anything else is refused (UI dims the target column; server returns 409).
The board auto-refreshes either way.

### SDLC rules

Run `rs-hack board rules` for the canonical ruleset (Rule01–Rule12 + Col01
column rule). The same rules are embedded in every continuation prompt
(`rs-hack board tickets --prompt <ID>`). Narrow to a situation with
`--context pickup | finishing | new-work | archive | refactor` — or use
`--format terse` for one-line rules without the *why* / *how to apply*
detail. For a planning-agent snapshot (counts, active owners, handoff
queue, smell), run `rs-hack board status`.

High-leverage rules to remember without looking:

- **Rule01** — first edit on pickup is `@yah:status(in-progress)` on the ticket
- **Rule03** — finishing a phase updates the *existing* relay in place (same R-number);
  new R-numbers only for parallel/independent tracks
- **Col01** — three end-states: more work → **Handoff** (same relay, Rule03);
  tasks met but unverified → **Review** + ping user; human signed off →
  archive. Never self-archive on the same turn you set review, and never
  drop finished work back into Active.
- **Rule04** — `status(done)` is staging; archive is the terminal action

### Epics

An epic is a coordination point, not a unit of work. Declare one with
`@yah:kind(epic)` on a relay; the board also *infers* epic-ness from any
relay that has **bare-R child relays** pointing at it via `@yah:parent(...)`.
Compound sub-tickets (`R007-T1`) never promote their parent to an epic —
a relay with only sub-tickets is still a plain relay-with-subtickets.

Epics get a computed status:

- **active** — at least one child relay is still live (not in `review`)
- **closed** — all children reached `review`/`done` (or have been archived)

Epics live in their own leftmost column on the board. They never appear in
Open / Active / Handoff / Review, so they can't be mistaken for claimable
work. Their own `@yah:status(...)` is ignored once they qualify as an epic.

Archiving an epic while it still has live children returns a 409 — archive
the children first.

### First action on pickup

When an agent claims a ticket, the **first edit** is setting
`@yah:status(in-progress)` on that ticket and saving. That is the claim
signal. Don't start modifying other code until the status line is updated.

### Archiving (not "done")

Tickets don't stay on the board after they ship. Click the `archive` button
on the ticket card — that strips the `@yah:…` annotation lines from source
and appends an audit record to `.hack/events.jsonl`. Treat `status(done)` as
a short-lived staging state, not a resting place.

### The event log

`.hack/events.jsonl` is a derivative audit log (not the source of truth):
`created`, `modified`, `archived`, `disappeared`. The server replays it on
startup and diffs against current source, so tickets that get accidentally
deleted ("clobbered") surface as `disappeared` events and can be restored
from the last-known snapshot.

### Slash commands

- `/comment` — log a progress summary to `.hack/summaries/`
- `/handoff` — write a structured relay for the next agent (`@yah:relay(...)`)
- `/refine` — turn a multi-phase plan into a relay + tickets

If the slash commands aren't available in your harness (or `.claude/commands/`
hasn't been populated yet), each prompt is also reachable as
`rs-hack board prompt <name>` — same content, no install required. Run
`rs-hack board prompt` (no arg) to list them.

### Never pick IDs yourself

Two agents running in parallel will race and both pick the same R-number (or
F/B/T number). Always use `rs-hack board claim` — it takes a file lock, scans
source for the next unused ID, and writes the annotation atomically:

```bash
rs-hack board claim --kind relay \
  --file src/module.rs --title "Short title" \
  --assignee agent:claude --status handoff \
  --handoff "What was completed" --next "First step"
```

Stdout is the claimed ID. Two shapes:

- **Bare relay** (`--kind relay` without `--parent`) → `R008`
- **Compound sub-ticket** (`--kind task|feature|bug` with `--parent R007`) → `R007-T1`, `R007-T2`, … — always `-T` regardless of kind; the feature/bug/task distinction survives as the `@yah:kind(...)` tag.

`--parent` is required for `--kind task|feature|bug`. Orphan bare IDs (`T01`, `F01`, `B01`) are rejected: they collide with compound sub-ticket numbering and scramble per-id event shards across workspaces. For one-off work, `board open --kind relay …` first and attach the task under it.

Add `--json` for `{id, file, line}`.

**Claim a sub-ticket inside the current relay**, don't spin up a new
relay for every chunk: `rs-hack board claim --kind task --parent R012`.
The relay is the baton; sub-tickets are the incremental checkpoints.

### Card actions

Each ticket card has two small buttons in the top-right:

- **prompt** (or **review** when the card is in the Review column) — copies a continuation prompt to the clipboard. Paste into Claude Code / whatever harness; eventually this becomes a direct agent-launch. For review-column cards the prompt is review-mode (verify + approve-or-send-back); for open/handoff it's a pickup prompt (`board tickets --prompt <ID>` output).
- **archive** — click once to arm (surfaces `@yah:verify(...)` commands if any), click again to commit. Strips the `@yah:` lines from source and logs an `archived` event.

Cards collapse by default to keep columns scannable; `▸` in the header expands to show handoff text, next steps, verify commands, and summaries.

### Where annotations go

The scanner parses Rust with `syn` and only reads doc comments attached to:

- **Module-level** (`//!` at file top, or inside `mod foo { //! … }`)
- **Top-level items** via `///` — `struct`, `enum`, `fn`, `impl` blocks, `mod`

It does **not** read `///` on enum variants, struct fields, methods inside
`impl` blocks, consts, statics, type aliases, or trait items. An annotation
placed there is invisible to the board — it will look fine in source but
won't show up as a ticket.

**Default to `//!` at the top of the file.** Use item-level `///` only when
the ticket genuinely tracks one specific top-level item (e.g. a ticket
about `fn foo` lives on `fn foo`). When in doubt, file-level is safest.

### Key annotations

- `@yah:ticket(ID, "title")` / `@yah:relay(ID, "title")` — define the item
- `@yah:kind(feature|bug|task|epic)` — override kind (epic declares a relay as a coordination point)
- `@yah:status(open|claimed|in-progress|handoff|review|done)` — column (ignored for epics)
- `@yah:assignee(agent:name)` — who's working on it
- `@yah:phase(P1)` / `@yah:parent(R001)` — ordering / hierarchy
- `@yah:handoff("…")` — message for the next agent
- `@yah:next("…")` — concrete next step (repeatable)
- `@yah:verify("…")` — how to confirm done (repeatable; rendered in the pickup prompt as fenced bash + a combined `&&` smoke test)
- `@yah:gotcha("…")` — pre-existing breakage / traps the next agent needs to know (repeatable; rendered *above* context in the prompt so they're the first thing read)
- `@yah:assumes("…")` — unverified claim baked into the handoff (repeatable; rendered in the prompt as risks for the next agent to confirm or challenge)
- `@yah:cleanup("…")` — deferred tech debt (repeatable)
- `@arch:see(path/to/doc.md)` — link to architecture docs
<!-- rs-hack:hack-board:end -->
