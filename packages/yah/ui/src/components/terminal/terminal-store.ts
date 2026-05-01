/* Module-level registry of live xterm.js Terminal instances.
 *
 * The Terminal objects must outlive any single React mount: when the
 * user flips away from the Terminal tab, the xterm DOM unmounts but
 * the Tauri side keeps streaming bytes — those need to land in the
 * Terminal's scrollback buffer regardless of whether anyone is
 * watching. So the store lives at module scope and React components
 * re-bind to the existing Terminal on remount.
 *
 * The store also owns a single subscription to the `terminal:event`
 * stream and routes events to the matching session. That keeps every
 * additional pane free of its own listener registration.
 */

import { useSyncExternalStore } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { Unicode11Addon } from "@xterm/addon-unicode11";

import { getEnv } from "../../env";
import type { TerminalOpenSpec, TerminalOpenLocalSpec } from "../../env/types";

export type SessionStatus =
  | "connecting"
  | "ready"
  | "closed"
  | "error";

export interface TerminalSession {
  /** Client-side id minted before the backend round-trip — stable for
   *  the entire session lifetime, used as the React key so the
   *  SessionPane (and the Terminal it hosts) doesn't remount when the
   *  backend handshake completes. */
  id: string;
  /** The rig that owned focus when this session was opened. Used by
   *  TerminalView to filter the rail + visible panes — switching rigs
   *  hides sessions belonging to the previous rig (their xterm
   *  Terminals stay alive in the background, consuming PTY bytes
   *  into scrollback) and surfaces only the new rig's sessions. Null
   *  for sessions opened with no active rig (rare). */
  rigId: string | null;
  /** Tauri-side session id, set once `terminal_open_*` resolves. Null
   *  while the SSH handshake is in flight. Renderer→backend IPC
   *  (terminal_input / terminal_resize / terminal_close) keys off this
   *  — keystrokes typed during the connecting window are dropped, and
   *  inbound `terminal:event` payloads route to a session by matching
   *  this against the event's `session_id`. */
  backendId: string | null;
  host: string;
  user: string;
  label: string;
  status: SessionStatus;
  /** Last error/closed reason — surfaced in the rail and pane footer. */
  statusDetail?: string;
  /** Server fingerprint reported on first connect (TOFU hook). */
  hostKeyFingerprint?: string;
  /** xterm.js Terminal — outlives React mounts. */
  term: Terminal;
  fit: FitAddon;
  search: SearchAddon;
  /** Element currently hosting `term.open(...)`, or null when detached. */
  attachedTo: HTMLElement | null;
}

type Listener = () => void;

/* Counter-backed client-side session id. Stable for the React key
   from the moment open()/openLocal() are called, so the SessionPane
   can mount + attach the Terminal *before* the backend responds —
   that's what makes the rail-spinner work. Distinct namespace from
   the backend's `term-…` ids so there's no risk of collision. */
let clientIdCounter = 0;
function mintClientSessionId(): string {
  clientIdCounter += 1;
  return `c-${Date.now().toString(36)}-${clientIdCounter}`;
}

/* xterm theme tuned to the yah ink/paper palette. Pulled from
   yah-ui/src/styles/globals.css; the values are static so we don't
   need to re-derive on every theme flip — the Terminal tab reads
   light first; we'll wire dark theme together with the global theme
   switcher in a follow-up. */
const XTERM_THEME = {
  background: "#0f0f0f",
  foreground: "#e6e6e6",
  cursor: "#d4a017",
  cursorAccent: "#0f0f0f",
  /* Translucent gold tint that reads as warm-on-dark without flipping
     the text colour — letting selectionForeground inherit keeps the
     original glyph colour, so the gold reads through instead of being
     masked by an overriding white. */
  selectionBackground: "rgba(212, 160, 23, 0.55)",
  selectionInactiveBackground: "rgba(212, 160, 23, 0.25)",
  black: "#1a1a1a",
  red: "#cf6a4c",
  green: "#8aaf6b",
  yellow: "#d4a017",
  blue: "#7e8aa8",
  magenta: "#a8859c",
  cyan: "#7da3a3",
  white: "#cccccc",
  brightBlack: "#5c5c5c",
  brightRed: "#e07d62",
  brightGreen: "#a3c97f",
  brightYellow: "#e8b943",
  brightBlue: "#9aa6c4",
  brightMagenta: "#c4a1b8",
  brightCyan: "#9bbfbf",
  brightWhite: "#ffffff",
};

