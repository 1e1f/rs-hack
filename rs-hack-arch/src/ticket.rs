//! @arch:layer(arch)
//! @arch:role(ticket)
//! @hack:relay(R001, "hack-board: two-noun model")
//! @hack:assignee(agent:claude)
//! @hack:phase(P1)
//! @hack:handoff("Refactored to two-noun model: Ticket + Relay. Feature/Bug/Task collapsed into Ticket with kind tag. Story/Epic replaced by Relay parent chain.")
//! @hack:verify("cargo test -p rs-hack-arch")
//! @arch:see(architecture/hack-board.md)
//!
//! Ticket and Relay aggregation from `@hack:` annotations.
//!
//! Two nouns:
//! - **Ticket**: a unit of work (kind: feature/bug/task is just a tag)
//! - **Relay**: a thread of work / coordination point (owns tickets, single agent)
//!
//! Three tags:
//! - **kind**: feature, bug, task (on tickets)
//! - **phase**: P1, P2 (ordering within a relay)
//! - **parent**: relay-to-relay hierarchy (epic = relay with child relays)

use crate::annotation::{AnnotationTarget, ArchAnnotation, ArchKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Whether this is a Ticket or a Relay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Ticket,
    Relay,
}

/// Status in the lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TicketStatus {
    Open,
    Claimed,
    InProgress,
    Handoff,
    Review,
    Done,
}

impl TicketStatus {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "open" => Self::Open,
            "claimed" => Self::Claimed,
            "in-progress" | "in_progress" | "inprogress" => Self::InProgress,
            "handoff" => Self::Handoff,
            "review" => Self::Review,
            "done" | "closed" | "complete" => Self::Done,
            _ => Self::Open,
        }
    }

    pub fn column(&self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Claimed => "Claimed",
            Self::InProgress => "In Progress",
            Self::Handoff => "Handoff",
            Self::Review => "Review",
            Self::Done => "Done",
        }
    }
}

/// A work item (Ticket or Relay) on the hack-board.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticket {
    /// ID (e.g., "F01", "B03", "T12", "R001")
    pub id: String,

    /// Human-readable title
    pub title: String,

    /// Ticket or Relay
    pub item_type: ItemType,

    /// Kind tag: "feature", "bug", "task" (for tickets). Inferred from ID prefix if not explicit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Current status
    pub status: TicketStatus,

    /// Who is working on this
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,

    /// Phase tag (e.g., "P1")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// Parent relay ID (e.g., "R001") — for relay hierarchy (epic = relay with children)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Severity (for bugs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,

    /// Handoff message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handoff: Option<String>,

    /// What the next agent should do
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub next_steps: Vec<String>,

    /// Deferred cleanup items
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub cleanup: Vec<String>,

    /// Verification criteria (for relays)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub verify: Vec<String>,

    /// Depends on these IDs
    pub depends_on: Vec<String>,

    /// Links to architecture docs
    pub see_also: Vec<String>,

    /// Source file
    pub file: PathBuf,

    /// Line number
    pub line: usize,

    /// AST target
    pub target: AnnotationTarget,
}

impl Ticket {
    /// Badge letter for display (inferred from kind or ID prefix).
    pub fn badge(&self) -> &str {
        if self.item_type == ItemType::Relay {
            return "R";
        }
        match self.kind.as_deref() {
            Some("bug") => "B",
            Some("feature") => "F",
            Some("task") => "T",
            _ => {
                // Infer from ID prefix
                match self.id.chars().next() {
                    Some('B') | Some('b') => "B",
                    Some('F') | Some('f') => "F",
                    Some('T') | Some('t') => "T",
                    _ => "T", // default to task
                }
            }
        }
    }
}

/// The board — a collection of tickets and relays.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketBoard {
    pub tickets: Vec<Ticket>,
}

impl TicketBoard {
    /// Build a board from annotations.
    pub fn from_annotations(annotations: &[ArchAnnotation]) -> Self {
        let mut by_target: HashMap<String, Vec<&ArchAnnotation>> = HashMap::new();
        for ann in annotations {
            if is_hack_relevant(&ann.kind) {
                let id = ann.target.id();
                by_target.entry(id).or_default().push(ann);
            }
        }

        by_target.retain(|_, anns| anns.iter().any(|a| is_hack_defining(&a.kind)));

        let mut tickets = Vec::new();
        for (_target_id, anns) in by_target {
            if let Some(ticket) = build_item(&anns) {
                tickets.push(ticket);
            }
        }

        tickets.sort_by(|a, b| a.id.cmp(&b.id));
        TicketBoard { tickets }
    }

