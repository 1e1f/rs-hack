//! @arch:layer(kg_store)
//! @arch:role(bridge)
//!
//! Agent runtime: per-ticket Claude Agent SDK sessions surfaced as Tauri
//! commands. Sister module to [`crate::commands`] (read/write KG surface)
//! and [`crate::api_keys`] (provider credentials).
//!
//! ## Lifecycle
//!
//! 1. Renderer calls [`agent_start_session`] with a rig + ticket id.
//! 2. We ask the daemon to [`KgService::assemble_prelude`] — the cached
//!    prefix that rides every turn (see R028-F2).
//! 3. We dispatch on `prelude.engine.is_claude()`:
//!    - **claude:\*** → Claude path (this module).
//!    - **else** → returns a clear "not yet wired" error pointing at
//!      R018-F2 (yah-runner). The dispatch matrix lives at this seam so
//!      runner crates can land independently (R018-F3).
//! 4. We mint a [`SessionId`], register the [`AgentSession`] in
//!    [`AgentSessions`], emit [`AgentEvent::Started`], and return to the
//!    renderer.
//! 5. Subsequent [`agent_send`] calls spawn a streaming HTTP request to
//!    Anthropic's `/v1/messages` endpoint with `stream: true`; SSE
//!    deltas fan out as [`AgentEvent::MessageDelta`] until
//!    `message_stop` flips to [`AgentEvent::MessageEnd`].
//! 6. [`agent_stop`] aborts any in-flight turn and drops the session.
//!
//! ## Streaming surface
//!
//! Events go to the renderer via Tauri's window event bus on the
//! channel `agent:event` (sister to `arch:event`). The payload is a
//! flattened [`AgentEvent`] with `sessionId` always present so the
//! renderer can route by session.
//!
//! ## Authentication (post-2026-04-04)
//!
//! This module is the **HTTP + Anthropic-native** cell of the runtime
//! matrix (`.yah/arch/authored/yah-agent-runtime.md`'s "Composable runtime
//! matrix"). It authenticates with the API-key path:
//! [`api_keys::get("anthropic")`] returns the `sk-ant-…` token from
//! Console; sessions started without a stored key fail fast with a
//! clear `Error` event.
//!
//! Pro/Max OAuth was originally R028-F3's primary research target. It
//! is now **TOS-blocked**: Anthropic's 2026-04-04 policy explicitly
//! bans consumer OAuth tokens in third-party tools (and names the
//! Agent SDK in the policy text). The cost-controlling subscription
//! path now lives in the **Process + MCP** cell — the Tauri host
//! spawns `claude` as a subprocess per session, lets Claude Code
//! handle its own OAuth, and exposes `KgToolRegistry` to it via an
//! MCP server (`yah-mcp`). Track that work under R028-F8 / R028-F9.
//!
//! This cell is kept warm anyway: cloud-provider auth (Bedrock,
//! Vertex, Foundry) routes through the same `/v1/messages` wire with
//! different headers, and if Anthropic relaxes the OAuth policy the
//! flip is one keychain slot + one `Authorization: Bearer` header.
//!
//! @yah:ticket(R028-F3, "Runner-HA crate: serves anthropic (HAk) + crab (HAo) + mcp-connect (+M) presets")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R028)
//! @yah:gotcha("Scope reshape on 2026-04-28: this ticket originally also owned the Pro/Max OAuth flow inside a Tauri webview. That work is now policy-blocked — Anthropic's 2026-04-04 TOS change bans consumer OAuth in third-party tools (and explicitly names the Agent SDK). Terminology locked in: this crate serves the HA-family presets — `anthropic` (HAk, API key), `crab` (HAo, OAuth — kept as a disabled-by-default slot), and `mcp-connect` (HAk+M / HAo+M, MCP-connector beta). The cost-controlling subscription path moved to the `claude` (PVd) preset in R028-F8 / R028-F9. See .yah/arch/authored/yah-agent-runtime.md 'Composable runtime matrix' for the canonical preset table + axis encoding (Transport · Protocol · Auth).")
//! @yah:gotcha("Runtime vocabulary (SessionId/Role/Message/AgentEvent) was lifted to yah-kg/src/agent.rs on 2026-04-28 so R018's non-Claude runner can share the contract. The Tauri seam now wraps emits in RigAgentEvent { rigId, ...event } (mirrors RigEvent for ArchEvent). Renderers consume the wrapped payload on 'agent:event'; runtime AgentEvent itself stays runner-agnostic.")
//! @yah:handoff("Architecture pivoted on 2026-04-28: a 3-axis runtime matrix (Transport · Protocol · Auth) with five named presets — anthropic (HAk), crab (HAo), mcp-connect (HAk+M / HAo+M), claude (PVd), openai (HOk) — replaced the earlier 'two first-class runtimes' framing. Driver: Anthropic's 2026-04-04 ban on consumer OAuth in third-party tools (named the Agent SDK by name) made the prior 'Pro/Max OAuth in a Tauri webview' next-step TOS-untenable. Cost-controlling subscription path moved to the `claude` (PVd) preset — R028-F8 wraps the official `claude` CLI as a subprocess and R028-F9 exposes our KgToolRegistry as an MCP server. Doc rewrites landed: .yah/arch/authored/yah-agent-runtime.md (canonical 'Composable runtime matrix' with axes / presets / force-relations / day-one defaults), .yah/arch/authored/yah-roadmap-2026Q2.md Track E, README.md (new 'Agent runtime (yah-tauri)' section recommending PVd as the default Anthropic preset), AgentProvidersPanel.tsx (Anthropic card no longer says 'OAuth coming soon'), agent.rs module docstring, settings-api-keys.md, agent-tool-calls.md, anno.rs, yah-runner/src/lib.rs, yah-kg-daemon/src/lib.rs. Prior @yah:think mapping work (model_supports_thinking, think_budget_tokens, build_anthropic_body extraction; 4 new agent::tests; cargo test -p yah-tauri --lib agent 6/6 green) is unchanged and still applies to this cell. The next agent picks up: validate the HOk preset (current openai/ollama path) end-to-end, then turn on HAo (crab — header swap from x-api-key → Authorization: Bearer with a separate keychain slot 'anthropic-oauth' that takes precedence when present). R028-F8 (claude / PVd) is the heavier follow-on, owned separately.")
//! @yah:next("Validate the HOk preset (openai / ollama / vLLM / Together) end-to-end against current code. cargo test -p yah-runner + cargo test -p yah-tauri --lib agent should both pass; live test with ollama serve on localhost:11434 should round-trip a Prelude → MessageDelta → MessageEnd cycle.")
//! @yah:next("HAo (crab) wiring: add a second keychain slot 'anthropic-oauth' alongside 'anthropic'. In send_claude / agent_list_models, prefer the OAuth slot when present and emit Authorization: Bearer + the anthropic-beta: oauth-2025-04-20 header instead of x-api-key. Mark the slot as 'experimental — may violate Anthropic TOS' in any UI surface. Settings panel needs a fourth field.")
//! @yah:next("Implement R028-F8 (claude / PVd): spawn `claude` as subprocess with `--output-format stream-json --input-format stream-json` per session; render Prelude via R028-F6's CLAUDE.md sink to <rig>/.claude/CLAUDE.md; configure the subprocess to connect to yah-mcp (R028-F9) for tool dispatch.")
//! @yah:next("Implement R028-F9 (yah-mcp crate): wrap KgToolRegistry as an MCP server. In-process stdio when launched by the PVd preset; TCP listener as a future affordance for the mcp-connect (+M modifier) preset.")
//! @yah:next("Per-engine model defaults: hard-coded DEFAULT_CLAUDE_MODEL='claude-opus-4-7'. Once R027 grows a workspace-default-model setting, read it in start_claude_session before falling back. Same pattern wanted for openai (HOk) via OpenAiCompatConfig::default_model.")
//! @yah:next("Worth considering: workspace-tunable think tier values. Today the Deep=16000/Standard=4000/Fast=1024 mapping is hard-coded. R027's settings panel could expose these as 'Thinking budgets'. Note: the claude (PVd) preset has its own thinking knob (`claude --thinking`) so this only applies to HA* presets.")
//! @yah:next("Optional polish: agent_get_prelude(sessionId) -> PreludeView returning the structured Prelude.sections (not just the rendered markdown string). Punt until users ask.")
//! @yah:handoff("HOk preset validated end-to-end: cargo test -p yah-runner 32/32 unit + 5/5 e2e (now 6 tests total) green; cargo test -p yah-tauri --lib 71/71 green; cargo build --workspace clean. New ignored test ollama_local_round_trip in yah-runner/tests/openai_compat_e2e.rs drives a real localhost:11434 Prelude → start → send cycle and asserts TurnStarted + ≥1 MessageDelta + TurnEnded(stop_reason=Some) — passed against a live ollama with qwen2.5-coder:1.5b. HAo (crab) wiring audit: keychain slot 'anthropic-oauth' exists (agent.rs:297), resolve_anthropic_auth prefers OAuth slot (agent.rs:446), AnthropicAuth::Oauth emits Authorization: Bearer + anthropic-beta: oauth-2025-04-20,claude-code-20250219 (with PinnedShape replay path when audit artifact loaded), system-prompt prefix enforced via ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX (agent.rs:313), Settings panel fourth field present at AgentProvidersPanel.tsx:63 with experimental-TOS footnote, NoSession.tsx engine spec='claude' recognises both anthropic + anthropic-oauth slots (line 62). Both HAk and HAo are functionally complete. Remaining R028-F3 next-items are either tied to R027 (workspace-default-model, tunable think-tier values) or deferred to Anthropic GA (mcp-connect +M body-field wiring, currently 'open question' in .yah/arch/authored/yah-agent-runtime.md).")
//! @yah:next("HAk+M / HAo+M (mcp-connect): currently blocked on Anthropic GA-ing the mcp_servers request-body field on /v1/messages. When that lands, plumb mcp_servers[] into build_anthropic_body in agent.rs alongside the existing tools[]. The yah-mcp server stood up in R028-F9 will be the natural backend for both this cell and the PVd subprocess cell.")
//! @yah:next("Per-engine model defaults: replace hard-coded DEFAULT_CLAUDE_MODEL='claude-opus-4-7' with a workspace-default-model lookup once R027 grows the setting. Same pattern wanted for OpenAiCompatConfig::default_model in yah-runner. Until R027 ships these settings, leave the hard-codes.")
//! @yah:next("Workspace-tunable think tier values (Deep=16000/Standard=4000/Fast=1024 today). When R027's settings panel lands a 'Thinking budgets' section, read those instead of the hard-codes in build_anthropic_body. Note: only applies to HA-family — PVd has its own --thinking knob.")
//! @yah:next("Optional polish: agent_get_prelude(sessionId) -> PreludeView returning structured Prelude.sections (not just the rendered markdown). Punt until users ask for an inspect-prelude UI.")
//! @yah:verify("cargo test -p yah-runner")
//! @yah:verify("cargo test -p yah-tauri --lib agent")
//! @yah:verify("ollama serve & cargo test -p yah-runner --test openai_compat_e2e -- --ignored ollama_local_round_trip")
//!
//! @yah:relay(R031, "Agent tool calls — registry, AST writers, structured approval")
//! @yah:status(open)
//! @yah:parent(R028)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//!
//! @yah:ticket(R031-F1, "Read-only tool registry: Tool trait + read_file/list_dir/grep/arch_*/read_arch_doc")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R031)
//! @yah:handoff("P1 read-only tool registry landed. Two-layer trait split: yah-runner gets the provider-agnostic ToolRegistry trait (schemas() + execute(name, args) → ToolOutcome) in yah-runner/src/tool.rs, re-exported alongside ToolSchema/ToolOutcome from yah-runner/src/lib.rs. OpenAiCompatRunner grew with_tools(config, Arc<dyn ToolRegistry>) constructor + tools() accessor; new() stays as the no-tools shorthand so existing call sites compile unchanged. Inner now stores Option<Arc<dyn ToolRegistry>> — held but not yet exercised (P2/F2 wiring will use it on the request body's tools[] and the SSE tool_calls[] dispatch loop). app/tauri/src/agent_tools.rs houses the concrete Tool trait + ToolContext (rig_id, rig_root, Arc<KgService>) + ToolError (InvalidArgs / SandboxEscape / Operation, mapped to ToolOutcome::fail with structured kind) + KgToolRegistry::standard_read_only(ctx) which wires eight tools: read_file (256KB cap, optional 1-based line range), list_dir (500-entry cap, skips target/node_modules/.git), grep (regex+optional glob, 200-hit cap, 4MB per-file scan cap, walkdir filter_entry skips noise dirs), arch_node, arch_neighbors (string dir 'in'|'out'|'both', edges parsed from snake_case names), arch_subgraph (default depth 2), arch_lookup (file:line → innermost-first NodeIds), read_arch_doc (delegates to KgService::read_authored_file's existing sandbox). Filesystem tools sandbox via resolve_in_sandbox: candidate.canonicalize() then starts_with(rig_root.canonicalize()) — mirrors kg_daemon::KgService::read_authored_file. agent.rs's start_runner_session now takes Arc<KgService> and constructs the registry per-runner so tools close over the right rig; both call sites (agent_start_session + agent_start_chat_session) wired. New deps in app/tauri/Cargo.toml: walkdir, glob, regex (runtime); tempfile, tokio test feature (dev). Tests: 9 unit covering registry size+schema shape, sandbox refusal (.. traversal), read_file content+bytes+truncation, line range, list_dir skips target, grep finds with glob filter, unknown tool dispatch error lists known tools, read_arch_doc rejects outside-sandbox paths even with daemon booted. cargo test -p yah-tauri --lib agent_tools 9/9; cargo test -p yah-tauri --lib 27/27; cargo test -p yah-runner 22/22; cargo build --workspace clean (only pre-existing warnings).")
//! @yah:next("R031-F2 (P2): build tools[] from registry on OpenAiCompatRunner's request body and parse choices[].delta.tool_calls[].function.{name,arguments} deltas. The registry is already on the runner — `inner.tools` is what the body builder reads. arch doc says cap loop iterations at 8 by default.")
//! @yah:gotcha("EdgeKind in kg::edge serializes as { edge: 'contains' } (internally tagged) but the LLM-facing schema asks for snake_case strings — parse_edge_kinds() in agent_tools.rs round-trips through serde_json::json!({ edge: name }) to map. If new EdgeKind variants land that aren't snake_case the parser rejects with `unknown edge kind` rather than panicking.")
//! @yah:gotcha("Tool trait lives host-side in app/tauri/src/agent_tools.rs (needs Arc<KgService> via ToolContext) and the runner-facing runner::ToolRegistry is the abstract surface. KgToolRegistry implements both: it owns Vec<Box<dyn Tool>> and impls ToolRegistry. P5 approval gate (R031-F5) will sit *between* the runner's execute() call and the Tool::execute() dispatch — write tools (R031-F4) get is_write()=true so the registry can route them through the gate without each tool checking.")
//! @yah:assumes("Provider tools[] schema requires `type: object` with a `properties` object — verified empirically against OpenAI docs but not the actual provider response. R031-F2's first round-trip will catch any schema shape that needs adjusting (test standard_registry_tool_schemas_are_object_typed_with_required asserts the invariant).")
//! @yah:verify("cargo test -p yah-tauri --lib agent_tools")
//! @yah:verify("cargo test -p yah-tauri --lib && cargo test -p yah-runner")
//! @yah:verify("cargo build --workspace")
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! @yah:ticket(R031-F4, "AST-native write tools: yah_add/remove/rename/transform + ts_* + write_arch_doc + edit_file fallback")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P4)
//! @yah:parent(R031)
//! @yah:handoff("R031-F4 P4 write surface landed in app/tauri/src/agent_tools.rs. Seven new Tool impls: YahAdd/YahRemove/YahRename/YahTransform/YahUpdate (subprocess to `yah` CLI on PATH so we inherit every refactor improvement landed in core hack engine — when R022 lifts CLI to lib API, swap is one Command::new -> in-process call) + WriteArchDoc (mirrors read_authored_file's sandbox: parent-dir canonicalize + .yah/arch/authored/ prefix check + .mmd extension gate) + EditFile (last-resort line fallback for files without an AST indexer; refuses on 0-match or >1-match — multi-match is the ambiguity case the LLM disambiguates by adding context). All writers set is_write()=true and call KgService::reindex_path(path, IndexReason::AgentEdit) after a successful mutation so the watcher seam fans out events with no extra plumbing. New constructor KgToolRegistry::with_experimental_writers(ctx) layers writers on top of standard_read_only — deliberately NOT wired into agent::start_runner_session so chat sessions stay read-only until F5 lands the approval gate. Helpers: run_yah_cli (cwd=rig_root, stdout/stderr capture with 64KB cap), reindex_glob (resolves `paths` glob against rig_root + reindex each canonicalized match that survives the sandbox check), opt_str/opt_bool/push_str_flags + dispatch_yah keep the five YahX tools nearly identical. yah_outcome envelope: { exit_code, stdout, stderr, touched[] }. New tests (9 added, 20 total in agent_tools): writer registry size + names; is_write set correctly per tool; writer schemas are object-typed; write_arch_doc happy path + sandbox refusal + non-mmd refusal; edit_file happy path + multi-match refusal + missing-string refusal + sandbox escape; yah_add envelope shape (succeeds whether yah is on PATH or not — asserts no panic + well-formed envelope). cargo test -p yah-tauri --lib agent_tools: 20/20. cargo test -p yah-tauri --lib: 59/59. cargo test -p yah-runner: 5/5. cargo build -p yah-tauri: clean (only pre-existing yah-rpc-ssh warning). The full workspace doesn't build today: pre-existing E0063 in yah/src/main.rs from the recent WorkItemAnno.agent_policy field addition — orthogonal to F4, owned elsewhere.")
//! @yah:gotcha("Subprocess approach was deliberate: yah/src/mcp/tools.rs already shells to `yah` for the rs-hack MCP server, and R022 (relay in that file) is the lifting-to-lib-API track. Doing the same here means R022's eventual landing replaces five Command::new sites with one in-process call. Going via `yah::editor::RustEditor::apply_operation` directly today would duplicate the high-level kind-and-flag dispatch that lives only in yah/src/main.rs's HackCommands::{Add,Remove,Update,Rename,Transform} arms. yah is not yet a Cargo dep of app/tauri — keep it that way until R022.")
//! @yah:gotcha("TS writers are NOT in the registry: yah-kg-ts only exposes a TsIndexer (read-only LanguageIndexer impl) — there is no ts_add/ts_remove/ts_rename surface to wrap. The ticket title still mentions ts_* because the parent design includes them; the next agent can land them once yah-kg-ts grows a write surface. Tracked under @yah:next.")
//! @yah:gotcha("Writers are reachable from tests via KgToolRegistry::with_experimental_writers(ctx) but NOT from chat sessions: agent::start_runner_session still calls standard_read_only. The wiring change to enable writers in production is a one-line swap in agent.rs and a settings flag, gated on R031-F5's approval gate landing. Doing that swap before F5 hands writers to the LLM with no approval — strictly an F5 follow-up, not an F4 polish.")
//! @yah:next("R031-F5 (P5): land KgToolRegistry::execute_gated(call, &ctx) at the host layer between runner::ToolRegistry::execute and Tool::execute. Approval rules + bash arg parser + settings UI per the F5 ticket. Once that lands, swap agent::start_runner_session's standard_read_only -> with_experimental_writers behind an opt-in settings flag.")
//! @yah:next("TS write surface in yah-kg-ts: today TsIndexer is read-only. ts_add / ts_remove / ts_rename Tool impls follow the same dispatch_yah shape once a Rust-callable TS write API exists (or once `yah` grows TS-aware subcommands and we shell to those). Adding placeholder Tool impls that return ToolError::Operation('ts writes not implemented') would just be noise — wait until the underlying surface lands.")
//! @yah:next("R022 (MCP in-process dispatch): once yah::cli::run(argv) -> Result<i32> ships, swap run_yah_cli's Command::new for an in-process call. Mirror the change in yah/src/mcp/tools.rs at the same time so the rs-hack MCP server and our agent_tools writers stay 1:1.")
//! @yah:next("Polish: the dispatch_yah envelope returns stdout verbatim, which embeds yah CLI's '💡 This was a DRY RUN' hint when --apply somehow doesn't reach (shouldn't happen, but defensive). Consider parsing the structured run-id line from yah's --json mode (yah supports it on most subcommands) so revert is reachable from the agent — coordinate with R031-F5 since revert is itself a write.")
//! @yah:verify("cargo test -p yah-tauri --lib agent_tools")
//! @yah:verify("cargo test -p yah-tauri --test agent_writers_e2e")
//! @yah:verify("cargo test -p yah-tauri --lib && cargo test -p yah-runner")
//! @yah:verify("cargo build -p yah-tauri")
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! @yah:ticket(R031-F5, "Structured approval: per-call gate, parsed-data rules, bash ENV/cmd/args parser, Settings UI")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P5)
//! @yah:parent(R031)
//! @yah:gotcha("Gate placement is load-bearing: the approval check lives at the HOST layer (inside KgToolRegistry, between the runner-shaped ToolRegistry::execute and Tool::execute), NOT inside runner::ToolRegistry::execute. R028's Anthropic path parses tool_use blocks in run_anthropic_turn directly — it never goes through the runner trait. KgToolRegistry::execute_gated is the single gate; ToolRegistry::execute is a thin shim. KgToolRegistry exposes tool(name)/tools() borrows so the Anthropic loop can iterate schemas + dispatch without round-tripping through the runner trait. See .yah/arch/authored/agent-tool-calls.md 'Approval gate placement (load-bearing)'.")
//! @yah:gotcha("Writers stay off the production hot path until the F4-flip lands. agent::start_runner_session still uses KgToolRegistry::standard_read_only — the gate is fully wired (FileApprovalStore + TauriApprovalRouter + bind_session post runner.start) but no write tool is reachable yet, so every call short-circuits to ApprovalDecision::Auto. Flipping standard_read_only → with_experimental_writers behind a settings flag (agent_writers_enabled, default false) is the production opt-in.")
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//! @yah:handoff("Backend complete (phase A + B). agent_approval.rs owns rule schema (versioned ApprovalRuleset envelope; ApprovalRule { Tool, ToolPath, BashCmd, BashCmdPattern } + ArgPattern { Exact, Any }), BashCall parser/synthesizer (refuses shell metacharacters; POSIX-safe re-quote), ApprovalGate::decide (read-only Auto, rule-match Allow, otherwise NeedsPrompt), ApprovalStore trait with InMemoryApprovalStore (tests) + FileApprovalStore (per-rig <rig>/.yah/agent-approval-rules.json, atomic write-then-rename; missing/malformed → empty ruleset, fail-safe deny), ApprovalChoice (Apply/Skip/AlwaysAllow{rule}), async ApprovalRouter trait + StaticApprovalRouter (tests) + TauriApprovalRouter (agent.rs — emits AgentEvent::ApprovalRequested + awaits oneshot via PendingApprovals, lives on AgentSessions). KgToolRegistry::execute_gated is the single gate site for both runner shapes; on NeedsPrompt it suspends through prompt_for_approval (router.request().await) and acts on the choice (Apply→run, Skip→ToolOutcome::fail{kind:approval_skipped}, AlwaysAllow→push rule then run). Without router/session_id falls back to structured approval_required failure so the LLM still gets a clear signal. KgToolRegistry holds Mutex<Option<SessionId>> + bind_session(&self, id) since the runner mints internally; host pattern is Arc<KgToolRegistry> alongside Arc<dyn ToolRegistry>, then bind_session after runner.start returns. Two new AgentEvent variants in yah-kg::agent: ApprovalRequested {sessionId, requestId, toolName, args, bash?} and ApprovalResolved {sessionId, requestId, decision}. Four Tauri commands registered in lib.rs::invoke_handler: agent_approval_decide, agent_approval_rules_list/add/remove. start_runner_session wires FileApprovalStore + TauriApprovalRouter + bind_session per session. Tests: agent_approval 25 unit (parser, synth, ruleset serde + unknown-version reject, gate decisions for every rule kind, file store round-trip + dedupe + remove + missing/malformed handling, PendingApprovals round-trip, StaticApprovalRouter), agent_tools 27 unit (gate auto-allows reads, blocks write without rule, allows seeded rule, router Apply/Skip/AlwaysAllow paths incl. OnceRouter panicking-on-2nd-call to prove rule persistence, falls through to approval_required without router), agent_writers_e2e 4 e2e via writers_with_apply_router(ctx) helper. cargo test -p yah-tauri --lib 103/103; --test agent_writers_e2e 4/4; cargo test --workspace clean; cargo build --workspace clean.")
//! @yah:next("Renderer #1: yah-ui chat-pane inline approval row. useChatSession listens for agent:event {kind:'approval_requested'} → render an Apply/Skip/AlwaysAllow row tagged with requestId; onClick → invoke('agent_approval_decide', {rigId, sessionId, requestId, choice}). For AlwaysAllow construct the rule client-side from the request shape (BashCmdPattern when bash field present, else Tool/ToolPath). Drop the row on approval_resolved.")
//! @yah:next("Renderer #2: SettingsModal 'Agents → Approval rules' section. invoke('agent_approval_rules_list', {rigId}) → render list with per-row Delete; invoke('agent_approval_rules_remove', {rigId, index}). Add-rule form lower priority — most rules ride in via inline AlwaysAllow.")
//! @yah:next("Production flip: switch start_runner_session from standard_read_only to with_experimental_writers gated on a settings flag (default false). Until then the wired gate is dormant — every tool is read-only Auto.")
//! @yah:next("R031-F6 (bash tool) is the natural follow-on. The gate already pre-parses 'bash'-named tool input via parse_bash and the BashCmd/BashCmdPattern rule shapes are live; F6 just needs the tool implementation that reads back the parsed call from the gate (or re-parses) and runs synthesize_bash through tokio::process::Command -c with cwd=rig_root.")
//!
//! @yah:ticket(R031-F6, "Constrained bash tool — default-deny, structured approval gate, cwd=rig_root")
//! @yah:status(open)
//! @yah:phase(P6)
//! @yah:parent(R031)
//! @yah:next("Bash tool routes through P5's structured approval gate (BashCmd / BashCmdPattern rules)")
//! @yah:next("Working dir = rig root. Stderr + stdout both stream back as ToolResult.result (truncated to a soft cap)")
//! @yah:next("Default deny everything; user must approve patterns. Lands LAST because without P5 the only safe shape is approve-every-call which doesn't scale")
//! @yah:next("Optional polish: timeout per call (30s default, configurable); kill-on-stop when the session aborts mid-execution")
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! @yah:ticket(R028-F8, "claude preset (PVd): wrap claude CLI as subprocess, stream-json drive, CLAUDE.md prelude")
//! @yah:assignee(agent:claude)
//! @yah:status(handoff)
//! @yah:phase(P3)
//! @yah:parent(R028)
//! @yah:gotcha("Policy-durable Anthropic preset (PVd in the runtime matrix). Spawn `claude` subprocess per session with --output-format stream-json --input-format stream-json. Auth is delegated entirely — Claude Code manages its own OAuth/login. We do NOT touch tokens. This is the README-recommended Anthropic default.")
//! @yah:gotcha("Engine routing kept 'claude' on the HA-family HTTP path (start_claude_session) — that preserves the day-one UI affordance (NoSession.tsx sends spec='claude' for users with sk-ant-… or anthropic-oauth keychain slots). PVd lives behind the explicit 'claude-cli' engine name. The architecture doc reserves 'claude' for PVd as the README default; the rename to swap them is a single dispatch-arm edit + a UI label change, intentionally deferred so this commit doesn't break existing settings.")
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//! @yah:handoff("Subprocess driver landed in app/tauri/src/agent_process.rs (~530 LOC, 12 unit tests). Spawns `claude --print --input-format stream-json --output-format stream-json --include-partial-messages --verbose --model <model>` with cwd=rig_root. Prelude sink is split: yah owns <rig>/.yah/CLAUDE.md (regenerated per session), and we idempotently inject one `@.yah/CLAUDE.md` import line at the top of root <rig>/CLAUDE.md (creates if absent, prepends with blank-line separator otherwise — preserves user content byte-for-byte). Why split: Claude Code's documented memory locations are <cwd>/CLAUDE.md and ~/.claude/CLAUDE.md — <cwd>/.claude/CLAUDE.md is NOT read, so the original ticket spec would have been a silent no-op. AgentSessions gains a third slot process_map; list_summaries iterates all three. Stream-json parser (translate_frame, pure fn) maps: stream_event/message_start → TurnStarted, stream_event/content_block_delta → MessageDelta, assistant whole-message frame → MessageDelta (only when partial deltas haven't already accumulated text — avoids double-emit), result/is_error=false → TurnEnded with subtype as stop_reason, result/is_error=true → TurnFailed, system/user frames dropped. Stop sequence: drop stdin → wait 1.5s grace → start_kill if still alive → wait. Files left in place across stop (.yah/CLAUDE.md is a session record overwritten on next start; root import line is stable across sessions). Dispatch matrix in agent.rs now: claude|anthropic|crab → start_claude_session (HA-family, HTTP); claude-cli → start_process_session (PVd, this ticket); openai|ollama → start_runner_session (HO-family). Kept 'claude' on the HA path for now to avoid breaking the existing UI (NoSession.tsx sends spec='claude'); 'claude-cli' is the explicit alias to opt into PVd. agent_send / agent_stop check the third map after the first two. emit_event_pub re-export added so agent_process can fan AgentEvents through the same agent:event channel. The actual rules-driven CLAUDE.md amalgamation (agent roles, dos/don'ts) is split out as R028-F10 — this ticket is the sink, F10 is the generator. cargo test -p yah-tauri --lib 46/46 green; cargo test --workspace clean; cargo build --workspace clean (only pre-existing warnings).")
//! @yah:next("Tool dispatch wiring lands with R028-F9 (yah-mcp). Until then the subprocess uses Claude Code's builtin tools + any MCP servers in the user's global ~/.claude/settings.json. When F9 ships, write <rig>/.claude/settings.json here registering the yah-mcp stdio server so KgToolRegistry tools appear inside the subprocess.")
//! @yah:next("UI affordance: add a 'Claude Code (PVd)' card to AgentProvidersPanel + a fourth row (spec='claude-cli', alwaysShow=true since auth is delegated) to NoSession.tsx's PRIORITY list. Today PVd is callable via the engine override but invisible in the picker.")
//! @yah:next("Engine name rename: when ready, swap dispatch so 'claude' → PVd (matches .yah/arch/authored/yah-agent-runtime.md and the README default) and 'anthropic'/'crab' continue to handle the HA path. Single-line dispatch edit + UI text change. Punted to keep this commit non-breaking for users on settings that already point 'claude' at HTTP.")
//! @yah:next("Live smoke: install claude CLI locally, attach a rig with a PVd-tagged ticket, send a turn, watch agent:event for SessionStarted → TurnStarted → MessageDelta(*) → TurnEnded. Verify .claude/CLAUDE.md is written on start and removed on stop.")
//! @yah:next("Cost/turn analytics: result frame carries num_turns, duration_ms, total_cost_usd, usage{}. ProcessSession could grow a turns counter from result.num_turns so list_summaries shows real counts (currently hardcoded 0 with a TODO comment). MessageEnd-style metric event in AgentEvent would benefit Anthropic-cache analytics for HA path too — coordinate with R028-F3 before adding.")
//!
//! @yah:ticket(R028-F9, "yah-mcp crate: KgToolRegistry as MCP server (stdio for subprocess, TCP for future)")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R028)
//! @yah:next("New crate yah-mcp wrapping desktop::agent_tools::KgToolRegistry. Two transports: stdio (launched by the claude (PVd) preset's subprocess on its own stdin/stdout bus) and TCP listener (for the mcp-connect (+M modifier) preset when Anthropic's MCP-connector beta GAs).")
//! @yah:next("Approval gate placement (continues R031-F5 design): KgToolRegistry::execute_gated stays the single gate; this crate routes MCP tool/call messages through that same gate. One gate, three callers (HA-family Anthropic dispatch, HOk OpenAI dispatch, yah-mcp MCP dispatch).")
//! @yah:next("Schema serialization: each Tool::schema returns the JSON-Schema shape MCP expects. Verify against the MCP protocol spec, especially the type/properties/required invariants — same shape OpenAI tools[] uses, modulo wrapper differences.")
//! @yah:next("Lifecycle: stdio mode lives for the subprocess lifetime (no separate process). TCP mode is a long-running yah-tauri sidecar — punt until needed.")
//! @yah:gotcha("Universal tool source for the runtime matrix. KgToolRegistry stays the source of truth; this crate is the MCP-protocol adapter. Tool::execute and Tool::schema already exist; this crate maps them onto MCP message shapes. Consumers: claude (PVd) preset via stdio; mcp-connect (+M modifier) preset via TCP when GA.")
//! @arch:see(.yah/arch/authored/yah-agent-runtime.md)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! @yah:ticket(R028-F11, "Per-turn usage on AgentEvent: input/output/cache tokens + thinking budget gauges")
//! @yah:status(handoff)
//! @yah:assignee(agent:claude)
//! @yah:parent(R028)
//! @yah:handoff("User wants gauges in the chat pane — input/output/cache-read tokens per turn + thinking-budget consumption — to cross-compare cost shape between the HA-family (HTTP) and PVd (subprocess) paths. AgentEvent::TurnEnded grows an optional TurnUsage struct populated from each runtime cell's terminal frame. Renderer gets a small per-pane Gauges component reading from the latest TurnEnded.")
//! @yah:next("Wire TurnUsage { input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens } onto AgentEvent::TurnEnded in yah-kg/src/agent.rs. Optional so older runners + chat sessions without a result frame stay valid.")
//! @yah:next("PVd capture in agent_process.rs::translate_frame: extract result.usage on the result frame, attach to TurnEnded. Test fixture covers shape.")
//! @yah:next("HA capture in agent.rs::run_anthropic_turn: parse message_start.usage (input + cache tokens) and accumulate output_tokens from each message_delta event. Attach to TurnEnded.")
//! @yah:next("HO capture in yah-runner/src/openai_compat.rs: set stream_options.include_usage=true on the request body, parse the final chunk's usage{prompt_tokens,completion_tokens}. OpenAI doesn't expose cache numbers — leave those None.")
//! @yah:next("Renderer Gauges component (yah-ui/src/components/agent/Gauges.tsx): per-session bar for input/output/cache-read + thinking-budget consumption. Slot into AgentView near StatusStrip. Read latest TurnEnded.usage from useChatSession; falls back to configured think budget when consumption isn't known.")
//! @yah:handoff("Per-turn TurnUsage now rides AgentEvent::TurnEnded for the HA-family + PVd cells. New struct in yah-kg/src/agent.rs: TurnUsage { input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens, thinking_tokens } — all Option<u32>, all skip_serializing_if=None on the wire. PVd capture (agent_process.rs::extract_result_usage): pulls from result.usage object, ignoring extra claude fields (server_tool_use, service_tier). HA capture (agent.rs::merge_anthropic_usage + run_anthropic_turn): two-frame merge — message_start.message.usage seeds input + cache numbers, each message_delta.usage updates output_tokens; later values overwrite earlier ones so the final delta wins. HO path (yah-runner/src/openai_compat.rs) emits usage:None today with a TODO pointing at stream_options.include_usage as the next step. Tests: extract_result_usage round-trips a realistic claude result frame; merge_anthropic_usage covers the message_start → message_delta sequence with the placeholder→canonical output_tokens flip. Test counts: yah-tauri --lib 48/48, yah-runner 32/32, yah-kg 62/62, openai_compat_e2e 5/5. cargo build --workspace clean.")
//! @yah:next("HO capture: set stream_options.include_usage=true on OpenAiCompatRunner's request body, parse the final chunk's usage{prompt_tokens,completion_tokens}. OpenAI doesn't expose cache_read_input_tokens — those stay None on the wire.")
//! @yah:next("Renderer Gauges component (yah-ui/src/components/agent/Gauges.tsx): per-session bar reading the latest TurnEnded.usage from useChatSession. Visualises input vs cache-read split (HA: shows real cache hit ratio; HO: input_tokens only) and output_tokens vs configured think budget (engine.think) when the engine declares one. Slot beneath StatusStrip in AgentView.")
//! @yah:next("Thinking-token separation on the HA path: today thinking_tokens stays None because Anthropic mixes thinking + answer output into a single output_tokens count. The thinking content blocks are streamed as content_block_start with type='thinking' — counting their text deltas (or summing reported per-block tokens once Anthropic surfaces them) lets us populate thinking_tokens precisely. Punted because empirically the bar 'output_tokens / configured budget' is enough for users to tune their @yah:think setting; precise thinking-vs-answer split is polish.")
//!
//! @yah:relay(R035, "Agent eval rig: save-it L3 target + yah-eval harness")
//! @yah:status(open)
//! @yah:next("Sequencing: harness lib + fixture (T1, T2) before any CLI surface; CLI subcommands (T3, T4) before the UI affordance (T7); replay capture (T6) deferred until live runs prove the wire shape stable so we don't capture into a format we'll have to migrate.")
//! @yah:next("Provider strategy: P1 pins one provider per run (default groq, env override). Multi-provider chains (e.g. plan-with-Sonnet, execute-with-Haiku) are an explicit follow-up — keep T4 single-provider so the eval-result JSON shape stays stable.")
//! @yah:next("CI gating: this relay never produces a CI gate. All sub-tickets land green when their own verify passes; running a full save-it eval against a real provider stays a maintainer-on-demand action. Document this in yah-eval's README.")
//! @arch:see(.yah/arch/authored/agent-eval-rig.md)
//! @arch:see(.yah/arch/authored/agent-tool-calls.md)
//!
//! @yah:ticket(R035-T1, "yah-eval crate: lib + bin scaffolding, eval YAML parsing, scoring loop")
//! @yah:status(open)
//! @yah:phase(P1)
//! @yah:parent(R035)
//! @yah:next("New crate yah-eval at workspace root. Public surface: EvalSpec (parsed YAML), Harness::run(spec, provider) -> EvalResult, mechanical scoring via verify_must_exit_zero shell exec.")
//! @yah:next("Bin entry yah-eval-cli (or yah eval subcommand — see relay strategy notes) wires CLI args to lib.")
//! @yah:next("Cargo.toml deps: yah-kg-daemon, yah-runner, yah-tauri (for KgToolRegistry), serde_yaml, tokio, anyhow.")
//! @yah:verify("cargo test -p yah-eval")
//! @yah:verify("cargo build -p yah-eval")
//!
//! @yah:ticket(R035-T2, "save-it fixture: pre-claimed tickets + Cargo + bun + Docker, embedded as tarball")
//! @yah:status(open)
//! @yah:phase(P1)
//! @yah:parent(R035)
//! @yah:next("Author yah-eval/fixtures/save-it/ as a real, buildable starter rig (Cargo.toml, src/{main,lib,handlers,models}.rs stubs, web/package.json, web/src/*.tsx stubs, migrations/001_init.sql empty, Dockerfile, docker-compose.yml, deploy.sh). Each file should compile/parse but have the actual feature gaps the agent fills in.")
//! @yah:next("Five @yah: tickets pre-seeded as @yah:status(open) with @yah:phase(P1..P5), @yah:assignee(agent:claude), @yah:verify lines per the arch doc table.")
//! @yah:next("Embed at build time via include_bytes! on a tar.gz produced by build.rs (so a fresh checkout doesn't need a separate fixture-fetch step).")
//! @yah:verify("cd <materialized-rig> && cargo build && cd web && bun install")
//!
//! @yah:ticket(R035-T3, "yah eval init <name> <dir>: extract embedded fixture + git init")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R035)
//! @yah:next("Subcommand wiring (clap arm under yah's main or yah-eval-cli's main, TBD by T1's harness placement decision).")
//! @yah:next("tar::Archive::unpack into <dir>; std::process::Command for git init && git add . && git commit. Reject if <dir> exists and is non-empty.")
//! @yah:next("Returns absolute path to materialized rig on stdout for shell-script chaining.")
//! @yah:verify("yah eval init save-it /tmp/eval-test-001 && test -f /tmp/eval-test-001/Cargo.toml && test -d /tmp/eval-test-001/.git")
//!
//! @yah:ticket(R035-T4, "yah eval run: drive ticket pickup, run verify, write evals/results/<id>.json")
//! @yah:status(open)
//! @yah:phase(P2)
//! @yah:parent(R035)
//! @yah:next("Spin a session via yah-runner / agent_tools::KgToolRegistry::with_experimental_writers (writers reachable bypassing R031-F5 gate is acceptable inside an eval rig — the rig is ephemeral).")
//! @yah:next("Pick tickets in @yah:phase order; on each ticket-to-review transition, run @yah:verify lines via shell; halt on first non-zero exit.")
//! @yah:next("Write evals/results/<run-id>.json: { provider, model, turn_count, tickets[], final_state }.")
//! @yah:next("Flags: turn-cap N per-ticket, budget-usd X (paid providers refuse to start without it; groq default = unlimited), provider <id>.")
//! @yah:verify("yah eval run save-it --provider groq --turn-cap 8 (smoke; needs a real GROQ_API_KEY)")
//!
//! @yah:ticket(R035-T5, "Inference stub: local Groq-shape mock for deterministic CI subset")
//! @yah:status(open)
//! @yah:phase(P3)
//! @yah:parent(R035)
//! @yah:next("Tiny axum server returning canned summaries+tags keyed off blake3(url). Listens on a free port; returns OpenAI-compat shape so the rig points GROQ_BASE_URL at it.")
//! @yah:next("Lets T1 (schema) and T5 (deploy) run deterministically without a real provider — those tickets are about plumbing, not inference quality.")
//! @yah:next("T2/T3 still need real inference: rate-limit handling is part of what the eval is testing. Stub never replaces those.")
//! @yah:verify("cargo test -p yah-eval inference_stub::round_trips")
//!
//! @yah:ticket(R035-T6, "Replay capture + replay run: deterministic regression suite for tool execution")
//! @yah:status(open)
//! @yah:phase(P4)
//! @yah:parent(R035)
//! @yah:next("On a live run, --record persists (prompt, tool_call, tool_result) jsonl alongside the result file.")
//! @yah:next("yah eval replay <run-id> drives the same captured tool calls against a fresh fixture and asserts ending state. Cheap CI-friendly cousin of the live run.")
//! @yah:next("Catches execution regressions (tool behavior changed) but NOT selection regressions (model picks a different tool). That's by design.")
//! @yah:next("Defer until live runs prove the wire shape stable so we don't burn-in a format we'll have to migrate.")
//! @yah:verify("yah eval replay <captured-id> exits 0 against a fresh fixture extracted from the same yah commit")
//!
//! @yah:ticket(R035-T7, "yah-ui operator surface: 'Run eval' under Settings → Agents")
//! @yah:status(open)
//! @yah:phase(P5)
//! @yah:parent(R035)
//! @yah:next("Tauri command wrapping yah-eval lib (NOT the CLI binary): fn agent_eval_run(rig_id, eval_name, provider) -> EvalResult.")
//! @yah:next("Settings panel section 'Agents → Evals' lists embedded fixtures, lets the user pick provider + turn cap, runs in a side session.")
//! @yah:next("Streams transcript into the existing chat-pane shape so the operator watches it live (sister to AgentEvent stream — emit the same events with a session marker so SessionList shows the eval as an in-flight session).")
//! @yah:verify("Manual: open yah-ui, settings → agents → evals → save-it → run; observe transcript, see final pass/fail row.")

