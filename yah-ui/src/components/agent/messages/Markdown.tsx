//! @yah:ticket(R036-F2, "Surface @arch:see on ticket cards as yah:// click-through links")
//! @yah:status(open)
//! @yah:assignee(agent:claude)
//! @yah:parent(R036)
//! @yah:next("Render see_also entries on ticket cards (TicketCard.tsx — find current shape) as clickable links")
//! @yah:next("Route via yah://arch/<path> through the existing yah:// router (App.tsx onYahLink)")
//! @yah:next("Confirm yah board show --prompt output also includes @arch:see references for pickup prompts")

import { createElement, type ReactNode, useCallback, useMemo, useRef } from "react";

interface MarkdownProps {
  source: string;
  /* Backed by App.tsx jumpToFile — `path:line` chips inside backticks
     keep the legacy click-to-arch behaviour while fenced code blocks and
     yah:// links flow through the markdown grammar below. */
  onJumpToFile?: (fileColon: string) => void;
  /* yah:// scheme router. yah://file/<path>#L<n> -> jumpToFile equivalent;
     yah://arch/<symbol> -> arch graph re-root. App owns the wiring. */
  onYahLink?: (href: string) => void;
  /* Optional fence-block override. Returning `null|undefined` falls
     through to the default `<pre><code>` rendering. The arch tab uses
     this to swap ```mermaid fences for live diagrams without forking
     the parser. Receives the raw markdown source for the fence so the
     copy-as-source handler keeps working transparently. */
  renderFence?: (
    lang: string | null,
    body: string,
    source: string,
  ) => ReactNode | null | undefined;
}

type Block =
  | { kind: "heading"; level: 1 | 2 | 3 | 4 | 5 | 6; text: string; source: string }
  | { kind: "fence"; lang: string | null; body: string; source: string }
  | { kind: "list"; ordered: boolean; items: string[]; source: string }
  | { kind: "table"; header: string[]; rows: string[][]; source: string }
  | { kind: "para"; text: string; source: string }
  | { kind: "blank" };

/* Tiny markdown renderer aimed at chat output. Handles fenced code,
   ATX headings, ordered/unordered lists, paragraphs, and inline
   `code` / bold / italic / links. Block elements carry their raw
   markdown source as a `data-md-source` attribute so the copy handler
   can swap rendered text for the original markdown. The browser's
   default text serialization already gives the raw body for fenced
   code (it's a <pre> with the source in a text node) — this swap
   matters most for prose with bold/italic/inline-code that would
   otherwise lose the asterisks, underscores, and backticks. */
export function Markdown({
  source,
  onJumpToFile,
  onYahLink,
  renderFence,
}: MarkdownProps) {
  const blocks = useMemo(() => parseBlocks(source), [source]);
  const containerRef = useRef<HTMLDivElement>(null);

  const handleCopy = useCallback((e: React.ClipboardEvent<HTMLDivElement>) => {
    const sel = window.getSelection();
    if (!sel || sel.rangeCount === 0 || sel.isCollapsed) return;
    const range = sel.getRangeAt(0);
    const root = containerRef.current;
    if (!root || !root.contains(range.commonAncestorContainer)) return;
    const md = sliceMarkdownSource(root, range);
    if (md == null) return;
    e.preventDefault();
    e.clipboardData.setData("text/plain", md);
  }, []);

  return (
    <div
      ref={containerRef}
      onCopy={handleCopy}
      className="font-display text-[15px] leading-relaxed text-ink"
    >
      {blocks.map((b, i) =>
        renderBlock(b, i, { onJumpToFile, onYahLink, renderFence }),
      )}
    </div>
  );
}

/* ---------- block parser ---------- */

