import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "../shared/Glyph";
import { AgentProvidersPanel } from "../agent/AgentProvidersPanel";
import {
  useApiKeys,
  type ApiKeyProvider,
  type ApiKeyTestResult,
} from "./api-keys-context";
import { IdentitiesSection } from "./IdentitiesSection";
import { getEnv } from "../../env";
import type {
  WireAgentSettings,
  WireApprovalRule,
  WireApprovalRuleset,
} from "../../env/types";

type SettingsSection = "general" | "agents" | "api-keys" | "identities";

interface SettingsModalProps {
  onClose: () => void;
  /** Initial section. Defaults to general; callers that open the modal from
   *  a contextual nudge (e.g. Infra empty state in P3) pass "api-keys". */
  initialSection?: SettingsSection;
  /** When set together with initialSection="api-keys", scrolls the matching
   *  provider row into view on first paint — used by the Infra tab's empty
   *  state to deposit the user directly at the Hetzner row. */
  initialFocus?: ApiKeyProvider;
  /** Active rig — needed by per-rig sections (Agents → Approval rules,
   *  Agents → Experimental). When unset, those sections render an
   *  "attach a rig" nudge instead of disabling them silently. */
  activeRigId?: string;
}

interface SectionDef {
  id: SettingsSection;
  label: string;
}

interface FutureSlotDef {
  label: string;
  hint: string;
}

const SECTIONS: SectionDef[] = [
  { id: "general", label: "General" },
  { id: "agents", label: "Agents" },
  { id: "api-keys", label: "API Keys" },
  { id: "identities", label: "Identities" },
];

/* Reserved nav rows for upcoming surfaces. Rendered disabled with a hint so
   the future shape of the modal is visible to operators today. Each lands as
   its own relay; this list is just the placeholder until then. */
const FUTURE_SLOTS: FutureSlotDef[] = [
  { label: "Notifications", hint: "soon" },
  { label: "Telemetry", hint: "soon" },
];

export function SettingsModal({
  onClose,
  initialSection = "general",
  initialFocus,
  activeRigId,
}: SettingsModalProps) {
  const [section, setSection] = useState<SettingsSection>(initialSection);

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Settings"
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 backdrop-blur-sm"
      onMouseDown={onClose}
    >
      <div
        onMouseDown={(e) => e.stopPropagation()}
        className="flex h-[480px] w-[680px] overflow-hidden rounded-[6px] border border-rule/50 bg-paper-2 shadow-[0_18px_60px_-12px_rgba(70,45,20,0.4)]"
      >
        <nav className="flex w-[180px] shrink-0 flex-col border-r border-rule/40 bg-vellum/40 p-3">
          <div className="mb-2 px-1.5 font-display text-[15px] font-medium text-ink">
            Settings
          </div>
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => setSection(s.id)}
              className={`mb-0.5 rounded px-2 py-1.5 text-left text-[12px] ${
                section === s.id
                  ? "bg-accent/15 text-ink"
                  : "text-ink-2 hover:bg-vellum/55"
              }`}
            >
              {s.label}
            </button>
          ))}
          <div className="mt-3 mb-1 px-2 text-[10px] uppercase tracking-wider text-ink-3/70">
            Coming soon
          </div>
          {FUTURE_SLOTS.map((slot) => (
            <div
              key={slot.label}
              className="mb-0.5 flex items-center justify-between rounded px-2 py-1.5 text-[12px] text-ink-3/60"
            >
              <span>{slot.label}</span>
              <span className="text-[10px] text-ink-3/50">{slot.hint}</span>
            </div>
          ))}
        </nav>

        <div className="relative flex min-w-0 flex-1 flex-col">
          <button
            onClick={onClose}
            title="Close"
            aria-label="Close settings"
            className="absolute right-2 top-2 flex items-center justify-center rounded p-1 text-ink-3 hover:bg-vellum/55 hover:text-ink"
          >
            <Icon name="x" size={14} />
          </button>
          <div className="flex-1 overflow-y-auto p-5">
            {section === "general" && <GeneralSection />}
            {section === "agents" && (
              <div className="flex flex-col gap-6">
                <AgentProvidersPanel
                  heading="Agents"
                  subheading="Credentials the agent runtime uses to dispatch turns. Same OS-keychain backing as API Keys; separated here because each provider has its own cost-tier story."
                />
                <AgentExperimentalSection rigId={activeRigId} />
                <ApprovalRulesSection rigId={activeRigId} />
              </div>
            )}
            {section === "api-keys" && (
              <ApiKeysSection initialFocus={initialFocus} />
            )}
            {section === "identities" && <IdentitiesSectionWrapper />}
          </div>
        </div>
      </div>
    </div>
  );
}

