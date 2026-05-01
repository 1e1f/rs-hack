import { Avatar } from "./Avatar";
import { Markdown } from "./Markdown";

interface AssistantMsgProps {
  content: string;
  /* Backed by App.tsx jumpToFile — switches to Architecture tab rooted at the
     given path:line. Optional so the component renders read-only when no
     handler is wired. */
  onJumpToFile?: (fileColon: string) => void;
  /* yah:// link router. Forwarded to <Markdown> for `[label](yah://...)`
     anchors in agent output. */
  onYahLink?: (href: string) => void;
}

/* Renders agent text as markdown — fenced code, headings, lists, and inline
   formatting — via <Markdown>. Cmd-C inside the bubble copies the original
   markdown source instead of the rendered DOM (handler lives in Markdown). */
export function AssistantMsg({
  content,
  onJumpToFile,
  onYahLink,
}: AssistantMsgProps) {
  return (
    <div className="flex gap-3">
      <Avatar kind="agent" />
      <div className="min-w-0 flex-1">
        <div className="eyebrow mb-1">Agent · claude</div>
        <Markdown source={content} onJumpToFile={onJumpToFile} onYahLink={onYahLink} />
      </div>
    </div>
  );
}
