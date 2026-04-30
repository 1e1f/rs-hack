//! @yah:ticket(R028-F7, "Agent provider credential bootstrap UI")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R028)
//! @yah:next("Add a fifth card for the claude (PVd) preset (R028-F8): launches `claude` CLI as the runner, no API-key needed since Claude Code manages its own OAuth. Card shape differs — instead of a paste-key affordance, surface a 'claude --version' liveness probe + a 'log in' button that shells out to `claude` first-run.")
//! @yah:next("Optional: probe localhost:11434 for ollama serve liveness; today the local fallback is silent")
//! @yah:gotcha("ChatGPT Plus/Pro does NOT include API access. Users with only the chat sub will see 401s and need a separately-billed sk-... key from platform.openai.com — surface the distinction in copy so they don't paste their chat session token.")
//! @yah:gotcha("The crab (HAo) card is shipped under the 'experimental' label because Anthropic's 2026-04-04 ToS bans consumer Pro/Max OAuth in third-party tools (named the Agent SDK). Slot exists for users who explicitly accept the personal-use TOS risk. The recommended subscription path is the claude (PVd) preset, not crab.")

import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import {
  useApiKeys,
  type ApiKeyProvider,
  type ApiKeyTestResult,
} from "../shell/api-keys-context";
import { getEnv } from "../../env";
import type { ClaudeCliProbe, OllamaServeProbe } from "../../env/types";

/* The three providers the agent runtime dispatches to today. Order is the
   *recommended* order: Claude first (cheapest with Pro/Max), then OpenAI,
   then Ollama (free if you self-host). */
interface AgentProviderDef {
  id: ApiKeyProvider;
  name: string;
  /* One-line cost tier hint that lets the user pick a path before reading
     the full description. Keep these short — they show up as a pill under
     the title. */
  costTier: string;
  /* Two- or three-sentence pitch for the card body. The user reads this
     once when bootstrapping; after that the card collapses to status. */
  description: string;
  /* External URL for "where do I get this credential?" — opens in the OS
     browser (Tauri intercepts http(s) links and routes to the shell open
     handler). */
  helpUrl: string;
  /* Subtitle for the status pill when no key is stored. Most providers
     read "Not configured", but Ollama defaults to "Local mode" because the
     runtime falls back to localhost:11434 when no cloud key is present. */
  unconfiguredLabel: string;
  /* Optional footnote — inline copy that appears below the action row.
     Used today for the Claude OAuth-coming-soon hint and the OpenAI
     ChatGPT-Plus-isn't-API caveat. */
  footnote?: string;
  /* Placeholder text inside the password input. Helps the user spot a
     wrong-format token before they click save. */
  inputPlaceholder: string;
}

const AGENT_PROVIDERS: AgentProviderDef[] = [
  {
    id: "anthropic",
    name: "Claude — anthropic (HAk)",
    costTier: "API key — pay-per-token",
    description:
      "Direct API access from console.anthropic.com. For the cost-controlling Pro/Max subscription path, use the Claude Code subprocess runner instead (no key needed — it manages its own login).",
    helpUrl: "https://console.anthropic.com/settings/keys",
    unconfiguredLabel: "Not configured",
    inputPlaceholder: "Paste sk-ant-… API key",
  },
  {
    id: "anthropic-oauth",
    name: "Claude — crab (HAo)",
    costTier: "OAuth bearer — Pro/Max subscription · experimental",
    description:
      "Drives /v1/messages with a long-lived OAuth bearer token from Claude Code's `claude setup-token` command. Run that in a terminal, paste the token here. Wins over the API-key slot when both are populated.",
    helpUrl: "https://code.claude.com/docs/en/authentication#generate-a-long-lived-token",
    unconfiguredLabel: "Not configured",
    footnote:
      "Anthropic's 2026-04-04 Consumer ToS prohibits Pro/Max OAuth tokens in third-party tools (and named the Agent SDK). Use at your own discretion. The recommended subscription path is the Claude Code subprocess runner (claude / PVd preset).",
    inputPlaceholder: "Paste `claude setup-token` output",
  },
  {
    id: "openai",
    name: "OpenAI",
    costTier: "API key — pay-per-token (no OAuth path)",
    description:
      "ChatGPT Plus/Pro does not include API access. Generate a separately-billed key at platform.openai.com/api-keys and paste it here. Auto prompt-caching kicks in on prefixes ≥1024 tokens.",
    helpUrl: "https://platform.openai.com/api-keys",
    unconfiguredLabel: "Not configured",
    footnote:
      "If you only have the chat subscription, you'll see 401s — the chat session token is not an API key.",
    inputPlaceholder: "Paste sk-… API key",
  },
  {
    id: "ollama",
    name: "Ollama",
    costTier: "Local — free  ·  Cloud — paid subscription",
    description:
      "Without a key, the agent dispatches to localhost:11434 — run ollama serve and you're set. Add a Cloud key to use the hosted models (larger context, more capable models).",
    helpUrl: "https://ollama.com",
    unconfiguredLabel: "Local mode (free)",
    inputPlaceholder: "Paste Ollama Cloud API key",
  },
];

