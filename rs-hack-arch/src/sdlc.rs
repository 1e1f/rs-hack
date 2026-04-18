//! @arch:layer(arch)
//! @arch:role(sdlc)
//!
//! Canonical SDLC rules for hack-board. The board's ruleset lives in code so
//! that `rs-hack board rules` and the continuation-prompt synthesizer read
//! from the same source. Prose in READMEs is a mirror, not the authority.

use serde::Serialize;

/// A single rule in the hack-board SDLC spec.
#[derive(Debug, Clone, Serialize)]
pub struct SdlcRule {
    pub id: &'static str,
    pub title: &'static str,
    /// One-line rule statement.
    pub rule: &'static str,
    /// Why the rule exists — helps agents judge edge cases.
    pub why: &'static str,
    /// How/when to apply the rule.
    pub apply: &'static str,
    /// Which contexts this rule is most relevant to.
    pub contexts: &'static [Context],
}

/// Situational contexts an agent might be in. Picking a context narrows the
/// ruleset to just what's actionable right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Context {
    /// Claiming a ticket from Open or Handoff.
    Pickup,
    /// Wrapping up a phase — writing the next handoff.
    Finishing,
    /// Creating new relays / tickets.
    NewWork,
    /// Archiving a ticket at the end of its life.
    Archive,
    /// Doing cross-cutting refactor / cleanup work that touches many tickets.
    Refactor,
}

impl Context {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "pickup" | "pick-up" | "claim" => Some(Self::Pickup),
            "finishing" | "finish" | "handoff" | "done" => Some(Self::Finishing),
            "new-work" | "new" | "create" | "claim-new" => Some(Self::NewWork),
            "archive" => Some(Self::Archive),
            "refactor" | "cleanup" => Some(Self::Refactor),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Pickup => "pickup",
            Self::Finishing => "finishing",
            Self::NewWork => "new-work",
            Self::Archive => "archive",
            Self::Refactor => "refactor",
        }
    }
}

