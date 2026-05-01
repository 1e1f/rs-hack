//! @arch:layer(kg_store)
//! @arch:role(bridge)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! Read-only agent tool registry — the host-side surface a runner offers
//! to the LLM. Sister to [`crate::agent`] (session lifecycle) and to the
//! abstract [`runner::ToolRegistry`] trait the runner consumes.
//!
//! ## Why two traits
//!
//! The runner sees [`runner::ToolRegistry`] — `schemas() + execute(name,
//! args)`. The host implements it with the concrete [`Tool`] trait below,
//! which has access to a [`ToolContext`] holding `KgService` + the rig
//! root. The split keeps `yah-runner` provider-agnostic (no daemon
//! deps) and the host able to add tools that cross into KG / filesystem
//! territory without leaking those types into the runtime.
//!
//! ## Sandbox
//!
//! Every tool that touches the filesystem canonicalizes its target path
//! and rejects anything that escapes the rig root. The pattern mirrors
//! [`kg_daemon::KgService::read_authored_file`] — canonicalize the
//! sandbox once, canonicalize the candidate, `starts_with` check, fall
//! through to a `sandbox_escape` error otherwise. Symlinks resolve to
//! their targets pre-check, so a symlink pointing outside the rig is
//! rejected even though the link itself lives inside.
//!
//! ## Read vs write
//!
//! Two constructors: [`KgToolRegistry::standard_read_only`] is what
//! production sessions get today (eight read-only tools, [`Tool::is_write`]
//! everywhere returns `false`). [`KgToolRegistry::with_experimental_writers`]
//! adds the R031-F4 write surface — `yah_add` / `yah_remove` / `yah_rename` /
//! `yah_transform` / `yah_update` (subprocess to the `yah` CLI), plus
//! `write_arch_doc` and `edit_file` (direct, sandboxed file writes). It is
//! deliberately *not* wired into [`crate::agent::start_runner_session`]:
//! writers ship behind R031-F5's structured approval gate. Until that gate
//! lands the experimental constructor exists for tests + a future opt-in
//! flag in the settings panel.
//!
//! Every writer calls [`KgService::reindex_path`] on the touched file
//! after the mutation succeeds so [`kg::event::IndexReason::AgentEdit`]
//! events fan out through the existing watcher seam — UI updates fall out
//! with no extra plumbing.

use async_trait::async_trait;
use kg::event::IndexReason;
use kg::ids::NodeId;
use kg_daemon::KgService;
use rpc::{Direction, LookupParams, NeighborsParams, SubgraphParams};
use runner::{ToolOutcome, ToolRegistry, ToolSchema};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::agent_approval::{
    mint_request_id, parse_bash, ApprovalChoice, ApprovalDecision, ApprovalGate, ApprovalRequest,
    ApprovalRouter, ApprovalStore, BashCall, BashParseError, InMemoryApprovalStore, PendingCall,
};
use crate::state::RigId;
use kg::agent::SessionId;

/// Hard cap on how many bytes a single read tool returns to the LLM.
/// The runner's per-stream cap is a different concern; this one keeps
/// a single tool call from blowing a session's context budget.
const READ_FILE_MAX_BYTES: u64 = 256 * 1024;

/// Hard cap on grep result count. The model gets a `truncated: true`
/// signal when the cap kicks in so it can refine the pattern.
const GREP_MAX_HITS: usize = 200;

/// Hard cap on bytes scanned per file in `grep`. Pure-Rust regex is
/// linear-time, but a 50MB asset has no business in tool output.
const GREP_MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// Hard cap on directory entries `list_dir` returns. Bigger directories
/// surface as `truncated: true` and a count for the LLM to ask again
/// with a deeper path.
const LIST_DIR_MAX_ENTRIES: usize = 500;

/// Per-call view of which rig + service + root the tool runs against.
/// Tools that touch the filesystem must canonicalize against `rig_root`
/// and reject any candidate that escapes; KG tools dispatch through
/// `svc`. `rig_id` is along for the ride for logging — no tool should
/// branch on it (rig identity is the registry's concern, not the
/// individual tool's).
#[derive(Clone)]
pub struct ToolContext {
    pub rig_id: RigId,
    pub rig_root: PathBuf,
    pub svc: Arc<KgService>,
}

/// Errors a [`Tool`] can surface. Most failure paths are in-band — the
/// LLM sees `{ ok: false, result: { error: ... } }` and adjusts. This
/// enum exists so the registry can distinguish a tool's own error
/// (which should ride back to the model) from a host-level dispatch
/// failure (unknown name, schema-violating args) which doesn't.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// Args didn't match the declared schema. Surfaces as a normal
    /// `ok: false` outcome so the LLM gets the parse-error message and
    /// retries.
    #[error("invalid arguments: {0}")]
    InvalidArgs(String),
    /// Path canonicalization escaped the rig root. The LLM sees this
    /// as a sandbox refusal it must respect.
    #[error("path {0} is outside the rig sandbox")]
    SandboxEscape(String),
    /// Underlying I/O / KG operation failed. The error message rides
    /// to the LLM verbatim — keep it actionable (path, what was being
    /// done) without leaking host-internal detail.
    #[error("{0}")]
    Operation(String),
}

impl ToolError {
    fn into_outcome(self) -> ToolOutcome {
        match self {
            ToolError::SandboxEscape(p) => ToolOutcome {
                ok: false,
                result: json!({
                    "error": format!("path {p} is outside the rig sandbox"),
                    "kind": "sandbox_escape",
                }),
            },
            ToolError::InvalidArgs(msg) => ToolOutcome {
                ok: false,
                result: json!({
                    "error": msg,
                    "kind": "invalid_arguments",
                }),
            },
            ToolError::Operation(msg) => ToolOutcome::fail(msg),
        }
    }
}

/// Concrete host-side tool — the surface every read/write capability
/// implements. Sister to [`runner::ToolRegistry`] which is what the
/// runner sees once tools are wrapped in a [`KgToolRegistry`].
#[async_trait]
pub trait Tool: Send + Sync + 'static {
    /// Stable string the runner uses to dispatch. Must match the schema
    /// in [`Tool::schema`] and be unique within a registry.
    fn name(&self) -> &'static str;

    /// JSON schema spec — name + description + input schema. The LLM
    /// reads `description` to decide which tool to call.
    fn schema(&self) -> ToolSchema;

    /// Side-effect signal. Read-only tools auto-execute; write tools
    /// route through the P5 approval gate. P1 ships only readers, but
    /// the slot is here so the registry's dispatch path won't need to
    /// change when P4 lands.
    fn is_write(&self) -> bool {
        false
    }

    /// Run the tool against `args` produced by the LLM. The wrapper
    /// converts a `Result<Value, ToolError>` into a [`ToolOutcome`]; the
    /// implementation just produces structured JSON or signals failure.
    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError>;
}

/// Concrete registry — the bag of [`Tool`]s the runner pulls schemas from
/// and dispatches into. One per (rig, runner) instance; per-rig because
/// the [`ToolContext`] points at one `KgService` + on-disk root.
///
/// Implements [`runner::ToolRegistry`] so the runner sees the
/// minimal `schemas + execute(name, args)` contract.
pub struct KgToolRegistry {
    tools: Vec<Box<dyn Tool>>,
    ctx: ToolContext,
    /// Approval gate consulted by [`Self::execute_gated`]. Always
    /// present so dispatch semantics are uniform — read-only tools
    /// auto-pass via [`ApprovalDecision::Auto`]; write tools route
    /// through the rule store. Constructors that don't take a store
    /// install an empty in-memory one (no rules → every write call
    /// either prompts the [`router`] or, if there isn't one, fails
    /// with an `approval_required` error so the LLM gets a structured
    /// "needs approval" message back).
    gate: Arc<ApprovalGate>,
    /// Owns the rule storage backing the gate. Held alongside `gate`
    /// (rather than only inside the gate) so the registry can persist
    /// `AlwaysAllow` rules from the inline-approval path without
    /// reaching into the gate's internals.
    store: Arc<dyn ApprovalStore>,
    /// Inline-prompt surface. When present, [`Self::execute_gated`]
    /// suspends on `NeedsPrompt` and awaits the user's choice
    /// (Apply / Skip / AlwaysAllow). When absent (the default —
    /// existing `standard_read_only` / `with_experimental_writers`
    /// constructors), `NeedsPrompt` falls through to the legacy
    /// "structured approval-required failure" so the LLM still gets
    /// a clear signal without the host hanging.
    router: Option<Arc<dyn ApprovalRouter>>,
    /// Session id used to tag [`ApprovalRequest`]s. Mutable because
    /// the runner mints the session id internally (after the
    /// registry is constructed and handed to it as
    /// `Arc<dyn ToolRegistry>`); the host calls
    /// [`Self::bind_session`] post-start to fill this in. `None`
    /// when no router is wired or before the post-start binding —
    /// in either case the registry's prompt path falls through to
    /// the no-router behaviour, so a stale `None` only manifests
    /// for write tools that run before the binding completes
    /// (which doesn't happen — runner.start returns before the
    /// runner can ever dispatch a tool).
    session_id: std::sync::Mutex<Option<SessionId>>,
}

