// Browser adapter — minimal stub returning empty data.
//
// In v1 we don't ship a hosted-web build that talks to a remote rig;
// running in the browser is for component-level inspection during
// development. Each call resolves with a sane empty value so screens
// render their empty states instead of throwing.
//
// When remote rigs ship, this file gets replaced with an HTTP+SSE
// implementation pointing at a daemon serving over TLS.

import type {
  AgentEventListener,
  ArchEventListener,
  FileEventListener,
  Rpc,
  TerminalEventListener,
  Unlisten,
} from "./index";
import type {
  HetznerCreateServerSpec,
  KVStore,
  SessionId,
  TerminalOpenSpec,
  TerminalOpenLocalSpec,
  WireAgentSettings,
  WireApprovalChoice,
  WireApprovalRule,
  WireApprovalRuleset,
  WireIdentity,
  WireRemoteRigSpec,
} from "./types";

const empty = async <T>(value: T): Promise<T> => value;

/** Two fake identities for component-level inspection of the Settings →
 *  Identities section in dev-server mode. Real fingerprints; the public
 *  key body is fabricated and won't match anything upstream. */
const BROWSER_MOCK_IDENTITIES: WireIdentity[] = [
  {
    id: "SHA256:browser-mock-yah-personal",
    name: "yah-personal",
    algorithm: "ssh-ed25519",
    publicKey: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBrowserMockYahPersonal leif@laptop",
    source: { kind: "yahGenerated", privateKeyPath: "/Users/dev/.yah/keys/yah-personal" },
    authorizedAt: [
      {
        kind: "hetzner",
        projectId: "default",
        keyIdInHetzner: 12345,
        name: "yah-personal",
        lastSeen: Date.now() - 2 * 3600 * 1000,
      },
      {
        kind: "github",
        account: "leif",
        keyId: 67890,
        title: "yah-personal",
        lastSeen: Date.now() - 2 * 3600 * 1000,
      },
    ],
    createdAt: Date.now() - 7 * 86400 * 1000,
    lastUsedAt: Date.now() - 30 * 60 * 1000,
  },
  {
    id: "SHA256:browser-mock-id-ed25519",
    name: "id_ed25519",
    algorithm: "ssh-ed25519",
    publicKey: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIBrowserMockIdEd25519 leif@laptop",
    source: {
      kind: "imported",
      privateKeyPath: "/Users/dev/.ssh/id_ed25519",
      publicKeyPath: "/Users/dev/.ssh/id_ed25519.pub",
    },
    authorizedAt: [],
    createdAt: Date.now() - 30 * 86400 * 1000,
    lastUsedAt: null,
  },
];

