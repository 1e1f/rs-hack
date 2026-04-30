import { useEffect, useMemo, useState } from "react";
import { getEnv } from "../../env";
import { Icon } from "../shared/Glyph";
import { Splash } from "../shared/Splash";
import {
  useApiKeys,
  type ApiKeyProvider,
} from "../shell/api-keys-context";
import {
  AgentProvidersPanel,
  useAnyAgentConfigured,
} from "./AgentProvidersPanel";

interface NoSessionProps {
  /* Currently-active rig — needed to start a chat. `null` means the user
     hasn't picked any rig yet (rigList may even be empty); we surface
     that as the "pick a rig first" splash. */
  rigId: string | null;
  /* Currently-selected relay (relay-anchored sessions). `null` means
     unanchored — chat is the natural CTA. */
  relayId: string | null;
  /* Optional handler invoked when the user clicks "Start a chat". The
     engine string is `provider` form (e.g. `"openai"`); model is
     forwarded as a separate arg so the user can override the host's
     default without learning the `provider:model` colon syntax. */
  onStartChat?: (engine: string, model?: string) => void;
  onStart?: (relayId: string) => void;
}

interface ChatEngineOption {
  /* Engine spec passed to the backend — `parse_engine_payload` in
     app/tauri/src/agent.rs accepts these. */
  spec: string;
  label: string;
  /* Sub-label used to disambiguate when both providers are configured. */
  costHint: string;
  /* Placeholder for the model input — example values that the host's
     default falls through to when the user leaves the field blank. */
  modelPlaceholder: string;
  /* Default seed value for the input. Empty → backend fallback. */
  modelSeed: string;
  /* Keychain slots that satisfy this option. The Claude row accepts
     either preset's slot — `anthropic` (HAk, sk-ant- API key) or
     `anthropic-oauth` (HAo, crab-style bearer from `claude
     setup-token`). The host's `resolve_anthropic_auth` picks OAuth
     when both are populated; either works. */
  keyProviders: ApiKeyProvider[];
  /* Bypass the configured-credential filter — the runtime has a local
     fallback path so the row should appear even with no key. Today
     only ollama uses this (loopback to localhost:11434). */
  alwaysShow?: boolean;
}

/* Order matches the Settings → Agents card order. */
const PRIORITY: ChatEngineOption[] = [
  {
    spec: "claude",
    label: "Claude",
    costHint: "anthropic / crab",
    modelPlaceholder: "claude-opus-4-7",
    modelSeed: "",
    keyProviders: ["anthropic", "anthropic-oauth"],
  },
  {
    /* PVd preset — wraps the `claude` CLI as a subprocess. Auth is
       delegated entirely (Claude Code manages its own login), so no
       keychain slot gates this row; we always show it and let the
       spawn surface a clear error if the binary isn't on PATH. Sister
       to the "Claude" row above — handy for cross-comparing the
       HTTP-direct (HA-family) and subprocess (PVd) paths in two
       concurrent chats. */
    spec: "claude-cli",
    label: "Claude Code",
    costHint: "subprocess · subscription",
    modelPlaceholder: "claude-opus-4-7",
    modelSeed: "",
    keyProviders: [],
    alwaysShow: true,
  },
  {
    spec: "openai",
    label: "OpenAI",
    costHint: "API key",
    modelPlaceholder: "gpt-4o",
    modelSeed: "",
    keyProviders: ["openai"],
  },
  {
    spec: "ollama",
    label: "Ollama",
    // Cloud models have to be entered exactly as the upstream serves
    // them (e.g. `gpt-oss:20b`); local Ollama accepts whatever you've
    // pulled. The placeholder hints at a known cloud free-tier name —
    // see https://docs.ollama.com/cloud — but the user is in charge.
    costHint: "local / cloud",
    modelPlaceholder: "gpt-oss:20b  (cloud)  ·  llama3.2  (local)",
    modelSeed: "",
    keyProviders: ["ollama"],
    alwaysShow: true,
  },
];

/* Three states:
   - No agent provider configured at all → render the credential bootstrap
     panel inline.
   - Configured + relay selected → splash with "start agent on this relay".
   - Configured + no relay → chat-ready picker (provider × model). */
