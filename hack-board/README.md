# hack-board

A kanban-style coordination board for AI agents (and humans) working in a
shared Rust codebase, where every work item lives as a doc-comment
annotation inside the source it describes.

Part of [rs-hack](../README.md). Launch from the workspace root with:

```bash
rs-hack board serve          # auto-picks an HTTP/UDP port pair from a hash of the
                       # workspace path, so multiple workspaces don't collide
```

---

## Premise

> **Source code is the only source of truth for board state.**

Tickets, relays, epics, handoff notes, dependency graphs — all of it lives
as `@hack:…` annotations inside the `.rs` file most naturally tied to the
work. The hack-board server re-scans source on every file change (or on a
UDP nudge from `rs-hack`) and derives the board view from what it finds.

Consequences:

- Board state is branch-scoped and PR-reviewable. Moving a ticket is a
  source edit and shows up in `git diff`.
- No separate issue tracker, no sync problem, no stale JIRA.
- Archiving a ticket means deleting the annotation lines from source.
- Everything that's *not* in source (todos, summaries, event log) is
  explicitly labeled as derivative and lives under `.hack/`.

---

## Data model

### Items

Two structural kinds plus a derived third:

| Kind | Annotation | What it is |
|---|---|---|
| **Ticket** | `@hack:ticket(R007-T1, "…")` (sub-ticket) or `@hack:ticket(T03, "…")` (standalone) | An incremental work unit. Usually lives inside a relay; the compound-ID form makes the parent explicit. Kind (feature/bug/task) is a `@hack:kind(...)` tag + badge letter. |
| **Relay** | `@hack:relay(R001, "…")` | A thread of work owned by a single agent at a time. Carries the baton across context resets. Sub-tickets live inside it. |
| **Epic** | `@hack:kind(epic)` on a relay, or *inferred* when **bare-R relays** declare `@hack:parent(R…)` | Coordination-of-relays, not coordination-of-tickets. Compound sub-tickets never promote their parent to epic. Status is computed from children. |

ID shapes and when to use each:

- **Bare relay** — `R001` … `R999` (3-digit zero-padded). Top-level thread of work.
- **Compound sub-ticket** — `R007-T1`, `R007-T2`, … Allocated under a relay via `rs-hack board claim --parent R007`. Always `-T` regardless of kind.
- **Standalone ticket** — `T03`, `F02`, `B01` (2-digit zero-padded). One-off work with no coordinating relay. Claim with no `--parent`.

**Never pick IDs yourself.** `rs-hack board claim` takes a `.hack/id.lock`
file lock, scans source for the current max, and writes the annotation
atomically. Two agents running in parallel will not collide.

### Columns / lifecycle

The board has five columns:

| Column | `@hack:status(...)` value(s) | Meaning |
|---|---|---|
| **Epics** | (derived) | Coordination points. Epic status is `active` / `closed` computed from children. |
| **Open** | `open` | Unclaimed. Also hosts `.hack/todo.md` entries (pre-ticket inbox). |
| **Active** | `claimed` \| `in-progress` | Someone is working on it now. Two statuses collapse into one column. |
| **Handoff** | `handoff` | Baton is down, ready for the next agent to pick up. |
| **Review** | `review` \| `done` | Work is complete; awaiting sign-off. Terminal-on-board. |

There is no `Done` column. "Done" is a short-lived staging value inside
`Review`; the terminal action is **archive**, which removes the `@hack:`
annotation lines from source and logs to `.hack/events.jsonl`.

**Moving cards.** Either edit `@hack:status(...)` in source directly, or
drag the card to a new column — the server rewrites the status line for
you under the enforced transition matrix:

- `open → active`
- `active → open | handoff | review`   (`active → open` is the admin undo)
- `handoff → active | review`
- `review → handoff`

Invalid drops are rejected (UI dims the column while dragging; server
returns 409 with the allowed transitions). Epics aren't draggable —
their status is derived.

**Card actions** live in the top-right of every ticket card:

- **prompt** / **review** — copies a continuation prompt. For `open` and
  `handoff` it's a pickup prompt (shelled out to `board tickets --prompt`).
  For `review`/`done` it's a review-mode prompt (verify commands + an
  approve-or-send-back decision tree, synthesized in the server).
- **archive** — first click arms and surfaces any declared
  `@hack:verify(...)` commands; second click commits.

Cards collapse by default; click `▸` to expand handoff text, next steps,
verify commands, and summaries.

### Epics

An epic is a relay that coordinates *other relays* rather than doing work
itself. Two ways to become an epic:

1. **Explicit:** `@hack:kind(epic)` on the relay (authoritative — survives
   having zero children).
2. **Inferred:** one or more **bare-R** relays declare `@hack:parent(R…)`
   pointing at this relay. Compound sub-tickets (`R007-T1`) never count —
   they make the parent a plain relay-with-subtickets, not an epic.