/* Identities reads `envKind` from the api-keys context (which already
   tracks it for the "Browser preview" banner). Wrapping here keeps the
   IdentitiesSection portable — it doesn't depend on the api-keys context
   directly. */
function IdentitiesSectionWrapper() {
  const { envKind } = useApiKeys();
  return <IdentitiesSection envKind={envKind} />;
}

/* General section is intentionally a stub for P1. Theme already lives in the
   title bar; the spec calls for keeping it there for now. R027-F1's goal is
   the shell — later relays can fold app-wide preferences in here. */
function GeneralSection() {
  return (
    <div>
      <div className="mb-2 font-display text-[15px] font-medium text-ink">
        General
      </div>
      <div className="text-[12px] text-ink-3">
        App-wide preferences will live here. Theme stays in the title bar
        for now.
      </div>
    </div>
  );
}

interface ProviderRowDef {
  id: ApiKeyProvider;
  name: string;
  hint: string;
}

const ACTIVE_PROVIDERS: ProviderRowDef[] = [
  {
    id: "cloudflare",
    name: "Cloudflare",
    hint: "DNS, Tunnel, Pages — scoped API token",
  },
  {
    id: "hetzner",
    name: "Hetzner Cloud",
    hint: "VM provisioning, Object Storage, Load Balancers",
  },
];

const RESERVED_PROVIDERS: { name: string; hint: string }[] = [
  { name: "DigitalOcean", hint: "soon" },
  { name: "AWS / S3", hint: "soon" },
];

/* Tokens persist via env().rpc.apiKey (OS keychain under Tauri). The
   browser-preview banner only renders in dev-server mode where there is no
   keychain to write to; under Tauri the section is unannotated since secure
   storage is the resting state. Each provider row owns its own
   Add/Test/Delete state — collapsed by default, expanded on Add to paste a
   token. */