use crate::agent_approval::{
    ApprovalChoice, ApprovalRequest, ApprovalRouter, ApprovalRule, ApprovalStore,
    FileApprovalStore, PendingApprovals,
};
use crate::agent_process::{self, ProcessSession};
use crate::agent_tools::{KgToolRegistry, ToolContext};
use crate::api_keys;
use crate::claude_shape::{self, PinnedShape};
use crate::state::{AppState, RigId};
use async_trait::async_trait;
use futures_util::StreamExt;
use kg::agent::{AgentEvent, Message, Role, SessionId, TurnUsage};
use kg::anno::{EngineRef, ThinkBudget};
use kg::prelude::Prelude;
use kg_daemon::KgService;
use rpc::AssemblePreludeParams;
use runner::{
    list_openai_compat_models, mint_session_id, tap_stream, OpenAiCompatConfig, OpenAiCompatRunner,
    Runner, SessionEventSink, SessionStore, ToolRegistry,
};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};

/// Tauri event channel name for agent events. The renderer subscribes
/// once at boot and routes every payload by `sessionId`.
pub const EVENT_NAME: &str = "agent:event";

/// Anthropic credential slot in the OS keychain (R027-F3 storage). The
/// renderer's API-keys panel writes here; provider clients read here.
/// Holds an `sk-ant-…` Console API key — pay-per-token billing. This is
/// the `anthropic` (HAk) preset's auth slot.
const ANTHROPIC_PROVIDER: &str = "anthropic";

