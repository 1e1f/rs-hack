//! @arch:layer(kg_store)
//! @arch:role(bridge)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! Structured approval gate for agent tool calls (R031-F5).
//!
//! The gate sits between the runner-shaped
//! [`runner::ToolRegistry::execute`] entry point and concrete
//! [`crate::agent_tools::Tool::execute`] dispatch, inside
//! [`crate::agent_tools::KgToolRegistry`]. Two callers feed it:
//!
//! - The OpenAI-compat path goes through
//!   [`runner::ToolRegistry::execute`], which forwards to
//!   [`crate::agent_tools::KgToolRegistry::execute_gated`].
//! - The Anthropic native `tool_use` loop in
//!   [`crate::agent::run_anthropic_turn`] (when it lands) calls
//!   `execute_gated` directly. Same gate, no protocol coupling.
//!
//! Three properties this module owns and wants tested in isolation:
//!
//! 1. **Bash arg parsing** — the bash tool's raw input
//!    `"ENV=val cmd arg1 arg2"` parses into a [`BashCall`] struct
//!    *before* the gate runs. Approval rules match the struct, and the
//!    invoked CLI is re-synthesized from the *approved* fields. That
//!    closes the regex-bypass class of attack — an attacker can't sneak
//!    extra env or trailing args past a glob match.
//! 2. **Versioned rule schema** — [`ApprovalRulesetV1`] is a
//!    `#[serde(tag = "version")]` envelope so future shape changes
//!    don't silently ignore old rules; an old client reading a newer
//!    file gets a clear deserialization error.
//! 3. **Gate decision is a value, not an action** — [`ApprovalGate::decide`]
//!    returns [`ApprovalDecision`] (`Auto` / `Allow` / `NeedsPrompt` /
//!    `Deny`). The interactive-prompt path (R031-F5 phase B) plumbs an
//!    async resolver that turns `NeedsPrompt` into the user's
//!    Apply / Skip / Always-allow click; until then the registry treats
//!    `NeedsPrompt` as deny-with-reason so the LLM gets a structured
//!    "this needs approval" message back.

use async_trait::async_trait;
use kg::agent::SessionId;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

/// Parsed bash invocation. Shell metacharacter expansion is *not* this
/// parser's job — that lives in the spawned shell. We only split env
/// pairs off the front, then take the first non-pair token as `cmd`
/// and the rest as `args`.
///
/// The parser is intentionally small. Quoted strings keep their interior
/// (so the LLM can pass `cmd "two words"` and the gate sees one arg);
/// backslash escapes a single character. Anything fancier — heredocs,
/// pipes, command substitution — would require the bash AST and is the
/// shell's job, not ours. Inputs containing those metacharacters are
/// rejected so an approved rule can't paper over them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BashCall {
    /// Environment variables prefixed to the command (e.g.
    /// `RUST_LOG=debug PATH=/foo cargo …` → `{RUST_LOG: debug, PATH: /foo}`).
    /// Stored in a `BTreeMap` so equality is order-insensitive — the
    /// agent isn't required to repeat the env in the same order across
    /// turns.
    pub env: BTreeMap<String, String>,
    /// First non-`KEY=VAL` token. The bash gate matches on this.
    pub cmd: String,
    /// Remaining tokens, in order, post-quote-stripping.
    pub args: Vec<String>,
}

/// Why a bash input couldn't be parsed into a [`BashCall`]. The gate
/// surfaces these to the agent verbatim so it can fix and retry —
/// "needs structured form" is friendlier than a silent reject.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BashParseError {
    /// Empty / whitespace-only / env-only input — no command word.
    #[error("no command found in bash input")]
    NoCommand,
    /// Quote opened but never closed before end of input.
    #[error("unterminated quote in bash input")]
    UnterminatedQuote,
    /// Trailing backslash with nothing to escape.
    #[error("trailing backslash in bash input")]
    TrailingBackslash,
    /// Bash metacharacter the gate doesn't reason about. `;` `&` `|` `>`
    /// `<` `` ` `` `$()` etc — these would need the shell's AST to
    /// decompose safely, so the gate refuses them. The agent should
    /// invoke separate tool calls for compound commands.
    #[error("bash input contains shell metacharacter '{0}' — split into separate tool calls")]
    ShellMetachar(char),
}

/// Parse a raw bash invocation into a [`BashCall`].
///
/// Algorithm: scan tokens left-to-right; while the next token matches
/// `KEY=VAL` and we haven't yet seen a command word, fold it into
/// `env`; otherwise the first non-pair token becomes `cmd` and the
/// rest become `args` in order. Quoting strips the quote characters
/// but keeps the interior (so `cmd "a b"` parses to `["a b"]`).
pub fn parse_bash(input: &str) -> Result<BashCall, BashParseError> {
    let tokens = tokenize_bash(input)?;
    let mut env = BTreeMap::new();
    let mut cmd: Option<String> = None;
    let mut args: Vec<String> = Vec::new();

    for tok in tokens {
        if cmd.is_none() {
            if let Some((k, v)) = split_env_pair(&tok) {
                env.insert(k, v);
                continue;
            }
            cmd = Some(tok);
        } else {
            args.push(tok);
        }
    }

    let cmd = cmd.ok_or(BashParseError::NoCommand)?;
    Ok(BashCall { env, cmd, args })
}

