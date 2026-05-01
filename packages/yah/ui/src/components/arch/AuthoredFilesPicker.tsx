//! @yah:ticket(R036-T3, "Migrate architecture/*.md into .yah/arch/authored/ and retire architecture/")
//! @yah:status(review)
//! @yah:assignee(agent:claude)
//! @yah:parent(R036)
//! @yah:next("Move every architecture/*.md → .yah/arch/authored/<same-name>.md (preserves filenames so existing references update mechanically)")
//! @yah:next("grep -r '@arch:see(architecture/' across crates and rewrite to @arch:see(.yah/arch/authored/...)")
//! @yah:next("Once architecture/ is empty, delete the directory and update any remaining doc-only references (READMEs, CLAUDE.md mentions)")
//! @yah:next("Depends on R036-F1 landing — don't move docs before the renderer can show them")

import { useAuthoredFiles } from "../../env/hooks";

interface AuthoredFilesPickerProps {
  rigId: string;
  /* `null` = no manual diagram chosen → ArchView shows the JIT graph. */
  value: string | null;
  onChange: (relPath: string | null) => void;
}

/* Sidebar section listing every `.mmd` and `.md` under
   `<rig>/.yah/arch/authored/`. Selection is exclusive: choosing a file
   swaps the canvas to a renderer keyed by extension (.mmd → raw mermaid
   stage, .md → markdown with inline mermaid-fence specialization);
   the "Live graph" row at the top reverts to the JIT-from-`@arch:`
   annotations render.

   Empty state explains the convention so the user knows where to drop
   new files (per project_yah_arch_dir.md memory). */
export function AuthoredFilesPicker({
  rigId,
  value,
  onChange,
}: AuthoredFilesPickerProps) {
  const { files, loading, error } = useAuthoredFiles(rigId);

  return (
    <div className="flex flex-col gap-1">
      <button
        onClick={() => onChange(null)}
        className={`flex w-full items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[12px] ${
          value === null
            ? "bg-vellum-2 text-ink"
            : "text-ink-3 hover:bg-vellum-2/70 hover:text-ink"
        }`}
      >
        <span className="truncate font-mono">Live graph</span>
        <span className="ml-auto text-[10px] italic text-ink-4">
          @arch: jit
        </span>
      </button>

      {loading && files.length === 0 && (
        <div className="px-2 py-1 text-[11px] italic text-ink-4">
          Looking under .yah/arch/authored…
        </div>
      )}
      {error && (
        <div className="px-2 py-1 text-[11px] italic text-oxblood">
          {error.message}
        </div>
      )}
      {!loading && !error && files.length === 0 && (
        <div className="px-2 py-1 text-[11px] italic text-ink-4">
          No authored .mmd/.md files. Drop one into{" "}
          <span className="font-mono">.yah/arch/authored/</span>.
        </div>
      )}

      {files.map((f) => {
        const selected = f.rel_path === value;
        const ext = extOf(f.rel_path);
        return (
          <button
            key={f.rel_path}
            onClick={() => onChange(f.rel_path)}
            className={`flex w-full items-center gap-1.5 rounded-[3px] px-2 py-[5px] text-left text-[12px] ${
              selected ? "bg-vellum-2 text-ink" : "text-ink-3 hover:bg-vellum-2/70 hover:text-ink"
            }`}
            title={f.rel_path}
          >
            <span className="truncate font-mono">{f.name}</span>
            {ext && (
              <span className="rounded-[2px] bg-paper-3/60 px-1 text-[9px] uppercase tracking-wide text-ink-4">
                {ext}
              </span>
            )}
            <span className="ml-auto tabular-nums text-[10px] italic text-ink-4">
              {formatBytes(f.bytes)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

function extOf(relPath: string): string | null {
  const dot = relPath.lastIndexOf(".");
  if (dot < 0) return null;
  return relPath.slice(dot + 1).toLowerCase();
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n}B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)}K`;
  return `${(n / (1024 * 1024)).toFixed(1)}M`;
}