function parseBlocks(src: string): Block[] {
  const lines = src.split("\n");
  const blocks: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      blocks.push({ kind: "blank" });
      i += 1;
      continue;
    }
    const fence = /^```(\S*)\s*$/.exec(line);
    if (fence) {
      const lang = fence[1] || null;
      const start = i;
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !/^```\s*$/.test(lines[i])) {
        body.push(lines[i]);
        i += 1;
      }
      const closed = i < lines.length;
      if (closed) i += 1;
      const sourceLines = lines.slice(start, closed ? i : lines.length);
      blocks.push({
        kind: "fence",
        lang,
        body: body.join("\n"),
        source: sourceLines.join("\n"),
      });
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length as 1 | 2 | 3 | 4 | 5 | 6;
      blocks.push({ kind: "heading", level, text: heading[2].trimEnd(), source: line });
      i += 1;
      continue;
    }
    if (/^\s*[-*+]\s+/.test(line) || /^\s*\d+\.\s+/.test(line)) {
      const ordered = /^\s*\d+\.\s+/.test(line);
      const items: string[] = [];
      const sourceLines: string[] = [];
      const re = ordered ? /^\s*\d+\.\s+(.*)$/ : /^\s*[-*+]\s+(.*)$/;
      while (i < lines.length) {
        const m = re.exec(lines[i]);
        if (!m) break;
        items.push(m[1]);
        sourceLines.push(lines[i]);
        i += 1;
      }
      blocks.push({ kind: "list", ordered, items, source: sourceLines.join("\n") });
      continue;
    }
    /* GFM-ish pipe table: header row of `| col | col |`, separator row
       of `|---|---|` (alignment colons accepted but ignored), then any
       number of data rows. We don't try to handle every GFM corner —
       this is the shape models actually emit when they want a table. */
    if (isTableRow(line) && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
      const header = splitRow(line);
      const sourceLines: string[] = [line, lines[i + 1]];
      i += 2;
      const rows: string[][] = [];
      while (i < lines.length && isTableRow(lines[i])) {
        rows.push(splitRow(lines[i]));
        sourceLines.push(lines[i]);
        i += 1;
      }
      blocks.push({
        kind: "table",
        header,
        rows,
        source: sourceLines.join("\n"),
      });
      continue;
    }
    // Paragraph: keep gathering until blank, fence, heading, list, or table.
    const paraLines: string[] = [];
    while (i < lines.length) {
      const l = lines[i];
      if (l.trim() === "") break;
      if (/^```/.test(l)) break;
      if (/^#{1,6}\s+/.test(l)) break;
      if (/^\s*[-*+]\s+/.test(l) || /^\s*\d+\.\s+/.test(l)) break;
      if (isTableRow(l) && i + 1 < lines.length && isTableSeparator(lines[i + 1])) {
        break;
      }
      paraLines.push(l);
      i += 1;
    }
    const para = paraLines.join("\n");
    blocks.push({ kind: "para", text: para, source: para });
  }
  return blocks;
}

/* ---------- block renderer ---------- */

interface InlineCtx {
  onJumpToFile?: (fileColon: string) => void;
  onYahLink?: (href: string) => void;
  renderFence?: (
    lang: string | null,
    body: string,
    source: string,
  ) => ReactNode | null | undefined;
}

function renderBlock(b: Block, key: number, ctx: InlineCtx): ReactNode {
  if (b.kind === "blank") return null;
  if (b.kind === "heading") {
    const tag = `h${b.level}`;
    const cls =
      b.level === 1
        ? "mt-2 mb-2 font-display text-[18px] font-medium text-ink"
        : b.level === 2
          ? "mt-2 mb-1.5 font-display text-[16px] font-medium text-ink"
          : "mt-1.5 mb-1 font-display text-[14px] font-medium text-ink-2";
    return createElement(
      tag,
      { key, "data-md-source": b.source, className: cls },
      renderInline(b.text, ctx),
    );
  }
  if (b.kind === "fence") {
    if (ctx.renderFence) {
      const custom = ctx.renderFence(b.lang, b.body, b.source);
      if (custom != null) {
        return (
          <div key={key} data-md-source={b.source} data-md-fence="1">
            {custom}
          </div>
        );
      }
    }
    return (
      <pre
        key={key}
        data-md-source={b.source}
        data-md-fence="1"
        className="my-2 overflow-x-auto rounded border border-rule/40 bg-paper-3/40 px-3 py-2 font-mono text-[12.5px] text-ink"
      >
        <code data-md-lang={b.lang ?? undefined}>{b.body}</code>
      </pre>
    );
  }
  if (b.kind === "list") {
    const Tag = b.ordered ? "ol" : "ul";
    const cls = b.ordered
      ? "my-1.5 ml-5 list-decimal space-y-1"
      : "my-1.5 ml-5 list-disc space-y-1";
    return (
      <Tag key={key} data-md-source={b.source} className={cls}>
        {b.items.map((item, j) => (
          <li key={j} data-md-source={(b.ordered ? `${j + 1}. ` : "- ") + item}>
            {renderInline(item, ctx)}
          </li>
        ))}
      </Tag>
    );
  }
  if (b.kind === "table") {
    return (
      <div
        key={key}
        data-md-source={b.source}
        className="my-2 overflow-x-auto"
      >
        <table className="w-full border-collapse font-display text-[14px]">
          <thead>
            <tr className="border-b border-rule/60">
              {b.header.map((h, j) => (
                <th
                  key={j}
                  className="px-2 py-1 text-left font-medium text-ink"
                >
                  {renderInline(h, ctx)}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {b.rows.map((row, ri) => (
              <tr
                key={ri}
                className="border-b border-rule/25 last:border-b-0"
              >
                {row.map((cell, ci) => (
                  <td key={ci} className="px-2 py-1 align-top text-ink">
                    {renderInline(cell, ctx)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  // paragraph
  return (
    <p key={key} data-md-source={b.source} className="my-1.5">
      {renderInline(b.text, ctx)}
    </p>
  );
}

/* ---------- table helpers ---------- */

function isTableRow(line: string): boolean {
  /* A pipe-table row starts (after optional whitespace) with `|` and has
     at least one more `|`. Single-pipe lines (e.g. ASCII art) fall into
     paragraphs. */
  const trimmed = line.trimStart();
  if (!trimmed.startsWith("|")) return false;
  return trimmed.indexOf("|", 1) !== -1;
}

function isTableSeparator(line: string): boolean {
  /* Separator row: `|`, then each cell is `:?-+:?` plus whitespace.
     We accept alignment colons but render every column left-aligned —
     parchment chrome doesn't have a use for centered tabular data. */
  const trimmed = line.trim();
  if (!trimmed.startsWith("|") || !trimmed.endsWith("|")) return false;
  const inner = trimmed.slice(1, -1);
  if (!inner) return false;
  return inner
    .split("|")
    .every((cell) => /^\s*:?-{1,}:?\s*$/.test(cell));
}

function splitRow(line: string): string[] {
  /* Strip leading + trailing pipes, then split on `|`. Backslash-pipes
     are escapes so the agent can include literal `|` in a cell — same
     rule GFM uses. */
  const trimmed = line.trim().replace(/^\|/, "").replace(/\|$/, "");
  const cells: string[] = [];
  let cur = "";
  let i = 0;
  while (i < trimmed.length) {
    const ch = trimmed[i];
    if (ch === "\\" && trimmed[i + 1] === "|") {
      cur += "|";
      i += 2;
      continue;
    }
    if (ch === "|") {
      cells.push(cur.trim());
      cur = "";
      i += 1;
      continue;
    }
    cur += ch;
    i += 1;
  }
  cells.push(cur.trim());
  return cells;
}

/* ---------- inline parser ----------

   Greedy left-to-right scan over: backtick code, **bold**, _italic_,
   [text](url), and PATH_LINE chips (legacy AssistantMsg behaviour).
   Plain prose passes through unchanged. */

const PATH_LINE = /^([^\s:`]+):(\d+)$/;