function ApiKeysSection({ initialFocus }: { initialFocus?: ApiKeyProvider }) {
  const { envKind } = useApiKeys();
  return (
    <div>
      <div className="mb-2 font-display text-[15px] font-medium text-ink">
        API Keys
      </div>
      {envKind === "browser" && (
        <div className="mb-3 rounded border border-amber-700/30 bg-amber-100/20 px-3 py-2 text-[11px] text-ink-2 dark:border-amber-300/20 dark:bg-amber-300/10">
          <span className="font-medium">Browser preview</span>
          {" — "}
          keys not persisted. Run under Tauri to use OS-keychain storage.
        </div>
      )}
      <div className="flex flex-col gap-2">
        {ACTIVE_PROVIDERS.map((p) => (
          <ProviderRow
            key={p.id}
            provider={p}
            focused={initialFocus === p.id}
          />
        ))}
      </div>
      <div className="mt-4 mb-1 px-1 text-[10px] uppercase tracking-wider text-ink-3/70">
        Reserved
      </div>
      <div className="flex flex-col gap-1.5">
        {RESERVED_PROVIDERS.map((p) => (
          <div
            key={p.name}
            className="flex items-center justify-between rounded border border-rule/30 bg-vellum/30 px-3 py-2 text-[12px] text-ink-3/70"
          >
            <span>{p.name}</span>
            <span className="text-[10px] text-ink-3/50">{p.hint}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

function ProviderRow({
  provider,
  focused,
}: {
  provider: ProviderRowDef;
  focused?: boolean;
}) {
  const api = useApiKeys();
  const stored = api.has(provider.id);
  const [adding, setAdding] = useState(false);
  const [draft, setDraft] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const rowRef = useRef<HTMLDivElement>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ApiKeyTestResult | null>(null);

  useEffect(() => {
    if (adding) inputRef.current?.focus();
  }, [adding]);

  /* Focus glow + scroll when the modal opens via a contextual nudge (e.g.
     Infra tab → "Configure Hetzner token"). Mount-only so reopening or
     switching sections later doesn't yank the scroll position. */
  useEffect(() => {
    if (focused) {
      rowRef.current?.scrollIntoView({ block: "center", behavior: "auto" });
    }
  }, [focused]);

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

  return (
    <div
      ref={rowRef}
      className={`rounded border bg-vellum/40 px-3 py-2.5 ${
        focused ? "border-accent/60 shadow-[0_0_0_2px_var(--color-accent)]/15" : "border-rule/40"
      }`}
    >
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="text-[13px] font-medium text-ink">
              {provider.name}
            </span>
            {stored && (
              <span className="flex items-center gap-1 rounded bg-emerald-700/15 px-1.5 py-0.5 text-[10px] text-emerald-800 dark:bg-emerald-400/15 dark:text-emerald-300">
                <Icon name="check" size={10} />
                stored
              </span>
            )}
          </div>
          <div className="mt-0.5 text-[11px] text-ink-3">{provider.hint}</div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {!stored && !adding && (
            <button
              onClick={startAdd}
              className="rounded bg-accent px-2 py-1 text-[11px] font-medium text-paper-2 hover:bg-accent-2"
            >
              Add
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
            placeholder={`Paste ${provider.name} API token`}
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
          className={`mt-2 text-[11px] ${testResult.ok ? "text-emerald-700 dark:text-emerald-300" : "text-oxblood"}`}
        >
          {testResult.ok ? `✓ ${testResult.detail}` : `✗ ${testResult.error}`}
        </div>
      )}
    </div>
  );
}

/* Agents → Experimental: per-rig opt-in for the write-tool surface
   (R031-F5 production flip). Disabled by default; flipping it makes
   write tools reachable to the agent on the next session start, where
   each call still routes through the approval gate. */
function AgentExperimentalSection({ rigId }: { rigId?: string }) {
  const [settings, setSettings] = useState<WireAgentSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!rigId) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    void (async () => {
      try {
        const env = await getEnv();
        const next = await env.rpc.agent.settings.get(rigId);
        if (!cancelled) setSettings(next);
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [rigId]);

  const toggle = useCallback(async () => {
    if (!rigId || !settings) return;
    const next: WireAgentSettings = {
      ...settings,
      agentWritersEnabled: !settings.agentWritersEnabled,
    };
    setSettings(next);
    try {
      const env = await getEnv();
      const persisted = await env.rpc.agent.settings.set(rigId, next);
      setSettings(persisted);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      // Best-effort revert so the toggle state matches reality.
      setSettings(settings);
    }
  }, [rigId, settings]);

  return (
    <section>
      <div className="mb-1 font-display text-[14px] font-medium text-ink">
        Experimental
      </div>
      <div className="mb-3 text-[11.5px] text-ink-3">
        Write-tool surface for agent sessions on this rig. Off by default —
        every write call still routes through the approval gate, so
        enabling this only unblocks the prompt; nothing runs unattended.
        Takes effect on the next session start.
      </div>
      {!rigId ? (
        <div className="rounded border border-rule/40 bg-vellum/40 px-3 py-2 text-[11.5px] text-ink-3">
          Attach a rig in the rig selector to manage its agent settings.
        </div>
      ) : (
        <div className="flex items-center justify-between rounded border border-rule/40 bg-vellum/40 px-3 py-2.5">
          <div className="min-w-0 flex-1">
            <div className="text-[12.5px] font-medium text-ink">
              Enable write tools
            </div>
            <div className="mt-0.5 text-[11px] text-ink-3">
              <span className="font-mono">yah_add</span>,{" "}
              <span className="font-mono">yah_remove</span>,{" "}
              <span className="font-mono">yah_rename</span>,{" "}
              <span className="font-mono">yah_transform</span>,{" "}
              <span className="font-mono">edit_file</span>,{" "}
              <span className="font-mono">write_arch_doc</span>.
            </div>
          </div>
          <button
            type="button"
            onClick={() => void toggle()}
            disabled={loading || !settings}
            aria-pressed={settings?.agentWritersEnabled ?? false}
            className={`shrink-0 rounded px-2.5 py-1 text-[11px] font-medium ${
              settings?.agentWritersEnabled
                ? "bg-accent text-paper-2 hover:bg-accent-2"
                : "border border-rule/50 bg-paper-2 text-ink-2 hover:bg-vellum/55"
            } disabled:pointer-events-none disabled:opacity-40`}
          >
            {loading
              ? "Loading…"
              : settings?.agentWritersEnabled
                ? "Enabled"
                : "Disabled"}
          </button>
        </div>
      )}
      {error && (
        <div className="mt-2 text-[11px] text-oxblood">{error}</div>
      )}
    </section>
  );
}

/* Agents → Approval rules: list of rules saved for this rig. Each row
   shows the rule shape and offers Delete; the rules-add path runs
   inline through the chat pane's "Always allow" button, so we don't
   yet expose an add-rule form here. Empty state explains the lifecycle. */
function ApprovalRulesSection({ rigId }: { rigId?: string }) {
  const [ruleset, setRuleset] = useState<WireApprovalRuleset | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!rigId) return;
    setLoading(true);
    setError(null);
    try {
      const env = await getEnv();
      const next = await env.rpc.agent.approval.rulesList(rigId);
      setRuleset(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [rigId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const remove = useCallback(
    async (index: number) => {
      if (!rigId) return;
      try {
        const env = await getEnv();
        const next = await env.rpc.agent.approval.rulesRemove(rigId, index);
        setRuleset(next);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [rigId],
  );

  const rules = ruleset?.rules ?? [];

  return (
    <section>
      <div className="mb-1 font-display text-[14px] font-medium text-ink">
        Approval rules
      </div>
      <div className="mb-3 text-[11.5px] text-ink-3">
        Persisted "always allow" matches. The agent's inline approval
        prompt populates this list when you click "Always allow" — each
        rule matches the parsed tool call structurally, never a
        rendered string.
      </div>
      {!rigId ? (
        <div className="rounded border border-rule/40 bg-vellum/40 px-3 py-2 text-[11.5px] text-ink-3">
          Attach a rig to manage its approval rules.
        </div>
      ) : loading && !ruleset ? (
        <div className="text-[11.5px] text-ink-3">Loading rules…</div>
      ) : rules.length === 0 ? (
        <div className="rounded border border-dashed border-rule/40 bg-vellum/30 px-3 py-2 text-[11.5px] italic text-ink-3">
          No saved rules yet. Approve a write tool from the chat pane
          with "Always allow" to add one.
        </div>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {rules.map((rule, i) => (
            <li
              key={i}
              className="flex items-center gap-3 rounded border border-rule/40 bg-vellum/40 px-3 py-2"
            >
              <RuleSummary rule={rule} />
              <button
                type="button"
                onClick={() => void remove(i)}
                className="shrink-0 rounded border border-rule/50 bg-paper-2 px-2 py-0.5 text-[11px] text-ink-3 hover:bg-vellum/55 hover:text-oxblood"
              >
                Delete
              </button>
            </li>
          ))}
        </ul>
      )}
      {error && (
        <div className="mt-2 text-[11px] text-oxblood">{error}</div>
      )}
    </section>
  );
}

function RuleSummary({ rule }: { rule: WireApprovalRule }) {
  switch (rule.kind) {
    case "tool":
      return (
        <div className="min-w-0 flex-1 text-[11.5px]">
          <span className="text-ink-3">tool</span>{" "}
          <span className="font-mono text-ink">{rule.name}</span>
        </div>
      );
    case "tool_path":
      return (
        <div className="min-w-0 flex-1 text-[11.5px]">
          <span className="text-ink-3">tool</span>{" "}
          <span className="font-mono text-ink">{rule.name}</span>{" "}
          <span className="text-ink-3">under</span>{" "}
          <span className="font-mono text-ink">{rule.glob}</span>
        </div>
      );
    case "bash_cmd":
      return (
        <div className="min-w-0 flex-1 text-[11.5px]">
          <span className="text-ink-3">bash</span>{" "}
          <span className="font-mono text-ink">{rule.cmd}</span>
        </div>
      );
    case "bash_cmd_pattern": {
      const argText = rule.args
        .map((a) => (a.kind === "exact" ? a.value : "*"))
        .join(" ");
      return (
        <div className="min-w-0 flex-1 text-[11.5px]">
          <span className="text-ink-3">bash</span>{" "}
          <span className="font-mono text-ink">{rule.cmd}</span>{" "}
          <span className="font-mono text-ink-2">{argText}</span>
        </div>
      );
    }
  }
}
