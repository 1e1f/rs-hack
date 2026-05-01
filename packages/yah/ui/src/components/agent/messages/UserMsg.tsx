import { Avatar } from "./Avatar";

interface UserMsgProps {
  content: string;
}

export function UserMsg({ content }: UserMsgProps) {
  return (
    <div className="flex gap-3">
      <Avatar kind="user" />
      <div className="min-w-0 flex-1">
        <div className="eyebrow mb-1">You</div>
        <div className="font-display text-[15px] leading-relaxed text-ink">
          {content}
        </div>
      </div>
    </div>
  );
}
