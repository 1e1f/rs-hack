//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Annotation overlay types.
//!
//! Annotations are human-authored decorations on structural KG nodes. They
//! live in source as `@yah:` directives inside doc comments. Three kinds:
//!
//! * **Tag** — set membership. `@yah:tag(audio, hot-path)` adds the
//!   annotated node to the named taxonomies. Stored as `EdgeKind::Tag`
//!   edges from the structural node to a synthetic `Tag` node so the
//!   "show me everything in `audio`" query is a 1-hop subgraph fetch.
//! * **Flow** — curated edge. `@yah:flow(audio::mixer → dispatch::loop,
//!   "shared frame buffer")` declares a meaningful coupling that
//!   `Calls`/`Imports` can't see. Endpoints resolve via qualified-name
//!   lookup; rotted endpoints surface as warnings, not silent drops.
//! * **Rule** — graph constraint. `@yah:rule(no-import-of: tag(view))`
//!   declares an invariant the validator checks. Rule semantics are
//!   reserved in the contract; the v1 extractor parses them but doesn't
//!   yet ship a validator.
//!
//! @yah:ticket(R017-F1, "yah-kg-validator crate + vocabulary (no-import-of, no-dependency-on, max-depth, must-tag)")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R017)
//! @yah:next("Walks KG, applies rules from index, returns Vec<Violation>")
//! @yah:next("Cargo.toml [workspace.metadata.arch] section declares legal rule kinds + tag namespaces")
//! @yah:next("Parser already handles @yah:rule(no-import-of: tag(view)); the type exists in the contract — wire validation")

use crate::agent_policy::AgentPolicyRule;
use crate::ids::NodeId;
use serde::{Deserialize, Serialize};

/// One annotation as observed on a node, with provenance back to its
/// source-line so authors can be pointed at the offending comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationRef {
    /// What was annotated.
    pub anchor: NodeId,
    /// Where the annotation lived in source. Both fields are convenience
    /// for tooling — the structural file is also in `anchor`'s NodeRef.
    pub source_file: String,
    pub source_line: u32,
    /// The annotation payload.
    pub kind: AnnotationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "anno", rename_all = "snake_case")]