impl KgToolRegistry {
    /// Build the standard read-only registry: read_file, list_dir,
    /// grep, arch_node, arch_neighbors, arch_subgraph, arch_lookup,
    /// read_arch_doc. The full set the agent gets at session start
    /// before any approval gating kicks in (P5).
    pub fn standard_read_only(ctx: ToolContext) -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ReadFile),
            Box::new(ListDir),
            Box::new(Grep),
            Box::new(ArchNode),
            Box::new(ArchNeighbors),
            Box::new(ArchSubgraph),
            Box::new(ArchLookup),
            Box::new(ReadArchDoc),
        ];
        let store: Arc<dyn ApprovalStore> = Arc::new(InMemoryApprovalStore::new());
        Self {
            tools,
            ctx,
            gate: Arc::new(ApprovalGate::new(Arc::clone(&store))),
            store,
            router: None,
            session_id: std::sync::Mutex::new(None),
        }
    }

    /// Read-only set + the R031-F4 experimental write surface:
    /// `yah_add` / `yah_remove` / `yah_rename` / `yah_transform` /
    /// `yah_update` (each subprocesses to the `yah` CLI on PATH so we
    /// inherit every refactor improvement landed in the core hack
    /// engine), plus `write_arch_doc` and `edit_file` (direct,
    /// sandboxed file writes). Each writer calls
    /// [`KgService::reindex_path`] on the touched path so watcher
    /// events fan out as `IndexReason::AgentEdit` and the UI updates.
    ///
    /// This constructor is **not** wired into
    /// [`crate::agent::start_runner_session`] — writers are gated
    /// behind R031-F5's structured approval flow before being handed
    /// to chat sessions. Use it from tests + the eventual
    /// `Settings → Agents → Approval rules` opt-in.
    pub fn with_experimental_writers(ctx: ToolContext) -> Self {
        let mut reg = Self::standard_read_only(ctx);
        reg.tools.push(Box::new(YahAdd));
        reg.tools.push(Box::new(YahRemove));
        reg.tools.push(Box::new(YahRename));
        reg.tools.push(Box::new(YahTransform));
        reg.tools.push(Box::new(YahUpdate));
        reg.tools.push(Box::new(WriteArchDoc));
        reg.tools.push(Box::new(EditFile));
        reg
    }

    /// Replace the registry's [`ApprovalGate`] with one backed by a
    /// caller-supplied [`ApprovalStore`]. Used by production wiring
    /// (KV-backed store) and by tests that need to seed rules. The
    /// builder shape lets `Self::standard_read_only(ctx).with_store(s)`
    /// stay one expression.
    pub fn with_store(mut self, store: Arc<dyn ApprovalStore>) -> Self {
        self.gate = Arc::new(ApprovalGate::new(Arc::clone(&store)));
        self.store = store;
        self
    }

    /// Borrow the underlying [`ApprovalStore`]. The Settings UI
    /// surface (Tauri commands `agent_approval_rules_*`) reads + edits
    /// rules through this — same store the gate consults, so changes
    /// take effect on the next tool call without any reload step.
    pub fn store(&self) -> Arc<dyn ApprovalStore> {
        Arc::clone(&self.store)
    }

    /// Plug in the inline-prompt surface. With a router set,
    /// [`Self::execute_gated`] suspends on a no-rule write call,
    /// emits an [`ApprovalRequest`] to the router, and awaits the
    /// user's [`ApprovalChoice`]. Without one, the registry falls
    /// back to the legacy "approval_required" failure outcome.
    pub fn with_router(mut self, router: Arc<dyn ApprovalRouter>) -> Self {
        self.router = Some(router);
        self
    }

    /// Tag this registry with the live chat session it serves. Used
    /// to populate [`ApprovalRequest::session_id`] so the renderer
    /// can route prompts back to the right pane. Builder form for
    /// tests where the session id is known up-front; production uses
    /// [`Self::bind_session`] after the runner mints the id.
    pub fn with_session(self, session_id: SessionId) -> Self {
        self.bind_session(session_id);
        self
    }

    /// Set the session id post-construction. Production path: the
    /// runner mints the session id after the registry is wrapped in
    /// `Arc<dyn ToolRegistry>` and handed off, so we can't pass the
    /// id through a builder. The host holds an `Arc<KgToolRegistry>`
    /// alongside the dyn-trait Arc and calls this immediately after
    /// `runner.start()` returns.
    pub fn bind_session(&self, session_id: SessionId) {
        *self.session_id.lock().unwrap() = Some(session_id);
    }

    /// Tool count — used by tests + the eventual settings panel.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Borrow a single tool by name. Used by the Anthropic native
    /// `tool_use` loop (when it lands) so it can iterate
    /// [`Tool::schema`] and dispatch via [`Self::execute_gated`]
    /// without round-tripping through the runner trait.
    pub fn tool(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| &**t as &dyn Tool)
    }

    /// Borrow every tool. Stable order — same as schema emission so
    /// the Anthropic loop's tools array matches the OpenAI shim.
    pub fn tools(&self) -> impl Iterator<Item = &dyn Tool> {
        self.tools.iter().map(|t| &**t as &dyn Tool)
    }

    /// Single dispatch entry point — runs the approval gate, then
    /// dispatches to [`Tool::execute`] if allowed. The runner-shaped
    /// [`ToolRegistry::execute`] is a thin shim that forwards here
    /// with the registry's own [`ToolContext`]; the Anthropic native
    /// `tool_use` loop will call this directly with its session
    /// context (see .yah/arch/authored/agent-tool-calls.md "Approval gate
    /// placement").
    ///
    /// Bash gets pre-parsed via [`parse_bash`] before the gate runs:
    /// rules match the structured [`BashCall`] (env / cmd / args),
    /// not the raw input string. Once approved, the tool itself is
    /// expected to re-synthesize its CLI from the parsed struct via
    /// [`crate::agent_approval::synthesize_bash`] — so an attacker
    /// can't paper extra env/args past a glob match.
    pub async fn execute_gated(&self, name: &str, args: Value, ctx: &ToolContext) -> ToolOutcome {
        let Some(tool) = self.tools.iter().find(|t| t.name() == name) else {
            return ToolOutcome::fail(format!(
                "unknown tool '{name}' — known tools: {}",
                self.tools
                    .iter()
                    .map(|t| t.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        };

        let parsed_bash = if name == "bash" {
            match args.get("command").and_then(|v| v.as_str()) {
                Some(s) => match parse_bash(s) {
                    Ok(call) => Some(call),
                    Err(e) => {
                        return bash_parse_outcome(e);
                    }
                },
                None => None,
            }
        } else {
            None
        };

        let pending = PendingCall {
            tool_name: name,
            args: &args,
            is_write: tool.is_write(),
            bash: parsed_bash.as_ref(),
        };
        match self.gate.decide(&pending) {
            ApprovalDecision::Auto | ApprovalDecision::Allow { .. } => {}
            ApprovalDecision::NeedsPrompt { reason } => {
                match self
                    .prompt_for_approval(name, &args, parsed_bash.as_ref())
                    .await
                {
                    PromptOutcome::Allowed => {}
                    PromptOutcome::Skipped => {
                        return ToolOutcome {
                            ok: false,
                            result: json!({
                                "error": "user declined this tool call",
                                "kind": "approval_skipped",
                                "tool": name,
                            }),
                        };
                    }
                    PromptOutcome::NoRouter => {
                        /* No interactive router wired (test path / read-only
                        registry). Fall back to the structured failure so
                        the LLM gets a clear signal it needs the user to
                        add a rule rather than the host hanging. */
                        return ToolOutcome {
                            ok: false,
                            result: json!({
                                "error": format!(
                                    "approval required: {reason}. Ask the user to add a rule under Settings → Agents → Approval rules.",
                                ),
                                "kind": "approval_required",
                                "tool": name,
                            }),
                        };
                    }
                }
            }
            ApprovalDecision::Deny { reason } => {
                return ToolOutcome {
                    ok: false,
                    result: json!({
                        "error": reason,
                        "kind": "approval_denied",
                        "tool": name,
                    }),
                };
            }
        }

        match tool.execute(args.clone(), ctx).await {
            Ok(value) => ToolOutcome::ok(stamp_smell(name, &args, value, true)),
            Err(e) => {
                let mut outcome = e.into_outcome();
                outcome.result = stamp_smell(name, &args, outcome.result, false);
                outcome
            }
        }
    }

    /// Surface a no-rule write call to the inline prompt surface and
    /// wait for the user's choice. AlwaysAllow rules are persisted to
    /// [`Self::store`] before we return — same store the gate
    /// consults, so the *next* tool call with the same shape skips
    /// the prompt without a reload.
    ///
    /// The router contract is "must resolve eventually". If the
    /// session aborts mid-prompt and the underlying oneshot drops,
    /// the router's `request` future drops too — caller's spawned
    /// task already got cancelled, so we never observe that path.
    async fn prompt_for_approval(
        &self,
        tool_name: &str,
        args: &Value,
        bash: Option<&BashCall>,
    ) -> PromptOutcome {
        let Some(router) = self.router.as_ref() else {
            return PromptOutcome::NoRouter;
        };
        // Without a session id we can't tag the prompt for the
        // renderer to route back. Fall through to NoRouter rather
        // than emitting an event no one can answer.
        let Some(session_id) = self.session_id.lock().unwrap().clone() else {
            return PromptOutcome::NoRouter;
        };
        let request = ApprovalRequest {
            session_id,
            request_id: mint_request_id(),
            tool_name: tool_name.to_string(),
            args: args.clone(),
            bash: bash.cloned(),
        };
        match router.request(request).await {
            ApprovalChoice::Apply => PromptOutcome::Allowed,
            ApprovalChoice::Skip => PromptOutcome::Skipped,
            ApprovalChoice::AlwaysAllow { rule } => {
                self.store.push(rule);
                PromptOutcome::Allowed
            }
        }
    }
}

/// Internal result of [`KgToolRegistry::prompt_for_approval`]. Kept
/// private to this module — the public API surfaces this as a
/// `ToolOutcome` shape (allowed → tool runs, skipped → fail with
/// `approval_skipped`, no-router → fail with `approval_required`).
enum PromptOutcome {
    Allowed,
    Skipped,
    NoRouter,
}

fn bash_parse_outcome(err: BashParseError) -> ToolOutcome {
    ToolOutcome {
        ok: false,
        result: json!({
            "error": err.to_string(),
            "kind": "bash_parse_error",
            "tool": "bash",
        }),
    }
}

/// Stamp a one-line `_smell` summary onto a tool result so the model
/// (and the renderer) can recount what just happened without
/// re-walking nested JSON. The smell line shape is
/// `<tool> <key-arg> · <signal> · ok|fail` — stable enough to quote
/// back, narrow enough that fabricating one in a recap stands out.
///
/// If `result` is a JSON object we add `_smell` in place; otherwise we
/// wrap the value as `{ _smell, value }` so the field always rides on
/// the top level. Existing schema fields are preserved verbatim.
fn stamp_smell(name: &str, args: &Value, result: Value, ok: bool) -> Value {
    let summary = smell_summary(name, args, &result, ok);
    match result {
        Value::Object(mut map) => {
            map.insert("_smell".into(), Value::String(summary));
            Value::Object(map)
        }
        other => json!({ "_smell": summary, "value": other }),
    }
}

fn smell_summary(name: &str, args: &Value, result: &Value, ok: bool) -> String {
    let status = if ok { "ok" } else { "fail" };
    let arg_str = smell_arg(name, args);
    let signal = smell_signal(name, result, ok);
    let mut out = String::with_capacity(64);
    out.push_str(name);
    if !arg_str.is_empty() {
        out.push(' ');
        out.push_str(&arg_str);
    }
    if !signal.is_empty() {
        out.push_str(" · ");
        out.push_str(&signal);
    }
    out.push_str(" · ");
    out.push_str(status);
    out
}

fn smell_arg(name: &str, args: &Value) -> String {
    let pick = |keys: &[&str]| -> String {
        for k in keys {
            if let Some(s) = args.get(k).and_then(|v| v.as_str()) {
                return s.to_string();
            }
        }
        String::new()
    };
    match name {
        "read_file" | "list_dir" | "edit_file" | "write_arch_doc" => pick(&["path", "rel_path"]),
        "read_arch_doc" => pick(&["rel_path", "path"]),
        "grep" => {
            let p = pick(&["pattern"]);
            let g = args.get("glob").and_then(|v| v.as_str()).unwrap_or("");
            if g.is_empty() {
                format!("\"{p}\"")
            } else {
                format!("\"{p}\" in {g}")
            }
        }
        "arch_node" | "arch_neighbors" | "arch_subgraph" => pick(&["id", "root"]),
        "arch_lookup" => {
            let f = pick(&["file"]);
            match args.get("line").and_then(|v| v.as_u64()) {
                Some(l) => format!("{f}:{l}"),
                None => f,
            }
        }
        "bash" => pick(&["command", "cmd"]),
        _ => pick(&["path", "rel_path", "id", "pattern", "command"]),
    }
}

fn smell_signal(name: &str, result: &Value, ok: bool) -> String {
    if !ok {
        if let Some(err) = result.get("error").and_then(|v| v.as_str()) {
            return truncate_signal(err);
        }
        if let Some(kind) = result.get("kind").and_then(|v| v.as_str()) {
            return kind.to_string();
        }
        return String::new();
    }
    match name {
        "read_file" => {
            let lines = result.get("lines").and_then(|v| v.as_u64());
            let bytes = result.get("bytes").and_then(|v| v.as_u64());
            let trunc = result
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut parts = Vec::new();
            if let Some(b) = bytes {
                parts.push(human_bytes(b));
            }
            if let Some(l) = lines {
                parts.push(format!("{l} lines"));
            }
            if trunc {
                parts.push("truncated".to_string());
            }
            parts.join(", ")
        }
        "list_dir" => {
            let entries = result
                .get("entries")
                .and_then(|v| v.as_array())
                .map(|a| a.len());
            entries.map(|n| format!("{n} entries")).unwrap_or_default()
        }
        "grep" => {
            let hits = result
                .get("hits")
                .and_then(|v| v.as_array())
                .map(|a| a.len());
            let trunc = result
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match (hits, trunc) {
                (Some(n), true) => format!("{n} hits, truncated"),
                (Some(n), false) => format!("{n} hits"),
                _ => String::new(),
            }
        }
        "read_arch_doc" => result
            .get("bytes")
            .and_then(|v| v.as_u64())
            .map(human_bytes)
            .unwrap_or_default(),
        "arch_neighbors" | "arch_subgraph" => result
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|a| format!("{} nodes", a.len()))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn human_bytes(n: u64) -> String {
    if n < 1024 {
        format!("{n}B")
    } else if n < 1024 * 1024 {
        format!("{:.1}KB", (n as f64) / 1024.0)
    } else {
        format!("{:.1}MB", (n as f64) / (1024.0 * 1024.0))
    }
}

fn truncate_signal(s: &str) -> String {
    let first_line = s.lines().next().unwrap_or("");
    if first_line.len() <= 80 {
        first_line.to_string()
    } else {
        format!("{}…", &first_line[..80])
    }
}

#[async_trait]
impl ToolRegistry for KgToolRegistry {
    fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    async fn execute(&self, name: &str, args: Value) -> ToolOutcome {
        /* Thin shim — the gate + dispatch live in `execute_gated`. The
        Anthropic native `tool_use` loop calls `execute_gated`
        directly so it never round-trips through this trait. */
        self.execute_gated(name, args, &self.ctx).await
    }
}

// ---------- Sandbox helper ----------

/// Resolve `rel` against `rig_root` and verify the canonical result
/// stays inside the sandbox. Mirrors
/// [`kg_daemon::KgService::read_authored_file`]'s pattern. Returns
/// the canonical absolute path on success.
fn resolve_in_sandbox(rig_root: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    let candidate = rig_root.join(rel);
    let candidate_canon = candidate
        .canonicalize()
        .map_err(|e| ToolError::Operation(format!("cannot resolve {rel}: {e}")))?;
    let root_canon = rig_root
        .canonicalize()
        .unwrap_or_else(|_| rig_root.to_path_buf());
    if !candidate_canon.starts_with(&root_canon) {
        return Err(ToolError::SandboxEscape(rel.to_string()));
    }
    Ok(candidate_canon)
}

/// Same as [`resolve_in_sandbox`] but for a path that needn't exist yet
/// — used by `list_dir` on directories we want to confirm-then-walk.
/// Falls through to a non-canonicalizing path-prefix check when the
/// candidate doesn't exist (still safe: if any path segment escapes the
/// root in string form, it's rejected).
fn resolve_existing_dir(rig_root: &Path, rel: &str) -> Result<PathBuf, ToolError> {
    let candidate = rig_root.join(rel);
    if !candidate.exists() {
        return Err(ToolError::Operation(format!(
            "directory does not exist: {rel}"
        )));
    }
    resolve_in_sandbox(rig_root, rel)
}

// ---------- read_file ----------

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    /// Rig-relative path of the file to read.
    path: String,
    /// Optional 1-based start line. Inclusive.
    #[serde(default)]
    start_line: Option<u32>,
    /// Optional 1-based end line. Inclusive. Without `start_line` this
    /// is a no-op (the whole file is returned).
    #[serde(default)]
    end_line: Option<u32>,
}

struct ReadFile;

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Read a file from the rig. Optionally restrict to a 1-based line range. \
                          Returns up to 256 KB; larger reads are truncated with a `truncated` flag. \
                          Sandboxed to the rig root — paths that escape are refused."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Rig-relative path." },
                    "start_line": { "type": "integer", "minimum": 1, "description": "1-based start line (inclusive)." },
                    "end_line":   { "type": "integer", "minimum": 1, "description": "1-based end line (inclusive)." }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadFileArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let canon = resolve_in_sandbox(&ctx.rig_root, &args.path)?;
        let metadata = std::fs::metadata(&canon)
            .map_err(|e| ToolError::Operation(format!("stat {}: {e}", args.path)))?;
        if !metadata.is_file() {
            return Err(ToolError::Operation(format!("{} is not a file", args.path)));
        }
        let total_bytes = metadata.len();
        let bytes = std::fs::read(&canon)
            .map_err(|e| ToolError::Operation(format!("read {}: {e}", args.path)))?;
        let raw = String::from_utf8_lossy(&bytes).into_owned();
        let (content, truncated) = match (args.start_line, args.end_line) {
            (None, None) => {
                if total_bytes > READ_FILE_MAX_BYTES {
                    let cap = READ_FILE_MAX_BYTES as usize;
                    let cut = raw
                        .char_indices()
                        .nth(cap)
                        .map(|(i, _)| i)
                        .unwrap_or(raw.len());
                    (raw[..cut].to_string(), true)
                } else {
                    (raw, false)
                }
            }
            (start, end) => {
                let start = start.unwrap_or(1).max(1) as usize;
                let end = end.unwrap_or(u32::MAX) as usize;
                let mut out = String::new();
                for (idx, line) in raw.lines().enumerate() {
                    let line_no = idx + 1;
                    if line_no < start {
                        continue;
                    }
                    if line_no > end {
                        break;
                    }
                    out.push_str(line);
                    out.push('\n');
                }
                (out, false)
            }
        };
        Ok(json!({
            "path": args.path,
            "content": content,
            "bytes": total_bytes,
            "truncated": truncated,
        }))
    }
}