/// Anthropic OAuth-bearer slot — the `crab` (HAo) preset's auth slot.
/// Holds a long-lived OAuth token from `claude setup-token` (Claude
/// Code's CLI command). When present, takes precedence over
/// [`ANTHROPIC_PROVIDER`] in the Anthropic-HTTP runner: the request
/// goes out as `Authorization: Bearer <token>` plus the
/// `anthropic-beta: oauth-2025-04-20` header instead of `x-api-key`.
///
/// **TOS-edge**: Anthropic's 2026-04-04 policy forbids consumer Pro/Max
/// OAuth tokens in third-party tools (and named the Agent SDK). Yah
/// supports the slot anyway for users who've made an informed,
/// personal-use call — see `.yah/arch/authored/yah-agent-runtime.md`'s
/// "Authentication reality" table. The recommended subscription path
/// is the `claude` (PVd) preset, which delegates auth entirely.
const ANTHROPIC_OAUTH_PROVIDER: &str = "anthropic-oauth";

/// `anthropic-beta` header value the OAuth path requires. Two values,
/// comma-joined: `oauth-2025-04-20` enables OAuth-token semantics and
/// `claude-code-20250219` declares the request follows the Claude Code
/// protocol. Sending only the first value triggers shape-mismatch
/// rate-limiting on the Anthropic side; both are mandatory. Verified
/// against the published Claude Code OAuth contract (April 2026).
const ANTHROPIC_OAUTH_BETA_VALUE: &str = "oauth-2025-04-20,claude-code-20250219";