pub enum AnnotationKind {
    /// `@yah:tag(name)` or `@yah:tag(ns:name)`.
    Tag(TagRef),
    /// `@yah:flow(<from> → <to>, "<reason>")`. `to_qualified` is the raw
    /// qualified-name string from source; the daemon resolves it against
    /// the structural index when emitting the `Flow` edge.
    Flow {
        to_qualified: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// `@yah:rule(<rule-kind>: <args>)`. Reserved — v1 extractors parse
    /// these into the structure but no validator runs against them yet.
    Rule {
        rule_kind: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
    },
    /// `@yah:relay(ID, "title")` plus the modifier directives that
    /// followed it in the same doc block (status, assignee, parent,
    /// handoff, next, gotcha, assumes, verify, cleanup, …). A relay is a
    /// thread of work; pass 2 of R017-F4 will promote these to synthetic
    /// `CommonKind::Relay` nodes with parent / depends_on edges.
    Relay(WorkItemAnno),
    /// `@yah:ticket(ID, "title")` plus its modifier directives. Tickets
    /// are leaf work units parented to a relay.
    Ticket(WorkItemAnno),
}

/// Payload shared by `Relay` and `Ticket` annotations. Field set mirrors
/// `yah::arch::ticket::Ticket` (the CLI extractor's mature model); pass
/// 2 of R017-F4 will unify the two parsers so this stays the only home.
///
/// Modifier directives (`@yah:status(...)`, `@yah:next("...")`, …) attach
/// to the most recent `@yah:relay(...)` or `@yah:ticket(...)` header in
/// the same doc string. A blank line *or* the next header closes a block.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemAnno {
    pub id: String,
    pub title: String,
    /// `@yah:kind(feature|bug|task|epic)` — overrides the natural kind
    /// (relays default to "relay"; tickets default to "task" or whatever
    /// the ID prefix implies in the CLI extractor).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TicketStatus>,
    /// `@yah:at(<rfc3339>)` — wall-clock of the most recent
    /// daemon-mediated mutation (currently: `move_ticket`). Written and
    /// rewritten by the daemon, not by hand. UTC, always `Z`. Provides
    /// per-ticket "last touched" precision so co-resident tickets in the
    /// same file don't share a single file-mtime.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// `@yah:parent(R001)` — for sub-tickets and zone (relay-of-relays)
    /// hierarchy. Resolved to a graph edge in pass 2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
    /// `@yah:handoff("...")` — repeatable. Stored in source order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub handoff: Vec<String>,
    /// `@yah:next("...")` — repeatable.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_steps: Vec<String>,
    /// `@yah:gotcha("...")` — repeatable. Pre-existing breakage / traps.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gotchas: Vec<String>,
    /// `@yah:assumes("...")` — repeatable. Unverified claims.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assumes: Vec<String>,
    /// `@yah:verify("...")` — repeatable. Acceptance commands.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verify: Vec<String>,
    /// `@yah:cleanup("...")` — repeatable. Deferred tech debt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cleanup: Vec<String>,
    /// `@arch:see(path)` — repeatable. Architecture-doc references the
    /// pickup prompt surfaces under the "Reference" section. Crosses the
    /// `@yah:` / `@arch:` namespace boundary because the parser collects
    /// these inside the same relay/ticket block — the doc reference is
    /// a property of the work-item, not of the structural anchor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub see_also: Vec<String>,
    /// `@yah:think(deep | standard | fast | budget=N)` — per-ticket
    /// thinking budget for the agent runtime. Read by the R028-F2 prelude
    /// assembler and translated into the Claude SDK's `thinking` config
    /// on each turn. `None` means "use the workspace default".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think: Option<ThinkBudget>,
    /// `@yah:engine(provider:model)` — per-ticket model selection. Drives
    /// runner dispatch (claude:* → Claude SDK runner; everything else →
    /// yah-runner). `None` means "use the workspace default engine".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<EngineRef>,
    /// Agent-policy rules folded onto this work-item from
    /// `@yah:rule(agent-role|agent-do|agent-dont: ...)` directives in the
    /// same doc block. The R028-F10 prelude assembler walks the parent
    /// chain and concatenates these into the "Roles / Do / Don't"
    /// CLAUDE.md section. Free-floating policy rules outside any work-item
    /// block remain as standalone `RawAnnotation::Rule` annotations and
    /// are collected by the daemon as workspace defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agent_policy: Vec<AgentPolicyRule>,
}

/// Discriminator between the two work-item header kinds. `@yah:relay(...)`
/// produces `Relay`, `@yah:ticket(...)` produces `Ticket`. Used both by
/// the parser (to drive header dispatch) and by the RPC layer (so wire
/// payloads can name the kind without leaking the internal `CommonKind`
/// enum's variants for non-work-item nodes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemType {
    Relay,
    Ticket,
}

/// Lifecycle column. Mirrors the kanban transitions enforced by the
/// board server (open → claimed/in-progress → handoff → review → done).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    /// Parse the canonical kebab-case form. Authors sometimes write
    /// `in_progress` or capitalize; accept those too.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Some(Self::Open),
            "claimed" => Some(Self::Claimed),
            "in-progress" | "in_progress" | "inprogress" => Some(Self::InProgress),
            "handoff" => Some(Self::Handoff),
            "review" => Some(Self::Review),
            "done" => Some(Self::Done),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Claimed => "claimed",
            Self::InProgress => "in-progress",
            Self::Handoff => "handoff",
            Self::Review => "review",
            Self::Done => "done",
        }
    }
}