class TerminalStore {
  private sessions = new Map<string, TerminalSession>();
  private listeners = new Set<Listener>();
  private activeId: string | null = null;
  /* Backend `terminal:event` payloads can arrive *before* the
     corresponding `terminal_open_*` invoke resolves — the writer
     task on the Tauri side emits `ready` from a spawned task that
     runs concurrently with the command's response. If that race
     wins, handleEvent has no session to match (backendId still
     null) and would silently drop the event, stranding the pane on
     the connecting spinner forever. We buffer unmatched events here
     and drain whenever a session's backendId is set. */
  private pendingEvents: import("../../env/types").WireTerminalEvent[] = [];
  /** `link-click` (cmd-click on a recognized file path); set by the
   *  view when it mounts so the click handler can route through the
   *  app's existing tab-nav helpers. */
  private linkHandler: ((target: string) => void) | null = null;
  private subscribePromise: Promise<void> | null = null;

  /** Subscribed lazily on first hook use; tearing down the listener
   *  isn't worth it when it's a singleton. */
  ensureSubscribed(): Promise<void> {
    if (this.subscribePromise) return this.subscribePromise;
    this.subscribePromise = (async () => {
      const env = await getEnv();
      await env.rpc.terminal.onEvent((e) => this.handleEvent(e));
    })();
    return this.subscribePromise;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Snapshot for `useSyncExternalStore` — must be referentially
   *  stable when nothing changed, otherwise React loops. We swap the
   *  cached snapshot only inside `notify`. */
  private snapshot: { sessions: TerminalSession[]; activeId: string | null } = {
    sessions: [],
    activeId: null,
  };
  getSnapshot(): typeof this.snapshot {
    return this.snapshot;
  }

  private notify() {
    this.snapshot = {
      sessions: [...this.sessions.values()],
      activeId: this.activeId,
    };
    for (const l of this.listeners) l();
  }

  setLinkHandler(fn: ((target: string) => void) | null) {
    this.linkHandler = fn;
  }
  getLinkHandler(): ((target: string) => void) | null {
    return this.linkHandler;
  }

  list(): TerminalSession[] {
    return [...this.sessions.values()];
  }
  get(id: string): TerminalSession | undefined {
    return this.sessions.get(id);
  }
  getActive(): TerminalSession | undefined {
    return this.activeId ? this.sessions.get(this.activeId) : undefined;
  }
  setActive(id: string | null) {
    if (id !== null && !this.sessions.has(id)) return;
    this.activeId = id;
    this.notify();
  }

  /** Open a fresh SSH session. Builds the Terminal synchronously and
   *  registers a `connecting`-state placeholder in the rail before
   *  the backend SSH handshake — so the operator gets immediate
   *  feedback (spinning rail entry + connecting overlay over the
   *  pane) instead of staring at an unresponsive button while the
   *  TLS negotiation lands. Returns the *client-side* session id;
   *  `session.backendId` is filled in once `terminal_open_ssh`
   *  resolves. */
  async open(spec: TerminalOpenSpec, opts: { rigId?: string | null } = {}): Promise<string> {
    await this.ensureSubscribed();
    const env = await getEnv();
    const clientId = mintClientSessionId();
    const ownerRigId = opts.rigId ?? null;

    const term = new Terminal({
      fontFamily: '"JetBrains Mono", "SF Mono", Menlo, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: "block",
      /* Match the backend's PTY default (DEFAULT_COLS/ROWS in
         terminal.rs). xterm's stock 80×24 fights zsh's PROMPT_SP
         when the PTY thinks it's 120-wide — zsh emits ~120 spaces
         to recover prompt position, xterm wraps them at col 80, and
         the wrapped row survives the prompt's `ESC[J` clear-from-
         cursor-down. fit() resizes both sides in lockstep on first
         attach, so this only matters for the brief pre-fit window
         before the operator sees anything. */
      cols: 120,
      rows: 32,
      /* 10k lines of scrollback — enough for `cargo build`, `journalctl`,
         and most postmortem trawling without ballooning memory. */
      scrollback: 10_000,
      allowProposedApi: true,
      /* macOS Option as Meta — matches Terminal.app's default; lets
         readline-style word jumps work over SSH. */
      macOptionIsMeta: true,
      theme: XTERM_THEME,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";

    /* Cmd-click on a recognized file path routes through the app's
       jumpToFile callback (currently re-roots ArchView on the file's
       basename). Plain clicks are ignored — the modifier-required
       gesture matches VS Code's "follow link" convention so it
       doesn't fight cursor selection. */
    term.registerLinkProvider({
      provideLinks: (lineNumber, callback) => {
        const line = term.buffer.active.getLine(lineNumber - 1);
        if (!line) return callback(undefined);
        const text = line.translateToString(true);
        const links: import("@xterm/xterm").ILink[] = [];
        for (const m of matchFilePaths(text)) {
          links.push({
            range: {
              start: { x: m.start + 1, y: lineNumber },
              end: { x: m.end, y: lineNumber },
            },
            text: m.match,
            activate: (event, target) => {
              if (!(event.metaKey || event.ctrlKey)) return;
              terminalStore.getLinkHandler()?.(target);
            },
          });
        }
        callback(links);
      },
    });

    /* Keystroke / resize pumps. Both bail when `backendId` is null —
       i.e. while the SSH handshake is in flight — so any stray
       keystrokes the user types into a connecting pane don't blow
       up on the IPC seam. xterm's onData also fires for terminal-
       generated CSI responses; same dropping rule applies, the
       remote end isn't there yet. */
    const encoder = new TextEncoder();
    term.onData((data) => {
      const s = this.sessions.get(clientId);
      if (!s?.backendId) return;
      const bytes = encoder.encode(data);
      void env.rpc.terminal.input(s.backendId, bytesToBase64(bytes));
    });
    term.onResize(({ cols, rows }) => {
      const s = this.sessions.get(clientId);
      if (!s?.backendId) return;
      void env.rpc.terminal.resize(s.backendId, cols, rows);
    });

    /* Cmd-K → local reset + Ctrl+L to the shell. term.reset() (not
       term.clear()) restores DEC special-graphics state so a remote
       MOTD that left alt-charset on doesn't bleed into the redrawn
       prompt (~→π, k→┐, z→≥). The follow-up Ctrl+L (\x0c) is what
       bash/zsh/fish all interpret as "clear screen + redraw the
       prompt", so the operator gets a fresh prompt instead of an
       empty buffer. Swallowed so Cmd+K isn't also forwarded to the
       PTY. */
    term.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.metaKey && !e.ctrlKey && !e.altKey && e.key === "k") {
        term.reset();
        const s = this.sessions.get(clientId);
        if (s?.backendId) {
          const ctrlL = bytesToBase64(new Uint8Array([0x0c]));
          void env.rpc.terminal.input(s.backendId, ctrlL);
        }
        return false;
      }
      return true;
    });

    /* Phase 1 (synchronous): register placeholder. SessionPane mounts
       immediately, attaches the Terminal to its host, and renders an
       empty xterm under a connecting overlay — operator gets feedback
       in the same frame as the click. */
    const session: TerminalSession = {
      id: clientId,
      rigId: ownerRigId,
      backendId: null,
      host: spec.host,
      user: spec.user ?? "root",
      label: spec.label ?? `${spec.user ?? "root"}@${spec.host}`,
      status: "connecting",
      statusDetail: `connecting to ${spec.host}…`,
      term,
      fit,
      search,
      attachedTo: null,
    };
    this.sessions.set(clientId, session);
    this.activeId = clientId;
    this.notify();

    /* Phase 2 (async): kick off the backend handshake without
       blocking. The caller (App.openTerminalForServer) returns
       immediately and the tab flips before the SSH round-trip. */
    void (async () => {
      try {
        const backendId = await env.rpc.terminal.openSsh(spec);
        session.backendId = backendId;
        /* Drain any backend events that arrived before backendId was
           known (the writer task can emit Ready before the command
           response lands). Status stays 'connecting' until the
           `ready` event fires — possibly via the drain path. */
        this.drainPendingFor(backendId);
        this.notify();
      } catch (err) {
        session.status = "error";
        session.statusDetail = err instanceof Error ? err.message : String(err);
        this.notify();
      }
    })();

    return clientId;
  }