/// Required system-prompt prefix on OAuth-authed `/v1/messages` calls.
/// Anthropic enforces that OAuth-token requests' first system block
/// begins with this exact string — it's the TOS-enforcement hook that
/// distinguishes Claude Code from third-party tools. Our prelude rides
/// in a *second* system block after this prefix; appending instructions
/// is allowed by the contract.
const ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX: &str =
    "You are Claude Code, Anthropic's official CLI for Claude.";

// Note: deliberately *not* setting a Claude-Code-mimicking User-Agent.
// Hardcoding `claude-cli/<version>` would either be stale or claim a
// version that doesn't exist — both make us *more* fingerprintable, not
// less. The documented OAuth requirements (beta header + system-prompt
// prefix) are what Anthropic actually enforces; the User-Agent was a
// belt-and-suspenders guess. PVd (R028-F8) makes this whole question
// moot — `claude` itself sends its own up-to-date signature.

/// OpenAI credential slot. ChatGPT Plus does **not** include API
/// access — users either paste a `sk-…` key from
/// https://platform.openai.com/api-keys or wire OAuth later (deferred).
const OPENAI_PROVIDER: &str = "openai";

/// Ollama credential slot. Optional — `ollama serve` on loopback needs
/// no auth, and the runner falls back to the local endpoint when this
/// slot is empty. Populated for Ollama Cloud (formerly Ollama Pro)
/// where bearer auth is required.
const OLLAMA_PROVIDER: &str = "ollama";

/// Default model for `@yah:engine(claude)` (no explicit model). The
/// arch doc keeps this configurable per workspace; until R027 grows a
/// model-default setting, hard-code the latest Opus.
const DEFAULT_CLAUDE_MODEL: &str = "claude-opus-4-7";

/// Anthropic API endpoint. Hard-coded; overridable later if we add a
/// staging-host setting.
const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Soft cap on the SSE stream's total bytes. A cooperating Anthropic
/// response is bounded by `max_tokens`, but a misbehaving upstream
/// shouldn't be able to fill the daemon's working set unbounded.
const MAX_STREAM_BYTES: usize = 32 * 1024 * 1024; // 32MB

/// Per-turn token cap forwarded to the Anthropic API. Independent of
/// the prelude's ring budget — that bounds *input*; this bounds
/// *output*. Both are needed.
const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 4096;

/// Anthropic's documented minimum for `thinking.budget_tokens`. Smaller
/// values reject with 400; we clamp `Budget { tokens: N }` and the
/// `Fast` tier up to this floor.
const ANTHROPIC_THINKING_MIN_BUDGET: u32 = 1024;

/// Authentication mode for the Anthropic-HTTP runner. Selected at
/// each Tauri command entry by [`resolve_anthropic_auth`]; the OAuth
/// slot wins when both are populated, since users who wired OAuth
/// presumably mean it.
///
/// Holding the token by-value (not by-ref) keeps the HTTP-spawn path
/// `'static` without reaching for `Arc<str>` plumbing.
///
/// The `Oauth` variant carries an optional [`PinnedShape`] reference
/// loaded at boot from the audit artifact (see
/// `.yah/arch/authored/yah-claude-shape-capture.md`). When present, [`apply`]
/// replays the pinned header set verbatim so the request matches Claude
/// Code's signature; when absent (degraded mode — TOML failed to
/// parse) we fall back to the bare `anthropic-beta` + `Authorization`
/// pair that's been the baseline since R028-F3 landed.
#[derive(Debug, Clone)]
enum AnthropicAuth {
    /// `anthropic` (HAk) preset — Console API key. Sent as `x-api-key`.
    ApiKey(String),
    /// `crab` (HAo) preset — long-lived OAuth bearer token from
    /// `claude setup-token`. Sent as `Authorization: Bearer …` plus
    /// the pinned-shape header set (or the hardcoded `anthropic-beta`
    /// fallback when no shape is loaded).
    Oauth {
        token: String,
        shape: Option<Arc<PinnedShape>>,
    },
}

impl AnthropicAuth {
    /// Apply this auth to a `reqwest::RequestBuilder`. Centralises the
    /// API-key vs Bearer split so call sites (`/v1/messages`,
    /// `/v1/models`) don't each re-derive the header set. Sets
    /// `anthropic-version` here too so callers don't have to remember.
    ///
    /// On the OAuth path with a loaded [`PinnedShape`], every pinned
    /// header is copied verbatim except the three that reqwest /
    /// transport own (authorization, host, content-length). The
    /// authorization slot is filled with yah's own bearer token.
    fn apply(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            Self::ApiKey(k) => req
                .header("x-api-key", k)
                .header("anthropic-version", ANTHROPIC_VERSION),
            Self::Oauth {
                token,
                shape: Some(shape),
            } => {
                let mut req = req.header("authorization", format!("Bearer {token}"));
                for (name, value) in &shape.headers {
                    if name.eq_ignore_ascii_case("authorization")
                        || name.eq_ignore_ascii_case("host")
                        || name.eq_ignore_ascii_case("content-length")
                    {
                        continue;
                    }
                    req = req.header(name.as_str(), value.as_str());
                }
                req
            }
            Self::Oauth { token, shape: None } => req
                .header("authorization", format!("Bearer {token}"))
                .header("anthropic-beta", ANTHROPIC_OAUTH_BETA_VALUE)
                .header("anthropic-version", ANTHROPIC_VERSION),
        }
    }

    /// System-prompt prefix this auth requires as the leading block on
    /// `/v1/messages`. `None` for ApiKey (no prefix needed) and for
    /// degraded OAuth without a loaded shape (we still use the
    /// hardcoded fallback below in that case).
    fn pinned_system_prefix(&self) -> Option<&str> {
        match self {
            Self::Oauth {
                shape: Some(shape), ..
            } => Some(shape.system_prefix.as_str()),
            _ => None,
        }
    }
}

/// Read both Anthropic credential slots and pick the one that wins.
/// OAuth precedence: if `anthropic-oauth` holds a token, use it (the
/// `crab` preset); otherwise fall back to `anthropic`'s Console API
/// key (the `anthropic` preset). When both are empty, returns a
/// user-readable error pointing at the Settings panel.
fn resolve_anthropic_auth() -> Result<AnthropicAuth, String> {
    let oauth = api_keys::get(ANTHROPIC_OAUTH_PROVIDER)
        .map_err(|e| format!("keychain read failed: {e}"))?;
    if let Some(t) = oauth {
        if !t.is_empty() {
            return Ok(AnthropicAuth::Oauth {
                token: t,
                shape: claude_shape::pinned(),
            });
        }
    }
    let api_key =
        api_keys::get(ANTHROPIC_PROVIDER).map_err(|e| format!("keychain read failed: {e}"))?;
    match api_key {
        Some(k) if !k.is_empty() => Ok(AnthropicAuth::ApiKey(k)),
        _ => Err(
            "no Anthropic credentials in keychain — set an API key (anthropic preset) or paste a `claude setup-token` bearer (crab preset) via Settings → Agents, or pick a 'Claude Code' engine to use the subprocess runner with your existing Pro/Max subscription"
                .to_string(),
        ),
    }
}

/// Tauri-side wire shape: rig-tagged envelope around the runtime-owned
/// [`AgentEvent`]. Mirrors the [`crate::event_bridge::RigEvent`] pattern
/// for `ArchEvent` — the runtime types stay runner-agnostic
/// (R018-shareable), and the rig context is added at the Tauri seam
/// where the renderer needs it for routing.
#[derive(Debug, Clone, Serialize)]
pub struct RigAgentEvent<'a> {
    #[serde(rename = "rigId")]
    pub rig_id: &'a RigId,
    #[serde(flatten)]
    pub event: &'a AgentEvent,
}

/// Claude-path session record. `running` holds the JoinHandle of any
/// in-flight turn; `agent_stop` aborts it. History grows by two on
/// each successful turn (user + assistant). The Anthropic API path is
/// hand-rolled (rather than going through `yah-runner`) so we can hold
/// Anthropic's native `tool_use` / `tool_result` content-block protocol
/// and `cache_control: ephemeral` markers without flattening into
/// OpenAI-shaped function-calling — see the matrix in
/// `.yah/arch/authored/yah-agent-runtime.md`.
pub struct AgentSession {
    pub id: SessionId,
    pub rig_id: RigId,
    pub ticket_id: String,
    pub engine: EngineRef,
    pub model: String,
    pub think: Option<ThinkBudget>,
    pub prelude_text: String,
    pub cache_key: String,
    pub history: Vec<Message>,
    pub running: Option<JoinHandle<()>>,
}

/// Non-Claude session record. The actual conversation state lives
/// inside [`OpenAiCompatRunner`] (the runner owns its own `SessionId
/// → InnerSession` map); this record is just the host-side companion
/// that lets us route turns back to the right runner and persist
/// events to the per-session JSONL log.
pub struct RunnerSession {
    pub id: SessionId,
    pub rig_id: RigId,
    pub ticket_id: String,
    pub engine: EngineRef,
    pub model: String,
    pub runner: Arc<OpenAiCompatRunner>,
    pub session_store: SessionStore,
    pub cache_key: String,
    pub estimated_tokens: u32,
    pub ring_depth: f32,
    pub turns: u32,
    pub running: Option<JoinHandle<()>>,
}

/// Process-wide registry of live sessions. Lives on [`AppState`] so
/// rigs detaching can sweep their orphans (a future polish — today
/// detach just leaves the sessions stale; the renderer's session list
/// is the source of truth).
///
/// Two parallel maps because the runtime shapes diverge enough — Claude
/// sessions own their history + prelude text; runner sessions delegate
/// to the runner — that one entry-type would either bloat or fragment.
/// Lookup tries Claude first (the default path), runner second.
#[derive(Default)]
pub struct AgentSessions {
    map: RwLock<HashMap<SessionId, Arc<Mutex<AgentSession>>>>,
    runner_map: RwLock<HashMap<SessionId, Arc<Mutex<RunnerSession>>>>,
    /// `claude` (PVd) preset — subprocess-driven sessions wrapping the
    /// `claude` CLI. Sister slot to [`Self::map`] / [`Self::runner_map`];
    /// kept separate so dispatch (`agent_send`, `agent_stop`) can route
    /// by lookup precedence: HA-family first, runner second, process
    /// third. Three slots not because the runtime *can't* fold them
    /// (every session is "id → state" at the abstract level), but
    /// because their state shapes diverge enough that a unified entry
    /// type would fragment more than help.
    process_map: RwLock<HashMap<SessionId, Arc<Mutex<ProcessSession>>>>,
    /// Cross-session approval queue (R031-F5). The
    /// [`TauriApprovalRouter`] inserts a pending entry when a write
    /// tool needs the user's go-ahead and awaits the oneshot; the
    /// `agent_approval_decide` Tauri command resolves it. Lives on
    /// `AgentSessions` so the renderer's reply path doesn't need a
    /// separate Tauri-managed handle. Cheap to clone — internally
    /// `Arc<AsyncMutex<HashMap<…>>>`.
    pending_approvals: PendingApprovals,
}