/// Per-ticket thinking budget for the agent runtime (R028). Translated
/// into the Claude Agent SDK's `thinking` config when the prelude is
/// assembled. The named tiers (`Deep`/`Standard`/`Fast`) keep authoring
/// terse; `Budget(tokens)` is the escape hatch for callers who need an
/// explicit token cap (e.g. an experiment comparing two budgets).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ThinkBudget {
    /// `@yah:think(deep)` — maximum thinking budget the runtime allows.
    Deep,
    /// `@yah:think(standard)` — workspace default thinking budget.
    Standard,
    /// `@yah:think(fast)` — minimal thinking; favour latency over depth.
    Fast,
    /// `@yah:think(budget=N)` — explicit token cap. The runtime clamps to
    /// whatever the chosen engine supports.
    Budget { tokens: u32 },
}

impl ThinkBudget {
    /// Parse a `@yah:think(...)` payload. Accepts the named tiers
    /// (`deep`/`standard`/`fast`, case-insensitive) or `budget=N` for an
    /// explicit token count.
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if let Some(rest) = trimmed.strip_prefix("budget=") {
            let n: u32 = rest
                .trim()
                .parse()
                .map_err(|_| format!("invalid budget token count {:?}", rest))?;
            return Ok(Self::Budget { tokens: n });
        }
        match trimmed.to_ascii_lowercase().as_str() {
            "deep" => Ok(Self::Deep),
            "standard" => Ok(Self::Standard),
            "fast" => Ok(Self::Fast),
            "" => Err("empty think payload".into()),
            other => Err(format!(
                "unknown think mode {:?} (expected deep|standard|fast|budget=N)",
                other
            )),
        }
    }

    /// Canonical string form, round-trips through `parse`.
    pub fn as_payload(&self) -> String {
        match self {
            Self::Deep => "deep".to_string(),
            Self::Standard => "standard".to_string(),
            Self::Fast => "fast".to_string(),
            Self::Budget { tokens } => format!("budget={}", tokens),
        }
    }
}

/// Per-ticket engine selection (R028). `provider` chooses the runner
/// (`claude` → Claude Agent SDK; anything else → yah-runner); `model`
/// names the specific model on that provider (e.g. `opus-4-7`,
/// `gpt-5`, `qwen3-coder`). A bare `@yah:engine(provider)` form (no
/// model) defers to the workspace's default model for that provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineRef {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl EngineRef {
    /// Parse a `@yah:engine(...)` payload. Canonical form is
    /// `provider:model` (e.g. `claude:opus-4-7`); a bare `provider`
    /// is also accepted and leaves `model` empty.
    pub fn parse(s: &str) -> Result<Self, String> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err("empty engine payload".into());
        }
        if let Some((prov, model)) = trimmed.split_once(':') {
            let prov = prov.trim();
            let model = model.trim();
            if prov.is_empty() {
                return Err("missing provider before colon".into());
            }
            if model.is_empty() {
                return Err("missing model after colon".into());
            }
            Ok(Self {
                provider: prov.to_string(),
                model: Some(model.to_string()),
            })
        } else {
            Ok(Self {
                provider: trimmed.to_string(),
                model: None,
            })
        }
    }

    /// True for engines that route to one of the Claude cells in the
    /// runtime matrix (`architecture/yah-agent-runtime.md`):
    /// HTTP+Anthropic-native (R028, hand-rolled `/v1/messages` with
    /// API-key auth) or Process+MCP (R028 P3, wraps `claude` CLI for
    /// the policy-durable subscription path). Anything else routes
    /// through yah-runner-openai-http (R018/R031). Used by the Tauri
    /// command surface for dispatch.
    pub fn is_claude(&self) -> bool {
        self.provider.eq_ignore_ascii_case("claude")
    }

    /// Canonical string form, round-trips through `parse`.
    pub fn as_payload(&self) -> String {
        match &self.model {
            Some(m) => format!("{}:{}", self.provider, m),
            None => self.provider.clone(),
        }
    }
}