  /** Spawn a local shell PTY for renderer-isolation diagnostics. Same
   *  Terminal/IPC setup as `open`, but routes through `terminal_open_local`
   *  (no SSH, no remote MOTD). The session lands in the same registry
   *  with `host: "local"` so the rail and tab selector handle it
   *  uniformly. */
  async openLocal(
    spec: TerminalOpenLocalSpec = {},
    opts: { rigId?: string | null } = {},
  ): Promise<string> {
    await this.ensureSubscribed();
    const env = await getEnv();
    const clientId = mintClientSessionId();
    const ownerRigId = opts.rigId ?? null;

    const term = new Terminal({
      fontFamily: '"JetBrains Mono", "SF Mono", Menlo, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: "block",
      cols: 120,
      rows: 32,
      scrollback: 10_000,
      allowProposedApi: true,
      macOptionIsMeta: true,
      theme: XTERM_THEME,
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.loadAddon(new Unicode11Addon());
    term.unicode.activeVersion = "11";

    const encoder = new TextEncoder();
    term.onData((data) => {
      const s = this.sessions.get(clientId);
      if (!s?.backendId) return;
      const bytes = encoder.encode(data);
      void env.rpc.terminal.input(s.backendId, bytesToBase64(bytes));
    });
    term.onResize(({ cols, rows }) => {
      const s = this.sessions.get(clientId);
      if (!s?.backendId) return;
      void env.rpc.terminal.resize(s.backendId, cols, rows);
    });

    term.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.metaKey && !e.ctrlKey && !e.altKey && e.key === "k") {
        term.reset();
        const s = this.sessions.get(clientId);
        if (s?.backendId) {
          const ctrlL = bytesToBase64(new Uint8Array([0x0c]));
          void env.rpc.terminal.input(s.backendId, ctrlL);
        }
        return false;
      }
      return true;
    });