    pub fn by_status(&self, status: &TicketStatus) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| &t.status == status).collect()
    }

    pub fn get(&self, id: &str) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.id == id)
    }

    pub fn assigned_to(&self, assignee: &str) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.assignee.as_deref() == Some(assignee)).collect()
    }

    pub fn handoffs(&self) -> Vec<&Ticket> {
        self.by_status(&TicketStatus::Handoff)
    }

    pub fn relays(&self) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.item_type == ItemType::Relay).collect()
    }

    /// Get tickets that belong to a relay (via parent tag).
    pub fn children_of(&self, relay_id: &str) -> Vec<&Ticket> {
        self.tickets.iter().filter(|t| t.parent.as_deref() == Some(relay_id)).collect()
    }

    pub fn to_markdown(&self) -> String {
        let columns = [
            TicketStatus::Open,
            TicketStatus::Claimed,
            TicketStatus::InProgress,
            TicketStatus::Handoff,
            TicketStatus::Review,
            TicketStatus::Done,
        ];

        let mut output = String::from("# Hack Board\n\n");

        for status in &columns {
            let tickets = self.by_status(status);
            if tickets.is_empty() {
                continue;
            }

            output.push_str(&format!("## {}\n\n", status.column()));
            for ticket in tickets {
                output.push_str(&format!(
                    "- **[{}] {}**: {}\n",
                    ticket.badge(), ticket.id, ticket.title
                ));
                if let Some(ref assignee) = ticket.assignee {
                    output.push_str(&format!("  assignee: {}\n", assignee));
                }
                if let Some(ref parent) = ticket.parent {
                    output.push_str(&format!("  parent: {}\n", parent));
                }
                if let Some(ref phase) = ticket.phase {
                    output.push_str(&format!("  phase: {}\n", phase));
                }
                if let Some(ref severity) = ticket.severity {
                    output.push_str(&format!("  severity: {}\n", severity));
                }
                if let Some(ref handoff) = ticket.handoff {
                    output.push_str(&format!("  handoff: {}\n", handoff));
                }
                if !ticket.next_steps.is_empty() {
                    output.push_str("  next:\n");
                    for step in &ticket.next_steps {
                        output.push_str(&format!("    - {}\n", step));
                    }
                }
                if !ticket.cleanup.is_empty() {
                    output.push_str("  cleanup:\n");
                    for item in &ticket.cleanup {
                        output.push_str(&format!("    - {}\n", item));
                    }
                }
                if !ticket.verify.is_empty() {
                    output.push_str("  verify:\n");
                    for v in &ticket.verify {
                        output.push_str(&format!("    - {}\n", v));
                    }
                }
                if !ticket.see_also.is_empty() {
                    output.push_str(&format!("  see: {}\n", ticket.see_also.join(", ")));
                }
                output.push_str(&format!("  source: {}:{}\n", ticket.file.display(), ticket.line));
            }
            output.push('\n');
        }

        let total = self.tickets.len();
        let done = self.by_status(&TicketStatus::Done).len();
        output.push_str(&format!("---\n{}/{} complete\n", done, total));
        output
    }

    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }
}