An epic's own `@hack:status(...)` is ignored. The board computes:

- **active** — any child relay is not in `review`/`done` and still exists
  in source
- **closed** — every child is either in `review`/`done` or archived
  (no longer in source)

Archiving an epic while it has live children returns `409`. Archive the
children first.

### Todos

Lightweight pre-tickets in `.hack/todo.md`. Use these when you know
something needs doing but haven't decided the shape yet. Each todo carries:

- `kind: feature | bug | task` — inherited when promoted to a ticket
- `stage: fresh | research | refine | split | ready` — what the next agent
  should do (planning signal, not work state)
- `see: <mode> <path>` — one or more references, each tagged with a mode:
  - **reference** — read for context, don't modify
  - **refine** — turn this doc into relay + phased tickets (`/refine`)
  - **implement** — build what the doc describes
  - **refactor** — doc/code drift; fix whichever side is wrong (but do
    **not** touch tickets already in `in-progress`)

Todos live in the **Open** column alongside `status:open` tickets.
Promoting a todo means creating the in-source annotations and then
either deleting the todo entry or calling `POST /api/todos/:id/promote`
with `{relay_id}` to link the promotion in the event log.

### Summaries

Freeform progress notes in `.hack/summaries/*.md`. Written by `/comment`
(or `rs-hack board summary`). A summary with a `ticket:` frontmatter field
attaches to that ticket's card; otherwise it lands in the board **Inbox**.
Each summary has a "Fork" button that generates a continuation prompt and
copies it to the clipboard.

### Events log

`.hack/events.jsonl` — append-only audit log. Derivative, not authoritative.
The server replays it on startup and diffs against the current source
scan, so accidental deletions ("clobbers") show up as `disappeared`
events. Typed events:

| Type | Emitted when |
|---|---|
| `created` | New ticket ID appears in source |
| `modified` | Tracked field changed; payload includes `{field: {before, after}}` |
| `archived` | Deliberate archive (strip annotation + log entry, one transaction) |
| `disappeared` | Ticket gone from source without an archive event |
| `todo_created` | New todo appears in `.hack/todo.md` |
| `todo_removed` | Todo disappears (edit or delete) |
| `todo_promoted` | Explicit promote endpoint, includes `relay_id` link |

---

## Rules (current conventions — candidates to formalize)

These are the conventions the board assumes. A future SDLC spec should
either ratify each one or replace it with something explicit.

### R1 — "First action on pickup = set `in-progress`"

When an agent claims a ticket from Open or Handoff, the *first* source
edit is flipping `@hack:status(in-progress)` on that ticket. This is the
claim signal; no other modifications come before it.

### R2 — "Never pick IDs yourself"

Every new relay/ticket is created via `rs-hack board claim`, which holds
`.hack/id.lock` during scan+write. An agent manually picking `max(id) + 1`
will collide with another concurrent agent.

### R3 — "Same-relay handoff is the default"

When an agent finishes a phase, the default handoff *updates the existing
relay in place* (same R-number, overwriting `@hack:handoff(...)`,
`@hack:next(...)`, etc.). New R-numbers only for *parallel* or
*independent* tracks.

### R4 — "Archive, don't settle in Review"

`status: done` is a short-lived staging state. Tickets don't rest there
— the terminal action is `POST /api/archive/:id`, which removes the
annotation from source and logs to `.hack/events.jsonl`. If a ticket
declared `@hack:verify(…)`, the archive confirm step surfaces those
commands before writing.

### R5 — "Do not modify items in `in-progress`"

When an agent is doing refactor work triggered by doc/code drift, it
touches items in `open` or `review` only. An item in `in-progress` has
an active owner whose context would be corrupted.

### R6 — "Epics coordinate, don't carry work"

An epic is a parent coordination point. Work happens on its children.
An epic's own `@hack:status(...)` is ignored; its state is derived.

### R7 — "Source edits are the state machine"

The only way to change board state is to edit source (or to go through
one of the explicit server endpoints — archive, promote, add-todo —
which themselves edit source or `.hack/`). There is no "move ticket"
API that doesn't ultimately result in a file write.

---

## Workflows

### Picking up work

1. Agent reads `rs-hack board tickets --prompt <ID>` (or clicks **Fork**
   on a relay/handoff card → copies the prompt to clipboard).
2. Agent edits the ticket's `@hack:status(in-progress)` line. First edit
   in the session, always.
3. Agent does the work.
4. When done, agent either:
   - Updates the relay in place with new handoff + next steps (same-relay
     handoff, R3) and sets `@hack:status(handoff)`, or
   - Sets `@hack:status(review)` and pings the user for sign-off.

### Creating new work