// ---------- list_dir ----------

#[derive(Debug, Deserialize)]
struct ListDirArgs {
    /// Rig-relative directory. Empty string lists the rig root.
    #[serde(default)]
    path: String,
}

struct ListDir;

#[async_trait]
impl Tool for ListDir {
    fn name(&self) -> &'static str {
        "list_dir"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "List immediate entries (files + subdirectories) of a rig directory. \
                          Returns up to 500 entries; larger directories surface as `truncated`. \
                          Sandboxed to the rig root."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Rig-relative directory. Empty string = rig root." }
                }
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ListDirArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let rel = if args.path.is_empty() {
            "."
        } else {
            &args.path
        };
        let canon = resolve_existing_dir(&ctx.rig_root, rel)?;
        if !canon.is_dir() {
            return Err(ToolError::Operation(format!(
                "{} is not a directory",
                args.path
            )));
        }
        let read_dir = std::fs::read_dir(&canon)
            .map_err(|e| ToolError::Operation(format!("read_dir {}: {e}", args.path)))?;
        let mut entries: Vec<Value> = Vec::new();
        let mut truncated = false;
        let mut total = 0usize;
        for entry in read_dir.flatten() {
            total += 1;
            if entries.len() >= LIST_DIR_MAX_ENTRIES {
                truncated = true;
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            // Skip the conventional rig-noise dirs by default — agents
            // chasing `target/` or `.git/` is wasted context.
            if matches!(name.as_str(), "target" | "node_modules" | ".git") {
                continue;
            }
            let file_type = entry.file_type().ok();
            let kind = match file_type {
                Some(ft) if ft.is_dir() => "dir",
                Some(ft) if ft.is_file() => "file",
                Some(ft) if ft.is_symlink() => "symlink",
                _ => "other",
            };
            let bytes = entry
                .metadata()
                .ok()
                .filter(|m| m.is_file())
                .map(|m| m.len());
            entries.push(json!({
                "name": name,
                "kind": kind,
                "bytes": bytes,
            }));
        }
        entries.sort_by(|a, b| {
            a["name"]
                .as_str()
                .unwrap_or("")
                .cmp(b["name"].as_str().unwrap_or(""))
        });
        Ok(json!({
            "path": args.path,
            "entries": entries,
            "total": total,
            "truncated": truncated,
        }))
    }
}

