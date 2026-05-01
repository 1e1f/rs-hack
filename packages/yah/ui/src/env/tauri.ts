// Tauri adapter — dynamically imported by env/index.ts only when running
// under the Tauri host. Calls into yah-tauri's #[tauri::command] handlers.

import type {
  AgentEventListener,
  ArchEventListener,
  FileEventListener,
  Rpc,
  TerminalEventListener,
  Unlisten,
} from "./index";
import type {
  ClaudeCliProbe,
  GetTicketParams,
  GetTicketResult,
  HetznerCreateServerSpec,
  HetznerImage,
  HetznerLocation,
  HetznerServer,
  HetznerServerType,
  HetznerSshKey,
  IndexReason,
  KVStore,
  Lang,
  LocalSshKey,
  ListAuthoredFilesResult,
  ListRelaysParams,
  ListRelaysResult,
  ListTicketsParams,
  ListTicketsResult,
  LookupParams,
  LookupResult,
  NeighborsParams,
  NeighborsResult,
  NodeFull,
  NodeId,
  OllamaServeProbe,
  ReadAuthoredFileResult,
  RootsParams,
  RootsResult,
  SessionId,
  StatsResult,
  Subgraph,
  SubgraphParams,
  TerminalOpenSpec,
  TerminalOpenLocalSpec,
  TicketPromptParams,
  TicketPromptResult,
  ValidateResult,
  WalkSummary,
  WireAgentSettings,
  WireApprovalChoice,
  WireApprovalRule,
  WireApprovalRuleset,
  WireAuthorization,
  WireDirListResult,
  WireFileReadRange,
  WireFileReadResult,
  WireIdentity,
  WireProbeReport,
  WireRemoteRigSpec,
  WireRigDto,
  WireRigFileEvent,
  WireScope,
  WireWatchId,
  WireSessionHistoryRow,
  WireSessionMeta,
  WireSessionSummary,
  WireSingleProbeResult,
  WireStartSessionResult,
  WireTerminalSessionSummary,
} from "./types";

// `@tauri-apps/api` and `@tauri-apps/plugin-store` are runtime dependencies
// of yah-ui — see package.json. Imports happen inside this file (loaded
// lazily by env/index.ts) so the browser bundle never needs them.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LazyStore } from "@tauri-apps/plugin-store";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

const ARCH_EVENT = "arch:event";
const AGENT_EVENT = "agent:event";
const TERMINAL_EVENT = "terminal:event";
const FILE_EVENT = "file:event";