    const session: TerminalSession = {
      id: clientId,
      rigId: ownerRigId,
      backendId: null,
      host: "local",
      user: spec.shell ?? "shell",
      label: spec.label ?? "local shell",
      status: "connecting",
      statusDetail: "spawning shell…",
      term,
      fit,
      search,
      attachedTo: null,
    };
    this.sessions.set(clientId, session);
    this.activeId = clientId;
    this.notify();

    void (async () => {
      try {
        const backendId = await env.rpc.terminal.openLocal(spec);
        session.backendId = backendId;
        this.drainPendingFor(backendId);
        this.notify();
      } catch (err) {
        session.status = "error";
        session.statusDetail = err instanceof Error ? err.message : String(err);
        this.notify();
      }
    })();

    return clientId;
  }

  /** Tear down a session. Closes the SSH side, disposes the Terminal,
   *  drops it from the map. */
  async close(id: string) {
    const session = this.sessions.get(id);
    if (!session) return;
    /* Only fire backend close if the handshake actually completed —
       calling terminal_close with a placeholder id would fail. If
       the user closes a session that's still connecting, we drop
       client state and let the in-flight `openSsh` resolve into
       nothing (the Phase-2 IIFE finds no session to update). */
    if (session.backendId) {
      try {
        const env = await getEnv();
        await env.rpc.terminal.close(session.backendId);
      } catch {
        /* swallow — we still want to drop client-side state */
      }
    }
    session.term.dispose();
    this.sessions.delete(id);
    if (this.activeId === id) {
      const next = this.sessions.keys().next();
      this.activeId = next.done ? null : next.value;
    }
    this.notify();
  }

  /** Bind a session's Terminal into a host element. Idempotent: if
   *  the session is already attached to the same element, nothing
   *  changes. Detaches from the previous host if any. */
  attach(id: string, host: HTMLElement) {
    const session = this.sessions.get(id);
    if (!session) return;
    if (session.attachedTo === host) return;
    /* xterm 6's `Terminal.open(host)` is intended to be called once
       per Terminal — calling it again to retarget a different host
       leaves the renderer in a state where the new host shows blank
       (#3357 in xterm.js). The renderer architecture instead gives
       each session its own dedicated host element (see TerminalView's
       SessionPane stack — inactive panes are layered with opacity 0
       rather than unmounted), and `attach` is therefore only ever
       called once per session. The StrictMode double-invocation of
       the mount effect is short-circuited by the equality guard
       above. */
    session.term.open(host);
    session.attachedTo = host;

    /* Default DOM renderer — no addon. xterm 6's Canvas/WebGL addons
       still peer-dep ^5.0.0; on xterm 6 they read theme + cursor via
       the old shape and silently render the cursor invisible and
       drop selectionBackground. The DOM renderer (xterm's default)
       honours the theme via CSS, paints the cursor as a styled
       <span>, and is plenty fast for an interactive shell — perf
       only becomes a renderer concern with sustained 100k+ chars/s
       streams. Wait one rAF so the host has been laid out before we
       measure rows/cols. */
    requestAnimationFrame(() => {
      if (session.attachedTo !== host) return;
      try {
        session.fit.fit();
      } catch {
        /* host not yet sized; resize observer catches up */
      }
    });
  }

  /** No-op in the React StrictMode era — keeping a non-null
   *  attachedTo is what lets the second strict-mode pass of the same
   *  effect short-circuit in `attach` instead of double-rendering the
   *  xterm DOM into the same host. Real teardown happens in `close`,
   *  which calls `term.dispose()`. */
  detach(_id: string) {}

  /** Force a fit pass — called by resize observers when the host's
   *  bounding box changes. */
  fit(id: string) {
    const session = this.sessions.get(id);
    if (!session || !session.attachedTo) return;
    try {
      session.fit.fit();
    } catch {
      /* fit can throw mid-tear-down; safe to ignore */
    }
  }

  private findByBackendId(backendId: string): TerminalSession | undefined {
    for (const s of this.sessions.values()) {
      if (s.backendId === backendId) return s;
    }
    return undefined;
  }

  /** Replay any buffered events whose backendId is now known. Called
   *  immediately after a `session.backendId = …` assignment in the
   *  open/openLocal Phase-2 IIFEs. */
  private drainPendingFor(backendId: string) {
    const remaining: import("../../env/types").WireTerminalEvent[] = [];
    for (const e of this.pendingEvents) {
      if (e.session_id === backendId) {
        this.applyEvent(e);
      } else {
        remaining.push(e);
      }
    }
    this.pendingEvents = remaining;
  }

  private handleEvent(e: import("../../env/types").WireTerminalEvent) {
    const session = this.findByBackendId(e.session_id);
    if (!session) {
      /* Race window: backend emitted before the command's session-id
         response landed in the renderer. Buffer; the Phase-2 IIFE
         in open/openLocal will drain once it sets backendId. */
      this.pendingEvents.push(e);
      return;
    }
    this.applyEvent(e);
  }

  private applyEvent(e: import("../../env/types").WireTerminalEvent) {
    const session = this.findByBackendId(e.session_id);
    if (!session) return;
    switch (e.kind) {
      case "ready":
        session.status = "ready";
        session.statusDetail = undefined;
        /* Terminals connect into a remote MOTD which may leave DEC
           special-graphics (G1/SO) latched if the banner script is
           sloppy about closing its escape sequences. Hard-reset
           xterm state once the PTY is live so the first bash prompt
           renders against a clean charset (otherwise ASCII bytes
           come through as DEC graphics — z→≥, k→┐, ~→π — sometimes
           visible as the random "zzzzz…" prefix line above the
           prompt on a fresh connect). */
        session.term.reset();
        break;
      case "host_key":
        session.hostKeyFingerprint = e.fingerprint;
        break;
      case "data": {
        const bytes = base64ToBytes(e.bytes_b64);
        session.term.write(bytes);
        return;
      }
      case "closed":
        session.status = "closed";
        session.statusDetail = e.reason;
        session.term.write(`\r\n\x1b[33m[session closed: ${e.reason}]\x1b[0m\r\n`);
        break;
      case "error":
        session.status = "error";
        session.statusDetail = e.message;
        session.term.write(`\r\n\x1b[31m[error: ${e.message}]\x1b[0m\r\n`);
        break;
    }
    this.notify();
  }
}

