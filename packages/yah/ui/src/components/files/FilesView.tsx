//! @yah:ticket(R033-T5, "Mount Monaco + monaco-vscode-api in <FilesView>; replace splash")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(.yah/arch/authored/yah-files-tab.md)
//! @yah:handoff("FilesView mounted with monaco-editor (TextMate-only highlighting). Replaced ComingSoon splash. Lazy-loads monaco from src/components/files/FilesView.tsx so the bundle pays for it only when the Files tab is visited. Build: 2905 modules, 8.61MB JS, 0.31MB CSS — Bun extracted monaco's CSS imports automatically into public/dist/main.css and index.html now links it. Yah-parchment theme is a placeholder token-set; the real port (scriptorium / vellum-by-candlelight) lives at R033-T8. MonacoEnvironment.getWorker is a no-op data-URL Worker so create() doesn't throw — language services that need workers are inert; main-thread TextMate syntax coloring is unaffected. Read-only with placeholder Rust source until R033-T7 wires file.read.")
//! @yah:next("monaco-vscode-api migration was deliberately deferred — P2 acceptance is TextMate highlighting (arch doc lines 188-189). The vscode-api upgrade becomes a hard prereq for R033-F9 (KG-overlay extension uses vscode API) and R033-T13 (vscode-languageclient over env.rpc.lsp). Easiest pull-in: do it as the first step of F9 since that's where the vscode shape genuinely earns its bundle weight.")
//! @yah:next("Real Monaco worker setup: Bun does not emit worker chunks today, so tokenization runs main-thread. For files <1MB this is fine; past that the editor jank shows. The Vite '?worker' suffix won't work under Bun; the Bun-native pattern is `new Worker(new URL('./monaco-worker-shim.ts', import.meta.url))` where the shim re-exports monaco's worker entry. Punt until perf complaints surface.")
//! @yah:next("main.css link added to public/index.html — the file existed before this change (xterm extracts there too) but was never linked. Verify nothing else regressed by visiting the Terminal tab.")
//! @yah:gotcha("After bun add monaco-editor, bun.lock and package.json are dirty in addition to the source changes. Pre-existing main.css generation in bun build is now exposed via the new <link> tag — if any xterm style was being patched programmatically that conflicts with main.css, the Terminal tab might shift visually.")
//!
//! @yah:ticket(R033-T7, "useFile hook: file.read + Monaco model swap on URI change")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(.yah/arch/authored/yah-files-tab.md)
//! @yah:handoff("useFile hook + Monaco model swap landed end-to-end. New env.rpc.fileRead(rigId, path, range?) on the Rpc trait (yah-ui/src/env/index.ts), wired in tauri.ts as invoke('file_read', { rigId, params: { path, range } }) — matches the existing #[tauri::command] file_read in app/yah/desktop/src/commands.rs. Browser stub rejects with a clear 'needs the Tauri host' error. New WireFileEncoding / WireFileReadRange / WireFileReadResult mirrors of rpc::FileEncoding/FileReadRange/FileReadResult in env/types.ts. New hook src/components/files/useFile.ts owns the IO + state machine ({ idle | loading | loaded | error }) and re-fires on rigId or path change with a disposed-flag guard for the in-flight switch race. Co-located languageForPath(path) maps extensions to Monaco language ids (rust, typescript, javascript, json, markdown, yaml, html, css, shell, python, go, java, c, cpp, sql, xml, ini-as-toml-stand-in, mermaid-as-markdown). FilesView wires the hook via useFile(rigId, selectedPath); a per-path model cache (Map<path, ITextModel>) survives swap-back so cursor/scroll position is preserved when the user revisits a file in the same session. Cache is wiped on rigId change so cross-rig leaks are impossible. Loading / idle / error states drive a single shared 'plaintext' placeholder model so we don't churn createModel/dispose on every state flip. Binary files (encoding === 'base64') render a binaryPlaceholder() with size; truncated UTF-8 reads render the bytes plus a bottom banner showing 'truncated at X MB of Y MB'. Build: cargo check -p desktop green; bun run typecheck green; bun build:js 2912 modules (was 2906) / 8.67MB JS / 0.31MB CSS. Rule11: T7 stub in commands.rs:109 was deleted (same dedupe pattern T5 used); annotation now lives only in FilesView.tsx.")
//! @yah:next("R033-T8 (theme port: scriptorium + vellum-by-candlelight tokens) is the next P2 sub-ticket. The current YAH_PARCHMENT_THEME in FilesView.tsx is a 5-rule placeholder; T8 lifts the full token table out of globals.css (oklch literals — Monaco won't resolve CSS custom properties) so the editor matches the chrome.")
//! @yah:next("Reveal-in-tree (called out by R033-T6) is now actionable: when selectedPath changes from outside the FileTree (KG-overlay openInFile, future arch.jumpToFile), the tree should walk the path, expand each ancestor via loadDir(), and scroll the leaf into view. Sub-ticket on T6's review (or fold into T9 once that lands).")
//! @yah:next("Watch-driven refresh of an open file is part of T14 (un-readonly + external-change prompt) — file.watch on the active path, mtime dedupe, and the 'file changed on disk' modal. Skipped here per the T7 scope ('file.read + Monaco model swap on URI change').")
//! @yah:verify("cd packages/yah/ui && bun run typecheck")
//! @yah:verify("cd packages/yah/ui && bun run build")
//! @yah:verify("cargo check -p desktop")
//! @yah:gotcha("monaco-editor's createModel + setModel is the primitive that survives URI change; setting editor.value on the same model breaks Monaco's undo stack and confuses the position-state cache the per-path model map relies on. Don't be tempted to 'optimize' by reusing one model and resetting its content.")
//! @yah:gotcha("The placeholder/loading/error states use a shared module-scope plaintextModel (singleton). It's keyed by exact text equality so back-to-back identical 'Loading X' calls reuse the model; calls with different text setValue() in place. Disposed models are re-created. If you ever render two FilesView instances at once, this becomes wrong — split into a useRef-scoped helper.")