impl Ticket {
    /// Generate a continuation prompt for the next agent.
    pub fn to_prompt(&self) -> String {
        let mut prompt = String::new();

        prompt.push_str(&format!("# Continue: {} — {}\n\n", self.id, self.title));

        prompt.push_str("## Context\n\n");
        prompt.push_str(&format!("`{}` ", self.id));
        if let Some(ref assignee) = self.assignee {
            prompt.push_str(&format!("(from {}) ", assignee));
        }
        prompt.push_str("is ready for continuation.\n\n");

        if let Some(ref phase) = self.phase {
            prompt.push_str(&format!("**Phase**: {}\n\n", phase));
        }
        if let Some(ref parent) = self.parent {
            prompt.push_str(&format!("**Parent relay**: {}\n\n", parent));
        }

        if let Some(ref handoff) = self.handoff {
            prompt.push_str("## What was completed\n\n");
            prompt.push_str(handoff);
            prompt.push_str("\n\n");
        }

        if !self.next_steps.is_empty() {
            prompt.push_str("## Next steps\n\n");
            for step in &self.next_steps {
                prompt.push_str(&format!("- {}\n", step));
            }
            prompt.push('\n');
        }

        if !self.cleanup.is_empty() {
            prompt.push_str("## Cleanup backlog\n\n");
            for item in &self.cleanup {
                prompt.push_str(&format!("- {}\n", item));
            }
            prompt.push('\n');
        }

        if !self.verify.is_empty() {
            prompt.push_str("## Verification\n\n");
            for v in &self.verify {
                prompt.push_str(&format!("- {}\n", v));
            }
            prompt.push('\n');
        }

        if !self.see_also.is_empty() {
            prompt.push_str("## Reference\n\n");
            for doc in &self.see_also {
                prompt.push_str(&format!("- Read: {}\n", doc));
            }
            prompt.push('\n');
        }

        prompt.push_str("## Source\n\n");
        prompt.push_str(&format!("Defined at `{}:{}`\n\n", self.file.display(), self.line));

        prompt.push_str("## Instructions\n\n");
        prompt.push_str("1. Read the reference docs and source context above\n");
        prompt.push_str("2. Complete the next steps listed\n");
        prompt.push_str("3. Address cleanup items if time permits\n");
        prompt.push_str("4. When done, run `/handoff` to create the next relay\n");

        prompt
    }

    /// Generate a relay markdown document.
    pub fn to_relay_doc(&self) -> String {
        let mut doc = String::new();

        doc.push_str(&format!("# {}: {}\n\n", self.id, self.title));
        doc.push_str(&format!("**Status**: {}\n", self.status.column()));
        if let Some(ref assignee) = self.assignee {
            doc.push_str(&format!("**Agent**: {}\n", assignee));
        }
        if let Some(ref phase) = self.phase {
            doc.push_str(&format!("**Phase**: {}\n", phase));
        }
        if let Some(ref parent) = self.parent {
            doc.push_str(&format!("**Parent**: {}\n", parent));
        }
        doc.push_str(&format!("**Source**: `{}:{}`\n", self.file.display(), self.line));
        doc.push('\n');

        if let Some(ref handoff) = self.handoff {
            doc.push_str("## Completed\n\n");
            doc.push_str(handoff);
            doc.push_str("\n\n");
        }

        if !self.next_steps.is_empty() {
            doc.push_str("## Next Steps\n\n");
            for step in &self.next_steps {
                doc.push_str(&format!("- {}\n", step));
            }
            doc.push('\n');
        }

        if !self.cleanup.is_empty() {
            doc.push_str("## Cleanup Backlog\n\n");
            for item in &self.cleanup {
                doc.push_str(&format!("- {}\n", item));
            }
            doc.push('\n');
        }

        if !self.verify.is_empty() {
            doc.push_str("## Verification\n\n");
            for v in &self.verify {
                doc.push_str(&format!("- {}\n", v));
            }
            doc.push('\n');
        }

        if !self.see_also.is_empty() {
            doc.push_str("## References\n\n");
            for s in &self.see_also {
                doc.push_str(&format!("- {}\n", s));
            }
            doc.push('\n');
        }

        doc
    }
}

// ── Internal helpers ────────────────────────────────────────────────────

fn is_hack_defining(kind: &ArchKind) -> bool {
    matches!(kind, ArchKind::Ticket { .. } | ArchKind::Relay { .. })
}

fn is_hack_relevant(kind: &ArchKind) -> bool {
    matches!(
        kind,
        ArchKind::Ticket { .. }
            | ArchKind::Relay { .. }
            | ArchKind::Kind(_)
            | ArchKind::Status(_)
            | ArchKind::Assignee(_)
            | ArchKind::Phase(_)
            | ArchKind::Parent(_)
            | ArchKind::HackSeverity(_)
            | ArchKind::Handoff(_)
            | ArchKind::Next(_)
            | ArchKind::Cleanup(_)
            | ArchKind::Verify(_)
            | ArchKind::See(_)
            | ArchKind::DependsOn { .. }
    )
}

