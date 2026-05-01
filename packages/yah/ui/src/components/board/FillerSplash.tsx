import { Splash } from "../shared/Splash";
import type { ColumnKey } from "../../types";

interface FillerSplashProps {
  columnKey: ColumnKey;
}

/* Decorative bottom-of-column splash for sparse columns (≤2 cards).
   Pushes itself to the bottom via `mt-auto`, sits at low opacity, and
   ignores pointer events so it never gets in the way of dnd-kit drops or
   scrolling. No caption — the cards above already tell the story. */
export function FillerSplash({ columnKey }: FillerSplashProps) {
  return (
    <div className="pointer-events-none mt-auto flex justify-center pt-6 text-ink-4 opacity-45">
      <Splash variant={columnKey} width={140} />
    </div>
  );
}