/// Re-synthesize an approved [`BashCall`] back into a CLI string. The
/// caller is `bash -c <this>` — quoting must be tight enough that an
/// arg like `a b` round-trips through the shell as a single argument.
///
/// Strategy: every arg + env value gets single-quoted and any embedded
/// `'` becomes `'\''`. POSIX shells handle that uniformly; bash does
/// not interpret anything inside single quotes except the closing
/// `'`. The resulting string is what gets executed — the agent's
/// original raw input is *not* trusted.
pub fn synthesize_bash(call: &BashCall) -> String {
    fn quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
    let mut out = String::new();
    for (k, v) in &call.env {
        out.push_str(k);
        out.push('=');
        out.push_str(&quote(v));
        out.push(' ');
    }
    out.push_str(&call.cmd);
    for a in &call.args {
        out.push(' ');
        out.push_str(&quote(a));
    }
    out
}

fn split_env_pair(tok: &str) -> Option<(String, String)> {
    let eq = tok.find('=')?;
    let key = &tok[..eq];
    if key.is_empty() {
        return None;
    }
    let valid_key = key.chars().enumerate().all(|(i, c)| {
        (i == 0 && (c.is_ascii_alphabetic() || c == '_'))
            || (i > 0 && (c.is_ascii_alphanumeric() || c == '_'))
    });
    if !valid_key {
        return None;
    }
    Some((key.to_string(), tok[eq + 1..].to_string()))
}

fn tokenize_bash(input: &str) -> Result<Vec<String>, BashParseError> {
    /* Two states: outside a quoted run (whitespace splits tokens) and
    inside one (quote terminator splits). Backslash escapes one char
    in either state. We refuse on bare metacharacters because their
    semantics need the shell's grammar. */
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' => {
                if in_token {
                    out.push(std::mem::take(&mut cur));
                    in_token = false;
                }
            }
            '\\' => {
                let next = chars.next().ok_or(BashParseError::TrailingBackslash)?;
                cur.push(next);
                in_token = true;
            }
            '"' | '\'' => {
                let quote_char = c;
                in_token = true;
                loop {
                    let inner = chars.next().ok_or(BashParseError::UnterminatedQuote)?;
                    if inner == quote_char {
                        break;
                    }
                    if inner == '\\' && quote_char == '"' {
                        let escaped = chars.next().ok_or(BashParseError::TrailingBackslash)?;
                        cur.push(escaped);
                    } else {
                        cur.push(inner);
                    }
                }
            }
            ';' | '&' | '|' | '>' | '<' | '`' | '(' | ')' | '{' | '}' => {
                return Err(BashParseError::ShellMetachar(c));
            }
            '$' => {
                if matches!(chars.peek(), Some('(') | Some('{')) {
                    return Err(BashParseError::ShellMetachar('$'));
                }
                cur.push(c);
                in_token = true;
            }
            other => {
                cur.push(other);
                in_token = true;
            }
        }
    }
    if in_token {
        out.push(cur);
    }
    Ok(out)
}

// ---------- Rule schema ----------

/// One approval rule. Matches the *parsed* tool call — never a rendered
/// string. Bash calls are matched on their parsed [`BashCall`] struct;
/// other tools match by name + optional argument-shape constraint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalRule {
    /// "Always allow this tool, regardless of args." Use sparingly —
    /// good fit for inherently scoped tools (`yah_rename` is OK,
    /// `bash` is not).
    Tool { name: String },
    /// "Allow `name` when its `path`-like argument matches this glob."
    /// The matched arg field is currently hard-coded to `path` —
    /// mirrors `read_file` / `list_dir` / `edit_file`'s schema. A
    /// future revision may parameterize the field name.
    ToolPath { name: String, glob: String },
    /// "Always allow this bash command, regardless of args." `cmd`
    /// matches the parsed [`BashCall::cmd`]; env and args are not
    /// constrained.
    BashCmd { cmd: String },
    /// "Allow `cmd` when args match this exact pattern sequence." Each
    /// [`ArgPattern`] either matches an exact string or any token; the
    /// pattern length must equal the call's args length unless an
    /// `Any` pattern stands in for a tail.
    BashCmdPattern { cmd: String, args: Vec<ArgPattern> },
}

/// Per-arg matcher inside [`ApprovalRule::BashCmdPattern`]. `Exact`
/// constrains a single position; `Any` is a wildcard for one token.
/// Variadic tails are intentionally *not* supported — explicit-length
/// patterns force the user to think about what they're authorizing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArgPattern {
    /// Match exactly this string.
    Exact { value: String },
    /// Match any single token at this position.
    Any,
}