interface AgentProvidersPanelProps {
  /* When true, render a compact heading + tighter cards (used in the
     AgentView empty state where vertical space is lean). Default false
     gives the roomy SettingsModal layout. */
  compact?: boolean;
  /* Optional intro line. SettingsModal sets a section header; AgentView's
     empty state sets a "no agent providers configured yet" cue. */
  heading?: string;
  subheading?: string;
}

export function AgentProvidersPanel({
  compact = false,
  heading,
  subheading,
}: AgentProvidersPanelProps) {
  const { envKind } = useApiKeys();
  return (
    <div className={compact ? "w-full max-w-[520px]" : "w-full"}>
      {heading && (
        <div
          className={`font-display text-ink ${
            compact ? "text-[14px]" : "text-[15px] font-medium"
          } mb-1`}
        >
          {heading}
        </div>
      )}
      {subheading && (
        <div className="mb-3 text-[12px] text-ink-3">{subheading}</div>
      )}
      {envKind === "browser" && (
        <div className="mb-3 rounded border border-amber-700/30 bg-amber-100/20 px-3 py-2 text-[11px] text-ink-2 dark:border-amber-300/20 dark:bg-amber-300/10">
          <span className="font-medium">Browser preview</span>
          {" — "}keys not persisted. Run under Tauri to use OS-keychain
          storage.
        </div>
      )}
      <div className="flex flex-col gap-2">
        {AGENT_PROVIDERS.map((p) => (
          <AgentProviderCard key={p.id} provider={p} compact={compact} />
        ))}
        <ClaudeSubprocessCard compact={compact} />
      </div>
    </div>
  );
}

/* Single provider card. Mirrors `ProviderRow` in SettingsModal but with the
   richer copy needed for cost-tier orientation: cost pill under the title,
   prose description, footnote affordance, and a "Get key →" external link.
   We keep it self-contained so AgentView can drop the panel in without
   plumbing through state. */