// ---------- grep ----------

#[derive(Debug, Deserialize)]
struct GrepArgs {
    /// Regex pattern (Rust regex crate syntax).
    pattern: String,
    /// Optional glob to filter file paths (e.g. `**/*.rs`). Matched
    /// against the rig-relative path.
    #[serde(default)]
    glob: Option<String>,
    /// Optional max hits override; capped at [`GREP_MAX_HITS`].
    #[serde(default)]
    max_hits: Option<usize>,
}

struct Grep;

#[async_trait]
impl Tool for Grep {
    fn name(&self) -> &'static str {
        "grep"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Search the rig for a regex pattern. Optionally filter files by glob \
                          (e.g. `**/*.rs`). Skips target/, node_modules/, .git/. Returns up to \
                          200 hits; truncated:true means the search is incomplete."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern":  { "type": "string", "description": "Rust regex syntax." },
                    "glob":     { "type": "string", "description": "Glob to filter rig-relative paths." },
                    "max_hits": { "type": "integer", "minimum": 1, "description": "Cap on returned hits (max 200)." }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: GrepArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let regex = regex::Regex::new(&args.pattern)
            .map_err(|e| ToolError::InvalidArgs(format!("regex compile failed: {e}")))?;
        let glob = match args.glob {
            Some(ref g) => Some(
                glob::Pattern::new(g)
                    .map_err(|e| ToolError::InvalidArgs(format!("glob parse failed: {e}")))?,
            ),
            None => None,
        };
        let cap = args.max_hits.unwrap_or(GREP_MAX_HITS).min(GREP_MAX_HITS);

        let mut hits: Vec<Value> = Vec::new();
        let mut truncated = false;

        let walker = walkdir::WalkDir::new(&ctx.rig_root)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !matches!(name.as_ref(), "target" | "node_modules" | ".git")
            });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let Ok(rel) = path.strip_prefix(&ctx.rig_root) else {
                continue;
            };
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if let Some(ref pat) = glob {
                if !pat.matches(&rel_str) {
                    continue;
                }
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.len() > GREP_MAX_FILE_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&bytes) else {
                continue;
            };
            for (idx, line) in text.lines().enumerate() {
                if regex.is_match(line) {
                    if hits.len() >= cap {
                        truncated = true;
                        break;
                    }
                    hits.push(json!({
                        "file": rel_str,
                        "line": (idx + 1) as u32,
                        "text": line.chars().take(400).collect::<String>(),
                    }));
                }
            }
            if truncated {
                break;
            }
        }
        Ok(json!({
            "pattern": args.pattern,
            "hits": hits,
            "truncated": truncated,
        }))
    }
}

// ---------- KG-backed tools ----------

#[derive(Debug, Deserialize)]
struct ArchNodeArgs {
    /// 32-char hex `NodeId` (the canonical wire form).
    id: String,
}

struct ArchNode;

#[async_trait]
impl Tool for ArchNode {
    fn name(&self) -> &'static str {
        "arch_node"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Fetch a knowledge-graph node by its hex id. Returns the full node \
                          (kind, qualified name, file:line span, doc, properties, annotations) \
                          when present, or null when the id isn't in the graph."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "32-char hex NodeId." }
                },
                "required": ["id"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ArchNodeArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let id = NodeId::from_hex(&args.id)
            .map_err(|e| ToolError::InvalidArgs(format!("bad node id: {e}")))?;
        let node = ctx.svc.node(id).await;
        Ok(json!({ "node": node }))
    }
}

#[derive(Debug, Deserialize)]
struct ArchNeighborsArgs {
    id: String,
    /// `in`, `out`, or `both`.
    #[serde(default = "default_direction")]
    dir: String,
    /// Optional list of edge kinds (snake_case): `contains`, `imports`,
    /// `calls`, `references`, `implements`, …. Omit to include every kind.
    #[serde(default)]
    edges: Option<Vec<String>>,
}

fn default_direction() -> String {
    "both".to_string()
}

struct ArchNeighbors;

#[async_trait]
impl Tool for ArchNeighbors {
    fn name(&self) -> &'static str {
        "arch_neighbors"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Walk the knowledge graph one hop from a node. Direction is `in`, `out`, \
                          or `both`. Optional `edges` filter restricts to specific edge kinds \
                          (e.g. ['contains', 'imports']). Returns the matching outgoing edges."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "32-char hex NodeId." },
                    "dir": {
                        "type": "string",
                        "enum": ["in", "out", "both"],
                        "default": "both"
                    },
                    "edges": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Edge-kind names (snake_case)."
                    }
                },
                "required": ["id"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ArchNeighborsArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let id = NodeId::from_hex(&args.id)
            .map_err(|e| ToolError::InvalidArgs(format!("bad node id: {e}")))?;
        let dir = match args.dir.as_str() {
            "in" => Direction::In,
            "out" => Direction::Out,
            "both" => Direction::Both,
            other => {
                return Err(ToolError::InvalidArgs(format!(
                    "dir must be in|out|both, got {other}"
                )))
            }
        };
        let edges = parse_edge_kinds(args.edges.as_deref())?;
        let result = ctx.svc.neighbors(NeighborsParams { id, dir, edges }).await;
        Ok(serde_json::to_value(result)
            .map_err(|e| ToolError::Operation(format!("encode neighbors: {e}")))?)
    }
}

#[derive(Debug, Deserialize)]
struct ArchSubgraphArgs {
    /// Hex NodeId of the subgraph root.
    root: String,
    /// Hop depth. Default `2`.
    #[serde(default = "default_depth")]
    depth: u8,
    /// Optional edge-kind filter (snake_case names).
    #[serde(default)]
    edges: Option<Vec<String>>,
    /// Optional cap on returned nodes. Daemon picks a default if absent.
    #[serde(default)]
    node_limit: Option<u32>,
}

fn default_depth() -> u8 {
    2
}

struct ArchSubgraph;

#[async_trait]
impl Tool for ArchSubgraph {
    fn name(&self) -> &'static str {
        "arch_subgraph"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Walk the knowledge graph to a depth (default 2) from a root node. \
                          Returns matching nodes + edges with a `truncated` flag if the cap was hit."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "root": { "type": "string", "description": "32-char hex NodeId." },
                    "depth": { "type": "integer", "minimum": 0, "default": 2 },
                    "edges": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Edge-kind names (snake_case)."
                    },
                    "node_limit": { "type": "integer", "minimum": 1 }
                },
                "required": ["root"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ArchSubgraphArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let root = NodeId::from_hex(&args.root)
            .map_err(|e| ToolError::InvalidArgs(format!("bad node id: {e}")))?;
        let edges = parse_edge_kinds(args.edges.as_deref())?;
        let result = ctx
            .svc
            .subgraph(SubgraphParams {
                root,
                depth: args.depth,
                edges,
                kinds: None,
                langs: None,
                node_limit: args.node_limit,
            })
            .await;
        Ok(serde_json::to_value(result)
            .map_err(|e| ToolError::Operation(format!("encode subgraph: {e}")))?)
    }
}

#[derive(Debug, Deserialize)]
struct ArchLookupArgs {
    /// Rig-relative file path.
    file: String,
    /// Optional 1-based line number.
    #[serde(default)]
    line: Option<u32>,
}

struct ArchLookup;

#[async_trait]
impl Tool for ArchLookup {
    fn name(&self) -> &'static str {
        "arch_lookup"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description:
                "Resolve a file:line to the innermost knowledge-graph nodes that span it. \
                          Returns ids inner-most first (method before its containing type before \
                          the module before the file)."
                    .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "file": { "type": "string", "description": "Rig-relative path." },
                    "line": { "type": "integer", "minimum": 1, "description": "1-based line number." }
                },
                "required": ["file"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ArchLookupArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let result = ctx
            .svc
            .lookup(LookupParams {
                file: args.file,
                line: args.line,
                col: None,
            })
            .await;
        Ok(serde_json::to_value(result)
            .map_err(|e| ToolError::Operation(format!("encode lookup: {e}")))?)
    }
}

// ---------- read_arch_doc ----------

#[derive(Debug, Deserialize)]
struct ReadArchDocArgs {
    /// Rig-relative path inside `<rig_root>/.yah/arch/authored/`.
    rel_path: String,
}

struct ReadArchDoc;

#[async_trait]
impl Tool for ReadArchDoc {
    fn name(&self) -> &'static str {
        "read_arch_doc"
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Read an authored architecture diagram (`.mmd`) from \
                          `<rig>/.yah/arch/authored/`. Sandboxed — paths outside that directory \
                          are refused."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "rel_path": {
                        "type": "string",
                        "description": "Rig-relative path inside .yah/arch/authored/."
                    }
                },
                "required": ["rel_path"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: ReadArchDocArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let result = ctx
            .svc
            .read_authored_file(rpc::ReadAuthoredFileParams {
                rel_path: args.rel_path,
            })
            .await
            .map_err(|e| ToolError::Operation(e.to_string()))?;
        Ok(serde_json::to_value(result)
            .map_err(|e| ToolError::Operation(format!("encode read_arch_doc: {e}")))?)
    }
}