export function NoSession({
  rigId,
  relayId,
  onStart,
  onStartChat,
}: NoSessionProps) {
  const anyConfigured = useAnyAgentConfigured();
  const api = useApiKeys();

  const chatOptions = useMemo<ChatEngineOption[]>(() => {
    return PRIORITY.filter(
      (opt) => opt.alwaysShow || opt.keyProviders.some((p) => api.has(p)),
    );
  }, [api]);

  if (!anyConfigured) {
    return (
      <div className="flex flex-1 items-start justify-center overflow-y-auto bg-paper/90 px-6 py-8">
        <AgentProvidersPanel
          compact
          heading="Configure an agent provider to begin"
          subheading="The agent runtime dispatches turns to one of these. Pick whichever path you prefer — you can change later from Settings → Agents."
        />
      </div>
    );
  }

  if (relayId) {
    return (
      <div className="flex flex-1 items-center justify-center bg-paper/90">
        <div className="flex flex-col items-center">
          <Splash
            variant="lantern"
            caption="No session yet"
            sub="Start an agent on this relay to begin. The session runs server-side on the rig host."
          />
          <button
            onClick={() => onStart?.(relayId)}
            className="mt-2 flex items-center gap-1.5 rounded border border-accent/40 bg-accent px-3 py-1.5 text-[12px] text-vellum hover:bg-accent-2"
          >
            <Icon name="play" size={12} />
            start agent on {relayId}
          </button>
        </div>
      </div>
    );
  }

  const canChat = Boolean(rigId);
  return (
    <div className="flex flex-1 items-start justify-center overflow-y-auto bg-paper/90 px-6 py-10">
      <div className="flex w-full max-w-[520px] flex-col items-center text-center">
        <Splash
          variant="lantern"
          caption="Just chat with the agent"
          sub={
            canChat
              ? "Pick a provider and (optionally) a model. Leave the model field blank to use the provider's default. KG-anchored chat (codebase Q&A) and relay-anchored chat are next."
              : "Pick a rig from the title-bar selector to start a chat."
          }
        />
        {canChat && (
          <div className="mt-4 flex w-full flex-col gap-2">
            {chatOptions.map((opt) => (
              <ChatEngineRow
                key={opt.spec}
                option={opt}
                onStart={(model) => onStartChat?.(opt.spec, model)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

type ModelsState =
  | { phase: "idle" }
  | { phase: "loading" }
  | { phase: "ready"; models: string[] }
  | { phase: "error"; message: string };

function ChatEngineRow({
  option,
  onStart,
}: {
  option: ChatEngineOption;
  onStart: (model?: string) => void;
}) {
  const [model, setModel] = useState(option.modelSeed);
  const [models, setModels] = useState<ModelsState>({ phase: "idle" });

  /* Probe the upstream catalogue when the row mounts. Cheap (one HTTP
     GET) and runs in the background — the text input stays usable
     while it resolves, which matters for local Ollama with empty
     /v1/models responses. */
  useEffect(() => {
    let cancelled = false;
    setModels({ phase: "loading" });
    void (async () => {
      try {
        const env = await getEnv();
        const ids = await env.rpc.agent.listModels(option.spec);
        if (cancelled) return;
        setModels({ phase: "ready", models: ids });
      } catch (e) {
        if (cancelled) return;
        setModels({
          phase: "error",
          message: e instanceof Error ? e.message : String(e),
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [option.spec]);

  const useDropdown =
    models.phase === "ready" && models.models.length > 0;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        const m = model.trim();
        onStart(m || undefined);
      }}
      className="flex items-center gap-2 rounded border border-rule/40 bg-vellum/40 px-3 py-2"
    >
      <div className="flex w-[110px] shrink-0 flex-col items-start text-left">
        <span className="text-[13px] font-medium text-ink">{option.label}</span>
        <span className="text-[10.5px] text-ink-3">{option.costHint}</span>
      </div>
      {useDropdown ? (
        <select
          value={model}
          onChange={(e) => setModel(e.target.value)}
          className="min-w-0 flex-1 rounded border border-rule/40 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none focus:border-accent/60"
        >
          <option value="">— provider default —</option>
          {(models as { phase: "ready"; models: string[] }).models.map((id) => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
      ) : (
        <input
          type="text"
          value={model}
          onChange={(e) => setModel(e.target.value)}
          placeholder={
            models.phase === "loading"
              ? "loading models…"
              : option.modelPlaceholder
          }
          spellCheck={false}
          className="min-w-0 flex-1 rounded border border-rule/40 bg-paper-2 px-2 py-1 font-mono text-[11px] text-ink outline-none placeholder:text-ink-4 focus:border-accent/60"
        />
      )}
      <button
        type="submit"
        className="flex items-center gap-1 rounded bg-accent px-2.5 py-1 text-[11.5px] text-vellum hover:bg-accent-2"
      >
        <Icon name="play" size={11} />
        Start
      </button>
    </form>
  );
}