impl AgentSessions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Borrow the pending-approval queue. Used by
    /// [`TauriApprovalRouter`] (which inserts entries) and
    /// `agent_approval_decide` (which resolves them).
    pub fn pending_approvals(&self) -> &PendingApprovals {
        &self.pending_approvals
    }

    pub async fn insert(&self, session: AgentSession) -> Arc<Mutex<AgentSession>> {
        let id = session.id.clone();
        let arc = Arc::new(Mutex::new(session));
        self.map.write().await.insert(id, arc.clone());
        arc
    }

    pub async fn get(&self, id: &SessionId) -> Option<Arc<Mutex<AgentSession>>> {
        self.map.read().await.get(id).cloned()
    }

    pub async fn remove(&self, id: &SessionId) -> Option<Arc<Mutex<AgentSession>>> {
        self.map.write().await.remove(id)
    }

    pub async fn insert_runner(&self, session: RunnerSession) -> Arc<Mutex<RunnerSession>> {
        let id = session.id.clone();
        let arc = Arc::new(Mutex::new(session));
        self.runner_map.write().await.insert(id, arc.clone());
        arc
    }

    pub async fn get_runner(&self, id: &SessionId) -> Option<Arc<Mutex<RunnerSession>>> {
        self.runner_map.read().await.get(id).cloned()
    }

    pub async fn remove_runner(&self, id: &SessionId) -> Option<Arc<Mutex<RunnerSession>>> {
        self.runner_map.write().await.remove(id)
    }

    pub async fn insert_process(&self, session: ProcessSession) -> Arc<Mutex<ProcessSession>> {
        let id = session.id.clone();
        let arc = Arc::new(Mutex::new(session));
        self.process_map.write().await.insert(id, arc.clone());
        arc
    }

    pub async fn get_process(&self, id: &SessionId) -> Option<Arc<Mutex<ProcessSession>>> {
        self.process_map.read().await.get(id).cloned()
    }

    pub async fn remove_process(&self, id: &SessionId) -> Option<Arc<Mutex<ProcessSession>>> {
        self.process_map.write().await.remove(id)
    }

    pub async fn list_summaries(&self) -> Vec<SessionSummary> {
        let mut out = Vec::new();
        {
            let map = self.map.read().await;
            for (_, sess) in map.iter() {
                let s = sess.lock().await;
                out.push(SessionSummary {
                    session_id: s.id.clone(),
                    rig_id: s.rig_id.clone(),
                    ticket_id: s.ticket_id.clone(),
                    engine: s.engine.as_payload(),
                    turns: (s.history.len() / 2) as u32,
                    running: s.running.is_some(),
                });
            }
        }
        {
            let map = self.runner_map.read().await;
            for (_, sess) in map.iter() {
                let s = sess.lock().await;
                out.push(SessionSummary {
                    session_id: s.id.clone(),
                    rig_id: s.rig_id.clone(),
                    ticket_id: s.ticket_id.clone(),
                    engine: s.engine.as_payload(),
                    turns: s.turns,
                    running: s.running.is_some(),
                });
            }
        }
        {
            // Process sessions don't track turn counts client-side
            // (claude owns the conversation history); we report 0
            // until R028-F8 follow-up wires it from the result frame's
            // `num_turns` field.
            let map = self.process_map.read().await;
            for (_, sess) in map.iter() {
                let s = sess.lock().await;
                out.push(SessionSummary {
                    session_id: s.id.clone(),
                    rig_id: s.rig_id.clone(),
                    ticket_id: s.ticket_id.clone(),
                    engine: s.engine.as_payload(),
                    turns: 0,
                    running: s.child.is_some(),
                });
            }
        }
        out
    }
}

/// Wire DTO for [`agent_list_sessions`]. Mirrors the renderer's
/// `SessionListRow` shape (yah-ui SessionList) so the env adapter
/// can pass it through unchanged.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub rig_id: RigId,
    pub ticket_id: String,
    pub engine: String,
    pub turns: u32,
    pub running: bool,
}

/// Result of [`agent_start_session`]. Carries the prelude's metadata so
/// the renderer can show the budget gauge / engine pill without a
/// follow-up RPC. The full prelude markdown isn't returned to the
/// renderer (it's already in the system prompt); UI inspection lives
/// behind a separate `agent_get_prelude` if/when the user asks.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartSessionResult {
    pub session_id: SessionId,
    pub ticket_id: String,
    pub engine: String,
    pub model: String,
    pub cache_key: String,
    pub estimated_tokens: u32,
    pub ring_depth: f32,
    pub truncated: bool,
}

// ---------- Tauri commands ----------

/// Open a session for `ticket_id` on `rig_id`. Idempotent at the
/// (rig, ticket) level: opening the same ticket twice returns two
/// independent sessions — the prelude assembly cost is cheap enough
/// that we don't share, and per-session history is the desired
/// affordance (each chat is its own context fork).
///
/// Dispatches by `prelude.engine.provider`:
/// - `claude` (or no engine — default) → in-tree Claude SDK runner.
/// - `openai` → yah-runner OpenAI-compat backend keyed on the `openai`
///   keychain slot. Plus accounts must paste an `sk-…` key from
///   platform.openai.com — Plus subscription doesn't include API
///   access on its own.
/// - `ollama` → yah-runner OpenAI-compat backend. With an `ollama` key
///   in the keychain we point at Ollama Cloud; without one we fall
///   back to local `ollama serve` on loopback.
/// - anything else → error pointing the user at engine docs.
#[tauri::command]
pub async fn agent_start_session(
    state: tauri::State<'_, AppState>,
    sessions: tauri::State<'_, AgentSessions>,
    app: AppHandle,
    rig_id: RigId,
    ticket_id: String,
) -> Result<StartSessionResult, String> {
    let svc = state
        .svc_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} not attached", rig_id.as_str()))?;
    let result = svc
        .assemble_prelude(AssemblePreludeParams::new(&ticket_id))
        .await;
    let prelude = result
        .prelude
        .ok_or_else(|| format!("ticket {} not on the board", ticket_id))?;

    let engine = prelude.engine.clone().unwrap_or_else(|| EngineRef {
        provider: "claude".into(),
        model: None,
    });

    match engine.provider.as_str() {
        // HA-family presets — `anthropic` (HAk, sk-ant-…) and `crab`
        // (HAo, OAuth bearer). `claude` historically routed here too;
        // accepting it preserves day-one settings for users on the
        // HTTP path until the R028-F8 rename lands. The architecture
        // doc reserves `claude` for the PVd preset (subprocess); the
        // explicit `claude-cli` alias below is the path forward.
        "claude" | "anthropic" | "crab" => {
            start_claude_session(&sessions, &app, rig_id, ticket_id, engine, prelude).await
        }
        // PVd preset — wraps the `claude` CLI as a subprocess. Auth is
        // delegated entirely (Claude Code manages its own OAuth /
        // login). README-recommended Anthropic default; lives behind
        // an explicit `claude-cli` engine name today so the existing
        // `claude` → HTTP routing stays a one-line edit when we swap
        // the default over.
        "claude-cli" => {
            let rig_root = state
                .path_for(&rig_id)
                .await
                .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
            agent_process::start_process_session(
                &sessions, &app, rig_id, ticket_id, engine, prelude, &rig_root,
            )
            .await
        }
        "openai" | "ollama" => {
            let rig_root = state
                .path_for(&rig_id)
                .await
                .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
            start_runner_session(
                &sessions, &app, rig_id, ticket_id, engine, prelude, &rig_root, svc,
            )
            .await
        }
        other => Err(format!(
            "unsupported engine provider '{other}' — known providers: claude, anthropic, crab, claude-cli, openai, ollama (see .yah/arch/authored/yah-agent-runtime.md)",
        )),
    }
}

async fn start_claude_session(
    sessions: &AgentSessions,
    app: &AppHandle,
    rig_id: RigId,
    ticket_id: String,
    engine: EngineRef,
    prelude: Prelude,
) -> Result<StartSessionResult, String> {
    let session_id = mint_session_id();
    let prelude_text = prelude.render();
    let cache_key = prelude.cache.key.clone();
    let estimated_tokens = prelude.estimated_tokens;
    let ring_depth = prelude.ring_depth;
    let truncated = prelude.truncated;
    let model = engine
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.to_string());

    let session = AgentSession {
        id: session_id.clone(),
        rig_id: rig_id.clone(),
        ticket_id: ticket_id.clone(),
        engine: engine.clone(),
        model: model.clone(),
        think: prelude.think,
        prelude_text,
        cache_key: cache_key.clone(),
        history: Vec::new(),
        running: None,
    };
    sessions.insert(session).await;

    emit_event(
        app,
        &rig_id,
        &AgentEvent::SessionStarted {
            session_id: session_id.clone(),
            ticket_id: ticket_id.clone(),
            engine: engine.as_payload(),
            cache_key: cache_key.clone(),
            estimated_tokens,
            ring_depth,
        },
    );

    Ok(StartSessionResult {
        session_id,
        ticket_id,
        engine: engine.as_payload(),
        model,
        cache_key,
        estimated_tokens,
        ring_depth,
        truncated,
    })
}

async fn start_runner_session(
    sessions: &AgentSessions,
    app: &AppHandle,
    rig_id: RigId,
    ticket_id: String,
    engine: EngineRef,
    prelude: Prelude,
    rig_root: &Path,
    svc: Arc<KgService>,
) -> Result<StartSessionResult, String> {
    let cache_key = prelude.cache.key.clone();
    let estimated_tokens = prelude.estimated_tokens;
    let ring_depth = prelude.ring_depth;
    let truncated = prelude.truncated;

    let config = build_openai_compat_config(&engine)?;
    // Per-rig read-only tool registry (R031-F1). Hands the runner an
    // `Arc<dyn ToolRegistry>` whose tools all close over this rig's
    // `KgService` and on-disk root. P2 will use the schemas to populate
    // the request body's `tools[]` and dispatch `tool_calls[]` deltas
    // back through the same registry.
    let tool_ctx = ToolContext {
        rig_id: rig_id.clone(),
        rig_root: rig_root.to_path_buf(),
        svc: Arc::clone(&svc),
    };
    // R031-F5: wire the per-rig approval store + the Tauri-shaped
    // router. The store is shared with the Settings UI commands
    // (`agent_approval_rules_*`) so rule edits there take effect on
    // the next tool call without a session restart. The router emits
    // `ApprovalRequested` on the agent:event channel and awaits the
    // user's reply via `agent_approval_decide`. With no writers in
    // the standard registry every call still hits the read-only Auto
    // path; the wiring is in place for the F4 writer flip + F2
    // OpenAI tool-dispatch loop.
    let approval_store: Arc<dyn ApprovalStore> =
        Arc::new(FileApprovalStore::load_or_empty(rig_root));
    let approval_router: Arc<dyn ApprovalRouter> = Arc::new(TauriApprovalRouter {
        pending: sessions.pending_approvals().clone(),
        app: app.clone(),
        rig_id: rig_id.clone(),
    });
    /* Hold an Arc<KgToolRegistry> alongside the dyn-trait Arc the
    runner needs. After the runner mints its session id we call
    `bind_session` on the concrete registry — interior mutability
    is the price for the runner being session-id-agnostic and the
    registry being per-rig (one registry, many sessions in the
    future). */
    // Per-rig opt-in flag for the experimental writer surface (R031-F5
    // production flip). Default is `false`, so a fresh rig stays
    // read-only until the operator flips the toggle in
    // Settings → Agents. The approval gate is wired either way; it's
    // dormant in the read-only configuration because no tool ever
    // surfaces `is_write() = true`.
    let writers_enabled = crate::agent_settings::load_or_default(rig_root).agent_writers_enabled();
    let base = if writers_enabled {
        KgToolRegistry::with_experimental_writers(tool_ctx)
    } else {
        KgToolRegistry::standard_read_only(tool_ctx)
    };
    let registry_concrete = Arc::new(
        base.with_store(Arc::clone(&approval_store))
            .with_router(Arc::clone(&approval_router)),
    );
    let registry_dyn: Arc<dyn ToolRegistry> =
        Arc::clone(&registry_concrete) as Arc<dyn ToolRegistry>;
    let runner = Arc::new(OpenAiCompatRunner::with_tools(config.clone(), registry_dyn));

    let session_id = runner
        .start(prelude, &ticket_id)
        .await
        .map_err(|e| format!("runner start failed: {e}"))?;
    registry_concrete.bind_session(session_id.clone());

    let model = engine
        .model
        .clone()
        .unwrap_or_else(|| config.default_model.clone());
    let session_store = SessionStore::new(rig_root);

    let runner_session = RunnerSession {
        id: session_id.clone(),
        rig_id: rig_id.clone(),
        ticket_id: ticket_id.clone(),
        engine: engine.clone(),
        model: model.clone(),
        runner: Arc::clone(&runner),
        session_store: session_store.clone(),
        cache_key: cache_key.clone(),
        estimated_tokens,
        ring_depth,
        turns: 0,
        running: None,
    };
    sessions.insert_runner(runner_session).await;

    let session_started = AgentEvent::SessionStarted {
        session_id: session_id.clone(),
        ticket_id: ticket_id.clone(),
        engine: engine.as_payload(),
        cache_key: cache_key.clone(),
        estimated_tokens,
        ring_depth,
    };
    // Persist before emit so a crash mid-emit still has the header.
    if let Err(e) = session_store.append(&session_id, &session_started).await {
        tracing::warn!(error = %e, "session log: SessionStarted append failed");
    }
    emit_event(app, &rig_id, &session_started);

    Ok(StartSessionResult {
        session_id,
        ticket_id,
        engine: engine.as_payload(),
        model,
        cache_key,
        estimated_tokens,
        ring_depth,
        truncated,
    })
}

/// Open an unanchored chat session — no ticket lookup, just a small
/// "you're working in this rig" prelude. Same dispatch matrix as
/// [`agent_start_session`], just builds the prelude in-process instead
/// of going through `arch.assemble_prelude`.
///
/// `engine` accepts either a bare provider (`"claude"`, `"openai"`,
/// `"ollama"`) or a `provider:model` form (`"openai:gpt-4o"`). An
/// explicit `model` argument overrides any model in the engine string.
#[tauri::command]
pub async fn agent_start_chat_session(
    state: tauri::State<'_, AppState>,
    sessions: tauri::State<'_, AgentSessions>,
    app: AppHandle,
    rig_id: RigId,
    engine: String,
    model: Option<String>,
) -> Result<StartSessionResult, String> {
    let rig_path = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} not attached", rig_id.as_str()))?;
    let svc = state
        .svc_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} not attached", rig_id.as_str()))?;
    let rig_name = rig_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| rig_id.as_str().to_string());

    let engine_ref = parse_engine_payload(&engine, model.as_deref());
    let prelude = build_chat_prelude(&rig_name, Some(engine_ref.clone()));
    let ticket_id = "chat".to_string();

    match engine_ref.provider.as_str() {
        "claude" | "anthropic" | "crab" => {
            start_claude_session(&sessions, &app, rig_id, ticket_id, engine_ref, prelude).await
        }
        "claude-cli" => {
            agent_process::start_process_session(
                &sessions, &app, rig_id, ticket_id, engine_ref, prelude, &rig_path,
            )
            .await
        }
        "openai" | "ollama" => {
            start_runner_session(
                &sessions, &app, rig_id, ticket_id, engine_ref, prelude, &rig_path, svc,
            )
            .await
        }
        other => Err(format!(
            "unsupported engine provider '{other}' — known providers: claude, anthropic, crab, claude-cli, openai, ollama",
        )),
    }
}