export const rpc: Rpc = {
  openRig: (_rigId: string) =>
    empty({
      filesSeen: 0,
      filesIndexed: 0,
      filesSkipped: 0,
      parseErrors: 0,
    }),
  closeRig: (_rigId: string) => empty(undefined),
  subgraph: (_rigId: string, { root }) =>
    empty({ root, nodes: [], edges: [], truncated: false }),
  lookup: (_rigId: string) => empty({ ids: [] }),
  node: (_rigId: string) => empty(null),
  neighbors: (_rigId: string) => empty({ edges: [] }),
  roots: (_rigId: string) => empty({ roots: [] }),
  stats: (_rigId: string) =>
    empty({
      node_count: 0,
      edge_count: 0,
      by_lang: {},
      by_kind: {},
    }),
  languages: (_rigId: string) => empty([]),
  rigList: () => empty([]),
  rigAttach: (path: string, name: string) =>
    empty({
      id: `rig:browser-${path}`,
      name,
      path,
      kind: "local" as const,
      reachable: true,
      lastActiveAt: Date.now(),
    }),
  rigAttachRemote: (spec: WireRemoteRigSpec) =>
    empty({
      id: `rig:browser-remote-${spec.user}@${spec.host}:${spec.port ?? 22}${spec.workspacePath}`,
      name: spec.name?.trim() || spec.host,
      path: spec.workspacePath,
      kind: "remote" as const,
      // Browser dev has no SSH transport — surface as unreachable so
      // the dot reads oxblood instead of pretending the rig is up.
      reachable: false,
      lastActiveAt: Date.now(),
      host: spec.host,
      port: spec.port,
      user: spec.user,
      keyPath: spec.keyPath,
    }),
  rigDetach: (_rigId: string) => empty(false),
  rigSetActive: (_rigId: string) => empty(false),
  listTickets: (_rigId: string) => empty({ tickets: [] }),
  listRelays: (_rigId: string) => empty({ relays: [] }),
  getTicket: (_rigId: string) => empty({ ticket: null }),
  validate: (_rigId: string) => empty({ violations: [] }),
  /* Browser dev mode has no daemon to render against; the TicketCard
     falls back to the local prompt.ts builder when this returns null. */
  ticketPrompt: (_rigId: string) => empty({ markdown: null }),
  listAuthoredFiles: (_rigId: string) => empty({ files: [] }),
  readAuthoredFile: (_rigId: string, relPath: string) =>
    empty({ rel_path: relPath, content: "", bytes: 0 }),
  /* Browser preview has no rig filesystem; FileTree shows an empty
     tree, dir.watch is a silent no-op, file.event never fires. */
  dirList: (_rigId: string, path: string) => empty({ path, entries: [] }),
  dirWatch: (_rigId: string, _path: string) => empty(0),
  fileUnwatch: (_rigId: string, _id: number) => empty(undefined),
  onFileEvent: async (_listener: FileEventListener): Promise<Unlisten> => {
    return () => {};
  },
  archiveTicket: (_rigId: string, _id: string) => empty(undefined),
  reindexPath: (_rigId: string) => empty(undefined),
  touch: (_rigId: string) => empty(undefined),
  onEvent: async (_listener: ArchEventListener): Promise<Unlisten> => {
    // No event source in the browser stub.
    return () => {};
  },
  // No agent runtime reachable from a tab — every entry point rejects
  // with a clear "Browser preview" message so the empty state in
  // AgentView surfaces the cause rather than a generic transport error.
  agent: {
    startSession: (_rigId: string, ticketId: string) =>
      Promise.reject<never>(
        new Error(
          `Browser preview: cannot start agent session for ticket ${ticketId} — run under Tauri.`,
        ),
      ),
    startChatSession: (_rigId: string, engine: string, _model?: string) =>
      Promise.reject<never>(
        new Error(
          `Browser preview: cannot start ${engine} chat session — run under Tauri.`,
        ),
      ),
    send: (sessionId: SessionId, _text: string) =>
      Promise.reject<never>(
        new Error(
          `Browser preview: cannot send to ${sessionId} — run under Tauri.`,
        ),
      ),
    stop: (_sessionId: SessionId) => empty(false),
    listSessions: () => empty([]),
    listModels: (_provider: string) => empty([] as string[]),
    onEvent: async (_listener: AgentEventListener): Promise<Unlisten> => {
      return () => {};
    },
    approval: {
      decide: (
        _rigId: string,
        _sessionId: SessionId,
        _requestId: string,
        _choice: WireApprovalChoice,
      ) => empty(false),
      rulesList: (_rigId: string) =>
        empty<WireApprovalRuleset>({ version: "1", rules: [] }),
      rulesAdd: (_rigId: string, _rule: WireApprovalRule) =>
        empty<WireApprovalRuleset>({ version: "1", rules: [] }),
      rulesRemove: (_rigId: string, _index: number) =>
        empty<WireApprovalRuleset>({ version: "1", rules: [] }),
    },
    settings: {
      get: (_rigId: string) =>
        empty<WireAgentSettings>({ version: "1", agentWritersEnabled: false }),
      set: (_rigId: string, settings: WireAgentSettings) => empty(settings),
    },
  },
  // No keychain reachable from a tab — set/delete swallow silently and
  // has always reports false. The Settings panel switches its banner to
  // "Browser preview — keys not persisted" when env.kind === 'browser'.
  apiKey: {
    set: (_provider: string, _token: string) => empty(undefined),
    has: (_provider: string) => empty(false),
    delete: (_provider: string) => empty(false),
  },
  // No filesystem / keychain from a tab. Return a fixed mock list so the
  // Settings → Identities section is inspectable in dev-server mode;
  // mutations reject loudly so the renderer surfaces "run under Tauri".
  identity: {
    list: () => empty(BROWSER_MOCK_IDENTITIES),
    create: (name: string) =>
      Promise.reject<WireIdentity>(
        new Error(`Browser preview: cannot generate identity ${name} — run under Tauri.`),
      ),
    import: (publicKeyPath: string) =>
      Promise.reject<WireIdentity>(
        new Error(`Browser preview: cannot import ${publicKeyPath} — run under Tauri.`),
      ),
    remove: (_id: string) => empty(false),
    probeAll: () =>
      empty({
        identitiesTotal: BROWSER_MOCK_IDENTITIES.length,
        localAdded: 0,
        hetzner: { kind: "skipped" as const, reason: "Browser preview — run under Tauri to probe Hetzner." },
        github: { kind: "skipped" as const, reason: "Browser preview — run under Tauri to probe GitHub." },
      }),
    probeHetzner: (_id: string) =>
      empty({ kind: "skipped" as const, reason: "Browser preview — run under Tauri." }),
    probeGithub: (_id: string) =>
      empty({ kind: "skipped" as const, reason: "Browser preview — run under Tauri." }),
    authorizeHetzner: (_id: string, name: string) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot authorize ${name} at Hetzner — run under Tauri.`),
      ),
    deauthorizeHetzner: (_id: string) => empty(false),
    authorizeGithub: (_id: string, title: string) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot authorize ${title} at GitHub — run under Tauri.`),
      ),
    deauthorizeGithub: (_id: string) => empty(false),
  },
  // No subprocess reach from a tab — surface "not installed" so the
  // AgentProvidersPanel renders its "run under Tauri to probe" hint
  // instead of pretending the binary or service is up.
  probe: {
    claudeCli: () =>
      empty({
        installed: false,
        version: null,
        path: null,
        error: "Browser preview — run under Tauri to probe `claude`.",
      }),
    ollamaServe: () =>
      empty({
        running: false,
        error: null,
      }),
  },
  // No Hetzner token reachable from a tab — return an empty list so the
  // Infra tab renders its "no servers" state without surfacing a
  // misleading transport error in browser preview.
  hetzner: {
    listServers: () => empty([]),
    listSshKeys: () => empty([]),
    uploadSshKey: (name: string, publicKey: string) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot upload SSH key ${name} (${publicKey.slice(0, 24)}…)`),
      ),
    createServer: (spec: HetznerCreateServerSpec) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot create server ${spec.name}`),
      ),
    listServerTypes: () => empty([]),
    listLocations: () => empty([]),
    listImages: () => empty([]),
  },
  // No filesystem access from a tab — listLocal returns []; generate
  // rejects loudly so the renderer surfaces the unsupported path
  // instead of silently producing fake key metadata.
  ssh: {
    listLocal: () => empty([]),
    generate: (name: string) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot generate SSH key ${name}`),
      ),
  },
  // No SSH transport from a tab — opening rejects so TerminalView
  // surfaces a clear "run under Tauri" message instead of an inert
  // pane. listSessions returns [] so the rail renders empty.
  terminal: {
    openSsh: (spec: TerminalOpenSpec) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot open SSH session to ${spec.host}`),
      ),
    openLocal: (_spec: TerminalOpenLocalSpec) =>
      Promise.reject<never>(
        new Error(`Browser preview: cannot spawn local PTY`),
      ),
    input: (_sessionId: string, _bytesB64: string) => empty(undefined),
    resize: (_sessionId: string, _cols: number, _rows: number) => empty(undefined),
    close: (_sessionId: string) => empty(false),
    listSessions: () => empty([]),
    onEvent: async (_listener: TerminalEventListener): Promise<Unlisten> => {
      return () => {};
    },
  },
};