/// The canonical rule set. Keep IDs stable — they're cited in prompts and PRs.
pub const RULES: &[SdlcRule] = &[
    SdlcRule {
        id: "R1",
        title: "First action on pickup is the claim",
        rule: "Your first action on picking up a ticket is `rs-hack board claim <ID>` \
               (Open → Active) or `rs-hack board move <ID> active` (Handoff → Active). \
               Either one flips `@hack:status(in-progress)` and sets assignee atomically. \
               No other code changes come before the claim is recorded.",
        why: "The status flip is the claim signal — until it lands in source, another agent \
              can't see the ticket is taken. The Prompt button copies a continuation prompt \
              to the clipboard; it does not move the card for you.",
        apply: "On pickup from Open or Handoff.",
        contexts: &[Context::Pickup],
    },
    SdlcRule {
        id: "R2",
        title: "Never pick IDs yourself",
        rule: "Allocate every relay, epic, and ticket via `rs-hack board open` or \
               `rs-hack board claim` — never hand-write an annotation with an ID you \
               chose by reading the board and picking `max+1`.",
        why: "The allocator holds `.hack/id.lock` during scan+write. Two agents running in \
              parallel will otherwise pick the same number and silently clobber one ticket \
              at merge time.",
        apply: "Relay IDs (`R008`) and compound sub-tickets (`R007-T1`) alike. If you find \
                yourself typing any ID into a source file, stop — use `open` or `claim` and \
                paste the stdout ID. (Legacy bare tickets with `@hack:parent(...)` are gone; \
                a ticket under a relay is always compound.)",
        contexts: &[Context::NewWork],
    },
    SdlcRule {
        id: "R3",
        title: "Same-relay handoff is the default",
        rule: "Finish a phase with `rs-hack board move R<n> handoff --handoff '…' --next '…'`. \
               Same R-number, same annotation block — the baton moves forward, the thread \
               doesn't fork. New R-numbers are only for genuinely parallel or independent \
               tracks (see R8).",
        why: "Each new R-number fragments what should be a single thread. The next agent has \
              to chase the chain; the board has to display every fragment.",
        apply: "Finishing a phase on a relay you own.",
        contexts: &[Context::Finishing, Context::NewWork],
    },
    SdlcRule {
        id: "R4",
        title: "Archive, don't settle in Review",
        rule: "`status(done)` is short-lived staging. The terminal action is archive — the \
               archive button (or `POST /api/archive/:id`) — which strips `@hack:…` lines \
               from source and appends to `.hack/events.jsonl`.",
        why: "Tickets parked in Review pollute the board and lose signal. Archive removes \
              them cleanly and preserves an audit trail in the event log.",
        apply: "Once signed off, archive. Don't treat `status(done)` as a resting Done column.",
        contexts: &[Context::Archive, Context::Finishing],
    },
    SdlcRule {
        id: "R5",
        title: "Do not modify items that are `in-progress`",
        rule: "Refactor / cleanup work touches only `open` or `review` items. Never edit \
               the annotations of a ticket that is `in-progress`.",
        why: "`in-progress` means an active owner has the file open in their context. \
              Rewriting its annotations underneath them corrupts that context and can cause \
              merge conflicts or silent status loss.",
        apply: "Cross-cutting refactors, doc/code drift fixes, bulk cleanups.",
        contexts: &[Context::Refactor],
    },
    SdlcRule {
        id: "R6",
        title: "Epics coordinate, they don't carry work",
        rule: "An epic is a parent coordination point; work happens on its children. Its \
               own `@hack:status(...)` is ignored — state is derived from children.",
        why: "Treating an epic as a workable ticket lets agents pick it up when they \
              should be picking up a child. Derived status keeps the board truthful.",
        apply: "If you land on an epic, claim one of its child relays (or a sub-ticket of \
                one) instead. An epic is any relay with `@hack:kind(epic)` explicitly, or \
                any relay that has other bare-R relays pointing at it via `@hack:parent(...)` — \
                compound sub-tickets like `R007-T1` don't promote their parent.",
        contexts: &[Context::NewWork, Context::Pickup],
    },
    SdlcRule {
        id: "R7",
        title: "Source is truth; the three verbs are the interface",
        rule: "The only way to change board state is to write source. In practice that \
               means `rs-hack board open` / `claim` / `move`, or the archive / add-todo \
               card actions — each of which edits source (or `.hack/`) for you. Anything \
               that sidesteps a source edit desyncs on the next scan.",
        why: "Source is the single source of truth. The board watcher rescans after every \
              write; without a write there's nothing to scan.",
        apply: "Changing status, phase, parent, assignee, handoff, next, verify, gotcha, \
                assumes, cleanup — reach for `board move` (or `open`/`claim` for creation). \
                Hand-editing the `@hack:…` line directly still works and is fine for one-off \
                corrections, but the verbs are what keep the transition matrix honest.",
        contexts: &[Context::Pickup, Context::Finishing, Context::Refactor],
    },
    SdlcRule {
        id: "R8",
        title: "Sub-tickets stay under the relay; new relays are for independent tracks",
        rule: "When the next chunk of work is a sub-unit of the current relay, claim a \
               sub-ticket under it (`rs-hack board open --kind task --parent R012` → \
               `R012-T1`), not a new bare relay. New R-numbers are reserved for threads \
               that genuinely run in parallel or independently from the current one.",
        why: "The relay is the baton across agent sessions; sub-tickets are checkpoints \
              inside that thread. A new relay per chunk fragments the baton — 'what's \
              still open under R012' is only answerable when R012's chunks are its \
              children.",
        apply: "Mid-relay, when the next step is a concrete sub-unit (a file to edit, a \
                test to add, a bug to fix). Use a bare R-relay only for a fork — an \
                independent track, or a child of an epic (each epic sub-phase is itself \
                a relay with its own sub-tickets).",
        contexts: &[Context::NewWork, Context::Finishing],
    },
    SdlcRule {
        id: "R9",
        title: "The relay is the cross-session scratchpad; work lives on sub-tickets",
        rule: "A relay carries a one-line purpose plus the context that spans its \
               sub-tickets but doesn't belong on any one of them: `@arch:see(...)` \
               pointers to architecture docs, cross-ticket ordering beyond simple \
               phases, shared assumptions/gotchas/constraints you've discovered, and \
               narrative `@hack:next(...)` guidance (strategy, caveats, pointers) for \
               the next picker. It does **not** carry discrete work. If a `@hack:next` \
               line names a concrete file to edit, test to add, bug to fix, or doc to \
               update, it's a ticket — open one per R8 and use `@hack:next` to say how \
               the tickets relate ('start T1, then T2 blocks on T3', 'watch out for Z \
               in all of these').",
        why: "Agent contexts are ephemeral; the relay is what survives between sessions. \
              Treating it as the shared scratchpad means the next agent opening a \
              continuation prompt inherits the full cross-ticket picture — not just \
              a checklist. But a scratchpad that accumulates chunks becomes unbounded; \
              it never closes because next keeps growing. A clear one-line purpose \
              closes when the deliverable lands; everything else lives on its own \
              ticket where it can be picked up, worked, and archived in isolation.",
        apply: "Writing a handoff — put on the relay anything that matters *across* \
                tickets (doc refs, ordering, shared gotchas, strategy) and open a \
                ticket for anything that's itself a concrete work unit. Before adding \
                a `@hack:next` line, ask: 'is this a concrete chunk of work?' If yes \
                → `rs-hack board open --kind task --parent R<n>`. If it's context the \
                next picker needs to read first → keep it on the relay.",
        contexts: &[Context::Finishing, Context::NewWork],
    },
    SdlcRule {
        id: "R10",
        title: "Scan in-flight relays before refining new work",
        rule: "Before `/refine` or `rs-hack board open --kind relay`, run \
               `rs-hack board inflight` and read every Open / Active / Handoff relay. \
               If your planned work overlaps an existing relay's purpose, either claim \
               the existing one (Open → Active), add your plan as a sub-ticket under it \
               (per R8), or — if it's genuinely independent — reference the neighboring \
               relay in your own arch doc so the next picker sees the relationship.",
        why: "Five agents planning in parallel will independently refine the same \
              problem unless they look first. Overlapping relays aren't disastrous — \
              code can touch the same files — but the baton fragments: two agents each \
              carry half the context, neither knows the other exists, and the board \
              shows two cards for one thread.",
        apply: "Planning-time only — before writing an arch doc, before allocating a \
                new relay ID. Not needed for sub-tickets under a relay you already own \
                (the parent already establishes the scope).",
        contexts: &[Context::NewWork],
    },
    SdlcRule {
        id: "C1",
        title: "Columns: pick the right one for your state",
        rule: "Open (`open`) → Active (`claimed` | `in-progress`) → Handoff (`handoff`) → \
               Review (`review` | `done`). Three end-states after work: **more work \
               remains** → Handoff (same relay, R3); **tasks met, awaiting sign-off** → \
               Review; **signed off** → archive (R4). Never drop done-but-unfinished work \
               back into Active; never skip Review on the way to archive.",
        why: "Two common mistakes: (1) moving a finished ticket back to Active when \
              there's more to do — that hides completed work; (2) self-archiving when the \
              tasks are met but no human has verified — that bypasses the one gate where \
              `@hack:verify(...)` gets exercised. Handoff passes a still-alive baton; \
              Review is the final checkpoint before archive.",
        apply: "More work to follow → `rs-hack board move R<n> handoff` with updated \
                handoff/next (same R-number per R3). Tasks met, nothing left to do → \
                `rs-hack board move <ID> review` and ping the user. Archive only after \
                the user confirms — never self-archive on the same turn you set review.",
        contexts: &[Context::Finishing, Context::Pickup],
    },
];