// ---------- yah_* (subprocess to `yah` CLI) ----------
//
// Wraps the rs-hack MCP surface (mcp__rs-hack__add / remove / rename /
// transform / update). Forwarding to the `yah` binary keeps a single source
// of truth for argument shapes + behavior — when R022 lifts the CLI to a
// `yah::cli::run(argv)` lib API, the only change here is swapping
// `Command::new` for an in-process call.
//
// Each tool:
//   1. Builds CLI args from the JSON payload (snake_case → kebab-case
//      flags, mirroring `yah/src/mcp/tools.rs`'s argv builder).
//   2. Always passes `--apply` — the LLM-facing tool is the *write* path;
//      the model uses `find` (read-only) for inspection.
//   3. Spawns `yah <subcommand> ...args` with cwd = `rig_root`.
//   4. After a zero-exit, walks `paths` via the same glob the CLI saw
//      and calls [`KgService::reindex_path`] on each match so watcher
//      events fan out as `IndexReason::AgentEdit`.
//
// Subprocess failures (binary not on PATH, non-zero exit) surface as
// in-band tool errors so the LLM gets actionable feedback and can retry
// with adjusted args. The bytes of stdout + stderr are returned verbatim
// (clipped to a generous cap) — the rs-hack CLI already speaks LLM-shaped
// hints, no need to re-render.

const YAH_TOOL_OUTPUT_CAP: usize = 64 * 1024;

/// Spawn `yah <subcommand> <args>` with cwd = `rig_root` and capture
/// stdout / stderr. The child inherits the parent's PATH so a fresh
/// `cargo install --path yah` is picked up without re-launching the
/// host. Returns the captured bytes + exit code; a missing binary is
/// surfaced as a `ToolError::Operation` pointing the user at install.
async fn run_yah_cli(
    rig_root: &Path,
    subcommand: &str,
    args: &[String],
) -> Result<(String, String, i32), ToolError> {
    let output = tokio::process::Command::new("yah")
        .args(subcommand.split_whitespace())
        .args(args)
        .current_dir(rig_root)
        .output()
        .await
        .map_err(|e| {
            ToolError::Operation(format!(
                "failed to spawn `yah {subcommand}`: {e} — is `yah` on PATH?"
            ))
        })?;
    let mut stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if stdout.len() > YAH_TOOL_OUTPUT_CAP {
        stdout.truncate(YAH_TOOL_OUTPUT_CAP);
        stdout.push_str("\n…[truncated]");
    }
    if stderr.len() > YAH_TOOL_OUTPUT_CAP {
        stderr.truncate(YAH_TOOL_OUTPUT_CAP);
        stderr.push_str("\n…[truncated]");
    }
    Ok((stdout, stderr, output.status.code().unwrap_or(-1)))
}

/// After a successful CLI write, expand the agent's `paths` argument
/// against `rig_root` and reindex every match. The CLI accepts a single
/// glob ("src/**/*.rs") or a literal path; either round-trips through
/// [`glob::glob_with`] without error. Skips entries that escape the
/// sandbox or that [`kg_daemon::is_eligible`]-style filters reject
/// (calling `reindex_path` on `target/` is a no-op anyway).
async fn reindex_glob(svc: &KgService, rig_root: &Path, raw_paths: &str) -> Vec<String> {
    let mut touched = Vec::new();
    let pattern_full = rig_root.join(raw_paths);
    let Some(pattern) = pattern_full.to_str() else {
        return touched;
    };
    let opts = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };
    let Ok(walker) = glob::glob_with(pattern, opts) else {
        return touched;
    };
    for entry in walker.flatten() {
        if !entry.is_file() {
            continue;
        }
        let Ok(canon) = entry.canonicalize() else {
            continue;
        };
        let Ok(root_canon) = rig_root.canonicalize() else {
            continue;
        };
        if !canon.starts_with(&root_canon) {
            continue;
        }
        if svc
            .reindex_path(&canon, IndexReason::AgentEdit)
            .await
            .is_ok()
        {
            if let Ok(rel) = canon.strip_prefix(&root_canon) {
                touched.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    touched
}

/// Pick out a string field from `args`. Returns `None` cleanly when the
/// key is missing or not a string — every yah tool argument is optional
/// at the JSON layer; the CLI itself enforces what's required.
fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str())
}

fn opt_bool(args: &Value, key: &str) -> bool {
    args.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// Push `--flag value` for every present string-typed key. Mirrors the
/// MCP server's argv builder so the agent's argument shape matches what
/// a human user (and the rs-hack MCP surface) sees today.
fn push_str_flags(args: &Value, keys: &[&str], cli_args: &mut Vec<String>) {
    for key in keys {
        if let Some(v) = opt_str(args, key) {
            cli_args.push(format!("--{}", key.replace('_', "-")));
            cli_args.push(v.to_string());
        }
    }
}

/// Build the structured outcome envelope every yah_* writer returns. The
/// LLM keys off `ok` / `exit_code` to decide whether to retry; `touched`
/// shows what we reindexed so it can plan follow-up reads precisely.
fn yah_outcome(stdout: String, stderr: String, exit_code: i32, touched: Vec<String>) -> Value {
    json!({
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
        "touched": touched,
    })
}

/// Given the JSON payload, run a yah CLI subcommand, then reindex the
/// `paths` glob. Shared body for every yah_* writer. The subcommand
/// names (`add`, `remove`, …) are what the CLI dispatches on, so we
/// stay 1:1 with `yah/src/mcp/tools.rs`.
async fn dispatch_yah(
    subcommand: &str,
    cli_args: Vec<String>,
    raw_paths: Option<&str>,
    ctx: &ToolContext,
) -> Result<Value, ToolError> {
    let (stdout, stderr, exit_code) = run_yah_cli(&ctx.rig_root, subcommand, &cli_args).await?;
    let touched = if exit_code == 0 {
        match raw_paths {
            Some(p) => reindex_glob(&ctx.svc, &ctx.rig_root, p).await,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    Ok(yah_outcome(stdout, stderr, exit_code, touched))
}

struct YahAdd;

#[async_trait]
impl Tool for YahAdd {
    fn name(&self) -> &'static str {
        "yah_add"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Add a Rust AST node — struct field, enum variant, impl method, derive, \
                 use statement, match arm, or doc comment. Auto-detects the operation \
                 from the args. Mirrors `mcp__rs-hack__add`. Always applies; the agent \
                 should `find` first to confirm targets. Reindexes the touched files."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string", "description": "Rig-relative path or glob (e.g. \"src/**/*.rs\")." },
                    "name": { "type": "string", "description": "Target name (struct/enum/function). Use `Enum::Variant` for variant literals." },
                    "kind": { "type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"], "description": "Semantic grouping. Mutually exclusive with `node_type`." },
                    "node_type": { "type": "string", "description": "Granular AST node type for surgical precision." },
                    "field_name": { "type": "string" },
                    "field_type": { "type": "string" },
                    "field_value": { "type": "string" },
                    "variant": { "type": "string" },
                    "method": { "type": "string", "description": "Full method definition (e.g. `pub fn id(&self) -> u64 { self.id }`)." },
                    "derive": { "type": "string", "description": "Comma-separated derives (e.g. `Clone,Debug`)." },
                    "use": { "type": "string", "description": "Use-statement path (e.g. `serde::Serialize`)." },
                    "match_arm": { "type": "string" },
                    "body": { "type": "string" },
                    "function": { "type": "string" },
                    "doc_comment": { "type": "string" },
                    "position": { "type": "string", "description": "`first`, `last`, or `after:item_name`." },
                    "literal_only": { "type": "boolean", "default": false },
                    "auto_detect": { "type": "boolean", "default": false },
                    "enum_name": { "type": "string" }
                },
                "required": ["paths"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let mut cli = Vec::new();
        push_str_flags(
            &args,
            &[
                "paths",
                "name",
                "kind",
                "node_type",
                "field_name",
                "field_type",
                "field_value",
                "variant",
                "method",
                "derive",
                "use",
                "match_arm",
                "body",
                "function",
                "doc_comment",
                "position",
                "enum_name",
            ],
            &mut cli,
        );
        if opt_bool(&args, "literal_only") {
            cli.push("--literal-only".into());
        }
        if opt_bool(&args, "auto_detect") {
            cli.push("--auto-detect".into());
        }
        cli.push("--apply".into());
        let paths = opt_str(&args, "paths");
        dispatch_yah("add", cli, paths, ctx).await
    }
}

struct YahRemove;

#[async_trait]
impl Tool for YahRemove {
    fn name(&self) -> &'static str {
        "yah_remove"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Remove a Rust AST node — struct field, enum variant, match arm, doc \
                 comment, derive, or whole item. Mirrors `mcp__rs-hack__remove`. Always \
                 applies; reindexes the touched files."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string" },
                    "name": { "type": "string" },
                    "kind": { "type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"] },
                    "node_type": { "type": "string" },
                    "field_name": { "type": "string" },
                    "variant": { "type": "string" },
                    "method": { "type": "string" },
                    "derive": { "type": "string" },
                    "match_arm": { "type": "string" },
                    "function": { "type": "string" },
                    "doc_comment": { "type": "boolean", "default": false },
                    "literal_only": { "type": "boolean", "default": false }
                },
                "required": ["paths", "name"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let mut cli = Vec::new();
        push_str_flags(
            &args,
            &[
                "paths",
                "name",
                "kind",
                "node_type",
                "field_name",
                "variant",
                "method",
                "derive",
                "match_arm",
                "function",
            ],
            &mut cli,
        );
        if opt_bool(&args, "doc_comment") {
            cli.push("--doc-comment".into());
        }
        if opt_bool(&args, "literal_only") {
            cli.push("--literal-only".into());
        }
        cli.push("--apply".into());
        let paths = opt_str(&args, "paths");
        dispatch_yah("remove", cli, paths, ctx).await
    }
}

struct YahRename;

#[async_trait]
impl Tool for YahRename {
    fn name(&self) -> &'static str {
        "yah_rename"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description:
                "Rename a Rust function, trait method, or enum variant across the workspace. \
                 Mirrors `mcp__rs-hack__rename`. Surgical edit mode preserves formatting; \
                 set `validate: false` to skip cargo check. Always applies."
                    .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string" },
                    "name": { "type": "string", "description": "Current name. Use `Enum::Variant` for variants." },
                    "to": { "type": "string", "description": "New name (no path/enum prefix)." },
                    "kind": { "type": "string", "enum": ["function", "enum", "identifier"] },
                    "node_type": { "type": "string", "enum": ["function-call", "identifier", "enum-variant", "type-ref"] },
                    "enum_path": { "type": "string" },
                    "function_path": { "type": "string" },
                    "edit_mode": { "type": "string", "enum": ["surgical", "reformat"], "default": "surgical" },
                    "validate": { "type": "boolean", "default": true }
                },
                "required": ["paths", "name", "to"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let mut cli = Vec::new();
        push_str_flags(
            &args,
            &[
                "paths",
                "name",
                "to",
                "kind",
                "node_type",
                "enum_path",
                "function_path",
                "edit_mode",
            ],
            &mut cli,
        );
        if let Some(false) = args.get("validate").and_then(|v| v.as_bool()) {
            cli.push("--no-validate".into());
        }
        cli.push("--apply".into());
        let paths = opt_str(&args, "paths");
        dispatch_yah("rename", cli, paths, ctx).await
    }
}