export function renderInline(text: string, ctx: InlineCtx): ReactNode[] {
  const out: ReactNode[] = [];
  let i = 0;
  let buf = "";
  let keyCounter = 0;
  const flushBuf = () => {
    if (buf) {
      out.push(<span key={`t${keyCounter++}`}>{buf}</span>);
      buf = "";
    }
  };
  while (i < text.length) {
    const ch = text[i];
    // backtick code
    if (ch === "`") {
      const end = text.indexOf("`", i + 1);
      if (end > i) {
        flushBuf();
        const inner = text.slice(i + 1, end);
        const k = keyCounter++;
        if (PATH_LINE.test(inner) && ctx.onJumpToFile) {
          out.push(
            <button
              key={`p${k}`}
              data-md-source={`\`${inner}\``}
              onClick={() => ctx.onJumpToFile?.(inner)}
              className="rounded border-b border-dashed border-current bg-accent/10 px-1.5 py-px font-mono text-[12.5px] text-accent hover:bg-accent/15"
            >
              {inner}
            </button>,
          );
        } else {
          out.push(
            <code
              key={`c${k}`}
              data-md-source={`\`${inner}\``}
              className="rounded bg-paper-3/30 px-1.5 py-px font-mono text-[12.5px]"
            >
              {inner}
            </code>,
          );
        }
        i = end + 1;
        continue;
      }
    }
    // **bold**
    if (ch === "*" && text[i + 1] === "*") {
      const end = text.indexOf("**", i + 2);
      if (end > i + 1) {
        flushBuf();
        const inner = text.slice(i + 2, end);
        out.push(
          <strong
            key={`b${keyCounter++}`}
            data-md-source={`**${inner}**`}
            className="font-semibold"
          >
            {renderInline(inner, ctx)}
          </strong>,
        );
        i = end + 2;
        continue;
      }
    }
    // _italic_  (we leave bare * for italic alone — it's noisy in prose)
    if (ch === "_") {
      const end = text.indexOf("_", i + 1);
      if (end > i && /\w/.test(text.slice(i + 1, end))) {
        flushBuf();
        const inner = text.slice(i + 1, end);
        out.push(
          <em
            key={`i${keyCounter++}`}
            data-md-source={`_${inner}_`}
            className="italic"
          >
            {renderInline(inner, ctx)}
          </em>,
        );
        i = end + 1;
        continue;
      }
    }
    // [text](url)
    if (ch === "[") {
      const close = text.indexOf("]", i + 1);
      if (close > i && text[close + 1] === "(") {
        const urlEnd = text.indexOf(")", close + 2);
        if (urlEnd > close) {
          flushBuf();
          const label = text.slice(i + 1, close);
          const href = text.slice(close + 2, urlEnd);
          out.push(renderLink(href, label, keyCounter++, ctx));
          i = urlEnd + 1;
          continue;
        }
      }
    }
    buf += ch;
    i += 1;
  }
  flushBuf();
  return out;
}