export const rpc: Rpc = {
  openRig(rigId: string) {
    return invoke<WalkSummary>("arch_open_rig", { rigId });
  },
  closeRig(rigId: string) {
    return invoke<void>("arch_close_rig", { rigId });
  },
  subgraph(rigId: string, params: SubgraphParams) {
    return invoke<Subgraph>("arch_subgraph", { rigId, params });
  },
  lookup(rigId: string, params: LookupParams) {
    return invoke<LookupResult>("arch_lookup", { rigId, params });
  },
  node(rigId: string, id: NodeId) {
    return invoke<NodeFull | null>("arch_node", { rigId, id });
  },
  neighbors(rigId: string, params: NeighborsParams) {
    return invoke<NeighborsResult>("arch_neighbors", { rigId, params });
  },
  roots(rigId: string, params: RootsParams) {
    return invoke<RootsResult>("arch_roots", { rigId, params });
  },
  stats(rigId: string) {
    return invoke<StatsResult>("arch_stats", { rigId });
  },
  languages(rigId: string) {
    return invoke<Lang[]>("arch_languages", { rigId });
  },
  rigList() {
    return invoke<WireRigDto[]>("rig_list");
  },
  rigAttach(path: string, name: string) {
    return invoke<WireRigDto>("rig_attach", { path, name });
  },
  rigAttachRemote(spec: WireRemoteRigSpec) {
    return invoke<WireRigDto>("rig_attach_remote", { spec });
  },
  rigDetach(rigId: string) {
    return invoke<boolean>("rig_detach", { rigId });
  },
  rigSetActive(rigId: string) {
    return invoke<boolean>("rig_set_active", { rigId });
  },
  listTickets(rigId: string, params?: ListTicketsParams) {
    return invoke<ListTicketsResult>("arch_list_tickets", {
      rigId,
      params: params ?? null,
    });
  },
  listRelays(rigId: string, params?: ListRelaysParams) {
    return invoke<ListRelaysResult>("arch_list_relays", {
      rigId,
      params: params ?? null,
    });
  },
  getTicket(rigId: string, params: GetTicketParams) {
    return invoke<GetTicketResult>("arch_get_ticket", { rigId, params });
  },
  validate(rigId: string, scope?: WireScope) {
    return invoke<ValidateResult>("arch_validate", {
      rigId,
      params: scope ? { scope } : null,
    });
  },
  ticketPrompt(rigId: string, params: TicketPromptParams) {
    return invoke<TicketPromptResult>("arch_ticket_prompt", { rigId, params });
  },
  listAuthoredFiles(rigId: string) {
    return invoke<ListAuthoredFilesResult>("arch_list_authored_files", { rigId });
  },
  readAuthoredFile(rigId: string, relPath: string) {
    return invoke<ReadAuthoredFileResult>("arch_read_authored_file", {
      rigId,
      params: { rel_path: relPath },
    });
  },
  dirList(rigId: string, path: string) {
    return invoke<WireDirListResult>("dir_list", {
      rigId,
      params: { path },
    });
  },
  fileRead(rigId: string, path: string, range?: WireFileReadRange) {
    return invoke<WireFileReadResult>("file_read", {
      rigId,
      params: { path, range: range ?? null },
    });
  },
  async dirWatch(rigId: string, path: string) {
    const r = await invoke<{ id: WireWatchId }>("dir_watch", {
      rigId,
      params: { path },
    });
    return r.id;
  },
  fileUnwatch(rigId: string, id: WireWatchId) {
    return invoke<void>("file_unwatch", {
      rigId,
      params: { id },
    });
  },
  async onFileEvent(listener: FileEventListener): Promise<Unlisten> {
    const off = await listen(FILE_EVENT, (e) => {
      listener(e.payload as WireRigFileEvent);
    });
    return () => off();
  },
  archiveTicket(rigId: string, id: string) {
    return invoke<void>("arch_archive_ticket", { rigId, id });
  },
  reindexPath(rigId: string, path: string, reason: IndexReason) {
    return invoke<void>("arch_reindex_path", { rigId, path, reason });
  },
  touch(rigId: string, paths: string[], tool: string, relay: string) {
    return invoke<void>("arch_touch", { rigId, paths, tool, relay });
  },
  async onEvent(listener: ArchEventListener): Promise<Unlisten> {
    const off = await listen(ARCH_EVENT, (e) => {
      // Tauri wraps payloads in `{ payload, event, ... }`. We forward
      // payload as-is — it serializes to our ArchEvent type.
      listener(e.payload as Parameters<ArchEventListener>[0]);
    });
    return () => off();
  },
  agent: {
    startSession(rigId: string, ticketId: string) {
      return invoke<WireStartSessionResult>("agent_start_session", {
        rigId,
        ticketId,
      });
    },
    startChatSession(rigId: string, engine: string, model?: string) {
      return invoke<WireStartSessionResult>("agent_start_chat_session", {
        rigId,
        engine,
        model: model ?? null,
      });
    },
    send(sessionId: SessionId, text: string) {
      return invoke<void>("agent_send", { sessionId, text });
    },
    stop(sessionId: SessionId) {
      return invoke<boolean>("agent_stop", { sessionId });
    },
    listSessions() {
      return invoke<WireSessionSummary[]>("agent_list_sessions");
    },
    listModels(provider: string) {
      return invoke<string[]>("agent_list_models", { provider });
    },
    async onEvent(listener: AgentEventListener): Promise<Unlisten> {
      const off = await listen(AGENT_EVENT, (e) => {
        listener(e.payload as Parameters<AgentEventListener>[0]);
      });
      return () => off();
    },
    approval: {
      decide(
        rigId: string,
        sessionId: SessionId,
        requestId: string,
        choice: WireApprovalChoice,
      ) {
        return invoke<boolean>("agent_approval_decide", {
          rigId,
          sessionId,
          requestId,
          choice,
        });
      },
      rulesList(rigId: string) {
        return invoke<WireApprovalRuleset>("agent_approval_rules_list", { rigId });
      },
      rulesAdd(rigId: string, rule: WireApprovalRule) {
        return invoke<WireApprovalRuleset>("agent_approval_rules_add", {
          rigId,
          rule,
        });
      },
      rulesRemove(rigId: string, index: number) {
        return invoke<WireApprovalRuleset>("agent_approval_rules_remove", {
          rigId,
          index,
        });
      },
    },
    settings: {
      get(rigId: string) {
        return invoke<WireAgentSettings>("agent_settings_get", { rigId });
      },
      set(rigId: string, settings: WireAgentSettings) {
        return invoke<WireAgentSettings>("agent_settings_set", {
          rigId,
          settings,
        });
      },
    },
    history: {
      list(rigId: string) {
        return invoke<WireSessionHistoryRow[]>("agent_session_history_list", { rigId });
      },
      reindex(rigId: string, sessionId: SessionId) {
        return invoke<WireSessionMeta>("agent_session_history_reindex", {
          rigId,
          sessionId,
        });
      },
    },
  },
  apiKey: {
    set(provider: string, token: string) {
      return invoke<void>("api_key_set", { provider, token });
    },
    has(provider: string) {
      return invoke<boolean>("api_key_has", { provider });
    },
    delete(provider: string) {
      return invoke<boolean>("api_key_delete", { provider });
    },
    importCodexOpenAi() {
      return invoke<boolean>("api_key_import_codex_openai");
    },
  },
  identity: {
    list() {
      return invoke<WireIdentity[]>("identity_list");
    },
    create(name: string) {
      return invoke<WireIdentity>("identity_create", { name });
    },
    import(publicKeyPath: string, name?: string) {
      return invoke<WireIdentity>("identity_import", {
        publicKeyPath,
        name: name ?? null,
      });
    },
    remove(id: string) {
      return invoke<boolean>("identity_remove", { id });
    },
    probeAll() {
      return invoke<WireProbeReport>("identity_probe_all");
    },
    probeHetzner(id: string) {
      return invoke<WireSingleProbeResult>("identity_probe_hetzner", { id });
    },
    probeGithub(id: string) {
      return invoke<WireSingleProbeResult>("identity_probe_github", { id });
    },
    authorizeHetzner(id: string, name: string) {
      return invoke<WireAuthorization>("identity_authorize_hetzner", {
        id,
        name,
      });
    },
    deauthorizeHetzner(id: string) {
      return invoke<boolean>("identity_deauthorize_hetzner", { id });
    },
    authorizeGithub(id: string, title: string) {
      return invoke<WireAuthorization>("identity_authorize_github", {
        id,
        title,
      });
    },
    deauthorizeGithub(id: string) {
      return invoke<boolean>("identity_deauthorize_github", { id });
    },
  },
  probe: {
    claudeCli() {
      return invoke<ClaudeCliProbe>("claude_cli_probe");
    },
    ollamaServe() {
      return invoke<OllamaServeProbe>("ollama_serve_probe");
    },
  },
  hetzner: {
    listServers() {
      return invoke<HetznerServer[]>("hetzner_list_servers");
    },
    listSshKeys() {
      return invoke<HetznerSshKey[]>("hetzner_list_ssh_keys");
    },
    uploadSshKey(name: string, publicKey: string) {
      return invoke<HetznerSshKey>("hetzner_upload_ssh_key", {
        name,
        publicKey,
      });
    },
    createServer(spec: HetznerCreateServerSpec) {
      return invoke<HetznerServer>("hetzner_create_server", { spec });
    },
    listServerTypes() {
      return invoke<HetznerServerType[]>("hetzner_list_server_types");
    },
    listLocations() {
      return invoke<HetznerLocation[]>("hetzner_list_locations");
    },
    listImages() {
      return invoke<HetznerImage[]>("hetzner_list_images");
    },
  },
  ssh: {
    listLocal() {
      return invoke<LocalSshKey[]>("ssh_key_list_local");
    },
    generate(name: string) {
      return invoke<LocalSshKey>("ssh_key_generate", { name });
    },
  },
  terminal: {
    openSsh(spec: TerminalOpenSpec) {
      return invoke<string>("terminal_open_ssh", { spec });
    },
    openLocal(spec: TerminalOpenLocalSpec) {
      return invoke<string>("terminal_open_local", { spec });
    },
    input(sessionId: string, bytesB64: string) {
      return invoke<void>("terminal_input", { sessionId, bytesB64 });
    },
    resize(sessionId: string, cols: number, rows: number) {
      return invoke<void>("terminal_resize", { sessionId, cols, rows });
    },
    close(sessionId: string) {
      return invoke<boolean>("terminal_close", { sessionId });
    },
    listSessions() {
      return invoke<WireTerminalSessionSummary[]>("terminal_list_sessions");
    },
    async onEvent(listener: TerminalEventListener): Promise<Unlisten> {
      const off = await listen(TERMINAL_EVENT, (e) => {
        listener(e.payload as Parameters<TerminalEventListener>[0]);
      });
      return () => off();
    },
  },
};

/** Native folder picker via the Tauri dialog plugin. Returns the chosen
 *  absolute path, or `null` if the user cancelled. */
export async function pickFolder(): Promise<string | null> {
  const result = await openDialog({ directory: true, multiple: false });
  if (result === null) return null;
  return Array.isArray(result) ? (result[0] ?? null) : result;
}

// Single store file in the platform app-data dir; keys are namespaced
// inside the file so different subsystems can share it without colliding.
const KV_STORE_PATH = "yah-ui-kv.json";

/** Build the Tauri-backed `KVStore`. The underlying `LazyStore` defers
 *  disk I/O until the first call, so this is cheap to construct on boot. */
export async function makeKv(): Promise<KVStore> {
  const store = new LazyStore(KV_STORE_PATH, { defaults: {}, autoSave: 250 });
  return {
    async get<T = unknown>(key: string): Promise<T | null> {
      const v = await store.get<T>(key);
      return v === undefined ? null : v;
    },
    async set<T = unknown>(key: string, value: T): Promise<void> {
      await store.set(key, value);
    },
    async remove(key: string): Promise<void> {
      await store.delete(key);
    },
    async keys(): Promise<string[]> {
      return store.keys();
    },
  };
}