function AgentProviderCard({
  provider,
  compact,
}: {
  provider: AgentProviderDef;
  compact: boolean;
}) {
  const api = useApiKeys();
  const stored = api.has(provider.id);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ApiKeyTestResult | null>(null);
  /* Ollama-only: the local fallback in agent.rs is silent when no serve
     is up — by the time the user sends their first turn they hit a
     connection-refused. Probing on mount lets the card show the actual
     localhost:11434 state under the cost-tier line. */
  const [ollamaServe, setOllamaServe] = useState<OllamaServeProbe | null>(
    null,
  );

  useEffect(() => {
    if (adding) inputRef.current?.focus();
  }, [adding]);

  useEffect(() => {
    if (provider.id !== "ollama") return;
    let cancelled = false;
    void (async () => {
      try {
        const env = await getEnv();
        const result = await env.rpc.probe.ollamaServe();
        if (!cancelled) setOllamaServe(result);
      } catch {
        if (!cancelled) setOllamaServe({ running: false });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [provider.id]);

  function startAdd() {
    setDraft("");
    setAdding(true);
    setTestResult(null);
  }
  function cancelAdd() {
    setAdding(false);
    setDraft("");
  }
  async function saveAdd() {
    const t = draft.trim();
    if (!t) return;
    try {
      await api.set(provider.id, t);
      setAdding(false);
      setDraft("");
      setTestResult(null);
    } catch (err) {
      setTestResult({
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }
  async function deleteToken() {
    try {
      await api.remove(provider.id);
      setConfirmDelete(false);
      setTestResult(null);
    } catch (err) {
      setTestResult({
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }
  async function runTest() {
    setTesting(true);
    setTestResult(null);
    const result = await api.test(provider.id);
    setTestResult(result);
    setTesting(false);
  }

  const statusLabel = stored ? "API key set" : provider.unconfiguredLabel;
  const statusOk = stored || provider.id === "ollama"; // ollama "local mode" is a green path

  return (
    <div
      className={`rounded border border-rule/40 bg-vellum/40 ${
        compact ? "px-3 py-2" : "px-3 py-2.5"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-medium text-ink">
              {provider.name}
            </span>
            <span
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] ${
                statusOk
                  ? "bg-emerald-700/15 text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-300"
                  : "bg-vellum/60 text-ink-3"
              }`}
            >
              {statusOk && <Icon name="check" size={10} />}
              {statusLabel}
            </span>
          </div>
          <div className="mt-0.5 text-[11px] text-ink-3">
            {provider.costTier}
          </div>
          {provider.id === "ollama" && ollamaServe && (
            <div
              className={`mt-0.5 text-[11px] ${
                ollamaServe.running
                  ? "text-emerald-700 dark:text-emerald-300"
                  : "text-ink-3"
              }`}
            >
              {ollamaServe.running
                ? "· Local serve detected on :11434"
                : "· Local serve not running on :11434"}
            </div>
          )}
          {!compact && (
            <div className="mt-1.5 text-[11px] leading-snug text-ink-2">
              {provider.description}
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {!stored && !adding && (
            <button
              onClick={startAdd}
              className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2"
            >
              Add key
            </button>
          )}
          {stored && !confirmDelete && (
            <>
              <button
                onClick={runTest}
                disabled={testing}
                className="rounded border border-ink-3/45 bg-paper-2 px-[5px] py-0.5 text-[11px] text-ink-2 hover:bg-vellum/55 disabled:pointer-events-none disabled:opacity-40"
              >
                {testing ? "Testing…" : "Test"}
              </button>
              <button
                onClick={() => setConfirmDelete(true)}
                className="rounded border border-ink-3/45 bg-paper-2 px-[5px] py-0.5 text-[11px] text-ink-3 hover:bg-vellum/55 hover:text-oxblood"
              >
                Delete
              </button>
            </>
          )}
          {confirmDelete && (
            <>
              <button
                onClick={() => setConfirmDelete(false)}
                className="rounded border border-ink-3/45 bg-paper-2 px-[5px] py-0.5 text-[11px] text-ink-3 hover:bg-vellum/55"
              >
                Cancel
              </button>
              <button
                onClick={deleteToken}
                className="rounded bg-oxblood px-2 py-1 text-[11px] font-medium text-paper-2 hover:opacity-90"
              >
                Confirm delete
              </button>
            </>
          )}
        </div>
      </div>
      {adding && (
        <form
          onSubmit={(e) => {
            e.preventDefault();
            saveAdd();
          }}
          className="mt-2 flex items-center gap-2"
        >
          <input
            ref={inputRef}
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder={provider.inputPlaceholder}
            className="min-w-0 flex-1 rounded border border-rule/50 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none focus:border-accent/60"
          />
          <button
            type="button"
            onClick={cancelAdd}
            className="rounded px-2 py-1 text-[11px] text-ink-2 hover:bg-vellum/55"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={!draft.trim()}
            className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
          >
            Save
          </button>
        </form>
      )}
      {testResult && !adding && (
        <div
          className={`mt-2 text-[11px] ${
            testResult.ok
              ? "text-emerald-700 dark:text-emerald-300"
              : "text-ink-3"
          }`}
        >
          {testResult.ok ? `✓ ${testResult.detail}` : `· ${testResult.error}`}
        </div>
      )}
      {provider.footnote && !compact && (
        <div className="mt-2 text-[10.5px] text-ink-3/80">
          {provider.footnote}
        </div>
      )}
      <div className="mt-1.5">
        <a
          href={provider.helpUrl}
          target="_blank"
          rel="noreferrer noopener"
          className="text-[10.5px] text-accent hover:underline"
        >
          Get a {provider.name.split(" ")[0]} key →
        </a>
      </div>
    </div>
  );
}

/* The claude (PVd) preset has no API-key affordance — Claude Code owns
   its own login — so its card has a different shape: a `claude --version`
   liveness probe + a "Log in" button that shells out to claude's
   first-run via a fresh local terminal session. */
function ClaudeSubprocessCard({ compact }: { compact: boolean }) {
  const [probe, setProbe] = useState<ClaudeCliProbe | null>(null);
  const [probing, setProbing] = useState(false);
  const [loginNote, setLoginNote] = useState<string | null>(null);

  const runProbe = useCallback(async () => {
    setProbing(true);
    try {
      const env = await getEnv();
      const result = await env.rpc.probe.claudeCli();
      setProbe(result);
    } catch (err) {
      setProbe({
        installed: false,
        error: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setProbing(false);
    }
  }, []);

  useEffect(() => {
    void runProbe();
  }, [runProbe]);

  async function login() {
    setLoginNote(null);
    try {
      const env = await getEnv();
      /* Spawn `claude` in a fresh local PTY. The user lands on
         claude's first-run prompt (OAuth in a browser, fall back to
         API-key paste). The terminal session shows up in the
         Terminal tab — yah doesn't auto-switch tabs to keep this
         a side-effect-only affordance, but the open spec carries a
         "Claude login" label so the rail is unambiguous. */
      const path =
        probe?.path && probe.path.length > 0 ? probe.path : "claude";
      await env.rpc.terminal.openLocal({
        shell: path,
        label: "Claude login",
      });
      setLoginNote(
        "Claude login session opened — switch to the Terminal tab to continue.",
      );
    } catch (err) {
      setLoginNote(
        err instanceof Error
          ? `Could not launch claude: ${err.message}`
          : `Could not launch claude: ${String(err)}`,
      );
    }
  }

  const installed = probe?.installed === true;
  const statusLabel = probing
    ? "Probing…"
    : installed
      ? probe?.version ?? "Detected"
      : "Not detected";
  const statusOk = installed;

  return (
    <div
      className={`rounded border border-rule/40 bg-vellum/40 ${
        compact ? "px-3 py-2" : "px-3 py-2.5"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-medium text-ink">
              Claude — claude (PVd)
            </span>
            <span
              className={`flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] ${
                statusOk
                  ? "bg-emerald-700/15 text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-300"
                  : "bg-vellum/60 text-ink-3"
              }`}
            >
              {statusOk && <Icon name="check" size={10} />}
              {statusLabel}
            </span>
          </div>
          <div className="mt-0.5 text-[11px] text-ink-3">
            Subprocess runner — Pro/Max subscription · recommended
          </div>
          {!compact && (
            <div className="mt-1.5 text-[11px] leading-snug text-ink-2">
              Wraps the official <code className="font-mono text-[10.5px]">claude</code> CLI as a subprocess. No API key
              needed — Claude Code manages its own login (Pro/Max OAuth or
              Console). The policy-durable Anthropic default (the README's
              recommendation).
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <button
            onClick={() => void runProbe()}
            disabled={probing}
            className="rounded border border-ink-3 bg-paper-2 px-2 py-1 text-[11px] text-ink-2 hover:bg-vellum/55 disabled:pointer-events-none disabled:opacity-40"
          >
            {probing ? "Probing…" : "Probe"}
          </button>
          <button
            onClick={() => void login()}
            disabled={probing}
            className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2 disabled:pointer-events-none disabled:opacity-40"
          >
            Log in
          </button>
        </div>
      </div>
      {probe?.error && !installed && !probing && (
        <div className="mt-2 text-[11px] text-ink-3">· {probe.error}</div>
      )}
      {probe?.path && installed && !compact && (
        <div className="mt-1.5 font-mono text-[10.5px] text-ink-3/80">
          {probe.path}
        </div>
      )}
      {loginNote && (
        <div className="mt-2 text-[11px] text-ink-2">{loginNote}</div>
      )}
      {!compact && (
        <div className="mt-2 text-[10.5px] text-ink-3/80">
          The Log in button spawns <code className="font-mono">claude</code> in a fresh local terminal —
          claude's first-run kicks off the OAuth flow in your browser. R028-F8 will plumb this binary into the
          PVd runner; until then the probe is bootstrap UX only.
        </div>
      )}
      <div className="mt-1.5">
        <a
          href="https://docs.claude.com/en/docs/claude-code/quickstart"
          target="_blank"
          rel="noreferrer noopener"
          className="text-[10.5px] text-accent hover:underline"
        >
          Install Claude Code →
        </a>
      </div>
    </div>
  );
}

/* True if at least one agent provider has a usable credential — Ollama
   counts as "available" even without a key because the runtime falls
   back to local. Used by AgentView's empty state to decide whether to
   show the bootstrap panel or the regular splash. */
export function useAnyAgentConfigured(): boolean {
  const api = useApiKeys();
  return (
    api.has("anthropic") ||
    api.has("anthropic-oauth") ||
    api.has("openai") ||
    api.has("ollama")
  );
}