/// Versioned envelope for the on-disk / kv rule list. The `version`
/// tag is required so a future shape (e.g. `V2` adding an `expires_at`
/// per rule) can deserialize cleanly while old clients reject unknown
/// versions instead of silently dropping rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "version")]
pub enum ApprovalRuleset {
    #[serde(rename = "1")]
    V1(ApprovalRulesetV1),
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRulesetV1 {
    pub rules: Vec<ApprovalRule>,
}

impl ApprovalRuleset {
    pub fn empty() -> Self {
        Self::V1(ApprovalRulesetV1::default())
    }

    pub fn rules(&self) -> &[ApprovalRule] {
        match self {
            Self::V1(v) => &v.rules,
        }
    }

    pub fn push(&mut self, rule: ApprovalRule) {
        match self {
            Self::V1(v) => v.rules.push(rule),
        }
    }

    pub fn len(&self) -> usize {
        self.rules().len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules().is_empty()
    }
}

// ---------- Decision + gate ----------

/// What the gate has decided about a single tool call. The runner
/// interprets each variant differently:
///
/// - `Auto` — read-only tool, run with no rule lookup.
/// - `Allow { rule_id }` — write tool matched by an existing rule. Run.
/// - `NeedsPrompt { reason }` — write tool with no matching rule; the
///   interactive UI takes over (phase B). Until that lands, the
///   registry maps this to a `Deny`-shaped outcome so the LLM gets a
///   structured "needs approval" message rather than hanging.
/// - `Deny { reason }` — gate refused. The LLM sees this as a tool
///   error and adapts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    Auto,
    Allow { rule_index: usize },
    NeedsPrompt { reason: String },
    Deny { reason: String },
}

/// Storage backend for [`ApprovalRuleset`]. The trait is small on
/// purpose so the in-memory implementation is trivial for tests; the
/// kv-backed implementation (R031-F5 phase B) plugs in without
/// touching the gate. Implementations must be cheap to call repeatedly
/// — the gate consults the store per tool call.
pub trait ApprovalStore: Send + Sync {
    fn snapshot(&self) -> ApprovalRuleset;
    fn replace(&self, ruleset: ApprovalRuleset);
    fn push(&self, rule: ApprovalRule) {
        let mut snap = self.snapshot();
        // De-dupe identical rules so the AlwaysAllow path is idempotent —
        // clicking the approval button twice on the same call shouldn't
        // bloat the rules list.
        if !snap.rules().iter().any(|r| r == &rule) {
            snap.push(rule);
            self.replace(snap);
        }
    }
    /// Drop the rule at `index` (no-op if out of range). Index-based
    /// rather than id-based because rules don't carry stable ids; the
    /// Settings UI passes back the position of the row the user
    /// clicked Delete on.
    fn remove_at(&self, index: usize) {
        let snap = self.snapshot();
        match snap {
            ApprovalRuleset::V1(mut v) => {
                if index < v.rules.len() {
                    v.rules.remove(index);
                    self.replace(ApprovalRuleset::V1(v));
                }
            }
        }
    }
}

/// Tests-only store. Keeps the ruleset behind a `RwLock`; cheap to
/// read, no I/O. Not used in production — phase B's KV-backed store
/// supersedes it.
pub struct InMemoryApprovalStore {
    inner: RwLock<ApprovalRuleset>,
}

impl InMemoryApprovalStore {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ApprovalRuleset::empty()),
        }
    }
}

impl Default for InMemoryApprovalStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalStore for InMemoryApprovalStore {
    fn snapshot(&self) -> ApprovalRuleset {
        self.inner.read().unwrap().clone()
    }

    fn replace(&self, ruleset: ApprovalRuleset) {
        *self.inner.write().unwrap() = ruleset;
    }
}

/// Filename for per-rig persisted approval rules. Kept under `.yah/`
/// so it's checked in alongside everything else yah owns. Rules
/// aren't secrets — keychain would be the wrong home for this — so
/// plain JSON works fine and a curious user can hand-edit it.
pub const APPROVAL_RULES_FILENAME: &str = "agent-approval-rules.json";

/// Per-rig file-backed [`ApprovalStore`]. Reads cache the snapshot in
/// memory; writes serialize the whole ruleset back to disk under a
/// write lock. Atomicity is best-effort (write-then-rename) — rule
/// edits are infrequent enough that crash-safety isn't a budget here.
///
/// Construct via [`Self::load_or_empty`]: a missing file produces an
/// empty in-memory ruleset (the registry treats empty as "every write
/// needs prompting"). A malformed file is logged and treated the same;
/// landing in a "deny everything" state is the correct failure mode
/// when rule semantics can't be trusted.
pub struct FileApprovalStore {
    path: PathBuf,
    inner: RwLock<ApprovalRuleset>,
}