/// A tag with optional namespace. `layer:core` parses to `TagRef { ns:
/// Some("layer"), name: "core" }`. `audio` parses to `TagRef { ns: None,
/// name: "audio" }`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TagRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub name: String,
}

impl TagRef {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            namespace: None,
            name: name.into(),
        }
    }

    pub fn namespaced(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            namespace: Some(namespace.into()),
            name: name.into(),
        }
    }

    /// Stable canonical string. Always prefixed `tag:` so synthetic Tag
    /// node ids never collide with structural-node qualified names.
    pub fn qualified(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("tag:{}:{}", ns, self.name),
            None => format!("tag:{}", self.name),
        }
    }

    /// Display label — the leaf name, or `ns:name` for namespaced tags.
    pub fn label(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}:{}", ns, self.name),
            None => self.name.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_ref_qualified_distinguishes_namespace() {
        assert_eq!(TagRef::new("audio").qualified(), "tag:audio");
        assert_eq!(
            TagRef::namespaced("layer", "core").qualified(),
            "tag:layer:core"
        );
    }

    #[test]
    fn tag_ref_label_is_human_readable() {
        assert_eq!(TagRef::new("audio").label(), "audio");
        assert_eq!(TagRef::namespaced("layer", "core").label(), "layer:core");
    }

    #[test]
    fn annotation_kind_serializes_with_tag_field() {
        let a = AnnotationKind::Tag(TagRef::new("audio"));
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"anno\":\"tag\""), "got {json}");
    }

    #[test]
    fn think_budget_parses_named_tiers() {
        assert_eq!(ThinkBudget::parse("deep").unwrap(), ThinkBudget::Deep);
        assert_eq!(
            ThinkBudget::parse("Standard").unwrap(),
            ThinkBudget::Standard
        );
        assert_eq!(ThinkBudget::parse("FAST").unwrap(), ThinkBudget::Fast);
    }

    #[test]
    fn think_budget_parses_explicit_token_count() {
        assert_eq!(
            ThinkBudget::parse("budget=4096").unwrap(),
            ThinkBudget::Budget { tokens: 4096 }
        );
    }

    #[test]
    fn think_budget_rejects_unknown_mode() {
        assert!(ThinkBudget::parse("medium").is_err());
        assert!(ThinkBudget::parse("budget=oops").is_err());
        assert!(ThinkBudget::parse("").is_err());
    }

    #[test]
    fn think_budget_round_trips_through_payload() {
        for orig in [
            ThinkBudget::Deep,
            ThinkBudget::Standard,
            ThinkBudget::Fast,
            ThinkBudget::Budget { tokens: 2048 },
        ] {
            let s = orig.as_payload();
            assert_eq!(ThinkBudget::parse(&s).unwrap(), orig, "round-trip {s:?}");
        }
    }

    #[test]
    fn engine_ref_parses_provider_model_form() {
        let e = EngineRef::parse("claude:opus-4-7").unwrap();
        assert_eq!(e.provider, "claude");
        assert_eq!(e.model.as_deref(), Some("opus-4-7"));
        assert!(e.is_claude());
    }

    #[test]
    fn engine_ref_parses_bare_provider() {
        let e = EngineRef::parse("claude").unwrap();
        assert_eq!(e.provider, "claude");
        assert!(e.model.is_none());
        assert!(e.is_claude());
    }

    #[test]
    fn engine_ref_rejects_malformed_payloads() {
        assert!(EngineRef::parse("").is_err());
        assert!(EngineRef::parse(":opus-4-7").is_err());
        assert!(EngineRef::parse("claude:").is_err());
    }

    #[test]
    fn engine_ref_is_claude_is_provider_only() {
        assert!(EngineRef::parse("claude:haiku-4-5").unwrap().is_claude());
        assert!(!EngineRef::parse("openai:gpt-5").unwrap().is_claude());
        assert!(!EngineRef::parse("qwen3-coder").unwrap().is_claude());
    }
}