```bash
# Single-relay effort
$(rs-hack board claim --kind relay --file src/mod.rs --title "…")

# Multi-track epic
EPIC=$(rs-hack board claim --kind epic --file src/lib.rs --title "…")
rs-hack board claim --kind relay --file src/a.rs --title "phase A" --parent $EPIC --phase P1
rs-hack board claim --kind relay --file src/b.rs --title "phase B" --parent $EPIC --phase P2

# Ticket under a relay
rs-hack board claim --kind task --file src/foo.rs --title "…" --parent $EPIC --phase P1
```

### Archiving

`POST /api/archive/:id` (or the `archive` button on every card):

1. Server reads the source file at `ticket.file`.
2. Walks outward from `ticket.line` across the contiguous `//!` /
   `///` doc-comment block.
3. Removes just the `@hack:…` lines within that block — preserves
   `@arch:see`, plain doc text, and the item being documented.
4. Appends an `archived` event to `.hack/events.jsonl` with the full
   ticket snapshot + the raw lines that were removed.
5. Triggers a rescan; the ticket disappears from the board.

Epic archive: refuses if any child relay still exists in source (409
with `blockingChildren` list). Archive children first.

---

## Anatomy

### Workspace layout

```
<workspace>/
├── src/**/*.rs                    # Source — the authoritative board state.
│                                  # All @hack: annotations live here.
├── architecture/**/*.md           # Referenced by @arch:see(...) annotations.
├── .hack/
│   ├── todo.md                    # Pre-ticket inbox (structured markdown).
│   ├── summaries/*.md             # Progress notes from /comment.
│   ├── events.jsonl               # Append-only audit log. Server replays
│   │                              # on startup to detect clobbered tickets.
│   └── id.lock                    # Held during `rs-hack board claim`.
└── .claude/commands/              # Slash commands — installed by
    ├── comment.md                 # `rs-hack board init`.
    ├── handoff.md
    └── refine.md
```

### Annotation reference

```rust
//! @hack:ticket(F01, "title")       // or @hack:relay(R001, "…")
//! @hack:kind(feature|bug|task|epic) // override kind (epic on relay only)
//! @hack:status(open|claimed|in-progress|handoff|review|done)
//! @hack:assignee(agent:claude)
//! @hack:phase(P1)
//! @hack:parent(R001)                // points at a relay
//! @hack:severity(low|medium|high|critical)
//! @hack:handoff("one-line summary of what was just done")
//! @hack:next("next concrete step")  // repeatable
//! @hack:verify("cargo test -p foo") // repeatable; prompt renders as fenced bash + combined smoke test
//! @hack:gotcha("pre-existing breakage next agent needs to know")   // repeatable; rendered ABOVE context in prompt
//! @hack:assumes("untested claim baked into the handoff")           // repeatable; rendered as risks
//! @hack:cleanup("deferred debt")    // repeatable
//! @hack:depends_on(T02)             // repeatable
//! @arch:see(architecture/doc.md)    // repeatable
```

---

## CLI

```bash
# See the whole board
rs-hack board tickets                    # markdown
rs-hack board tickets -f json            # JSON
rs-hack board tickets --epics            # epics only
rs-hack board tickets --status handoff   # column filter
rs-hack board tickets --assignee agent:claude

# Atomic ID + annotation writer
rs-hack board claim --kind relay|epic|feature|bug|task \
  --file <path> --title "…" \
  [--assignee …] [--status …] [--phase …] [--parent …] \
  [--severity …] [--handoff "…"] [--next "…"] [--verify "…"] \
  [--cleanup "…"] [--see path.md] \
  [--json]

# Pickup prompt / relay doc
rs-hack board tickets --prompt R001      # synthesizes a continuation prompt
rs-hack board tickets --relay-doc R001   # markdown doc for the relay

# Write a progress note
rs-hack board summary "text…" --ticket R001 --author agent:claude

# Start the board UI
rs-hack board serve [--path .] [--open]
rs-hack board init [--force]                  # install slash commands + CLAUDE.md snippet
```

---

## HTTP API