/** Browser stub for the folder picker. There's no native equivalent we can
 *  reach from a tab, so we just return null — App.tsx falls back to a
 *  prompt() for browser-dev rig attaches. */
export async function pickFolder(): Promise<string | null> {
  return null;
}

// Local-storage keys are page-global, so namespace everything we own
// behind a single prefix. Anyone reading the storage from devtools can
// still tell at a glance which entries belong to yah-ui.
const KV_PREFIX = "yah-ui:";

function safeStorage(): Storage | null {
  try {
    if (typeof window === "undefined") return null;
    return window.localStorage;
  } catch {
    // Some embeds (incognito quotas, sandboxed iframes) throw on access.
    return null;
  }
}

export const kv: KVStore = {
  async get<T = unknown>(key: string): Promise<T | null> {
    const s = safeStorage();
    if (!s) return null;
    const raw = s.getItem(KV_PREFIX + key);
    if (raw === null) return null;
    try {
      return JSON.parse(raw) as T;
    } catch {
      return null;
    }
  },
  async set<T = unknown>(key: string, value: T): Promise<void> {
    const s = safeStorage();
    if (!s) return;
    try {
      s.setItem(KV_PREFIX + key, JSON.stringify(value));
    } catch {
      // Quota exceeded or serialization failure — best-effort cache, drop silently.
    }
  },
  async remove(key: string): Promise<void> {
    const s = safeStorage();
    if (!s) return;
    s.removeItem(KV_PREFIX + key);
  },
  async keys(): Promise<string[]> {
    const s = safeStorage();
    if (!s) return [];
    const out: string[] = [];
    for (let i = 0; i < s.length; i++) {
      const k = s.key(i);
      if (k !== null && k.startsWith(KV_PREFIX)) {
        out.push(k.slice(KV_PREFIX.length));
      }
    }
    return out;
  },
};