/// Return rules relevant to a given context, plus R7 (always relevant).
pub fn rules_for(ctx: Context) -> Vec<&'static SdlcRule> {
    RULES
        .iter()
        .filter(|r| r.contexts.contains(&ctx) || r.id == "R7")
        .collect()
}

/// Render a rule list as compact markdown. `terse` omits the Why/Apply lines
/// for embedding in a pickup prompt; full form is for `board rules`.
pub fn format_markdown(rules: &[&SdlcRule], terse: bool) -> String {
    let mut out = String::new();
    for r in rules {
        out.push_str(&format!("**{} — {}**\n", r.id, r.title));
        out.push_str(r.rule);
        out.push('\n');
        if !terse {
            out.push_str(&format!("- *Why:* {}\n", r.why));
            out.push_str(&format!("- *How to apply:* {}\n", r.apply));
        }
        out.push('\n');
    }
    out
}

/// Short playbook for a pickup prompt: the rules an agent needs the moment
/// they open a continuation prompt. Ordered by immediacy, not by rule number.
pub fn pickup_playbook() -> String {
    let ordered_ids = ["R1", "C1", "R3", "R4", "R2"];
    let rules: Vec<&SdlcRule> = ordered_ids
        .iter()
        .filter_map(|id| RULES.iter().find(|r| r.id == *id))
        .collect();
    format_markdown(&rules, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<_> = RULES.iter().map(|r| r.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate rule id in RULES");
    }

    #[test]
    fn pickup_rules_include_r1() {
        let rules = rules_for(Context::Pickup);
        assert!(rules.iter().any(|r| r.id == "R1"));
    }

    #[test]
    fn finishing_rules_include_r3_and_c1() {
        let rules = rules_for(Context::Finishing);
        assert!(rules.iter().any(|r| r.id == "R3"));
        assert!(rules.iter().any(|r| r.id == "C1"));
    }

    #[test]
    fn context_parse_roundtrips() {
        for ctx in [
            Context::Pickup,
            Context::Finishing,
            Context::NewWork,
            Context::Archive,
            Context::Refactor,
        ] {
            assert_eq!(Context::parse(ctx.label()), Some(ctx));
        }
    }

    #[test]
    fn pickup_playbook_is_nonempty_and_leads_with_r1() {
        let md = pickup_playbook();
        assert!(md.contains("**R1"));
        assert!(md.find("**R1").unwrap() < md.find("**C1").unwrap());
    }
}