Server default port is `3333 + 2 × (hash(workspace) % 333)` (UDP nudge
port is HTTP + 1). Override with `HACK_PORT` / `HACK_UDP_PORT`.

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/tickets` | Current ticket JSON |
| `GET` | `/api/summaries` | Current summaries |
| `GET` | `/api/events` | SSE stream of board updates |
| `GET` | `/api/archive` | Archived tickets (filtered from events log) |
| `GET` | `/api/history` | Full raw event log |
| `GET` | `/api/files?ext=md&q=…` | Workspace file search (for todo refs) |
| `GET` | `/api/prompt/:id` | Continuation prompt — branches to review-mode when ticket is in `review`/`done` |
| `GET` | `/api/relay-doc/:id` | Markdown doc for relay |
| `GET` | `/api/todo-prompt/:id` | Synthesized prompt for a todo (mode-aware) |
| `POST` | `/api/archive/:id` | Archive a ticket (409 if epic with live children) |
| `POST` | `/api/status/:id` | Drag-to-move — rewrites `@hack:status(...)` in source, 409 on disallowed transition |
| `POST` | `/api/nudge` | Force re-scan |
| `POST` | `/api/todos` | Create a todo |
| `POST` | `/api/todos/:id/promote` | Mark as promoted (optional `{relay_id}` link) |
| `DELETE` | `/api/todos/:id` | Remove a todo |
| `POST` | `/api/promote/:summary_id` | Promote a summary to a new relay |

---

## Slash commands

Installed by `rs-hack board init` into `.claude/commands/`:

- **`/comment`** — freeform progress summary to `.hack/summaries/`.
- **`/handoff`** — same-relay continuation (default) or new-relay track.
  Uses `rs-hack board claim`.
- **`/refine`** — turn a multi-phase plan into a relay + tickets.
  Uses `rs-hack board claim` per item.

---

## Running it

```bash
# From workspace root
rs-hack board serve

# Equivalent for local-dev hack-board itself
cd hack-board
bun run src/server.ts     # reads HACK_WORKSPACE or cwd
```

Environment:

- `HACK_WORKSPACE` — workspace to scan (defaults to cwd)
- `HACK_PORT` / `HACK_UDP_PORT` — override the workspace-hashed defaults
- `RS_HACK_BIN` — path to the `rs-hack` binary (defaults to
  `<workspace>/target/debug/rs-hack` then `rs-hack` on PATH)

The server watches the workspace (`fs.watch` recursive) and also listens
for UDP nudge packets from `rs-hack` itself. Changes debounce at 300ms
before triggering a rescan.

---

## Open questions for SDLC formalization

Things the current system encodes as conventions rather than rules,
which a formal spec should either ratify or replace:

1. **Who enforces R1–R7?** Today the rules live in prose (this file,
   `/handoff`, `/refine`, the ticket prompt). They're not machine-checked.
   Candidates for enforcement: a server-side hook that rejects commits
   moving an in-progress ticket to `done` without a prior `review` hop;
   a lint that catches agents picking IDs manually.
2. **Review sign-off is a human gesture.** Nothing distinguishes "AI said
   it's done" from "human reviewed and accepted." Do we want a
   `@hack:reviewed_by(human:name)` annotation? A separate `signed-off`
   status between `review` and archive?
3. **Cascade archive.** Currently epics must be archived one-child-at-a-time.
   When do we introduce `?cascade=true`?
4. **Concurrency scope.** The lock prevents ID collision. It does *not*
   prevent two agents from both editing the same ticket's `@hack:status`
   line. Is that a problem worth solving, or does source control catch it?
5. **Relay "versions".** R3 says same R-number across iterations. Do we
   ever want `R001.2` / `R001.3` for auditability, or is the event log
   enough?
6. **Todo → ticket linkage.** `todo_promoted` records a free-form
   `relay_id`. Should the target relay carry a back-reference annotation
   (`@hack:from_todo(T-abc)`)?
7. **Parent tracking on `depends_on`.** We parse `@hack:depends_on(...)`
   and display pills, but nothing blocks archiving a ticket whose
   dependency hasn't been archived. Is a block warranted?
8. **Test/verify enforcement.** The archive UI surfaces `@hack:verify(…)`
   commands but doesn't run them. A future step could shell them out and
   refuse archive on non-zero exit.
9. **Multi-agent presence.** The board doesn't show which agents are
   currently connected or what ticket they're looking at. With multiple
   parallel agents, some form of lightweight presence would prevent the
   two-agents-on-one-ticket problem `claim` already partly solves.
10. **Branch semantics.** Board state is branch-scoped because it lives
    in source. Merging a PR merges board state. No explicit rules yet
    about "when an agent sees a ticket in Review on branch X and Done
    on branch Y" — today that's a merge conflict resolved the normal
    way.

---

## Source pointers

- Server: [`src/server.ts`](src/server.ts) — Bun HTTP + SSE + file
  watcher + event log replay.
- UI: [`src/app.tsx`](src/app.tsx) — React frontend, built to
  `public/dist/app.js` via `bun build`.
- Annotation parser + board model: [`../rs-hack-arch/src/ticket.rs`](../rs-hack-arch/src/ticket.rs).
- `claim` command: [`../rs-hack/src/main.rs`](../rs-hack/src/main.rs)
  (search for `handle_claim` / `IdLock`).
- Slash command templates: [`../templates/commands/`](../templates/commands/).
- CLAUDE.md snippet: [`../templates/claude-md-hackboard.md`](../templates/claude-md-hackboard.md).