export const terminalStore = new TerminalStore();

/** React hook — re-renders on any change in the session set, the
 *  active id, or any session's status fields. */
export function useTerminalStore(): {
  sessions: TerminalSession[];
  activeId: string | null;
} {
  return useSyncExternalStore(
    (l) => {
      void terminalStore.ensureSubscribed();
      return terminalStore.subscribe(l);
    },
    () => terminalStore.getSnapshot(),
    () => terminalStore.getSnapshot(),
  );
}

/* File-path matcher for the link provider. The regex looks for paths
   that include a directory separator OR end in a recognized source
   extension, optionally followed by `:line` or `:line:col`. The
   leading lookbehind drops matches that are part of an identifier
   (e.g. avoids matching `x.foo.rs` inside `mod.x.foo.rs`).

   Heuristic, not authoritative — false positives can happen on
   word-like segments that resemble paths. The activate handler is
   guarded by cmd/ctrl, so a stray match doesn't trigger a jump
   unless the user explicitly clicks with the modifier held. */
const SOURCE_EXTS = [
  "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "rb", "go",
  "java", "kt", "swift", "c", "h", "cc", "cpp", "hpp", "cs", "ex",
  "exs", "erl", "hrl", "elm", "hs", "ml", "mli", "fs", "fsi", "clj",
  "cljs", "edn", "lisp", "lua", "nim", "cr", "jl", "d", "dart", "nix",
  "sh", "bash", "zsh", "fish", "ps1", "psm1", "toml", "yaml", "yml",
  "json", "xml", "html", "css", "scss", "sass", "vue", "svelte", "md",
  "mdx", "txt", "log", "sql", "graphql", "proto", "tf", "Dockerfile",
];