/// Parse the renderer's engine string. Accepts:
/// - bare provider: `"openai"`
/// - provider+model: `"openai:gpt-4o"`
///
/// An explicit `model` override always wins; otherwise the model in the
/// payload (if any) is used. The runner's default model fills in last.
fn parse_engine_payload(engine: &str, model_override: Option<&str>) -> EngineRef {
    let trimmed = engine.trim();
    let (provider, payload_model) = match trimmed.split_once(':') {
        Some((p, m)) if !m.is_empty() => (p.to_string(), Some(m.to_string())),
        _ => (trimmed.to_string(), None),
    };
    EngineRef {
        provider,
        model: model_override.map(String::from).or(payload_model),
    }
}

/// Build the chat-mode prelude. A small "you are in rig X" header
/// plus the shared yah:// output-conventions section, so unanchored
/// chat and ticket-anchored sessions teach the same convention.
fn build_chat_prelude(rig_name: &str, engine: Option<EngineRef>) -> Prelude {
    use kg::prelude::{
        output_conventions_section, CacheControl, CacheTtl, PreludeSection, PreludeSectionKind,
    };

    let header = format!(
        "# yah chat\n\n\
         You are an agent assisting the user in the **{rig_name}** workspace. \
         This is an unanchored chat — no ticket, relay, or document is attached.\n\n\
         Use it for general questions, brainstorming, codebase orientation, \
         or quick checks. If the user wants focused work on something specific, \
         they can attach a ticket from the board (or open an arch-doc session \
         once that lands)."
    );
    let sections = vec![
        PreludeSection {
            kind: PreludeSectionKind::Chat,
            markdown: header,
        },
        output_conventions_section(),
    ];
    let combined = render_sections(&sections);
    let estimated_tokens = ((combined.len() as f32) / 4.0).ceil() as u32;
    let mut hasher = blake3::Hasher::new();
    hasher.update(combined.as_bytes());
    let hash = hasher.finalize();
    let cache_key: String = hash
        .as_bytes()
        .iter()
        .take(16)
        .map(|b| format!("{:02x}", b))
        .collect();
    Prelude {
        sections,
        cache: CacheControl {
            key: cache_key,
            ttl: CacheTtl::Ephemeral,
        },
        engine,
        think: None,
        estimated_tokens,
        ring_depth: (estimated_tokens as f32) / 200_000.0,
        truncated: false,
    }
}

/// Concatenate sections the same way `Prelude::render()` does, for the
/// chat-mode builder which assembles its `Prelude` by hand instead of
/// going through `prelude::assemble`.
fn render_sections(sections: &[kg::prelude::PreludeSection]) -> String {
    let mut out = String::new();
    for (i, s) in sections.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&s.markdown);
        if !s.markdown.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

/// Map `engine.provider` → an [`OpenAiCompatConfig`], reading the
/// matching keychain slot. Returns a user-readable error for the
/// renderer's toast surface when a key is missing.
fn build_openai_compat_config(engine: &EngineRef) -> Result<OpenAiCompatConfig, String> {
    match engine.provider.as_str() {
        "openai" => {
            let key = api_keys::get(OPENAI_PROVIDER)
                .map_err(|e| format!("keychain read failed: {e}"))?
                .ok_or_else(|| {
                    "no OpenAI API key in keychain — paste one from \
                     platform.openai.com/api-keys via Settings → API Keys. \
                     ChatGPT Plus does not include API access; you'll need \
                     a separately-billed key."
                        .to_string()
                })?;
            Ok(OpenAiCompatConfig::openai(key))
        }
        "ollama" => match api_keys::get(OLLAMA_PROVIDER)
            .map_err(|e| format!("keychain read failed: {e}"))?
        {
            Some(key) => Ok(OpenAiCompatConfig::ollama_cloud(key)),
            None => Ok(OpenAiCompatConfig::ollama_local()),
        },
        other => Err(format!(
            "build_openai_compat_config: unexpected provider '{other}'"
        )),
    }
}

/// Append a user turn and stream the assistant's reply. Returns
/// immediately — the actual streaming happens in a spawned task that
/// emits [`AgentEvent`]s on the `agent:event` channel.
#[tauri::command]
pub async fn agent_send(
    sessions: tauri::State<'_, AgentSessions>,
    app: AppHandle,
    session_id: SessionId,
    text: String,
) -> Result<(), String> {
    if let Some(session) = sessions.get(&session_id).await {
        return send_claude(&app, session, session_id, text).await;
    }
    if let Some(session) = sessions.get_runner(&session_id).await {
        return send_runner(&app, session, session_id, text).await;
    }
    if let Some(session) = sessions.get_process(&session_id).await {
        return agent_process::send_process(&app, session, session_id, text).await;
    }
    Err(format!("session {} not found", session_id.as_str()))
}

async fn send_claude(
    app: &AppHandle,
    session: Arc<Mutex<AgentSession>>,
    session_id: SessionId,
    text: String,
) -> Result<(), String> {
    // OAuth slot wins when both are populated — see resolve_anthropic_auth.
    let auth = resolve_anthropic_auth()?;

    {
        let mut s = session.lock().await;
        if let Some(prev) = s.running.take() {
            // Cooperative supersede: a second send before the previous
            // turn finished aborts the in-flight stream rather than
            // queueing — matches Claude Code / Cursor behaviour and
            // keeps the user from accidentally racing two responses.
            prev.abort();
        }
        s.history.push(Message {
            role: Role::User,
            content: text,
        });
    }

    let app_clone = app.clone();
    let session_clone = session.clone();
    let session_id_for_task = session_id.clone();
    let rig_id_for_task = session.lock().await.rig_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let outcome = run_anthropic_turn(&app_clone, &session_clone, &auth).await;
        if let Err(message) = outcome {
            emit_event(
                &app_clone,
                &rig_id_for_task,
                &AgentEvent::Error {
                    session_id: session_id_for_task.clone(),
                    message,
                },
            );
        }
        // Clear the running handle after the task settles. Best-effort:
        // a `stop` racing with completion will already have cleared it.
        let mut s = session_clone.lock().await;
        s.running = None;
    });

    session.lock().await.running = Some(handle);
    Ok(())
}

async fn send_runner(
    app: &AppHandle,
    session: Arc<Mutex<RunnerSession>>,
    session_id: SessionId,
    text: String,
) -> Result<(), String> {
    let (runner, store, rig_id) = {
        let mut s = session.lock().await;
        if let Some(prev) = s.running.take() {
            prev.abort();
        }
        (
            Arc::clone(&s.runner),
            s.session_store.clone(),
            s.rig_id.clone(),
        )
    };

    let app_clone = app.clone();
    let session_clone = session.clone();
    let session_id_for_task = session_id.clone();
    let handle = tauri::async_runtime::spawn(async move {
        let stream = match runner.send(&session_id_for_task, text).await {
            Ok(s) => s,
            Err(e) => {
                let ev = AgentEvent::Error {
                    session_id: session_id_for_task.clone(),
                    message: format!("{e}"),
                };
                if let Err(persist_err) = store.append(&session_id_for_task, &ev).await {
                    tracing::warn!(error = %persist_err, "session log: Error append failed");
                }
                emit_event(&app_clone, &rig_id, &ev);
                let mut s = session_clone.lock().await;
                s.running = None;
                return;
            }
        };
        let mut tapped = tap_stream(store, session_id_for_task.clone(), stream);
        while let Some(ev) = tapped.next().await {
            emit_event(&app_clone, &rig_id, &ev);
        }
        let mut s = session_clone.lock().await;
        s.turns = s.turns.saturating_add(1);
        s.running = None;
    });

    session.lock().await.running = Some(handle);
    Ok(())
}

/// Abort any in-flight turn and drop the session. Idempotent.
#[tauri::command]
pub async fn agent_stop(
    sessions: tauri::State<'_, AgentSessions>,
    app: AppHandle,
    session_id: SessionId,
) -> Result<bool, String> {
    if let Some(session) = sessions.remove(&session_id).await {
        let rig_id = {
            let mut s = session.lock().await;
            if let Some(handle) = s.running.take() {
                handle.abort();
            }
            s.rig_id.clone()
        };
        emit_event(
            &app,
            &rig_id,
            &AgentEvent::SessionEnded {
                session_id: session_id.clone(),
            },
        );
        return Ok(true);
    }
    if let Some(session) = sessions.remove_runner(&session_id).await {
        let (rig_id, runner) = {
            let mut s = session.lock().await;
            if let Some(handle) = s.running.take() {
                handle.abort();
            }
            (s.rig_id.clone(), Arc::clone(&s.runner))
        };
        let _ = runner.stop(&session_id).await;
        emit_event(
            &app,
            &rig_id,
            &AgentEvent::SessionEnded {
                session_id: session_id.clone(),
            },
        );
        return Ok(true);
    }
    if let Some(session) = sessions.remove_process(&session_id).await {
        let rig_id = { session.lock().await.rig_id.clone() };
        if let Err(e) = agent_process::stop_process(session).await {
            tracing::warn!(error = %e, "process session stop failed");
        }
        emit_event(
            &app,
            &rig_id,
            &AgentEvent::SessionEnded {
                session_id: session_id.clone(),
            },
        );
        return Ok(true);
    }
    Ok(false)
}

/// Catalogue of models the renderer can offer in the chat picker.
/// Hits each provider's `/v1/models` (Anthropic uses its own endpoint
/// shape but follows the same `data[].id` convention). Sorted, deduped.
///
/// Reads the same keychain slot the runtime would use to start a
/// session — so a missing key surfaces here before the user picks a
/// model and tries to send a turn.
#[tauri::command]
pub async fn agent_list_models(provider: String) -> Result<Vec<String>, String> {
    match provider.as_str() {
        "openai" => {
            let key = api_keys::get(OPENAI_PROVIDER)
                .map_err(|e| format!("keychain read failed: {e}"))?
                .ok_or_else(|| {
                    "no OpenAI API key in keychain — set one in Settings → Agents".to_string()
                })?;
            list_openai_compat_models("https://api.openai.com/v1/chat/completions", Some(&key))
                .await
                .map_err(|e| e.to_string())
        }
        "ollama" => {
            // Cloud + key when the keychain has one, local fallback
            // otherwise — same dispatch the runtime uses.
            let key =
                api_keys::get(OLLAMA_PROVIDER).map_err(|e| format!("keychain read failed: {e}"))?;
            let (endpoint, key_ref): (&str, Option<&str>) = match key.as_deref() {
                Some(k) => ("https://ollama.com/v1/chat/completions", Some(k)),
                None => ("http://localhost:11434/v1/chat/completions", None),
            };
            list_openai_compat_models(endpoint, key_ref)
                .await
                .map_err(|e| e.to_string())
        }
        "claude" | "claude-cli" => {
            // Anthropic's `/v1/models` is the same wire as `/v1/messages`
            // for auth purposes — works with `x-api-key` (the
            // `anthropic` preset) and `Authorization: Bearer` (the
            // `crab` preset). Hand-rolled because the OpenAI-compat
            // helper assumes bearer-only semantics.
            //
            // The `claude-cli` (PVd) preset shares this catalogue — the
            // subprocess accepts `--model <id>` for any Anthropic
            // model, and the user's `claude` CLI uses its own auth to
            // call the actual API. We piggyback on whatever HA-family
            // key the user has configured to fetch the list; if no
            // Anthropic key is present we return a small hardcoded
            // fallback so the dropdown still renders.
            let auth = match resolve_anthropic_auth() {
                Ok(a) => a,
                Err(_) if provider == "claude-cli" => {
                    return Ok(vec![
                        "claude-opus-4-7".into(),
                        "claude-sonnet-4-6".into(),
                        "claude-haiku-4-5".into(),
                    ]);
                }
                Err(e) => return Err(e),
            };
            let req = reqwest::Client::new().get("https://api.anthropic.com/v1/models");
            let resp = auth
                .apply(req)
                .send()
                .await
                .map_err(|e| format!("models request failed: {e}"))?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!(
                    "Anthropic models endpoint returned {}: {}",
                    status,
                    body.chars().take(256).collect::<String>(),
                ));
            }
            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| format!("models response parse failed: {e}"))?;
            let mut out: Vec<String> = json
                .get("data")
                .and_then(|d| d.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            out.sort();
            out.dedup();
            Ok(out)
        }
        other => Err(format!(
            "unsupported provider '{other}' — known: claude, claude-cli, openai, ollama",
        )),
    }
}

/// Snapshot of every live session — used by the AgentView's session
/// list rail to enumerate panes across rigs.
#[tauri::command]
pub async fn agent_list_sessions(
    sessions: tauri::State<'_, AgentSessions>,
) -> Result<Vec<SessionSummary>, String> {
    Ok(sessions.list_summaries().await)
}

// ---------- Approval gate (R031-F5) ----------

/// Bridge between [`KgToolRegistry::execute_gated`]'s prompt path
/// and the renderer's chat pane. Inserts a pending entry in
/// [`AgentSessions::pending_approvals`], emits an
/// [`AgentEvent::ApprovalRequested`] on the `agent:event` channel,
/// and awaits the user's reply on the oneshot.
///
/// The renderer fans out to an inline approval row in `useChatSession`
/// and posts the user's choice via `agent_approval_decide`, which
/// resolves the oneshot. The await unblocks and the gate proceeds
/// (allow → execute, skip → fail with `approval_skipped`,
/// always_allow → push rule + execute).
struct TauriApprovalRouter {
    pending: PendingApprovals,
    app: AppHandle,
    rig_id: RigId,
}

