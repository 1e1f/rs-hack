// useFile — load a single rig-relative path through env.rpc.fileRead and
// expose its content + load state for FilesView's Monaco model swap.
//
// Scope is deliberately small: one path → one fetch. Watch-driven
// refresh, mtime-aware writes, and external-change prompts live in
// R033-T14. Binary files (encoding === "base64") surface as `binary`
// state — Monaco gets a placeholder string and FilesView shows a
// "binary file" overlay.

import { useEffect, useState } from "react";
import { getEnv } from "../../env";
import type { WireFileEncoding } from "../../env/types";

export type FileState =
  | { status: "idle" }
  | { status: "loading"; path: string }
  | {
      status: "loaded";
      path: string;
      content: string;
      encoding: WireFileEncoding;
      bytes: number;
      totalBytes: number;
      truncated: boolean;
    }
  | { status: "error"; path: string; message: string };

const IDLE: FileState = { status: "idle" };

export function useFile(rigId: string, path: string | null): FileState {
  const [state, setState] = useState<FileState>(IDLE);

  useEffect(() => {
    if (!path) {
      setState(IDLE);
      return;
    }

    let disposed = false;
    setState({ status: "loading", path });

    (async () => {
      try {
        const env = await getEnv();
        const result = await env.rpc.fileRead(rigId, path);
        if (disposed) return;
        setState({
          status: "loaded",
          path,
          content: result.content,
          encoding: result.encoding,
          bytes: result.bytes,
          totalBytes: result.total_bytes,
          truncated: result.truncated,
        });
      } catch (e) {
        if (disposed) return;
        setState({
          status: "error",
          path,
          message: e instanceof Error ? e.message : String(e),
        });
      }
    })();

    return () => {
      disposed = true;
    };
  }, [rigId, path]);

  return state;
}

/* Map a rig-relative path's extension to a Monaco language id. Defaults
   to "plaintext"; unknown extensions still render (no syntax) instead
   of throwing inside Monaco's tokenizer. The set is the visible-in-this-
   workspace languages — extend as the indexer grows. */
const EXT_TO_LANG: Record<string, string> = {
  rs: "rust",
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  json: "json",
  jsonc: "json",
  md: "markdown",
  mdx: "markdown",
  toml: "ini", // monaco lacks a toml tokenizer; ini is the closest stand-in
  yaml: "yaml",
  yml: "yaml",
  html: "html",
  css: "css",
  scss: "scss",
  sh: "shell",
  bash: "shell",
  zsh: "shell",
  py: "python",
  go: "go",
  java: "java",
  c: "c",
  h: "c",
  cpp: "cpp",
  hpp: "cpp",
  sql: "sql",
  xml: "xml",
  mmd: "markdown", // mermaid renders as markdown until we ship a tokenizer
};

export function languageForPath(path: string): string {
  const dot = path.lastIndexOf(".");
  if (dot < 0 || dot === path.length - 1) return "plaintext";
  const ext = path.slice(dot + 1).toLowerCase();
  return EXT_TO_LANG[ext] ?? "plaintext";
}
