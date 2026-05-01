import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { getEnv } from "../../env";

/* OS-keychain-backed token storage via env().rpc.apiKey. The renderer never
   sees a token after first set — `has(provider)` is the only credential
   affordance available here. The "test" affordance round-trips Rust-side and
   currently lands provider-by-provider (Hetzner with R027-F6, Cloudflare in
   a follow-up); until then it returns a "verify pending" result so the UI
   stays consistent. */

export type ApiKeyProvider =
  | "cloudflare"
  | "hetzner"
  | "anthropic"
  | "anthropic-oauth"
  | "openai"
  | "ollama";

/* Agent providers share the same keychain backing as infra providers but
   serve a different surface (the agent runtime — see app/tauri/src/agent.rs).
   Keeping them in one context means `has()` is uniform across the app and
   the AgentView empty-state can probe with the same hook the SettingsModal
   uses. */
export const AGENT_PROVIDERS: ApiKeyProvider[] = [
  "anthropic",
  "anthropic-oauth",
  "openai",
  "ollama",
];

const PROVIDERS: ApiKeyProvider[] = [
  "cloudflare",
  "hetzner",
  ...AGENT_PROVIDERS,
];

export type ApiKeyTestResult =
  | { ok: true; detail: string }
  | { ok: false; error: string };

export interface ApiKeysApi {
  /** Sync read of the cached has-state. Initial load is async (see provider);
   *  consumers see `false` until the first probe resolves on mount. */
  has: (provider: ApiKeyProvider) => boolean;
  set: (provider: ApiKeyProvider, token: string) => Promise<void>;
  remove: (provider: ApiKeyProvider) => Promise<void>;
  importCodexOpenAi: () => Promise<boolean>;
  test: (provider: ApiKeyProvider) => Promise<ApiKeyTestResult>;
  /** "tauri" once boot resolves; "browser" under dev-server; null while the
   *  env adapter is still loading. SettingsModal uses this to swap its
   *  banner copy. */
  envKind: "tauri" | "browser" | null;
}

const ApiKeysContext = createContext<ApiKeysApi | null>(null);

export function ApiKeysProvider({ children }: { children: ReactNode }) {
  const [hasMap, setHasMap] = useState<Record<string, boolean>>({});
  const [envKind, setEnvKind] = useState<"tauri" | "browser" | null>(null);

  // Boot: probe each known provider once so synchronous has() reflects
  // keychain reality. We swallow per-provider failures — a missing
  // keychain entry is the common case, not an error.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const env = await getEnv();
      if (cancelled) return;
      setEnvKind(env.kind);
      const next: Record<string, boolean> = {};
      for (const p of PROVIDERS) {
        try {
          next[p] = await env.rpc.apiKey.has(p);
        } catch {
          next[p] = false;
        }
      }
      if (cancelled) return;
      setHasMap(next);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const has = useCallback(
    (provider: ApiKeyProvider) => Boolean(hasMap[provider]),
    [hasMap],
  );

  const set = useCallback(
    async (provider: ApiKeyProvider, token: string) => {
      const env = await getEnv();
      await env.rpc.apiKey.set(provider, token);
      setHasMap((prev) => ({ ...prev, [provider]: true }));
    },
    [],
  );

  const remove = useCallback(async (provider: ApiKeyProvider) => {
    const env = await getEnv();
    await env.rpc.apiKey.delete(provider);
    setHasMap((prev) => ({ ...prev, [provider]: false }));
  }, []);

  const importCodexOpenAi = useCallback(async () => {
    const env = await getEnv();
    const imported = await env.rpc.apiKey.importCodexOpenAi();
    if (imported) {
      setHasMap((prev) => ({ ...prev, openai: true }));
    }
    return imported;
  }, []);

  const test = useCallback(
    async (provider: ApiKeyProvider): Promise<ApiKeyTestResult> => {
      const env = await getEnv();
      if (env.kind === "browser") {
        return {
          ok: false,
          error:
            "Browser preview — verify is not available without secure storage.",
        };
      }
      if (!hasMap[provider]) {
        return { ok: false, error: "No token stored" };
      }
      // Tokens live in the OS keychain after F4 — verify must round-trip
      // Rust-side (provider client reads the token, hits the upstream API).
      // Hetzner lands with R027-F6; Cloudflare in a follow-up. Agent
      // providers (anthropic/openai/ollama) verify on the next start_session
      // call — no idle health-check round-trip yet.
      const pending: Record<ApiKeyProvider, string> = {
        hetzner: "Verify pending the Rust-side Hetzner client (R027-F6).",
        cloudflare: "Verify pending the Rust-side Cloudflare client.",
        anthropic:
          "Verified implicitly on the next agent start — no idle round-trip yet.",
        "anthropic-oauth":
          "Verified implicitly on the next agent start — no idle round-trip yet.",
        openai:
          "Verified implicitly on the next agent start — no idle round-trip yet.",
        ollama:
          "Verified implicitly on the next agent start — no idle round-trip yet.",
      };
      return { ok: false, error: pending[provider] };
    },
    [hasMap],
  );

  const api = useMemo<ApiKeysApi>(
    () => ({ has, set, remove, importCodexOpenAi, test, envKind }),
    [has, set, remove, importCodexOpenAi, test, envKind],
  );

  return (
    <ApiKeysContext.Provider value={api}>{children}</ApiKeysContext.Provider>
  );
}

export function useApiKeys(): ApiKeysApi {
  const ctx = useContext(ApiKeysContext);
  if (!ctx) {
    throw new Error("useApiKeys must be used inside <ApiKeysProvider>");
  }
  return ctx;
}