#[async_trait]
impl ApprovalRouter for TauriApprovalRouter {
    async fn request(&self, request: ApprovalRequest) -> ApprovalChoice {
        let rx = self.pending.register(request.request_id.clone()).await;
        emit_event(
            &self.app,
            &self.rig_id,
            &AgentEvent::ApprovalRequested {
                session_id: request.session_id.clone(),
                request_id: request.request_id.clone(),
                tool_name: request.tool_name.clone(),
                args: request.args.clone(),
                bash: request
                    .bash
                    .as_ref()
                    .and_then(|b| serde_json::to_value(b).ok()),
            },
        );
        match rx.await {
            Ok(choice) => choice,
            // Sender dropped without resolving — the session was
            // stopped or the host is shutting down. Surface as Skip
            // so the gate fails cleanly instead of hanging.
            Err(_) => ApprovalChoice::Skip,
        }
    }
}

/// User's reply to an inline approval prompt. The renderer posts the
/// choice through this command; we resolve the matching oneshot in
/// [`PendingApprovals`] and the gate's await unblocks.
///
/// Returns `true` if the request id was still pending; `false` when
/// it had already been resolved or never registered (renderer
/// double-click, session closed mid-prompt). Idempotent on the
/// second-click path.
#[tauri::command]
pub async fn agent_approval_decide(
    sessions: tauri::State<'_, AgentSessions>,
    app: AppHandle,
    rig_id: RigId,
    session_id: SessionId,
    request_id: String,
    choice: ApprovalChoice,
) -> Result<bool, String> {
    let decision_label = match &choice {
        ApprovalChoice::Apply => "apply",
        ApprovalChoice::Skip => "skip",
        ApprovalChoice::AlwaysAllow { .. } => "always-allow",
    };
    let resolved = sessions
        .pending_approvals()
        .resolve(&request_id, choice)
        .await;
    // Emit ApprovalResolved regardless — the renderer uses it to drop
    // the inline approval row, and a no-op resolve still wants the UI
    // to reconcile (the row already disappeared on the originating
    // pane; this fires across panes).
    emit_event(
        &app,
        &rig_id,
        &AgentEvent::ApprovalResolved {
            session_id,
            request_id,
            decision: decision_label.to_string(),
        },
    );
    Ok(resolved)
}

/// Read the rules file for `rig_id`. Returns the same JSON shape the
/// store persists, so the Settings UI can dispatch on the
/// `version` tag and render rule-list rows from `rules[]`. A fresh
/// rig with no file returns an empty ruleset.
#[tauri::command]
pub async fn agent_approval_rules_list(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<serde_json::Value, String> {
    let rig_root = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
    let store = FileApprovalStore::load_or_empty(&rig_root);
    serde_json::to_value(store.snapshot())
        .map_err(|e| format!("approval rules serialize failed: {e}"))
}

/// Append a rule to `rig_id`'s rules file. Idempotent —
/// [`ApprovalStore::push`] de-dupes equal rules. Returns the updated
/// rule list so the Settings UI can re-render without a follow-up
/// list call.
#[tauri::command]
pub async fn agent_approval_rules_add(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    rule: ApprovalRule,
) -> Result<serde_json::Value, String> {
    let rig_root = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
    let store = FileApprovalStore::load_or_empty(&rig_root);
    store.push(rule);
    serde_json::to_value(store.snapshot())
        .map_err(|e| format!("approval rules serialize failed: {e}"))
}

/// Remove the rule at `index` (0-based) from `rig_id`'s rules file.
/// Out-of-range is a no-op, matching the Settings UI's "delete this
/// row" semantics — a stale index from a concurrent edit shouldn't
/// throw. Returns the updated rule list.
#[tauri::command]
pub async fn agent_approval_rules_remove(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    index: usize,
) -> Result<serde_json::Value, String> {
    let rig_root = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
    let store = FileApprovalStore::load_or_empty(&rig_root);
    store.remove_at(index);
    serde_json::to_value(store.snapshot())
        .map_err(|e| format!("approval rules serialize failed: {e}"))
}

/// Read the per-rig agent settings blob (today: just
/// `agent_writers_enabled`). A fresh rig with no file returns the
/// default settings — writers disabled.
#[tauri::command]
pub async fn agent_settings_get(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
) -> Result<serde_json::Value, String> {
    let rig_root = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
    let settings = crate::agent_settings::load_or_default(&rig_root);
    serde_json::to_value(settings).map_err(|e| format!("agent settings serialize failed: {e}"))
}

/// Write the per-rig agent settings blob. Takes effect on the next
/// session start — already-running sessions keep the registry shape
/// they were minted with. Returns the updated settings so the UI can
/// reconcile without a follow-up `_get`.
#[tauri::command]
pub async fn agent_settings_set(
    state: tauri::State<'_, AppState>,
    rig_id: RigId,
    settings: crate::agent_settings::AgentSettings,
) -> Result<serde_json::Value, String> {
    let rig_root = state
        .path_for(&rig_id)
        .await
        .ok_or_else(|| format!("rig {} has no on-disk path", rig_id.as_str()))?;
    crate::agent_settings::save(&rig_root, &settings)
        .map_err(|e| format!("agent settings save failed: {e}"))?;
    serde_json::to_value(&settings).map_err(|e| format!("agent settings serialize failed: {e}"))
}

// ---------- Anthropic streaming ----------

/// Whether the given Anthropic model id supports extended thinking.
/// Extended thinking shipped with `claude-3-7-sonnet` (Feb 2025) and
/// every `claude-4-*` family member ships with it on. Older 3.5/3.0
/// models — including the original Haiku 3 family — silently 400 on
/// the `thinking` field, so we omit it for them.
fn model_supports_thinking(model: &str) -> bool {
    let m = model.to_ascii_lowercase();
    m.starts_with("claude-opus-4")
        || m.starts_with("claude-sonnet-4")
        || m.starts_with("claude-haiku-4")
        || m.starts_with("claude-3-7")
}

/// Translate a [`ThinkBudget`] tier into the `budget_tokens` value
/// Anthropic's `/v1/messages` expects. Tiers are deliberately coarse
/// — a workspace-default-model setting (R027 follow-up) can refine
/// the numeric mapping per model later. `Budget { tokens }` is the
/// escape hatch for callers who need an explicit cap; we still clamp
/// up to Anthropic's documented minimum so the API doesn't 400.
fn think_budget_tokens(budget: &ThinkBudget) -> u32 {
    match budget {
        ThinkBudget::Deep => 16_000,
        ThinkBudget::Standard => 4_000,
        ThinkBudget::Fast => ANTHROPIC_THINKING_MIN_BUDGET,
        ThinkBudget::Budget { tokens } => (*tokens).max(ANTHROPIC_THINKING_MIN_BUDGET),
    }
}

/// Assemble the JSON body for a single `/v1/messages` turn. Pulled out
/// of [`run_anthropic_turn`] so the `@yah:think` mapping (and the
/// model-thinking-capability check) can be unit-tested without standing
/// up an HTTP server.
///
/// When `session.think` is `Some(_)` and the chosen model supports
/// extended thinking, the body grows a `thinking: { type: "enabled",
/// budget_tokens: N }` field and `max_tokens` is bumped by `N` so the
/// model has headroom for both the thinking trace *and* the actual
/// output (Anthropic requires `max_tokens > budget_tokens`). Otherwise
/// the body matches the pre-think baseline.
fn build_anthropic_body(session: &AgentSession, auth: &AnthropicAuth) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = session
        .history
        .iter()
        .map(|m| {
            serde_json::json!({
                "role": match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                },
                "content": m.content,
            })
        })
        .collect();

    // OAuth-authed requests must lead with the Claude Code identity
    // prefix; Anthropic enforces that on the `claude-code-20250219`
    // beta. Our prelude rides as a *second* system block — appending
    // instructions after the prefix is allowed by the contract, and
    // splitting them into separate blocks lets each carry its own
    // `cache_control: ephemeral` so the long-lived prelude keeps its
    // cache hit even though the prefix is short.
    //
    // Prefer the pinned-shape prefix when a [`PinnedShape`] is loaded
    // (the audit artifact captures Anthropic's enforced literal); fall
    // back to the hardcoded copy on degraded OAuth (no shape) or when
    // the API-key path runs.
    let system_blocks: Vec<serde_json::Value> = match auth {
        AnthropicAuth::Oauth { .. } => {
            let prefix = auth
                .pinned_system_prefix()
                .filter(|s| !s.is_empty())
                .unwrap_or(ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX);
            vec![
                serde_json::json!({
                    "type": "text",
                    "text": prefix,
                    "cache_control": { "type": "ephemeral" },
                }),
                serde_json::json!({
                    "type": "text",
                    "text": session.prelude_text,
                    "cache_control": { "type": "ephemeral" },
                }),
            ]
        }
        AnthropicAuth::ApiKey(_) => vec![serde_json::json!({
            "type": "text",
            "text": session.prelude_text,
            "cache_control": { "type": "ephemeral" },
        })],
    };

    let mut body = serde_json::json!({
        "model": session.model,
        "max_tokens": DEFAULT_MAX_OUTPUT_TOKENS,
        "stream": true,
        "system": system_blocks,
        "messages": messages,
    });

    if let Some(budget) = session.think.as_ref() {
        if model_supports_thinking(&session.model) {
            let budget_tokens = think_budget_tokens(budget);
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": budget_tokens,
            });
            // max_tokens must exceed budget_tokens — give the model
            // DEFAULT_MAX_OUTPUT_TOKENS of post-thinking output room.
            body["max_tokens"] =
                serde_json::Value::from(budget_tokens.saturating_add(DEFAULT_MAX_OUTPUT_TOKENS));
        }
    }
    body
}

/// Merge Anthropic's `usage` JSON object into a running [`TurnUsage`]
/// accumulator. Anthropic splits the numbers across two SSE events:
/// `message_start.message.usage` carries `input_tokens` +
/// `cache_read_input_tokens` + `cache_creation_input_tokens` (and a
/// placeholder `output_tokens` ≈ 1), while each `message_delta.usage`
/// carries the running cumulative `output_tokens`. We let later
/// values overwrite earlier ones — that means the final
/// `message_delta.usage.output_tokens` wins, as desired.
///
/// Pulled out of [`run_anthropic_turn`] so the merge logic can be
/// unit-tested without standing up an HTTP server.
fn merge_anthropic_usage(acc: &mut TurnUsage, usage: &serde_json::Value) {
    let read_u32 = |k: &str| usage.get(k).and_then(|v| v.as_u64()).map(|n| n as u32);
    if let Some(v) = read_u32("input_tokens") {
        acc.input_tokens = Some(v);
    }
    if let Some(v) = read_u32("output_tokens") {
        acc.output_tokens = Some(v);
    }
    if let Some(v) = read_u32("cache_read_input_tokens") {
        acc.cache_read_input_tokens = Some(v);
    }
    if let Some(v) = read_u32("cache_creation_input_tokens") {
        acc.cache_creation_input_tokens = Some(v);
    }
}

/// Drive one turn against `/v1/messages` with `stream: true`. Folds SSE
/// chunks into [`AgentEvent`]s. On success, appends the assistant turn
/// to the session's history. `auth` selects between the
/// `anthropic` (HAk, `x-api-key`) and `crab` (HAo, `Authorization:
/// Bearer` + `anthropic-beta`) presets via [`AnthropicAuth::apply`].
async fn run_anthropic_turn(
    app: &AppHandle,
    session: &Arc<Mutex<AgentSession>>,
    auth: &AnthropicAuth,
) -> Result<(), String> {
    let (session_id, rig_id, body) = {
        let s = session.lock().await;
        (
            s.id.clone(),
            s.rig_id.clone(),
            build_anthropic_body(&s, auth),
        )
    };

    emit_event(
        app,
        &rig_id,
        &AgentEvent::TurnStarted {
            session_id: session_id.clone(),
        },
    );

    let client = reqwest::Client::new();
    let req = client
        .post(ANTHROPIC_MESSAGES_URL)
        .header("content-type", "application/json")
        .json(&body);
    let resp = auth
        .apply(req)
        .send()
        .await
        .map_err(|e| format!("Anthropic request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Anthropic returned {}: {}",
            status,
            body.chars().take(512).collect::<String>()
        ));
    }

    // Once we're inside the stream loop, any failure carries the
    // partial accumulator and emits as `TurnFailed` (symmetric to the
    // success-path `TurnEnded`). Pre-stream failures above this point
    // return `Err` so the caller emits `Error` — there's no
    // accumulator to flush.
    let mut accumulated = String::new();
    let mut stop_reason: Option<String> = None;
    let mut total_bytes: usize = 0;
    let mut buffer = String::new();
    // Anthropic emits usage twice: input + cache numbers ride
    // `message_start.message.usage` (with output_tokens=1 baseline);
    // the running `output_tokens` rides each `message_delta.usage`
    // until end-of-stream. We accumulate both into one struct.
    let mut usage_acc: TurnUsage = TurnUsage::default();
    let mut have_usage = false;

    macro_rules! turn_failed {
        ($message:expr) => {{
            emit_event(
                app,
                &rig_id,
                &AgentEvent::TurnFailed {
                    session_id: session_id.clone(),
                    text: accumulated.clone(),
                    message: $message,
                },
            );
            return Ok(());
        }};
    }

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let bytes = match chunk {
            Ok(b) => b,
            Err(e) => turn_failed!(format!("stream read failed: {}", e)),
        };
        total_bytes = total_bytes.saturating_add(bytes.len());
        if total_bytes > MAX_STREAM_BYTES {
            turn_failed!(format!(
                "stream exceeded soft cap of {} bytes — aborting",
                MAX_STREAM_BYTES
            ));
        }
        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(e) => turn_failed!(format!("non-utf8 chunk: {}", e)),
        };
        buffer.push_str(text);

        // SSE frames are `data: {...}\n\n`-separated. Pull complete
        // frames out of the buffer; leave any half-frame for the next
        // iteration.
        while let Some(idx) = buffer.find("\n\n") {
            let frame = buffer[..idx].to_string();
            buffer.drain(..idx + 2);
            for line in frame.lines() {
                let Some(payload) = line.strip_prefix("data: ") else {
                    continue;
                };
                if payload == "[DONE]" {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<serde_json::Value>(payload) else {
                    continue;
                };
                match event.get("type").and_then(|v| v.as_str()) {
                    Some("message_start") => {
                        if let Some(usage) = event.get("message").and_then(|m| m.get("usage")) {
                            merge_anthropic_usage(&mut usage_acc, usage);
                            have_usage = true;
                        }
                    }
                    Some("content_block_delta") => {
                        if let Some(text) = event
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            accumulated.push_str(text);
                            emit_event(
                                app,
                                &rig_id,
                                &AgentEvent::MessageDelta {
                                    session_id: session_id.clone(),
                                    text: text.to_string(),
                                },
                            );
                        }
                    }
                    Some("message_delta") => {
                        if let Some(reason) = event
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|r| r.as_str())
                        {
                            stop_reason = Some(reason.to_string());
                        }
                        // message_delta.usage carries the running
                        // output_tokens count. Final emission before
                        // message_stop replaces our accumulator's
                        // output_tokens with the canonical total.
                        if let Some(usage) = event.get("usage") {
                            merge_anthropic_usage(&mut usage_acc, usage);
                            have_usage = true;
                        }
                    }
                    Some("error") => {
                        let msg = event
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(|m| m.as_str())
                            .unwrap_or("anthropic error")
                            .to_string();
                        turn_failed!(msg);
                    }
                    _ => {}
                }
            }
        }
    }

    {
        let mut s = session.lock().await;
        s.history.push(Message {
            role: Role::Assistant,
            content: accumulated.clone(),
        });
    }

    emit_event(
        app,
        &rig_id,
        &AgentEvent::TurnEnded {
            session_id,
            text: accumulated,
            stop_reason,
            usage: have_usage.then_some(usage_acc),
        },
    );
    Ok(())
}