import { useEffect, useRef, useState } from "react";
import { Splash } from "../shared/Splash";
import { FileTree } from "./FileTree";
import { languageForPath, useFile } from "./useFile";
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

const EMPTY_PLACEHOLDER = `// No file selected.
//
// Click a file in the tree on the left to open it.
`;

/* Monaco's "binary file" view. Editing it makes no sense; we keep
   the editor mounted (cheaper than tearing it down between selections)
   and surface the byte count. Shown when file.read returns
   encoding === "base64". */
function binaryPlaceholder(path: string, totalBytes: number): string {
  return `// ${path}
//
// Binary file (${totalBytes.toLocaleString()} bytes) — preview
// unavailable. The renderer treats anything that isn't valid UTF-8
// as binary and surfaces this placeholder instead of the raw bytes.
`;
}

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
  /* Cached monaco namespace so the model-swap effect doesn't have to
     re-`await import("monaco-editor")` every time the path changes. */
  const monacoRef = useRef<typeof import("monaco-editor") | null>(null);
  /* Per-path model cache. Switching back to a previously-opened file
     reuses its model so the cursor + scroll position survive the swap.
     Cleared on rig change so models from another rig's filesystem don't
     leak. */
  const modelCacheRef = useRef<Map<string, Monaco.editor.ITextModel>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [ready, setReady] = useState(false);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const file = useFile(rigId, selectedPath);

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
          value: EMPTY_PLACEHOLDER,
          language: "plaintext",
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
        monacoRef.current = monaco;
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
      monacoRef.current = null;
      for (const model of modelCacheRef.current.values()) {
        model.dispose();
      }
      modelCacheRef.current.clear();
    };
  }, []);

  /* Wipe per-path model cache on rig switch — paths under one rig don't
     necessarily exist (or have the same content) under another. */
  useEffect(() => {
    for (const model of modelCacheRef.current.values()) {
      model.dispose();
    }
    modelCacheRef.current.clear();
  }, [rigId]);

  /* Drive the Monaco model from the useFile hook's state. The split is
     deliberate: hook owns IO + state, this effect owns Monaco. Loading
     and idle paint on the editor as plaintext placeholders so the user
     sees one editor with changing content rather than a flashing mount
     dance. Errors fall back to the splash render below. */
  useEffect(() => {
    const editor = editorRef.current;
    const monaco = monacoRef.current;
    if (!editor || !monaco) return;

    if (file.status === "idle") {
      editor.setModel(plaintextModel(monaco, EMPTY_PLACEHOLDER));
      return;
    }
    if (file.status === "loading") {
      editor.setModel(
        plaintextModel(monaco, `// Loading ${file.path}…\n`),
      );
      return;
    }
    if (file.status === "error") {
      // The error pane below renders; keep the editor showing the
      // placeholder so the swap-back-on-recovery path is short.
      editor.setModel(
        plaintextModel(monaco, `// ${file.path}\n//\n// Failed to load.\n`),
      );
      return;
    }

    // status === "loaded"
    const cache = modelCacheRef.current;
    let model = cache.get(file.path);
    if (!model || model.isDisposed()) {
      const text =
        file.encoding === "base64"
          ? binaryPlaceholder(file.path, file.totalBytes)
          : file.content;
      const language =
        file.encoding === "base64" ? "plaintext" : languageForPath(file.path);
      model = monaco.editor.createModel(text, language);
      cache.set(file.path, model);
    }
    editor.setModel(model);
  }, [file]);

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
        {file.status === "error" && ready && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 border-t border-ink-3/30 bg-vellum-2/95 px-3 py-2 text-[12px] text-ink-1">
            <span className="font-medium">Could not read {file.path}:</span>{" "}
            {file.message}
          </div>
        )}
        {file.status === "loaded" && file.truncated && (
          <div className="pointer-events-none absolute inset-x-0 bottom-0 border-t border-ink-3/30 bg-vellum/85 px-3 py-2 text-[12px] text-ink-2">
            File truncated at {(file.bytes / 1024 / 1024).toFixed(1)} MB of{" "}
            {(file.totalBytes / 1024 / 1024).toFixed(1)} MB total.
          </div>
        )}
      </div>
    </div>
  );
}

/* Single shared model for placeholder / loading / error states. Reused
   so we don't churn `createModel`/`dispose` on every state flip. */
let placeholderModel: Monaco.editor.ITextModel | null = null;
let placeholderText: string | null = null;
function plaintextModel(
  monaco: typeof import("monaco-editor"),
  text: string,
): Monaco.editor.ITextModel {
  if (
    placeholderModel &&
    !placeholderModel.isDisposed() &&
    placeholderText === text
  ) {
    return placeholderModel;
  }
  if (placeholderModel && !placeholderModel.isDisposed()) {
    placeholderModel.setValue(text);
    placeholderText = text;
    return placeholderModel;
  }
  placeholderModel = monaco.editor.createModel(text, "plaintext");
  placeholderText = text;
  return placeholderModel;
}