function renderLink(
  href: string,
  label: string,
  key: number,
  ctx: InlineCtx,
): ReactNode {
  const isYah = href.startsWith("yah://");
  const source = `[${label}](${href})`;
  if (isYah) {
    return (
      <button
        key={`y${key}`}
        data-md-source={source}
        onClick={() => ctx.onYahLink?.(href)}
        className="rounded border-b border-dashed border-current bg-accent/10 px-1 py-px text-accent hover:bg-accent/15"
        title={href}
      >
        {label}
      </button>
    );
  }
  return (
    <a
      key={`a${key}`}
      data-md-source={source}
      href={href}
      target="_blank"
      rel="noreferrer noopener"
      className="text-accent underline decoration-dotted underline-offset-2 hover:text-accent-2"
    >
      {label}
    </a>
  );
}

/* ---------- copy-as-source ---------- */

/* Walk the selection from the closest ancestor with `data-md-source`
   on each end and stitch the source slices together. Returns `null`
   when the selection straddles multiple top-level blocks (in which
   case we let the browser do its default thing — concatenating raw
   markdown across non-contiguous blocks would be misleading). */
function sliceMarkdownSource(root: HTMLElement, range: Range): string | null {
  const startEl = closestSource(range.startContainer, root);
  const endEl = closestSource(range.endContainer, root);
  if (!startEl || !endEl) return null;
  if (startEl !== endEl) {
    // Different leaf elements — fall back to walking up to the nearest
    // shared block ancestor. If they're both inside the same block,
    // emit the full block's source; otherwise bail and let the browser
    // serialize.
    const startBlock = closestBlockSource(startEl);
    const endBlock = closestBlockSource(endEl);
    if (startBlock && startBlock === endBlock) {
      return startBlock.getAttribute("data-md-source") ?? null;
    }
    return null;
  }
  // Single source-element. If the selection covers it fully, return
  // the source verbatim; if it's a sub-string of a text-only node,
  // return the selection's text as-is.
  const fullText = startEl.textContent ?? "";
  const selectedText = range.toString();
  if (selectedText === fullText) {
    return startEl.getAttribute("data-md-source");
  }
  // Partial selection — within a fenced code block we still want raw
  // text (which is just the selected substring), so return it as-is.
  if (startEl.matches('[data-md-fence="1"], [data-md-fence="1"] *')) {
    return selectedText;
  }
  return null;
}

function closestSource(node: Node, root: HTMLElement): HTMLElement | null {
  let cur: Node | null = node;
  while (cur && cur !== root) {
    if (cur.nodeType === Node.ELEMENT_NODE) {
      const el = cur as HTMLElement;
      if (el.hasAttribute("data-md-source")) return el;
    }
    cur = cur.parentNode;
  }
  return null;
}

function closestBlockSource(el: HTMLElement): HTMLElement | null {
  let cur: HTMLElement | null = el;
  while (cur) {
    const tag = cur.tagName;
    if (
      tag === "P" ||
      tag === "PRE" ||
      tag === "UL" ||
      tag === "OL" ||
      /^H[1-6]$/.test(tag)
    ) {
      return cur.hasAttribute("data-md-source") ? cur : null;
    }
    cur = cur.parentElement;
  }
  return null;
}