/// Tauri-host implementation of [`runner::SessionEventSink`]: wraps
/// each emit in [`RigAgentEvent`] and forwards to the renderer's
/// `agent:event` bus. This is the seam yah-agentd (R032-T3) replaces
/// with a JSON-RPC-notification sink to host sessions without Tauri.
pub struct TauriRigSink<'a> {
    pub app: &'a AppHandle,
    pub rig_id: &'a RigId,
}

impl<'a> SessionEventSink for TauriRigSink<'a> {
    fn emit(&self, event: &AgentEvent) {
        let payload = RigAgentEvent {
            rig_id: self.rig_id,
            event,
        };
        if let Err(e) = self.app.emit(EVENT_NAME, &payload) {
            tracing::warn!(error = %e, "failed to emit agent event");
        }
    }
}

fn emit_event(app: &AppHandle, rig_id: &RigId, event: &AgentEvent) {
    TauriRigSink { app, rig_id }.emit(event);
}

/// Public re-export of [`emit_event`] for sister runner modules
/// (`agent_process`) that need to fan AgentEvents through the same
/// `agent:event` channel without re-implementing the rig-tag wrapper.
pub fn emit_event_pub(app: &AppHandle, rig_id: &RigId, event: &AgentEvent) {
    emit_event(app, rig_id, event);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Session-id mint format / uniqueness are owned by yah-runner now —
    // see `yah-runner/src/session_id.rs` tests.

    fn fixture_session(model: &str, think: Option<ThinkBudget>) -> AgentSession {
        AgentSession {
            id: SessionId::new("session:deadbeef"),
            rig_id: RigId("rig:abc".into()),
            ticket_id: "R028-F3".into(),
            engine: EngineRef {
                provider: "claude".into(),
                model: Some(model.into()),
            },
            model: model.into(),
            think,
            prelude_text: "you are an agent".into(),
            cache_key: "cache".into(),
            history: vec![Message {
                role: Role::User,
                content: "hi".into(),
            }],
            running: None,
        }
    }

    #[test]
    fn model_supports_thinking_recognises_4x_and_3_7() {
        assert!(model_supports_thinking("claude-opus-4-7"));
        assert!(model_supports_thinking("claude-sonnet-4-6"));
        assert!(model_supports_thinking("claude-haiku-4-5"));
        assert!(model_supports_thinking("claude-3-7-sonnet-20250219"));
        assert!(!model_supports_thinking("claude-3-5-sonnet-20241022"));
        assert!(!model_supports_thinking("claude-3-haiku-20240307"));
        assert!(!model_supports_thinking("claude-2.1"));
    }

    #[test]
    fn think_budget_tokens_clamps_to_anthropic_minimum() {
        assert_eq!(think_budget_tokens(&ThinkBudget::Fast), 1024);
        assert_eq!(
            think_budget_tokens(&ThinkBudget::Budget { tokens: 200 }),
            1024,
            "values below the floor must round up to avoid 400 from Anthropic",
        );
        assert_eq!(
            think_budget_tokens(&ThinkBudget::Budget { tokens: 8000 }),
            8000
        );
        assert_eq!(think_budget_tokens(&ThinkBudget::Standard), 4000);
        assert_eq!(think_budget_tokens(&ThinkBudget::Deep), 16_000);
    }

    #[test]
    fn anthropic_body_omits_thinking_when_session_has_no_think_budget() {
        let s = fixture_session("claude-opus-4-7", None);
        let body = build_anthropic_body(&s, &AnthropicAuth::ApiKey("k".into()));
        assert!(body.get("thinking").is_none());
        assert_eq!(body["max_tokens"], DEFAULT_MAX_OUTPUT_TOKENS);
        assert_eq!(body["model"], "claude-opus-4-7");
        assert_eq!(body["stream"], true);
        // System prompt rides cache_control: ephemeral so the prelude
        // hits Anthropic's prompt cache turn-over-turn.
        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    }

    #[test]
    fn anthropic_body_adds_thinking_field_and_bumps_max_tokens_on_supported_model() {
        let s = fixture_session("claude-opus-4-7", Some(ThinkBudget::Standard));
        let body = build_anthropic_body(&s, &AnthropicAuth::ApiKey("k".into()));
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 4000);
        // Anthropic requires max_tokens > budget_tokens; we add the
        // default output cap on top so the model has actual room to
        // *answer* after thinking.
        assert_eq!(body["max_tokens"], 4000 + DEFAULT_MAX_OUTPUT_TOKENS);
    }

    #[test]
    fn anthropic_body_drops_thinking_field_on_unsupported_model() {
        let s = fixture_session("claude-3-5-sonnet-20241022", Some(ThinkBudget::Deep));
        let body = build_anthropic_body(&s, &AnthropicAuth::ApiKey("k".into()));
        assert!(
            body.get("thinking").is_none(),
            "older models 400 if the thinking field is present",
        );
        assert_eq!(body["max_tokens"], DEFAULT_MAX_OUTPUT_TOKENS);
    }

    #[test]
    fn anthropic_body_oauth_path_prepends_claude_code_system_prefix() {
        // OAuth-authed requests must lead with the Claude Code identity
        // prefix; our prelude rides as a second system block. API-key
        // path doesn't need the prefix and stays a single block.
        // Degraded-OAuth path (no pinned shape) falls back to the
        // hardcoded constant; with a shape, the pinned prefix wins.
        let s = fixture_session("claude-opus-4-7", None);
        let oauth = build_anthropic_body(
            &s,
            &AnthropicAuth::Oauth {
                token: "t".into(),
                shape: None,
            },
        );
        assert_eq!(oauth["system"].as_array().map(|a| a.len()), Some(2));
        assert_eq!(
            oauth["system"][0]["text"],
            ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX,
        );
        // Both blocks ride cache_control: ephemeral so the prelude keeps
        // its cache hit even though the prefix is short.
        assert_eq!(oauth["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(oauth["system"][1]["cache_control"]["type"], "ephemeral");

        let api_key = build_anthropic_body(&s, &AnthropicAuth::ApiKey("k".into()));
        assert_eq!(api_key["system"].as_array().map(|a| a.len()), Some(1));
    }

    #[test]
    fn anthropic_body_oauth_uses_pinned_system_prefix_when_shape_present() {
        // The audit step's pinned prefix wins over the hardcoded
        // constant — that's the whole point of the shape pin. When the
        // pinned prefix is the empty string (a malformed audit), the
        // hardcoded fallback kicks in so we don't ship a request with
        // no identity prefix at all.
        let s = fixture_session("claude-opus-4-7", None);
        let shape = Arc::new(PinnedShape {
            claude_version: "test".into(),
            audited_at: "2026-04-29T00:00:00Z".into(),
            headers: HashMap::new(),
            system_prefix: "You are Claude Code, audited.".into(),
            system_block_count: 2,
            system_cache_control: true,
            body_extras: HashMap::new(),
        });
        let body = build_anthropic_body(
            &s,
            &AnthropicAuth::Oauth {
                token: "t".into(),
                shape: Some(shape.clone()),
            },
        );
        assert_eq!(body["system"][0]["text"], "You are Claude Code, audited.");

        // Empty pinned prefix → fallback to hardcoded.
        let empty = Arc::new(PinnedShape {
            claude_version: "test".into(),
            audited_at: "2026-04-29T00:00:00Z".into(),
            headers: HashMap::new(),
            system_prefix: String::new(),
            system_block_count: 2,
            system_cache_control: true,
            body_extras: HashMap::new(),
        });
        let body_empty = build_anthropic_body(
            &s,
            &AnthropicAuth::Oauth {
                token: "t".into(),
                shape: Some(empty),
            },
        );
        assert_eq!(
            body_empty["system"][0]["text"],
            ANTHROPIC_CLAUDE_CODE_SYSTEM_PREFIX,
        );
    }

    #[test]
    fn anthropic_auth_apply_skips_reqwest_owned_headers_on_pinned_oauth() {
        // The skip-list is the only way to keep reqwest from
        // double-writing the authorization header (we set the bearer;
        // the audit might capture a literal bearer too) or from fighting
        // the transport over host / content-length. apply() must drop
        // those names, case-insensitive, no matter what the pinned TOML
        // contains.
        let mut headers = HashMap::new();
        headers.insert("Authorization".into(), "Bearer should-be-replaced".into());
        headers.insert("HOST".into(), "evil.example.com".into());
        headers.insert("Content-Length".into(), "999".into());
        headers.insert("anthropic-beta".into(), "pin-test".into());
        headers.insert("user-agent".into(), "claude-cli/test".into());
        let shape = Arc::new(PinnedShape {
            claude_version: "test".into(),
            audited_at: "2026-04-29T00:00:00Z".into(),
            headers,
            system_prefix: "p".into(),
            system_block_count: 2,
            system_cache_control: true,
            body_extras: HashMap::new(),
        });
        let auth = AnthropicAuth::Oauth {
            token: "real-token".into(),
            shape: Some(shape),
        };
        let req = reqwest::Client::new().post("https://example.invalid/");
        let built = auth
            .apply(req)
            .build()
            .expect("apply produces a buildable request");
        let hdrs = built.headers();
        // Bearer comes from the token slot, not from the pin.
        let auth_value = hdrs
            .get(reqwest::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        assert_eq!(auth_value, "Bearer real-token");
        // Pinned headers other than the skip-list ride through.
        assert_eq!(
            hdrs.get("anthropic-beta").and_then(|v| v.to_str().ok()),
            Some("pin-test"),
        );
        assert_eq!(
            hdrs.get("user-agent").and_then(|v| v.to_str().ok()),
            Some("claude-cli/test"),
        );
        // host/content-length are reqwest's; they may be unset on the
        // built request but must not carry the spoofed pinned values.
        if let Some(h) = hdrs.get("host") {
            assert_ne!(h, "evil.example.com", "host must not come from the pin");
        }
    }

    #[test]
    fn anthropic_auth_apply_falls_back_when_oauth_shape_missing() {
        // Degraded mode: BASELINE_TOML failed to parse and pinned()
        // returned None. apply() must still produce a valid OAuth
        // request (bearer + hardcoded anthropic-beta + version) — the
        // shape pin is a polish layer, not a hard requirement for the
        // OAuth path to work at all.
        let auth = AnthropicAuth::Oauth {
            token: "real-token".into(),
            shape: None,
        };
        let req = reqwest::Client::new().post("https://example.invalid/");
        let built = auth.apply(req).build().expect("apply builds a request");
        let hdrs = built.headers();
        assert_eq!(
            hdrs.get(reqwest::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer real-token"),
        );
        assert_eq!(
            hdrs.get("anthropic-beta").and_then(|v| v.to_str().ok()),
            Some(ANTHROPIC_OAUTH_BETA_VALUE),
        );
        assert_eq!(
            hdrs.get("anthropic-version").and_then(|v| v.to_str().ok()),
            Some(ANTHROPIC_VERSION),
        );
    }

    #[test]
    fn merge_anthropic_usage_takes_input_from_message_start_and_output_from_message_delta() {
        // Realistic two-frame sequence: message_start.message.usage
        // sets input + cache numbers and a placeholder output_tokens=1;
        // a later message_delta.usage updates output_tokens to the
        // running total. The merger must keep the input numbers and
        // overwrite the output. Same shape Anthropic documents at
        // https://docs.anthropic.com/en/api/messages-streaming#raw-http-stream-events.
        let mut acc = TurnUsage::default();
        let start_usage = serde_json::json!({
            "input_tokens": 1234,
            "cache_read_input_tokens": 1000,
            "cache_creation_input_tokens": 0,
            "output_tokens": 1,
        });
        merge_anthropic_usage(&mut acc, &start_usage);
        assert_eq!(acc.input_tokens, Some(1234));
        assert_eq!(acc.cache_read_input_tokens, Some(1000));
        assert_eq!(acc.cache_creation_input_tokens, Some(0));
        assert_eq!(acc.output_tokens, Some(1));

        let delta_usage = serde_json::json!({ "output_tokens": 567 });
        merge_anthropic_usage(&mut acc, &delta_usage);
        assert_eq!(
            acc.output_tokens,
            Some(567),
            "later message_delta output_tokens overwrites the placeholder",
        );
        // Input/cache numbers from message_start must not be clobbered
        // by the output-only delta — the merger preserves any field
        // the delta payload doesn't carry.
        assert_eq!(acc.input_tokens, Some(1234));
        assert_eq!(acc.cache_read_input_tokens, Some(1000));
    }

    #[test]
    fn rig_agent_event_wraps_with_rig_id_alongside_runtime_kind() {
        // The Tauri seam wraps the runtime AgentEvent with a rig-tagged
        // envelope so the renderer can route by rigId. The flatten
        // attribute on RigAgentEvent.event is what makes this a single
        // flat JSON object instead of nesting `{ event: {...} }`.
        let rig = RigId("rig:abc123".into());
        let ev = AgentEvent::MessageDelta {
            session_id: SessionId::new("session:deadbeef"),
            text: "hi".into(),
        };
        let payload = RigAgentEvent {
            rig_id: &rig,
            event: &ev,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"rigId\":\"rig:abc123\""), "{json}");
        assert!(json.contains("\"kind\":\"message_delta\""), "{json}");
        assert!(
            json.contains("\"sessionId\":\"session:deadbeef\""),
            "{json}"
        );
    }
}