struct YahTransform;

#[async_trait]
impl Tool for YahTransform {
    fn name(&self) -> &'static str {
        "yah_transform"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Comment, remove, or replace any matched AST nodes. Lower-level than \
                 `yah_remove` — operates by node_type + content_filter. Mirrors \
                 `mcp__rs-hack__transform`. Always applies."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string" },
                    "node_type": {
                        "type": "string",
                        "enum": ["macro-call", "method-call", "function-call", "enum-usage", "struct-literal", "match-arm", "identifier", "type-ref"]
                    },
                    "action": { "type": "string", "enum": ["comment", "remove", "replace"] },
                    "name": { "type": "string" },
                    "content_filter": { "type": "string" },
                    "with": { "type": "string", "description": "Replacement code (required when action=replace)." }
                },
                "required": ["paths", "node_type", "action"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let mut cli = Vec::new();
        push_str_flags(
            &args,
            &[
                "paths",
                "node_type",
                "action",
                "name",
                "content_filter",
                "with",
            ],
            &mut cli,
        );
        cli.push("--apply".into());
        let paths = opt_str(&args, "paths");
        dispatch_yah("transform", cli, paths, ctx).await
    }
}

struct YahUpdate;

#[async_trait]
impl Tool for YahUpdate {
    fn name(&self) -> &'static str {
        "yah_update"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description:
                "Update an existing Rust AST node — struct field type, enum variant body, \
                 match arm body, or doc comment. Mirrors `mcp__rs-hack__update`. Always applies."
                    .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "paths": { "type": "string" },
                    "name": { "type": "string" },
                    "kind": { "type": "string", "enum": ["struct", "function", "enum", "impl", "trait", "mod"] },
                    "node_type": { "type": "string" },
                    "field_name": { "type": "string" },
                    "field_type": { "type": "string" },
                    "variant": { "type": "string" },
                    "match_arm": { "type": "string" },
                    "body": { "type": "string" },
                    "function": { "type": "string" },
                    "doc_comment": { "type": "string" }
                },
                "required": ["paths", "name"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let mut cli = Vec::new();
        push_str_flags(
            &args,
            &[
                "paths",
                "name",
                "kind",
                "node_type",
                "field_name",
                "field_type",
                "variant",
                "match_arm",
                "body",
                "function",
                "doc_comment",
            ],
            &mut cli,
        );
        cli.push("--apply".into());
        let paths = opt_str(&args, "paths");
        dispatch_yah("update", cli, paths, ctx).await
    }
}

// ---------- write_arch_doc ----------

#[derive(Debug, Deserialize)]
struct WriteArchDocArgs {
    /// Rig-relative path inside `<rig>/.yah/arch/authored/`. Created if
    /// the parent directory doesn't exist yet.
    rel_path: String,
    /// Mermaid (`.mmd`) document contents.
    content: String,
}

struct WriteArchDoc;

#[async_trait]
impl Tool for WriteArchDoc {
    fn name(&self) -> &'static str {
        "write_arch_doc"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description: "Write an authored architecture diagram (`.mmd`) under \
                          `<rig>/.yah/arch/authored/`. Sister to `read_arch_doc` — same \
                          sandbox + same `.mmd` extension requirement. Parent directories \
                          are created. Reindexes the file."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "rel_path": {
                        "type": "string",
                        "description": "Rig-relative path inside .yah/arch/authored/ (must end in .mmd)."
                    },
                    "content": { "type": "string", "description": "Mermaid document body." }
                },
                "required": ["rel_path", "content"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: WriteArchDocArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        let sandbox_rel = Path::new(".yah").join("arch").join("authored");
        let sandbox = ctx.rig_root.join(&sandbox_rel);
        std::fs::create_dir_all(&sandbox)
            .map_err(|e| ToolError::Operation(format!("create {}: {e}", sandbox.display())))?;
        let candidate = ctx.rig_root.join(&args.rel_path);
        if let Some(parent) = candidate.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ToolError::Operation(format!("create {}: {e}", parent.display())))?;
        }
        // Mirror read_authored_file's check: candidate must canonicalize
        // inside the sandbox. We canonicalize the parent (which exists
        // by now) and join the file name — `candidate` itself may not
        // exist yet on first write.
        let parent = candidate
            .parent()
            .ok_or_else(|| ToolError::InvalidArgs("rel_path has no parent".into()))?;
        let parent_canon = parent
            .canonicalize()
            .map_err(|e| ToolError::Operation(format!("canonicalize {}: {e}", parent.display())))?;
        let sandbox_canon = sandbox.canonicalize().map_err(|e| {
            ToolError::Operation(format!("canonicalize {}: {e}", sandbox.display()))
        })?;
        if !parent_canon.starts_with(&sandbox_canon) {
            return Err(ToolError::SandboxEscape(args.rel_path));
        }
        let file_name = candidate
            .file_name()
            .ok_or_else(|| ToolError::InvalidArgs("rel_path has no file name".into()))?;
        if Path::new(file_name).extension().and_then(|s| s.to_str()) != Some("mmd") {
            return Err(ToolError::InvalidArgs(
                "write_arch_doc only accepts .mmd files".into(),
            ));
        }
        let final_path = parent_canon.join(file_name);
        std::fs::write(&final_path, &args.content)
            .map_err(|e| ToolError::Operation(format!("write {}: {e}", final_path.display())))?;
        // .mmd files aren't indexed by the KG today, but reindex_path
        // is filtered through `is_eligible` so the call is a cheap
        // no-op when the path is ineligible — kept for symmetry with
        // the AST writers and so future indexers pick up arch docs
        // without changing this site.
        let _ = ctx
            .svc
            .reindex_path(&final_path, IndexReason::AgentEdit)
            .await;
        Ok(json!({
            "rel_path": args.rel_path,
            "bytes": args.content.len(),
        }))
    }
}

// ---------- edit_file ----------

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    /// Rig-relative path. Sandboxed to the rig root.
    path: String,
    /// Exact substring to replace. Must occur exactly once in the file.
    old_string: String,
    /// Replacement substring.
    new_string: String,
}

struct EditFile;

#[async_trait]
impl Tool for EditFile {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn is_write(&self) -> bool {
        true
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().into(),
            description:
                "Last-resort line-based file editor for files with no AST indexer (markdown, \
                 json, plain text). Replaces an EXACT single occurrence of `old_string` \
                 with `new_string`. Fails if `old_string` is missing or matches more than \
                 once — pass more context to disambiguate. Reach for `yah_*` first when \
                 editing Rust (and `ts_*` once those land)."
                    .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Rig-relative path." },
                    "old_string": { "type": "string", "description": "Exact substring to replace (must occur once)." },
                    "new_string": { "type": "string", "description": "Replacement substring." }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        }
    }

    async fn execute(&self, args: Value, ctx: &ToolContext) -> Result<Value, ToolError> {
        let args: EditFileArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArgs(e.to_string()))?;
        if args.old_string.is_empty() {
            return Err(ToolError::InvalidArgs(
                "old_string must not be empty".into(),
            ));
        }
        let canon = resolve_in_sandbox(&ctx.rig_root, &args.path)?;
        let metadata = std::fs::metadata(&canon)
            .map_err(|e| ToolError::Operation(format!("stat {}: {e}", args.path)))?;
        if !metadata.is_file() {
            return Err(ToolError::Operation(format!("{} is not a file", args.path)));
        }
        let bytes = std::fs::read(&canon)
            .map_err(|e| ToolError::Operation(format!("read {}: {e}", args.path)))?;
        let content = String::from_utf8(bytes)
            .map_err(|_| ToolError::Operation(format!("{} is not valid UTF-8", args.path)))?;
        let occurrences = content.matches(&args.old_string).count();
        if occurrences == 0 {
            return Err(ToolError::Operation(format!(
                "old_string not found in {}",
                args.path
            )));
        }
        if occurrences > 1 {
            return Err(ToolError::Operation(format!(
                "old_string matches {occurrences} times in {} — pass more context to disambiguate",
                args.path
            )));
        }
        let updated = content.replacen(&args.old_string, &args.new_string, 1);
        std::fs::write(&canon, &updated)
            .map_err(|e| ToolError::Operation(format!("write {}: {e}", args.path)))?;
        let _ = ctx.svc.reindex_path(&canon, IndexReason::AgentEdit).await;
        Ok(json!({
            "path": args.path,
            "bytes_written": updated.len(),
        }))
    }
}

// ---------- helpers ----------