impl FileApprovalStore {
    /// Load the rules file at `<rig_root>/.yah/<APPROVAL_RULES_FILENAME>`.
    /// On any read or parse error, the in-memory ruleset starts empty —
    /// the gate then default-denies write tools, which is the safe
    /// fallback for a corrupt rules file.
    pub fn load_or_empty(rig_root: &Path) -> Self {
        let path = rig_root.join(".yah").join(APPROVAL_RULES_FILENAME);
        let initial = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<ApprovalRuleset>(&bytes) {
                Ok(rs) => rs,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "approval rules file malformed; starting empty (every write tool will need prompting)",
                    );
                    ApprovalRuleset::empty()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => ApprovalRuleset::empty(),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "approval rules file read failed; starting empty",
                );
                ApprovalRuleset::empty()
            }
        };
        Self {
            path,
            inner: RwLock::new(initial),
        }
    }

    fn persist_locked(&self, ruleset: &ApprovalRuleset) {
        let bytes = match serde_json::to_vec_pretty(ruleset) {
            Ok(b) => b,
            Err(e) => {
                tracing::error!(error = %e, "approval rules serialize failed; not persisting");
                return;
            }
        };
        if let Some(parent) = self.path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(
                    path = %parent.display(),
                    error = %e,
                    "approval rules: create_dir_all failed",
                );
                return;
            }
        }
        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = std::fs::write(&tmp, &bytes) {
            tracing::warn!(path = %tmp.display(), error = %e, "approval rules write failed");
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            tracing::warn!(
                from = %tmp.display(),
                to = %self.path.display(),
                error = %e,
                "approval rules atomic rename failed",
            );
        }
    }
}

impl ApprovalStore for FileApprovalStore {
    fn snapshot(&self) -> ApprovalRuleset {
        self.inner.read().unwrap().clone()
    }

    fn replace(&self, ruleset: ApprovalRuleset) {
        {
            let mut guard = self.inner.write().unwrap();
            *guard = ruleset.clone();
        }
        // Persist outside the in-memory lock so a slow filesystem
        // doesn't block readers; a concurrent replace might race the
        // file but the in-memory view is the source of truth and
        // every replace re-persists, so the file converges to the
        // last write.
        self.persist_locked(&ruleset);
    }
}

/// The gate. Owns a reference to the store and renders a single
/// [`ApprovalDecision`] per call.
///
/// Construction is cheap — the gate holds an `Arc<dyn ApprovalStore>`
/// and consults it lazily. One gate per [`crate::agent_tools::KgToolRegistry`].
pub struct ApprovalGate {
    store: std::sync::Arc<dyn ApprovalStore>,
}

impl ApprovalGate {
    pub fn new(store: std::sync::Arc<dyn ApprovalStore>) -> Self {
        Self { store }
    }

    /// Decide whether `call` is allowed. `is_write` should mirror the
    /// concrete `Tool::is_write()` of the dispatched tool — the gate
    /// trusts the registry to pass the same value the runner sees.
    ///
    /// For bash specifically the caller pre-parses the input with
    /// [`parse_bash`]; the parsed call is what the gate matches.
    /// Bash inputs that fail to parse are denied with the parse
    /// error verbatim (the agent gets feedback to re-shape its call).
    pub fn decide(&self, call: &PendingCall) -> ApprovalDecision {
        if !call.is_write {
            return ApprovalDecision::Auto;
        }
        let ruleset = self.store.snapshot();
        for (idx, rule) in ruleset.rules().iter().enumerate() {
            if rule_matches(rule, call) {
                return ApprovalDecision::Allow { rule_index: idx };
            }
        }
        ApprovalDecision::NeedsPrompt {
            reason: format!("no approval rule matches write tool '{}'", call.tool_name),
        }
    }
}

/// A tool call ready for the gate. The caller (the registry) knows
/// `tool_name` + `args` + whether the tool is `is_write`, plus — for
/// the bash tool — the pre-parsed [`BashCall`]. Pre-parsing belongs to
/// the caller so the gate doesn't have to reason about which tool
/// wraps a shell.
#[derive(Debug, Clone)]
pub struct PendingCall<'a> {
    pub tool_name: &'a str,
    pub args: &'a Value,
    pub is_write: bool,
    pub bash: Option<&'a BashCall>,
}

fn rule_matches(rule: &ApprovalRule, call: &PendingCall) -> bool {
    match rule {
        ApprovalRule::Tool { name } => name == call.tool_name,
        ApprovalRule::ToolPath { name, glob } => {
            if name != call.tool_name {
                return false;
            }
            let Some(path) = call.args.get("path").and_then(|v| v.as_str()) else {
                return false;
            };
            glob_match(glob, path)
        }
        ApprovalRule::BashCmd { cmd } => {
            call.tool_name == "bash" && call.bash.map(|b| &b.cmd == cmd).unwrap_or(false)
        }
        ApprovalRule::BashCmdPattern { cmd, args } => {
            if call.tool_name != "bash" {
                return false;
            }
            let Some(b) = call.bash else { return false };
            if &b.cmd != cmd {
                return false;
            }
            if b.args.len() != args.len() {
                return false;
            }
            b.args
                .iter()
                .zip(args.iter())
                .all(|(actual, pat)| match pat {
                    ArgPattern::Exact { value } => value == actual,
                    ArgPattern::Any => true,
                })
        }
    }
}

