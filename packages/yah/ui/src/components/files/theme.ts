// Monaco themes — scriptorium (light) + vellum-by-candlelight (dark).
//
// The colors are oklch-equivalent literals from the design tokens in
// styles/globals.css; Monaco's theme JSON does not resolve CSS custom
// properties, so a hand-port is the only option. Keep this file in
// sync when globals.css gains a new --color-* on the parchment palette.
//
// Token mapping is conservative: yah's heraldic palette has five
// accent hues and we route them by semantic role rather than syntax
// category, so the editor sits in the same color space as the chrome
// instead of cosplaying Material/Solarized. Roles:
//
//   oxblood  — keywords, control flow, tags                (the loud "verb")
//   forest   — strings, attribute values                   (the quoted "noun")
//   midnight — numbers, constants, namespaces, predefined  (the cool literal)
//   plum     — functions, regexp                           (the named "doer")
//   brass    — types, annotations, attribute names         (the structural "frame")

import type * as Monaco from "monaco-editor";

/* Hex literals mirror styles/globals.css @theme block. Names match the
   CSS custom-property leaf so a future cross-reference is one grep. */
const SCRIPTORIUM = {
  vellum: "fbf4e2",
  vellum2: "f6ecd2",
  ink: "261c12",
  ink2: "4a3a25",
  ink3: "7a6647",
  ink4: "a0896a",
  rule: "c5b186",
  oxblood: "7a1f1f",
  forest: "3a5d3a",
  midnight: "2c3e6c",
  plum: "5a2a52",
  brass: "8a5e1f", // darker than --color-brass for legibility on parchment
};

const VELLUM_BY_CANDLELIGHT = {
  vellum: "1f1a13",
  vellum2: "28201a",
  ink: "ecdcb4",
  ink2: "c8b387",
  ink3: "8e7a55",
  ink4: "6a5a3e",
  rule: "4a3e2a",
  oxblood: "b03a3a",
  forest: "8eb88e", // brighter than --color-forest dark for editor contrast
  midnight: "7a92c8",
  plum: "b07ca4",
  brass: "f0bf60",
};

/* Token rules shared by both themes — only foreground hex differs.
   Builder takes the per-mode palette and emits the rule list. The
   token names follow Monaco's basic-languages tokenizer output (see
   monaco-editor/esm/vs/basic-languages/{rust,typescript,…}); unknown
   tokens fall through to `""` (the editor default). */
function makeRules(p: typeof SCRIPTORIUM): Monaco.editor.ITokenThemeRule[] {
  return [
    { token: "", foreground: p.ink },
    { token: "comment", foreground: p.ink3, fontStyle: "italic" },
    { token: "comment.doc", foreground: p.ink3, fontStyle: "italic" },
    { token: "string", foreground: p.forest },
    { token: "string.escape", foreground: p.brass },
    { token: "string.quote", foreground: p.forest },
    { token: "number", foreground: p.midnight },
    { token: "number.float", foreground: p.midnight },
    { token: "number.hex", foreground: p.midnight },
    { token: "regexp", foreground: p.plum },
    { token: "keyword", foreground: p.oxblood },
    { token: "keyword.flow", foreground: p.oxblood },
    { token: "keyword.json", foreground: p.oxblood },
    { token: "operator", foreground: p.ink3 },
    { token: "delimiter", foreground: p.ink3 },
    { token: "delimiter.bracket", foreground: p.ink2 },
    { token: "delimiter.parenthesis", foreground: p.ink2 },
    { token: "type", foreground: p.brass },
    { token: "type.identifier", foreground: p.brass },
    { token: "namespace", foreground: p.midnight },
    { token: "identifier", foreground: p.ink },
    { token: "function", foreground: p.plum },
    { token: "variable", foreground: p.ink2 },
    { token: "variable.parameter", foreground: p.ink2 },
    { token: "constant", foreground: p.midnight },
    { token: "predefined", foreground: p.midnight },
    { token: "tag", foreground: p.oxblood },
    { token: "tag.id", foreground: p.brass },
    { token: "attribute.name", foreground: p.brass },
    { token: "attribute.value", foreground: p.forest },
    { token: "annotation", foreground: p.brass }, // Rust #[derive(...)]
    { token: "metatag", foreground: p.brass },
    { token: "key.json", foreground: p.brass }, // JSON keys
    { token: "string.value.json", foreground: p.forest },
  ];
}

/* Workbench-level colors keyed by Monaco's color id namespace. The set
   below is the readable subset of editor.* — Monaco accepts more keys
   but we only override what visibly drifts from the parchment chrome. */
function makeColors(p: typeof SCRIPTORIUM, isDark: boolean): Record<string, string> {
  const selectionAlpha = isDark ? "55" : "44";
  const matchAlpha = isDark ? "33" : "33";
  return {
    "editor.background": `#${p.vellum}`,
    "editor.foreground": `#${p.ink}`,
    "editorLineNumber.foreground": `#${p.ink4}`,
    "editorLineNumber.activeForeground": `#${p.ink2}`,
    "editor.selectionBackground": `#${p.brass}${selectionAlpha}`,
    "editor.inactiveSelectionBackground": `#${p.brass}22`,
    "editor.selectionHighlightBackground": `#${p.brass}22`,
    "editor.wordHighlightBackground": `#${p.brass}${matchAlpha}`,
    "editor.findMatchBackground": `#${p.brass}66`,
    "editor.findMatchHighlightBackground": `#${p.brass}33`,
    "editor.lineHighlightBackground": `#${p.vellum2}`,
    "editor.lineHighlightBorder": `#${p.vellum2}`,
    "editorCursor.foreground": `#${p.oxblood}`,
    "editorWhitespace.foreground": `#${p.rule}55`,
    "editorIndentGuide.background1": `#${p.rule}66`,
    "editorIndentGuide.activeBackground1": `#${p.rule}`,
    "editorBracketMatch.background": `#${p.brass}33`,
    "editorBracketMatch.border": `#${p.brass}`,
    "editorGutter.background": `#${p.vellum}`,
    "editorOverviewRuler.border": `#${p.rule}66`,
    "scrollbarSlider.background": `#${p.ink4}66`,
    "scrollbarSlider.hoverBackground": `#${p.ink3}88`,
    "scrollbarSlider.activeBackground": `#${p.ink3}`,
    "minimap.background": `#${p.vellum}`,
  };
}

export const SCRIPTORIUM_THEME: Monaco.editor.IStandaloneThemeData = {
  base: "vs",
  inherit: true,
  rules: makeRules(SCRIPTORIUM),
  colors: makeColors(SCRIPTORIUM, false),
};

export const VELLUM_BY_CANDLELIGHT_THEME: Monaco.editor.IStandaloneThemeData = {
  base: "vs-dark",
  inherit: true,
  rules: makeRules(VELLUM_BY_CANDLELIGHT),
  colors: makeColors(VELLUM_BY_CANDLELIGHT, true),
};

export const SCRIPTORIUM_NAME = "yah-scriptorium";
export const VELLUM_BY_CANDLELIGHT_NAME = "yah-vellum-by-candlelight";

export function registerYahThemes(monaco: typeof import("monaco-editor")): void {
  monaco.editor.defineTheme(SCRIPTORIUM_NAME, SCRIPTORIUM_THEME);
  monaco.editor.defineTheme(VELLUM_BY_CANDLELIGHT_NAME, VELLUM_BY_CANDLELIGHT_THEME);
}

export function themeNameFor(mode: "light" | "dark"): string {
  return mode === "dark" ? VELLUM_BY_CANDLELIGHT_NAME : SCRIPTORIUM_NAME;
}