const FILE_PATH_RE = new RegExp(
  /* path with at least one slash, optional ./ or ~/, then word chars
     and basic punctuation, optionally followed by :line[:col] */
  `(?:\\.{1,2}/|~/|/)?` +
    `[\\w.\\-]+(?:/[\\w.\\-]+)+` +
    `(?:\\.(?:${SOURCE_EXTS.join("|")}))?` +
    `(?::\\d+(?::\\d+)?)?` +
    /* OR a basename with a recognized extension (no slashes) */
    `|` +
    `[\\w.\\-]+\\.(?:${SOURCE_EXTS.join("|")})(?::\\d+(?::\\d+)?)?`,
  "g",
);

interface PathMatch {
  match: string;
  start: number;
  end: number;
}

function matchFilePaths(text: string): PathMatch[] {
  const out: PathMatch[] = [];
  FILE_PATH_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = FILE_PATH_RE.exec(text)) !== null) {
    /* Skip pure-numeric or single-component matches without an
       extension or slash — too noisy as link targets. */
    const s = m[0];
    if (!/[\\/]/.test(s) && !/\.[A-Za-z]/.test(s)) continue;
    out.push({ match: s, start: m.index, end: m.index + s.length });
  }
  return out;
}

function bytesToBase64(bytes: Uint8Array): string {
  /* btoa expects a binary string; manual chunked iteration keeps the
     stack from blowing up on multi-megabyte pastes. */
  let s = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    s += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(s);
}

function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
  return out;
}