/// Minimal glob matcher — `*` matches any run of non-`/` characters,
/// `**` matches across `/` boundaries, everything else is a literal.
/// Sufficient for the rule schema (paths under `crates/**`,
/// `**/*.rs`, etc); a future revision may delegate to the `glob` crate
/// for full fnmatch semantics.
fn glob_match(pattern: &str, candidate: &str) -> bool {
    fn rec(pat: &[u8], s: &[u8]) -> bool {
        if pat.is_empty() {
            return s.is_empty();
        }
        if pat.starts_with(b"**") {
            let rest = &pat[2..];
            // `**` followed by `/` consumes optional directory segments
            // (including zero).
            let after_sep = if rest.starts_with(b"/") {
                &rest[1..]
            } else {
                rest
            };
            for split in 0..=s.len() {
                if rec(after_sep, &s[split..]) {
                    return true;
                }
            }
            return false;
        }
        if pat[0] == b'*' {
            let rest = &pat[1..];
            let mut i = 0;
            loop {
                if rec(rest, &s[i..]) {
                    return true;
                }
                if i >= s.len() || s[i] == b'/' {
                    return false;
                }
                i += 1;
            }
        }
        if !s.is_empty() && pat[0] == s[0] {
            return rec(&pat[1..], &s[1..]);
        }
        false
    }
    rec(pattern.as_bytes(), candidate.as_bytes())
}

// ---------- Async router (phase B: inline prompt) ----------

/// Snapshot of one tool call awaiting user approval, packaged for the
/// renderer. Rides on [`kg::agent::AgentEvent::ApprovalRequested`]
/// so `useChatSession` can render an inline approval row in the chat
/// pane.
///
/// `bash` is set when the call is the bash tool — the renderer shows
/// the structured form (env / cmd / args) and uses it to suggest a
/// [`ApprovalRule::BashCmdPattern`] for the "Always allow this
/// pattern" button. For other tools `bash` is `None` and the
/// suggestion is a [`ApprovalRule::Tool`] / [`ApprovalRule::ToolPath`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub session_id: SessionId,
    pub request_id: String,
    pub tool_name: String,
    pub args: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bash: Option<BashCall>,
}

/// User's reply to an [`ApprovalRequest`]. The renderer constructs
/// this when the user clicks an inline approval button:
/// - **Apply** → run this call once, no rule persisted.
/// - **Skip** → refuse this call. Surfaces as a `ToolOutcome::fail` so
///   the LLM can adjust.
/// - **AlwaysAllow** → run this call AND persist the carried rule.
///   The renderer fills `rule` from the call shape; the user can edit
///   before submitting (e.g. swap an `Exact` arg for an `Any`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ApprovalChoice {
    Apply,
    Skip,
    AlwaysAllow {
        /// Rule to persist on top of allowing this call. Renderer
        /// constructs from the request's parsed form; pre-filled but
        /// editable.
        rule: ApprovalRule,
    },
}

/// Surface the gate uses to ferry approval asks to the user. One
/// implementation in production ([`TauriApprovalRouter`] in
/// [`crate::agent`]); tests use [`StaticApprovalRouter`] to feed
/// pre-canned decisions.
#[async_trait]
pub trait ApprovalRouter: Send + Sync + 'static {
    /// Surface `request` to the user and await their reply. Must
    /// resolve eventually — the gate awaits this, blocking the
    /// underlying tool call. A router that never resolves stalls the
    /// tool call indefinitely (which is the user's prerogative —
    /// `agent_stop` cancels the parent task).
    async fn request(&self, request: ApprovalRequest) -> ApprovalChoice;
}

/// Test/no-op router — returns the same choice for every request.
/// Use alongside [`InMemoryApprovalStore`] in unit tests that
/// exercise [`crate::agent_tools::KgToolRegistry::execute_gated`]
/// past the prompt.
pub struct StaticApprovalRouter {
    choice: ApprovalChoice,
}

impl StaticApprovalRouter {
    pub fn new(choice: ApprovalChoice) -> Self {
        Self { choice }
    }
}

#[async_trait]
impl ApprovalRouter for StaticApprovalRouter {
    async fn request(&self, _request: ApprovalRequest) -> ApprovalChoice {
        self.choice.clone()
    }
}

/// Pending-approval map — shared between the router (which inserts
/// an entry and awaits the oneshot) and the Tauri
/// `agent_approval_decide` command (which looks up the entry by
/// request id and resolves the oneshot).
///
/// Cheap to clone via [`Arc`]. One per [`crate::agent::AgentSessions`]
/// so every rig's approval prompts route through the same map; the
/// request id is opaque + UUID-shaped, so collisions across rigs
/// would be a freak occurrence.
#[derive(Default, Clone)]
pub struct PendingApprovals {
    inner: Arc<AsyncMutex<HashMap<String, oneshot::Sender<ApprovalChoice>>>>,
}

