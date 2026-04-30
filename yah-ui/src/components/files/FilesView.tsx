//! @yah:ticket(R033-T5, "Mount Monaco + monaco-vscode-api in <FilesView>; replace splash")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(architecture/yah-files-tab.md)
//! @yah:handoff("FilesView mounted with monaco-editor (TextMate-only highlighting). Replaced ComingSoon splash. Lazy-loads monaco from src/components/files/FilesView.tsx so the bundle pays for it only when the Files tab is visited. Build: 2905 modules, 8.61MB JS, 0.31MB CSS — Bun extracted monaco's CSS imports automatically into public/dist/main.css and index.html now links it. Yah-parchment theme is a placeholder token-set; the real port (scriptorium / vellum-by-candlelight) lives at R033-T8. MonacoEnvironment.getWorker is a no-op data-URL Worker so create() doesn't throw — language services that need workers are inert; main-thread TextMate syntax coloring is unaffected. Read-only with placeholder Rust source until R033-T7 wires file.read.")
//! @yah:next("monaco-vscode-api migration was deliberately deferred — P2 acceptance is TextMate highlighting (arch doc lines 188-189). The vscode-api upgrade becomes a hard prereq for R033-F9 (KG-overlay extension uses vscode API) and R033-T13 (vscode-languageclient over env.rpc.lsp). Easiest pull-in: do it as the first step of F9 since that's where the vscode shape genuinely earns its bundle weight.")
//! @yah:next("Real Monaco worker setup: Bun does not emit worker chunks today, so tokenization runs main-thread. For files <1MB this is fine; past that the editor jank shows. The Vite '?worker' suffix won't work under Bun; the Bun-native pattern is `new Worker(new URL('./monaco-worker-shim.ts', import.meta.url))` where the shim re-exports monaco's worker entry. Punt until perf complaints surface.")
//! @yah:next("main.css link added to public/index.html — the file existed before this change (xterm extracts there too) but was never linked. Verify nothing else regressed by visiting the Terminal tab.")
//! @yah:gotcha("After bun add monaco-editor, bun.lock and package.json are dirty in addition to the source changes. Pre-existing main.css generation in bun build is now exposed via the new <link> tag — if any xterm style was being patched programmatically that conflicts with main.css, the Terminal tab might shift visually.")

import { useEffect, useRef, useState } from "react";
import { Splash } from "../shared/Splash";
import { FileTree } from "./FileTree";
import type * as Monaco from "monaco-editor";

/* yah's parchment theme on Monaco. Background + token colors are pulled
   from the design tokens in globals.css (oklch literals here because
   Monaco's theme JSON does not resolve CSS custom properties). The
   proper port — full token table tied to scriptorium /
   vellum-by-candlelight — lands in R033-T8. This is the "good enough"
   placeholder so the editor doesn't look like default VS Code dark in
   the parchment chrome. */
const YAH_PARCHMENT_THEME: Monaco.editor.IStandaloneThemeData = {
  base: "vs",
  inherit: true,
  rules: [
    { token: "comment", foreground: "8a7256", fontStyle: "italic" },
    { token: "keyword", foreground: "7a3d2a" },
    { token: "string", foreground: "5e4a25" },
    { token: "number", foreground: "5e4a25" },
    { token: "type", foreground: "8a4a1f" },
  ],
  colors: {
    "editor.background": "#f4ecd6",
    "editor.foreground": "#3a2f1d",
    "editorLineNumber.foreground": "#a89372",
    "editorLineNumber.activeForeground": "#7a6240",
    "editor.selectionBackground": "#d4c39a",
    "editor.lineHighlightBackground": "#ece2c4",
    "editorCursor.foreground": "#7a3d2a",
    "editorIndentGuide.background1": "#d8c9a8",
  },
};

const PLACEHOLDER_SOURCE = `// Files tab — Monaco shell (R033-T5)
//
// File loading wires in via R033-T7 (useFile hook + file.read RPC).
// File tree wires in via R033-T6 (<FileTree> + dir.watch).
// LSP services come online with R033-T13 (vscode-languageclient).
// Pure TextMate highlighting for now; the upgrade path to
// monaco-vscode-api (for KG-overlay extension + LSP) lands once the
// renderer needs vscode API surfaces.

fn placeholder() {
    println!("hello from monaco");
}
`;

/* Single-instance MonacoEnvironment shim. Monaco wants Web Workers for
   tokenization; the Bun bundle does not currently emit worker chunks
   (R033-T5 follow-up: real worker setup). A no-op data-URL worker keeps
   `monaco.editor.create` from throwing — language services that need
   workers (TS hovers, JSON validation) are inert until then; basic
   TextMate syntax highlighting runs main-thread and is unaffected. */
let monacoEnvInstalled = false;
function ensureMonacoEnv() {
  if (monacoEnvInstalled) return;
  monacoEnvInstalled = true;
  (self as unknown as { MonacoEnvironment?: Monaco.Environment }).MonacoEnvironment = {
    getWorker(_workerId, _label) {
      // Inert worker: monaco posts to it, nothing happens, no throw.
      return new Worker(
        URL.createObjectURL(new Blob([""], { type: "application/javascript" })),
      );
    },
  };
}

interface FilesViewProps {
  rigId: string;
}

export function FilesView({ rigId }: FilesViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const editorRef = useRef<Monaco.editor.IStandaloneCodeEditor | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  /* Selected-file path. Today this just highlights the row in the
     tree; R033-T7 will lift it into the useFile hook + Monaco model
     swap. */
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;

    /* Lazy-load monaco-editor so the main bundle stays slim — the Files
       tab paid for the Monaco bundle only when the user visits it. */
    (async () => {
      try {
        ensureMonacoEnv();
        const monaco = await import("monaco-editor");
        if (disposed || !containerRef.current) return;

        monaco.editor.defineTheme("yah-parchment", YAH_PARCHMENT_THEME);
        const editor = monaco.editor.create(containerRef.current, {
          value: PLACEHOLDER_SOURCE,
          language: "rust",
          theme: "yah-parchment",
          readOnly: true,
          automaticLayout: true,
          fontSize: 13,
          fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          renderLineHighlight: "line",
        });
        editorRef.current = editor;
        setReady(true);
      } catch (e) {
        if (!disposed) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    })();

    return () => {
      disposed = true;
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, []);

  if (error) {
    return (
      <div className="flex h-full items-center justify-center">
        <Splash
          variant="review"
          caption="The scriptorium would not open"
          sub={`Monaco failed to mount: ${error}`}
        />
      </div>
    );
  }

  return (
    <div className="flex h-full w-full overflow-hidden">
      <aside className="h-full w-64 shrink-0 border-r border-ink-3/20 bg-vellum/40">
        <FileTree
          rigId={rigId}
          selectedPath={selectedPath}
          onSelect={setSelectedPath}
        />
      </aside>
      <div className="relative flex-1">
        <div ref={containerRef} className="absolute inset-0" />
        {!ready && (
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center bg-[oklch(var(--color-vellum)/0.5)]">
            <div className="font-display text-[14px] text-ink-2 [font-variant-caps:all-small-caps]">
              Lighting the scriptorium…
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
