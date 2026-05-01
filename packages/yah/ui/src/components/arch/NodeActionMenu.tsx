import { useEffect, useRef } from "react";
import type { ArchNode } from "../../types";

interface NodeActionMenuProps {
  node: ArchNode;
  /* Viewport-coordinate anchor (the click event's clientX/clientY). Menu
     positions itself with a small offset and clamps to the viewport. */
  x: number;
  y: number;
  onClose: () => void;
  onJumpToSource: (node: ArchNode) => void;
  onReroot: (node: ArchNode) => void;
  onOpenInAgent: (node: ArchNode) => void;
}

/* Click-on-node action sheet: Jump to source / Re-root here / Open in agent.
   Floats at fixed position from the click; click-outside or Escape closes. */
export function NodeActionMenu({
  node,
  x,
  y,
  onClose,
  onJumpToSource,
  onReroot,
  onOpenInAgent,
}: NodeActionMenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    function onMouseDown(e: MouseEvent) {
      if (ref.current?.contains(e.target as Node)) return;
      onClose();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  /* Clamp so the 220×~120 menu stays on-screen from a near-edge click. */
  const W = 220;
  const H = 124;
  const left = Math.min(x + 4, window.innerWidth - W - 8);
  const top = Math.min(y + 4, window.innerHeight - H - 8);

  return (
    <div
      ref={ref}
      style={{ left, top, width: W }}
      className="fixed z-50 rounded-[5px] border border-rule/50 bg-vellum p-1.5 shadow-[0_2px_4px_rgba(70,45,20,0.12),0_18px_40px_-16px_rgba(70,45,20,0.28)] backdrop-blur-[2px]"
    >
      <div className="border-b border-rule/40 px-2 pb-1 pt-0.5 font-mono text-[10px] text-ink-4">
        {node.shortName}
      </div>
      <div className="mt-1 flex flex-col">
        <Item onClick={() => { onJumpToSource(node); onClose(); }} hint={`${node.file.split("/").pop()}:${node.line}`}>
          Jump to source
        </Item>
        <Item onClick={() => { onReroot(node); onClose(); }}>
          Re-root here
        </Item>
        <Item onClick={() => { onOpenInAgent(node); onClose(); }}>
          Open in agent
        </Item>
      </div>
    </div>
  );
}

function Item({
  children,
  onClick,
  hint,
}: {
  children: React.ReactNode;
  onClick: () => void;
  hint?: string;
}) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2 rounded px-2 py-[5px] text-left text-[12px] text-ink hover:bg-vellum-2"
    >
      <span className="flex-1">{children}</span>
      {hint && <span className="font-mono text-[10px] text-ink-4">{hint}</span>}
    </button>
  );
}