/// Build a Ticket or Relay from a group of annotations on the same target.
fn build_item(anns: &[&ArchAnnotation]) -> Option<Ticket> {
    let mut id = None;
    let mut title = String::new();
    let mut item_type = None;
    let mut kind = None;
    let mut status = TicketStatus::Open;
    let mut assignee = None;
    let mut phase = None;
    let mut parent = None;
    let mut severity = None;
    let mut handoff = None;
    let mut next_steps = Vec::new();
    let mut cleanup = Vec::new();
    let mut verify = Vec::new();
    let mut see_also = Vec::new();
    let mut depends_on = Vec::new();

    let first = anns.first()?;
    let file = first.file.clone();
    let line = first.line;
    let target = first.target.clone();

    for ann in anns {
        match &ann.kind {
            ArchKind::Ticket { id: tid, title: ttitle } => {
                id = Some(tid.clone());
                title = ttitle.clone();
                item_type = Some(ItemType::Ticket);
            }
            ArchKind::Relay { id: rid, title: rtitle } => {
                id = Some(rid.clone());
                title = rtitle.clone();
                item_type = Some(ItemType::Relay);
                if status == TicketStatus::Open {
                    status = TicketStatus::Handoff;
                }
            }
            ArchKind::Kind(k) => kind = Some(k.clone()),
            ArchKind::Status(s) => status = TicketStatus::parse(s),
            ArchKind::Assignee(a) => assignee = Some(a.clone()),
            ArchKind::Phase(p) => phase = Some(p.clone()),
            ArchKind::Parent(p) => parent = Some(p.clone()),
            ArchKind::HackSeverity(s) => severity = Some(s.clone()),
            ArchKind::Handoff(h) => handoff = Some(h.clone()),
            ArchKind::Next(n) => next_steps.push(n.clone()),
            ArchKind::Cleanup(c) => cleanup.push(c.clone()),
            ArchKind::Verify(v) => verify.push(v.clone()),
            ArchKind::See(s) => see_also.push(s.clone()),
            ArchKind::DependsOn { target: dep, .. } => depends_on.push(dep.clone()),
            _ => {}
        }
    }

    let id = id?;
    let item_type = item_type?;

    // Infer kind from legacy aliases (bug/feature/task parsed as Ticket)
    // or from ID prefix if no explicit @hack:kind
    if kind.is_none() && item_type == ItemType::Ticket {
        kind = match id.chars().next() {
            Some('B') | Some('b') => Some("bug".to_string()),
            Some('F') | Some('f') => Some("feature".to_string()),
            _ => None, // T or unknown — badge() handles the default
        };
    }

    Some(Ticket {
        id,
        title,
        item_type,
        kind,
        status,
        assignee,
        phase,
        parent,
        severity,
        handoff,
        next_steps,
        cleanup,
        verify,
        depends_on,
        see_also,
        file,
        line,
        target,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::extract_from_source;
    use std::path::Path;

    #[test]
    fn test_ticket_extraction() {
        let source = r#"
//! @hack:ticket(F01, "Implement voice allocation")
//! @hack:status(in-progress)
//! @hack:assignee(agent:claude)
//! @hack:phase(P2)
//! @arch:see(architecture/vivarium/voice_allocation.md)

pub mod voice_alloc;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let ticket = &board.tickets[0];
        assert_eq!(ticket.id, "F01");
        assert_eq!(ticket.item_type, ItemType::Ticket);
        assert_eq!(ticket.kind.as_deref(), Some("feature")); // inferred from F prefix
        assert_eq!(ticket.badge(), "F");
        assert_eq!(ticket.status, TicketStatus::InProgress);
        assert_eq!(ticket.assignee.as_deref(), Some("agent:claude"));
        assert_eq!(ticket.phase.as_deref(), Some("P2"));
    }

    #[test]
    fn test_bug_extraction() {
        let source = r#"
//! @hack:ticket(B01, "Panic on zero-length buffer")
//! @hack:status(open)
//! @hack:severity(high)

pub fn process_buffer() {}
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let ticket = &board.tickets[0];
        assert_eq!(ticket.id, "B01");
        assert_eq!(ticket.badge(), "B"); // inferred from B prefix
        assert_eq!(ticket.severity.as_deref(), Some("high"));
    }

    #[test]
    fn test_legacy_bug_alias() {
        // @hack:bug(...) still works as an alias for @hack:ticket(...)
        let source = r#"
//! @hack:bug(B02, "Old-style bug annotation")
//! @hack:status(open)

pub fn buggy() {}
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        assert_eq!(board.tickets[0].badge(), "B");
    }

    #[test]
    fn test_relay_extraction_and_prompt() {
        let source = r#"
//! @hack:relay(R001, "ProcessBlock Unification Phase 4")
//! @hack:assignee(agent:claude)
//! @hack:phase(P4)
//! @hack:handoff("AudioProcessor -> ProcessBlock across ~80 files.")
//! @hack:next("Simplify add_control_node_unified -> add_node_named")
//! @hack:next("Inline/remove intermediary functions")
//! @hack:cleanup("CvRbjFilter is dead code")
//! @hack:verify("cargo test -p vivarium")
//! @arch:see(architecture/vivarium/PROCESS_BLOCK_UNIFICATION.md)

pub mod process_block;
"#;
        let annotations = extract_from_source(source, Path::new("engine.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets.len(), 1);
        let relay = &board.tickets[0];
        assert_eq!(relay.id, "R001");
        assert_eq!(relay.item_type, ItemType::Relay);
        assert_eq!(relay.badge(), "R");
        assert_eq!(relay.status, TicketStatus::Handoff);
        assert_eq!(relay.next_steps.len(), 2);
        assert_eq!(relay.cleanup.len(), 1);
        assert_eq!(relay.verify.len(), 1);

        let prompt = relay.to_prompt();
        assert!(prompt.contains("# Continue: R001"));
        assert!(prompt.contains("ProcessBlock Unification"));
        assert!(prompt.contains("## Verification"));
        assert!(prompt.contains("cargo test"));
        assert!(prompt.contains("/handoff"));

        let md = board.to_markdown();
        assert!(md.contains("[R] R001"));
    }

    #[test]
    fn test_parent_relay() {
        let source = r#"
//! @hack:relay(R005, "CV Port Bridge")
//! @hack:parent(R001)
//! @hack:assignee(agent:claude)

pub mod cv_bridge;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        assert_eq!(board.tickets[0].parent.as_deref(), Some("R001"));

        let prompt = board.tickets[0].to_prompt();
        assert!(prompt.contains("**Parent relay**: R001"));
    }

    #[test]
    fn test_ticket_with_parent_and_phase() {
        let source = r#"
//! @hack:ticket(T01, "Add cv_to_hz to RbjBiquadNode")
//! @hack:parent(R005)
//! @hack:phase(P1)
//! @hack:status(open)

pub struct RbjBiquadNode;
"#;
        let annotations = extract_from_source(source, Path::new("test.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);

        let t = &board.tickets[0];
        assert_eq!(t.id, "T01");
        assert_eq!(t.badge(), "T");
        assert_eq!(t.parent.as_deref(), Some("R005"));
        assert_eq!(t.phase.as_deref(), Some("P1"));
    }

    #[test]
    fn test_board_markdown() {
        let source = r#"
//! @hack:ticket(T01, "Do the thing")
//! @hack:status(done)

pub mod a;
"#;
        let annotations = extract_from_source(source, Path::new("a.rs")).unwrap();
        let board = TicketBoard::from_annotations(&annotations);
        let md = board.to_markdown();
        assert!(md.contains("1/1 complete"));
    }

    #[test]
    fn test_mixed_arch_and_hack() {
        let source = r#"
//! @arch:layer(vivarium)
//! @arch:role(synthesis)
//! @hack:ticket(T03, "Add polyphony support")
//! @hack:status(claimed)
//! @hack:assignee(agent:claude)

pub struct Synth;
"#;
        let annotations = extract_from_source(source, Path::new("synth.rs")).unwrap();

        let board = TicketBoard::from_annotations(&annotations);
        assert_eq!(board.tickets.len(), 1);
        assert_eq!(board.tickets[0].id, "T03");

        let graph = crate::graph::ArchGraph::from_annotations(annotations);
        let ctx = crate::query::get_file_context(&graph, "synth");
        assert_eq!(ctx.layer.as_deref(), Some("vivarium"));
    }
}