/// Map a `["contains", "imports"]` JSON array to `Vec<EdgeKind>`. The
/// LLM-facing schema uses snake_case names rather than the internally-
/// tagged `{ edge: contains }` shape so the model doesn't have to learn
/// our serde wire format.
fn parse_edge_kinds(raw: Option<&[String]>) -> Result<Option<Vec<kg::edge::EdgeKind>>, ToolError> {
    let Some(slice) = raw else {
        return Ok(None);
    };
    let mut out = Vec::with_capacity(slice.len());
    for name in slice {
        let json = json!({ "edge": name });
        let kind: kg::edge::EdgeKind = serde_json::from_value(json).map_err(|_| {
            ToolError::InvalidArgs(format!(
                "unknown edge kind '{name}' — see EdgeKind in kg::edge"
            ))
        })?;
        out.push(kind);
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kg_store::IndexerRegistry;
    use tempfile::TempDir;

    fn fake_ctx(rig_root: PathBuf) -> ToolContext {
        ToolContext {
            rig_id: RigId("rig:test12345678".into()),
            rig_root,
            svc: Arc::new(KgService::new(IndexerRegistry::new())),
        }
    }

    /// Permissive store for tests that exercise the underlying tool
    /// behaviour rather than the gate itself — every writer tool gets
    /// a `Tool { name }` rule. Composes with
    /// `KgToolRegistry::with_experimental_writers(ctx).with_store(...)`
    /// so the gate auto-allows. Gate dispatch is exercised separately
    /// in [`gate_*`] tests below.
    fn allow_all_writes_store() -> Arc<dyn crate::agent_approval::ApprovalStore> {
        use crate::agent_approval::{ApprovalRule, InMemoryApprovalStore};
        let store: Arc<dyn crate::agent_approval::ApprovalStore> =
            Arc::new(InMemoryApprovalStore::new());
        for name in [
            "yah_add",
            "yah_remove",
            "yah_rename",
            "yah_transform",
            "yah_update",
            "write_arch_doc",
            "edit_file",
            "bash",
        ] {
            store.push(ApprovalRule::Tool { name: name.into() });
        }
        store
    }

    #[test]
    fn standard_registry_exposes_eight_read_only_tools() {
        let tmp = TempDir::new().unwrap();
        let registry = KgToolRegistry::standard_read_only(fake_ctx(tmp.path().to_path_buf()));
        let names: Vec<_> = registry.schemas().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 8, "{names:?}");
        for expected in [
            "read_file",
            "list_dir",
            "grep",
            "arch_node",
            "arch_neighbors",
            "arch_subgraph",
            "arch_lookup",
            "read_arch_doc",
        ] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
    }

    #[test]
    fn standard_registry_tool_schemas_are_object_typed_with_required() {
        // Every schema we hand the LLM must be a `type: object` with a
        // `properties` object — the OpenAI function-calling spec rejects
        // anything else. Catching it here prevents a runtime "tools array
        // rejected" error against a live provider.
        let tmp = TempDir::new().unwrap();
        let registry = KgToolRegistry::standard_read_only(fake_ctx(tmp.path().to_path_buf()));
        for schema in registry.schemas() {
            assert_eq!(
                schema.input_schema["type"], "object",
                "{}: type must be object",
                schema.name
            );
            assert!(
                schema.input_schema["properties"].is_object(),
                "{}: properties must be an object",
                schema.name
            );
        }
    }

    #[tokio::test]
    async fn read_file_refuses_paths_that_escape_the_rig_root() {
        // Sandbox check: a path with `..` segments must be rejected
        // even if it would resolve to a real file outside the rig.
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "leak").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry = KgToolRegistry::standard_read_only(ctx.clone());

        // Construct `..` traversal that resolves to the sibling tmp
        let escape = format!(
            "../{}/secret.txt",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        let outcome = registry
            .execute("read_file", json!({ "path": escape }))
            .await;
        assert!(!outcome.ok, "expected sandbox rejection, got {outcome:?}");
        assert_eq!(outcome.result["kind"], "sandbox_escape");
    }

    #[tokio::test]
    async fn read_file_returns_content_with_total_byte_count() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("hello.txt");
        std::fs::write(&path, "hello world\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry = KgToolRegistry::standard_read_only(ctx);
        let outcome = registry
            .execute("read_file", json!({ "path": "hello.txt" }))
            .await;
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["content"], "hello world\n");
        assert_eq!(outcome.result["bytes"], 12);
        assert_eq!(outcome.result["truncated"], false);
    }

    #[tokio::test]
    async fn read_file_honours_line_range() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("multi.txt");
        std::fs::write(&path, "one\ntwo\nthree\nfour\nfive\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry = KgToolRegistry::standard_read_only(ctx);
        let outcome = registry
            .execute(
                "read_file",
                json!({ "path": "multi.txt", "start_line": 2, "end_line": 4 }),
            )
            .await;
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["content"], "two\nthree\nfour\n");
    }

    #[tokio::test]
    async fn list_dir_lists_immediate_children_and_skips_target() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::create_dir(tmp.path().join("target")).unwrap();
        std::fs::write(tmp.path().join("Cargo.toml"), "x").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry = KgToolRegistry::standard_read_only(ctx);
        let outcome = registry.execute("list_dir", json!({ "path": "" })).await;
        assert!(outcome.ok, "{outcome:?}");
        let entries = outcome.result["entries"].as_array().unwrap();
        let names: Vec<_> = entries
            .iter()
            .map(|e| e["name"].as_str().unwrap().to_string())
            .collect();
        assert!(names.contains(&"src".to_string()));
        assert!(names.contains(&"Cargo.toml".to_string()));
        assert!(
            !names.contains(&"target".to_string()),
            "target/ should be skipped: {names:?}"
        );
    }

    #[tokio::test]
    async fn grep_finds_pattern_with_glob_filter_and_returns_file_line() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src").join("a.rs"),
            "fn alpha() {}\nfn beta() {}\n",
        )
        .unwrap();
        std::fs::write(tmp.path().join("README.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry = KgToolRegistry::standard_read_only(ctx);
        let outcome = registry
            .execute("grep", json!({ "pattern": "alpha", "glob": "**/*.rs" }))
            .await;
        assert!(outcome.ok, "{outcome:?}");
        let hits = outcome.result["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0]["file"], "src/a.rs");
        assert_eq!(hits[0]["line"], 1);
    }

    #[tokio::test]
    async fn unknown_tool_name_returns_dispatch_error_with_available_list() {
        let tmp = TempDir::new().unwrap();
        let registry = KgToolRegistry::standard_read_only(fake_ctx(tmp.path().to_path_buf()));
        let outcome = registry.execute("does_not_exist", json!({})).await;
        assert!(!outcome.ok);
        let err = outcome.result["error"].as_str().unwrap();
        assert!(err.contains("does_not_exist"), "{err}");
        assert!(err.contains("read_file"), "{err}");
    }

    #[tokio::test]
    async fn read_arch_doc_refuses_paths_outside_authored_sandbox() {
        // Even with a real file living in the rig but outside
        // .yah/arch/authored/, the daemon must refuse — the host's
        // sandbox isn't enough; the per-tool sandbox layered on top is
        // what keeps this surface narrow.
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("README.md"), "x").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        // Boot the daemon so read_authored_file has a rig_root to check.
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry = KgToolRegistry::standard_read_only(ctx);
        let outcome = registry
            .execute("read_arch_doc", json!({ "rel_path": "README.md" }))
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        let err = outcome.result["error"].as_str().unwrap();
        assert!(
            err.contains("authored") || err.contains("sandbox") || err.contains("does not exist"),
            "{err}"
        );
    }

    // ---------- writer tests (R031-F4) ----------

    #[test]
    fn experimental_writers_registry_exposes_seven_extra_tools() {
        // Read-only stays at 8; experimental adds 7 (yah_add/remove/rename/
        // transform/update + write_arch_doc + edit_file). Locking the count
        // here catches accidental drift when P5 lands the approval gate.
        let tmp = TempDir::new().unwrap();
        let registry =
            KgToolRegistry::with_experimental_writers(fake_ctx(tmp.path().to_path_buf()));
        let names: Vec<_> = registry.schemas().into_iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 15, "{names:?}");
        for expected in [
            "yah_add",
            "yah_remove",
            "yah_rename",
            "yah_transform",
            "yah_update",
            "write_arch_doc",
            "edit_file",
        ] {
            assert!(names.iter().any(|n| n == expected), "missing {expected}");
        }
    }

    #[test]
    fn writer_tools_set_is_write_true() {
        // The P5 approval gate routes by `is_write()`. A writer slipping
        // through with the default `false` would silently bypass the gate
        // — assert the bit explicitly so a future Tool impl that forgets
        // to override `is_write` fails this test loudly.
        let tmp = TempDir::new().unwrap();
        let registry =
            KgToolRegistry::with_experimental_writers(fake_ctx(tmp.path().to_path_buf()));
        let writer_names = [
            "yah_add",
            "yah_remove",
            "yah_rename",
            "yah_transform",
            "yah_update",
            "write_arch_doc",
            "edit_file",
        ];
        for tool in &registry.tools {
            let is_writer = writer_names.contains(&tool.name());
            assert_eq!(
                tool.is_write(),
                is_writer,
                "{}: is_write should be {is_writer}",
                tool.name()
            );
        }
    }

    #[test]
    fn writer_schemas_are_object_typed_with_required() {
        // Same invariant as the read-only registry — provider rejects any
        // tools[] entry whose input_schema isn't `type: object`. Catch a
        // typo here rather than waiting for the live OpenAI/Anthropic
        // request to fail.
        let tmp = TempDir::new().unwrap();
        let registry =
            KgToolRegistry::with_experimental_writers(fake_ctx(tmp.path().to_path_buf()));
        for schema in registry.schemas() {
            assert_eq!(
                schema.input_schema["type"], "object",
                "{}: type must be object",
                schema.name
            );
            assert!(
                schema.input_schema["properties"].is_object(),
                "{}: properties must be an object",
                schema.name
            );
        }
    }

    #[tokio::test]
    async fn write_arch_doc_creates_authored_file_inside_sandbox() {
        let tmp = TempDir::new().unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "write_arch_doc",
                json!({
                    "rel_path": ".yah/arch/authored/sample.mmd",
                    "content": "graph TD\n  A --> B\n",
                }),
            )
            .await;
        assert!(outcome.ok, "{outcome:?}");
        let body =
            std::fs::read_to_string(tmp.path().join(".yah/arch/authored/sample.mmd")).unwrap();
        assert_eq!(body, "graph TD\n  A --> B\n");
        assert_eq!(outcome.result["bytes"], body.len() as u64);
    }

    #[tokio::test]
    async fn write_arch_doc_refuses_paths_outside_sandbox() {
        // The sandbox lives below the rig root, but write_arch_doc must
        // refuse anything outside `<rig>/.yah/arch/authored/` — even
        // paths that are otherwise inside the rig. Mirrors the read-side
        // check so a reader and writer agree on what's authored content.
        let tmp = TempDir::new().unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "write_arch_doc",
                json!({
                    "rel_path": "src/escape.mmd",
                    "content": "graph TD\n",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["kind"], "sandbox_escape");
    }

    #[tokio::test]
    async fn write_arch_doc_rejects_non_mmd_extension() {
        let tmp = TempDir::new().unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "write_arch_doc",
                json!({
                    "rel_path": ".yah/arch/authored/bad.txt",
                    "content": "x",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        let err = outcome.result["error"].as_str().unwrap();
        assert!(err.contains(".mmd"), "{err}");
    }

    #[tokio::test]
    async fn edit_file_replaces_unique_substring_and_writes_back() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("notes.md");
        std::fs::write(&path, "# Heading\n\nbody one\nbody two\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "notes.md",
                    "old_string": "body one",
                    "new_string": "body uno",
                }),
            )
            .await;
        assert!(outcome.ok, "{outcome:?}");
        let body = std::fs::read_to_string(&path).unwrap();
        assert_eq!(body, "# Heading\n\nbody uno\nbody two\n");
    }

    #[tokio::test]
    async fn edit_file_refuses_when_old_string_matches_multiple_times() {
        // Multi-match is the failure case the LLM has to disambiguate by
        // expanding context — the tool must NOT pick one occurrence.
        // Without this guard a "rename foo to bar" call could silently
        // miss every other `foo` in the file.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("dup.md");
        std::fs::write(&path, "foo\nfoo\nfoo\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "dup.md",
                    "old_string": "foo",
                    "new_string": "bar",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        let err = outcome.result["error"].as_str().unwrap();
        assert!(err.contains("3 times"), "{err}");
        // File untouched.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "foo\nfoo\nfoo\n");
    }

    #[tokio::test]
    async fn edit_file_refuses_when_old_string_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("plain.md");
        std::fs::write(&path, "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "missing",
                    "new_string": "x",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        let err = outcome.result["error"].as_str().unwrap();
        assert!(err.contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn edit_file_refuses_paths_that_escape_the_rig_root() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "leak").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let escape = format!(
            "../{}/secret.txt",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": escape,
                    "old_string": "leak",
                    "new_string": "patched",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["kind"], "sandbox_escape");
        // The outside file is untouched.
        assert_eq!(
            std::fs::read_to_string(outside.path().join("secret.txt")).unwrap(),
            "leak"
        );
    }

    #[tokio::test]
    async fn yah_add_surfaces_subprocess_failure_when_yah_binary_unavailable() {
        // We don't require `yah` to be on PATH in the test harness — but
        // when it isn't, the failure must surface as an in-band tool
        // error (not a panic) so the LLM can adjust. We can't reliably
        // cover the success path here without bundling the binary, so the
        // happy-path assertion is left to the verify smoke (run `yah`
        // installed on the dev machine).
        let tmp = TempDir::new().unwrap();
        // Create an empty rust file so even if yah IS on PATH the
        // operation is a benign no-op rather than mutating real source.
        std::fs::write(tmp.path().join("empty.rs"), "").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        let registry =
            KgToolRegistry::with_experimental_writers(ctx).with_store(allow_all_writes_store());
        let outcome = registry
            .execute(
                "yah_add",
                json!({
                    "paths": "empty.rs",
                    "name": "Foo",
                    "kind": "struct",
                    "field_name": "id",
                    "field_type": "u64",
                }),
            )
            .await;
        // Either yah is on PATH (outcome is ok=true with whatever the
        // CLI emitted) or it isn't (outcome is ok=false with a clear
        // error). Both are valid; what we're asserting is that we don't
        // panic and the envelope is well-formed.
        if !outcome.ok {
            let err = outcome.result["error"].as_str().unwrap();
            assert!(err.contains("yah"), "{err}");
        } else {
            assert!(outcome.result.get("exit_code").is_some());
            assert!(outcome.result.get("stdout").is_some());
        }
    }

    // ---------- gate dispatch (R031-F5 phase A) ----------

    #[tokio::test]
    async fn gate_auto_allows_read_only_calls_with_no_rules() {
        // The empty store on standard_read_only must not interfere with
        // read-only dispatch — `is_write()=false` short-circuits to
        // `Auto`. Regression for the case where a future change makes
        // the gate consult the store unconditionally.
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("hi.txt"), "ok").unwrap();
        let registry = KgToolRegistry::standard_read_only(fake_ctx(tmp.path().to_path_buf()));
        let outcome = registry
            .execute("read_file", json!({ "path": "hi.txt" }))
            .await;
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["content"], "ok");
    }

    #[tokio::test]
    async fn gate_blocks_write_call_with_no_matching_rule() {
        // Writers in the experimental registry without a permissive
        // store: every dispatch returns the structured
        // `approval_required` envelope so the LLM gets a clear signal
        // (and the file stays untouched).
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plain.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry = KgToolRegistry::with_experimental_writers(ctx);
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["kind"], "approval_required");
        assert_eq!(outcome.result["tool"], "edit_file");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("plain.md")).unwrap(),
            "alpha\n",
            "file must stay untouched when gate blocks the call",
        );
    }

    #[tokio::test]
    async fn gate_allows_write_when_tool_rule_seeded() {
        use crate::agent_approval::{ApprovalRule, InMemoryApprovalStore};
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plain.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");

        let store: Arc<dyn crate::agent_approval::ApprovalStore> =
            Arc::new(InMemoryApprovalStore::new());
        store.push(ApprovalRule::Tool {
            name: "edit_file".into(),
        });
        let registry = KgToolRegistry::with_experimental_writers(ctx).with_store(store);
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            )
            .await;
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("plain.md")).unwrap(),
            "beta\n",
        );
    }

    /* Bash-tool-specific gate behaviour (parser refusal of
    metacharacters, structured rule matching on the parsed
    BashCall) is covered in `agent_approval::tests`. Once R031-F6
    lands a concrete `bash` Tool in the writers registry, add a
    registry-level integration test here exercising the
    structured-approval ↔ tool-execute round trip end-to-end. */

    // ---------- gate dispatch (R031-F5 phase B: inline router) ----------

    #[tokio::test]
    async fn gate_router_apply_runs_the_call() {
        // With a router wired and a session id, an `Apply` choice
        // gates the tool through to execute. Mirror the file-edit
        // flow so we can verify the tool actually ran (vs the gate
        // short-circuiting to ok=true without doing anything).
        use crate::agent_approval::{ApprovalChoice, StaticApprovalRouter};
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plain.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let router: Arc<dyn ApprovalRouter> =
            Arc::new(StaticApprovalRouter::new(ApprovalChoice::Apply));
        let registry = KgToolRegistry::with_experimental_writers(ctx)
            .with_router(router)
            .with_session(SessionId::new("session:test01234567"));
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            )
            .await;
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("plain.md")).unwrap(),
            "beta\n",
        );
    }

    #[tokio::test]
    async fn gate_router_skip_returns_approval_skipped() {
        // `Skip` is the user explicitly declining this call. The
        // surface to the LLM must distinguish that from
        // approval_required (no router) so the model knows the user
        // *saw* the request and chose not to run it.
        use crate::agent_approval::{ApprovalChoice, StaticApprovalRouter};
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plain.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let router: Arc<dyn ApprovalRouter> =
            Arc::new(StaticApprovalRouter::new(ApprovalChoice::Skip));
        let registry = KgToolRegistry::with_experimental_writers(ctx)
            .with_router(router)
            .with_session(SessionId::new("session:test01234567"));
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["kind"], "approval_skipped");
        // File untouched.
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("plain.md")).unwrap(),
            "alpha\n",
        );
    }

    #[tokio::test]
    async fn gate_router_always_allow_persists_rule_and_skips_next_prompt() {
        // The point of AlwaysAllow is the *second* call doesn't
        // prompt. Use a counting router that returns AlwaysAllow
        // exactly once then panics — if the second call hits the
        // router, the test fails. Real proof the rule was persisted.
        use crate::agent_approval::{ApprovalChoice, ApprovalRequest, ApprovalRule};
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct OnceRouter {
            count: AtomicUsize,
        }

        #[async_trait]
        impl ApprovalRouter for OnceRouter {
            async fn request(&self, _request: ApprovalRequest) -> ApprovalChoice {
                let n = self.count.fetch_add(1, Ordering::SeqCst);
                if n > 0 {
                    panic!("router called twice — AlwaysAllow rule wasn't persisted");
                }
                ApprovalChoice::AlwaysAllow {
                    rule: ApprovalRule::Tool {
                        name: "edit_file".into(),
                    },
                }
            }
        }

        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.md"), "x\n").unwrap();
        std::fs::write(tmp.path().join("b.md"), "y\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let router: Arc<dyn ApprovalRouter> = Arc::new(OnceRouter {
            count: AtomicUsize::new(0),
        });
        let registry = KgToolRegistry::with_experimental_writers(ctx)
            .with_router(router)
            .with_session(SessionId::new("session:test01234567"));
        // First call — router returns AlwaysAllow.
        let r1 = registry
            .execute(
                "edit_file",
                json!({
                    "path": "a.md",
                    "old_string": "x",
                    "new_string": "X",
                }),
            )
            .await;
        assert!(r1.ok, "{r1:?}");
        // Second call — gate must find the persisted rule and bypass
        // the router (router would panic on second call).
        let r2 = registry
            .execute(
                "edit_file",
                json!({
                    "path": "b.md",
                    "old_string": "y",
                    "new_string": "Y",
                }),
            )
            .await;
        assert!(r2.ok, "{r2:?}");
    }

    #[tokio::test]
    async fn gate_falls_through_to_approval_required_without_router() {
        // Belt-and-suspenders: the existing
        // gate_blocks_write_call_with_no_matching_rule already covers
        // the no-router path, but make the contract explicit — even
        // with a session_id set, no router means we can't ask, so we
        // surface approval_required (not approval_skipped).
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("plain.md"), "alpha\n").unwrap();
        let ctx = fake_ctx(tmp.path().to_path_buf());
        ctx.svc.boot(tmp.path().to_path_buf()).await.expect("boot");
        let registry = KgToolRegistry::with_experimental_writers(ctx)
            .with_session(SessionId::new("session:test01234567"));
        let outcome = registry
            .execute(
                "edit_file",
                json!({
                    "path": "plain.md",
                    "old_string": "alpha",
                    "new_string": "beta",
                }),
            )
            .await;
        assert!(!outcome.ok, "{outcome:?}");
        assert_eq!(outcome.result["kind"], "approval_required");
    }
}