impl PendingApprovals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new pending entry; returns the receiver the gate
    /// awaits.
    pub async fn register(&self, request_id: String) -> oneshot::Receiver<ApprovalChoice> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.insert(request_id, tx);
        rx
    }

    /// Resolve a pending entry. Returns `false` if no entry by that
    /// id exists (e.g. the session aborted before the user replied,
    /// or the renderer double-sent a decision).
    pub async fn resolve(&self, request_id: &str, choice: ApprovalChoice) -> bool {
        let Some(tx) = self.inner.lock().await.remove(request_id) else {
            return false;
        };
        tx.send(choice).is_ok()
    }
}

/// Mint a request id. Opaque to the renderer — it just round-trips
/// it back in `agent_approval_decide`.
pub fn mint_request_id() -> String {
    /* blake3-of-time would also work; rand keeps the dependency
    footprint small since we already pull blake3 elsewhere but
    not a global RNG-safe random source for short-lived ids. */
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("approve:{nanos:016x}{n:04x}")
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    #[test]
    fn parse_bash_splits_env_cmd_and_args() {
        let r = parse_bash("RUST_LOG=debug PATH=/foo cargo build -p yah-tauri").unwrap();
        assert_eq!(r.cmd, "cargo");
        assert_eq!(r.args, vec!["build", "-p", "yah-tauri"]);
        assert_eq!(r.env.get("RUST_LOG").map(String::as_str), Some("debug"));
        assert_eq!(r.env.get("PATH").map(String::as_str), Some("/foo"));
    }

    #[test]
    fn parse_bash_treats_post_cmd_eq_as_arg() {
        // `--target=x86_64` is an arg, not an env pair, because cmd is set.
        let r = parse_bash("cargo build --target=x86_64-unknown-linux-gnu").unwrap();
        assert_eq!(r.cmd, "cargo");
        assert_eq!(r.args, vec!["build", "--target=x86_64-unknown-linux-gnu"]);
        assert!(r.env.is_empty());
    }

    #[test]
    fn parse_bash_handles_double_quoted_args() {
        let r = parse_bash(r#"echo "hello world" plain"#).unwrap();
        assert_eq!(r.cmd, "echo");
        assert_eq!(r.args, vec!["hello world", "plain"]);
    }

    #[test]
    fn parse_bash_handles_single_quoted_args_with_no_escape() {
        // Inside single quotes nothing is interpreted — backslash is
        // literal, matching POSIX shells.
        let r = parse_bash(r#"echo 'a\b'"#).unwrap();
        assert_eq!(r.args, vec![r"a\b"]);
    }

    #[test]
    fn parse_bash_rejects_shell_metacharacters() {
        assert_eq!(
            parse_bash("echo a; echo b").unwrap_err(),
            BashParseError::ShellMetachar(';')
        );
        assert_eq!(
            parse_bash("foo | bar").unwrap_err(),
            BashParseError::ShellMetachar('|')
        );
        assert_eq!(
            parse_bash("foo > out.txt").unwrap_err(),
            BashParseError::ShellMetachar('>')
        );
        assert_eq!(
            parse_bash("foo $(bar)").unwrap_err(),
            BashParseError::ShellMetachar('$')
        );
    }

    #[test]
    fn parse_bash_rejects_unterminated_quote() {
        assert_eq!(
            parse_bash(r#"echo "oops"#).unwrap_err(),
            BashParseError::UnterminatedQuote
        );
    }

    #[test]
    fn parse_bash_rejects_empty() {
        assert_eq!(parse_bash("").unwrap_err(), BashParseError::NoCommand);
        assert_eq!(parse_bash("   ").unwrap_err(), BashParseError::NoCommand);
        assert_eq!(parse_bash("X=1").unwrap_err(), BashParseError::NoCommand);
    }

    #[test]
    fn synthesize_bash_round_trips_through_quoting() {
        let call = BashCall {
            env: BTreeMap::from([("RUST_LOG".into(), "debug".into())]),
            cmd: "echo".into(),
            args: vec!["hello world".into(), "it's me".into()],
        };
        let s = synthesize_bash(&call);
        // Single-quoted env value, single-quoted args, embedded ' is escaped.
        assert!(s.contains("RUST_LOG='debug'"));
        assert!(s.contains("'hello world'"));
        assert!(s.contains(r"'it'\''s me'"));
    }

    #[test]
    fn ruleset_round_trips_through_serde() {
        let mut rs = ApprovalRuleset::empty();
        rs.push(ApprovalRule::Tool {
            name: "yah_rename".into(),
        });
        rs.push(ApprovalRule::BashCmdPattern {
            cmd: "cargo".into(),
            args: vec![
                ArgPattern::Exact {
                    value: "build".into(),
                },
                ArgPattern::Any,
            ],
        });
        let json = serde_json::to_string(&rs).unwrap();
        // Version tag is on the wire so phase-B clients can dispatch.
        assert!(json.contains(r#""version":"1""#));
        let back: ApprovalRuleset = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rs);
    }

    #[test]
    fn ruleset_rejects_unknown_version() {
        let json = r#"{"version":"99","rules":[]}"#;
        assert!(serde_json::from_str::<ApprovalRuleset>(json).is_err());
    }

    fn gate_with(rules: Vec<ApprovalRule>) -> ApprovalGate {
        let store = Arc::new(InMemoryApprovalStore::new());
        store.replace(ApprovalRuleset::V1(ApprovalRulesetV1 { rules }));
        ApprovalGate::new(store)
    }

    #[test]
    fn read_only_calls_auto_pass() {
        let gate = gate_with(vec![]);
        let args = json!({});
        let call = PendingCall {
            tool_name: "read_file",
            args: &args,
            is_write: false,
            bash: None,
        };
        assert_eq!(gate.decide(&call), ApprovalDecision::Auto);
    }

    #[test]
    fn write_call_with_no_rule_needs_prompt() {
        let gate = gate_with(vec![]);
        let args = json!({});
        let call = PendingCall {
            tool_name: "yah_rename",
            args: &args,
            is_write: true,
            bash: None,
        };
        assert!(matches!(
            gate.decide(&call),
            ApprovalDecision::NeedsPrompt { .. }
        ));
    }

    #[test]
    fn tool_rule_allows_matching_write() {
        let gate = gate_with(vec![ApprovalRule::Tool {
            name: "yah_rename".into(),
        }]);
        let args = json!({});
        let call = PendingCall {
            tool_name: "yah_rename",
            args: &args,
            is_write: true,
            bash: None,
        };
        assert_eq!(
            gate.decide(&call),
            ApprovalDecision::Allow { rule_index: 0 }
        );
    }

    #[test]
    fn tool_path_rule_matches_glob_on_path_arg() {
        let gate = gate_with(vec![ApprovalRule::ToolPath {
            name: "edit_file".into(),
            glob: "src/**/*.rs".into(),
        }]);
        let allowed = json!({"path": "src/lib/foo.rs"});
        let denied = json!({"path": "scripts/build.sh"});
        let allowed_call = PendingCall {
            tool_name: "edit_file",
            args: &allowed,
            is_write: true,
            bash: None,
        };
        let denied_call = PendingCall {
            tool_name: "edit_file",
            args: &denied,
            is_write: true,
            bash: None,
        };
        assert!(matches!(
            gate.decide(&allowed_call),
            ApprovalDecision::Allow { .. }
        ));
        assert!(matches!(
            gate.decide(&denied_call),
            ApprovalDecision::NeedsPrompt { .. }
        ));
    }

    #[test]
    fn bash_cmd_rule_allows_any_args() {
        let gate = gate_with(vec![ApprovalRule::BashCmd {
            cmd: "cargo".into(),
        }]);
        let args = json!({"input": "ignored"});
        let bash = parse_bash("cargo build --release").unwrap();
        let call = PendingCall {
            tool_name: "bash",
            args: &args,
            is_write: true,
            bash: Some(&bash),
        };
        assert!(matches!(gate.decide(&call), ApprovalDecision::Allow { .. }));

        // Different cmd with the same rule: no match.
        let bash_other = parse_bash("rm -rf /").unwrap();
        let call_other = PendingCall {
            tool_name: "bash",
            args: &args,
            is_write: true,
            bash: Some(&bash_other),
        };
        assert!(matches!(
            gate.decide(&call_other),
            ApprovalDecision::NeedsPrompt { .. }
        ));
    }

    #[test]
    fn bash_cmd_pattern_requires_exact_arg_sequence() {
        let gate = gate_with(vec![ApprovalRule::BashCmdPattern {
            cmd: "cargo".into(),
            args: vec![
                ArgPattern::Exact {
                    value: "build".into(),
                },
                ArgPattern::Exact { value: "-p".into() },
                ArgPattern::Any,
            ],
        }]);
        let args = json!({});
        let allowed = parse_bash("cargo build -p yah-tauri").unwrap();
        let allowed_call = PendingCall {
            tool_name: "bash",
            args: &args,
            is_write: true,
            bash: Some(&allowed),
        };
        assert!(matches!(
            gate.decide(&allowed_call),
            ApprovalDecision::Allow { .. }
        ));

        // Wrong arg count.
        let extra = parse_bash("cargo build -p yah-tauri --release").unwrap();
        let extra_call = PendingCall {
            tool_name: "bash",
            args: &args,
            is_write: true,
            bash: Some(&extra),
        };
        assert!(matches!(
            gate.decide(&extra_call),
            ApprovalDecision::NeedsPrompt { .. }
        ));

        // Wrong exact value.
        let wrong = parse_bash("cargo test -p yah-tauri").unwrap();
        let wrong_call = PendingCall {
            tool_name: "bash",
            args: &args,
            is_write: true,
            bash: Some(&wrong),
        };
        assert!(matches!(
            gate.decide(&wrong_call),
            ApprovalDecision::NeedsPrompt { .. }
        ));
    }

    #[test]
    fn glob_match_basics() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
        assert!(glob_match("foo*", "foobar"));
        assert!(!glob_match("foo*", "foo/bar"));
        assert!(glob_match("foo/*.rs", "foo/x.rs"));
        assert!(!glob_match("foo/*.rs", "foo/sub/x.rs"));
        assert!(glob_match("foo/**/*.rs", "foo/a/b/c.rs"));
        assert!(glob_match("foo/**/*.rs", "foo/x.rs"));
        assert!(glob_match("**/*.rs", "x.rs"));
        assert!(glob_match("**/*.rs", "a/b/c.rs"));
    }

    // ---------- file store + async router ----------

    #[test]
    fn file_store_round_trips_through_disk() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileApprovalStore::load_or_empty(tmp.path());
        store.push(ApprovalRule::Tool {
            name: "yah_rename".into(),
        });
        store.push(ApprovalRule::BashCmdPattern {
            cmd: "cargo".into(),
            args: vec![ArgPattern::Exact {
                value: "build".into(),
            }],
        });
        // Reload from disk — the new instance reads the same file.
        let reloaded = FileApprovalStore::load_or_empty(tmp.path());
        let rules: Vec<_> = reloaded.snapshot().rules().to_vec();
        assert_eq!(rules.len(), 2);
        assert!(rules
            .iter()
            .any(|r| matches!(r, ApprovalRule::Tool { name } if name == "yah_rename")));
        assert!(rules.iter().any(|r| matches!(
            r,
            ApprovalRule::BashCmdPattern { cmd, .. } if cmd == "cargo"
        )));
    }

    #[test]
    fn file_store_push_dedupes() {
        // Two pushes of the same rule must not duplicate — the
        // AlwaysAllow click is idempotent.
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileApprovalStore::load_or_empty(tmp.path());
        let rule = ApprovalRule::Tool {
            name: "edit_file".into(),
        };
        store.push(rule.clone());
        store.push(rule.clone());
        assert_eq!(store.snapshot().rules().len(), 1);
    }

    #[test]
    fn file_store_remove_at_handles_out_of_range() {
        let store = FileApprovalStore::load_or_empty(tempfile::TempDir::new().unwrap().path());
        store.push(ApprovalRule::Tool {
            name: "edit_file".into(),
        });
        // Doesn't panic; doesn't change the rule list.
        store.remove_at(99);
        assert_eq!(store.snapshot().rules().len(), 1);
        store.remove_at(0);
        assert_eq!(store.snapshot().rules().len(), 0);
    }

    #[test]
    fn file_store_treats_missing_file_as_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = FileApprovalStore::load_or_empty(tmp.path());
        assert!(store.snapshot().is_empty());
    }

    #[test]
    fn file_store_treats_malformed_file_as_empty() {
        // Corrupt rules file shouldn't take down the host. Empty
        // ruleset = "every write tool needs prompting" which is the
        // safe failure mode.
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join(".yah");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(APPROVAL_RULES_FILENAME), "not valid json").unwrap();
        let store = FileApprovalStore::load_or_empty(tmp.path());
        assert!(store.snapshot().is_empty());
    }

    #[tokio::test]
    async fn pending_approvals_resolve_round_trip() {
        let pending = PendingApprovals::new();
        let id = mint_request_id();
        let rx = pending.register(id.clone()).await;
        let resolved = pending.resolve(&id, ApprovalChoice::Apply).await;
        assert!(resolved);
        let choice = rx.await.unwrap();
        assert!(matches!(choice, ApprovalChoice::Apply));
    }

    #[tokio::test]
    async fn pending_approvals_resolve_unknown_id_returns_false() {
        let pending = PendingApprovals::new();
        let resolved = pending.resolve("nope", ApprovalChoice::Skip).await;
        assert!(!resolved);
    }

    #[tokio::test]
    async fn static_router_returns_canned_choice() {
        let router = StaticApprovalRouter::new(ApprovalChoice::AlwaysAllow {
            rule: ApprovalRule::Tool {
                name: "edit_file".into(),
            },
        });
        let req = ApprovalRequest {
            session_id: SessionId::new("session:test01234567"),
            request_id: mint_request_id(),
            tool_name: "edit_file".into(),
            args: serde_json::json!({}),
            bash: None,
        };
        match router.request(req).await {
            ApprovalChoice::AlwaysAllow { rule } => {
                assert!(matches!(rule, ApprovalRule::Tool { name } if name == "edit_file"));
            }
            other => panic!("expected AlwaysAllow, got {other:?}"),
        }
    }
}
