import { useEffect, useRef } from "react";
import type { ReactNode, RefObject } from "react";

interface MenuProps {
  open: boolean;
  onClose: () => void;
  anchorRef?: RefObject<HTMLElement | null>;
  align?: "left" | "right";
  width?: number | string;
  children: ReactNode;
}

/* Anchored popover used by RigSelector / RelaySelector / SplitMode menu.
   Positions itself absolutely below the parent container; assumes the
   trigger and Menu share a `position: relative` wrapper. */
export function Menu({
  open,
  onClose,
  anchorRef,
  align = "left",
  width,
  children,
}: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handler(e: MouseEvent) {
      const target = e.target as Node;
      if (ref.current?.contains(target)) return;
      if (anchorRef?.current?.contains(target)) return;
      onClose();
    }
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open, onClose, anchorRef]);

  if (!open) return null;

  const alignClass = align === "right" ? "right-0" : "left-0";
  return (
    <div
      ref={ref}
      className={`absolute top-[calc(100%+6px)] ${alignClass} z-50 min-w-[220px] rounded-[5px] border border-rule/50 bg-vellum p-1.5 shadow-[0_2px_4px_rgba(70,45,20,0.12),0_18px_40px_-16px_rgba(70,45,20,0.28)] backdrop-blur-[2px]`}
      style={{ width }}
    >
      {children}
    </div>
  );
}

interface MenuItemProps {
  children: ReactNode;
  onClick?: () => void;
  hint?: ReactNode;
  leading?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
}

export function MenuItem({
  children,
  onClick,
  hint,
  leading,
  disabled,
  danger,
}: MenuItemProps) {
  const colorClass = danger ? "text-oxblood" : "text-ink";
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`flex w-full items-center gap-2 rounded px-2 py-[5px] text-left text-[12px] ${colorClass} hover:bg-vellum-2 disabled:pointer-events-none disabled:opacity-40`}
    >
      {leading && (
        <span className="inline-flex w-3.5 shrink-0 text-ink-3">{leading}</span>
      )}
      <span className="flex-1">{children}</span>
      {hint && <span className="text-[11px] text-ink-3">{hint}</span>}
    </button>
  );
}
