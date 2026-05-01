//! @yah:ticket(R033-T6, "<FileTree> component: virtualized + dir.watch subscription")
//! @yah:assignee(agent:claude)
//! @yah:status(in-progress)
//! @yah:phase(P2)
//! @yah:parent(R033)
//! @arch:see(.yah/arch/authored/yah-files-tab.md)
//! @yah:verify("cd yah-ui && bunx tsc --noEmit")
//! @yah:verify("cd yah-ui && bun run build:js")

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getEnv } from "../../env";
import type { WireDirEntry, WireRigFileEvent, WireWatchId } from "../../env/types";

interface FileTreeProps {
  rigId: string;
  /** rig-relative path to the currently-open file. Highlighted in the
   *  tree so the user can see context when files were opened from
   *  outside the tree (e.g. arch.jumpToFile, KG-overlay). */
  selectedPath?: string | null;
  /** Fires when the user activates a file row (single click). The
   *  consumer (FilesView → useFile) decides what to do with the path. */
  onSelect: (relPath: string) => void;
}

/* Node shape for the in-memory tree. Each directory keeps its loaded
   children in a Map keyed by basename, so dir.watch event handlers can
   patch a single entry without re-reading the whole branch. */
interface TreeNode {
  /** Rig-relative path (POSIX). Empty for the rig root. */
  path: string;
  name: string;
  kind: "file" | "dir" | "other";
  size?: number;
  is_symlink?: boolean;
  /** Only meaningful for `kind === "dir"`. Loaded lazily on first
   *  expand. */
  children?: Map<string, TreeNode>;
  /** True once dir.list has populated `children`. */
  loaded?: boolean;
  /** Last error message from a failed dir.list. Surfaces inline so the
   *  user understands why a directory looks empty. */
  error?: string;
}

const ROOT_PLACEHOLDER: TreeNode = {
  path: "",
  name: "",
  kind: "dir",
};

function joinRel(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

function entryToNode(entry: WireDirEntry, parentPath: string): TreeNode {
  return {
    path: joinRel(parentPath, entry.name),
    name: entry.name,
    kind: entry.kind,
    size: entry.size,
    is_symlink: entry.is_symlink,
  };
}

/* Walk to the directory at `path` from `root`. Returns null when any
   segment doesn't resolve to a loaded directory — callers should treat
   that as "stale event for a not-yet-expanded branch" and ignore it. */
function findDir(root: TreeNode, path: string): TreeNode | null {
  if (!path) return root;
  const parts = path.split("/").filter(Boolean);
  let cur = root;
  for (const part of parts) {
    if (!cur.children) return null;
    const next = cur.children.get(part);
    if (!next || next.kind !== "dir") return null;
    cur = next;
  }
  return cur;
}

export function FileTree({ rigId, selectedPath, onSelect }: FileTreeProps) {
  const [root, setRoot] = useState<TreeNode>(() => ({ ...ROOT_PLACEHOLDER }));
  const [expanded, setExpanded] = useState<Set<string>>(() => new Set([""]));
  const [topLevelError, setTopLevelError] = useState<string | null>(null);

  /* Live ref into the root so async event handlers see the latest tree
     without re-binding on every render. setRoot still drives renders. */
  const rootRef = useRef(root);
  rootRef.current = root;

  /* Watch handle for the rig-root subscription. Wiped on rigId change so
     we don't double-watch when the user switches rigs mid-session. */
  const watchIdRef = useRef<WireWatchId | null>(null);

  /* Triggers a re-render after we mutate the tree in place via the
     children Maps. setRoot({...root}) bumps the reference; the Map
     instances inside survive so child branches don't lose their
     loaded state. */
  const bumpRoot = useCallback(() => {
    setRoot((prev) => ({ ...prev }));
  }, []);

  const loadDir = useCallback(
    async (relPath: string) => {
      try {
        const env = await getEnv();
        const r = await env.rpc.dirList(rigId, relPath);
        const dir = findDir(rootRef.current, relPath);
        if (!dir) return; // raced with rig switch / ancestor removal
        const next = new Map<string, TreeNode>();
        for (const entry of r.entries) {
          const existing = dir.children?.get(entry.name);
          /* Preserve already-loaded grandchildren on a re-list — the
             dir.watch refresh path lists the parent again, but we don't
             want to discard expanded sub-trees. */
          const node = entryToNode(entry, relPath);
          if (existing && existing.kind === "dir" && node.kind === "dir") {
            node.children = existing.children;
            node.loaded = existing.loaded;
          }
          next.set(entry.name, node);
        }
        dir.children = next;
        dir.loaded = true;
        dir.error = undefined;
        if (relPath === "") setTopLevelError(null);
        bumpRoot();
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        const dir = findDir(rootRef.current, relPath);
        if (dir) {
          dir.error = msg;
          dir.loaded = true;
          bumpRoot();
        }
        if (relPath === "") setTopLevelError(msg);
      }
    },
    [rigId, bumpRoot],
  );

  /* Boot: list the rig root, arm a recursive dir.watch on it, fan
     events into refreshes for affected (and currently-loaded) dirs. */
  useEffect(() => {
    let disposed = false;
    let unlistenFileEvent: (() => void) | null = null;
    let armedWatchId: WireWatchId | null = null;

    /* Reset tree when rig changes. */
    setRoot({ ...ROOT_PLACEHOLDER });
    setExpanded(new Set([""]));
    setTopLevelError(null);
    watchIdRef.current = null;

    (async () => {
      const env = await getEnv();
      await loadDir("");
      if (disposed) return;

      try {
        armedWatchId = await env.rpc.dirWatch(rigId, "");
        if (disposed) {
          /* Caught a race with rig switch; release the handle we just
             armed so the daemon doesn't accumulate stale watchers. */
          await env.rpc.fileUnwatch(rigId, armedWatchId);
          return;
        }
        watchIdRef.current = armedWatchId;
      } catch (e) {
        if (!disposed) {
          setTopLevelError(
            `dir.watch failed: ${e instanceof Error ? e.message : String(e)}`,
          );
        }
      }

      unlistenFileEvent = await env.rpc.onFileEvent((event: WireRigFileEvent) => {
        if (disposed) return;
        if (event.rig_id !== rigId) return;
        if (
          watchIdRef.current === null ||
          event.watch_id !== watchIdRef.current
        ) {
          return;
        }
        /* Refresh the parent of the changed path — the entry-level
           shape (created/removed/modified) is best-effort, so re-listing
           the parent is the cheapest path to a consistent tree. */
        const lastSlash = event.path.lastIndexOf("/");
        const parent = lastSlash < 0 ? "" : event.path.slice(0, lastSlash);
        const dir = findDir(rootRef.current, parent);
        if (dir && dir.loaded) {
          void loadDir(parent);
        }
      });
    })();

    return () => {
      disposed = true;
      if (unlistenFileEvent) unlistenFileEvent();
      if (armedWatchId !== null) {
        /* Best-effort detach. Errors here are silent — the daemon GC's
           orphan handles when the rig closes. */
        void getEnv().then((env) =>
          env.rpc.fileUnwatch(rigId, armedWatchId!).catch(() => {}),
        );
      }
    };
  }, [rigId, loadDir]);

  const toggleExpanded = useCallback(
    (path: string, isDir: boolean, loaded: boolean) => {
      if (!isDir) return;
      setExpanded((prev) => {
        const next = new Set(prev);
        if (next.has(path)) next.delete(path);
        else next.add(path);
        return next;
      });
      if (!loaded) void loadDir(path);
    },
    [loadDir],
  );

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <div className="border-b border-ink-3/20 px-3 py-2">
        <div className="font-display text-[11px] uppercase tracking-wider text-ink-2 [font-variant-caps:all-small-caps]">
          Files
        </div>
      </div>
      {topLevelError ? (
        <div className="px-3 py-3 text-[12px] text-oxblood">
          {topLevelError}
        </div>
      ) : null}
      <div className="flex-1 overflow-auto py-1 font-mono text-[12px]">
        <TreeRows
          node={root}
          depth={0}
          expanded={expanded}
          selectedPath={selectedPath ?? null}
          onToggle={toggleExpanded}
          onSelect={onSelect}
        />
      </div>
    </div>
  );
}

interface TreeRowsProps {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  selectedPath: string | null;
  onToggle: (path: string, isDir: boolean, loaded: boolean) => void;
  onSelect: (relPath: string) => void;
}

/* Recursive renderer. Sufficient for v1 — most rig tree branches are
   <500 nodes once collapsed; if perf bites under a deeply-expanded
   monorepo, swap this for a flattened virtualized list (R033-T6
   cleanup). */
function TreeRows({
  node,
  depth,
  expanded,
  selectedPath,
  onToggle,
  onSelect,
}: TreeRowsProps) {
  const isRoot = node.path === "" && node.kind === "dir";

  /* Skip rendering the root row itself; its children are the visible
     top of the tree. */
  if (isRoot) {
    return (
      <RootChildren
        node={node}
        depth={depth}
        expanded={expanded}
        selectedPath={selectedPath}
        onToggle={onToggle}
        onSelect={onSelect}
      />
    );
  }
  return null;
}

function RootChildren({
  node,
  depth,
  expanded,
  selectedPath,
  onToggle,
  onSelect,
}: TreeRowsProps) {
  if (!node.loaded) {
    return (
      <div
        className="px-3 py-1 text-ink-2 italic"
        style={{ paddingLeft: 12 + depth * 12 }}
      >
        loading…
      </div>
    );
  }
  if (node.error) {
    return (
      <div
        className="px-3 py-1 text-oxblood"
        style={{ paddingLeft: 12 + depth * 12 }}
      >
        {node.error}
      </div>
    );
  }
  const children = node.children ? Array.from(node.children.values()) : [];
  if (children.length === 0) {
    return (
      <div
        className="px-3 py-1 text-ink-2/60 italic"
        style={{ paddingLeft: 12 + depth * 12 }}
      >
        empty
      </div>
    );
  }
  return (
    <>
      {children.map((child) => (
        <TreeRow
          key={child.path}
          node={child}
          depth={depth}
          expanded={expanded}
          selectedPath={selectedPath}
          onToggle={onToggle}
          onSelect={onSelect}
        />
      ))}
    </>
  );
}

interface TreeRowProps {
  node: TreeNode;
  depth: number;
  expanded: Set<string>;
  selectedPath: string | null;
  onToggle: (path: string, isDir: boolean, loaded: boolean) => void;
  onSelect: (relPath: string) => void;
}

function TreeRow({
  node,
  depth,
  expanded,
  selectedPath,
  onToggle,
  onSelect,
}: TreeRowProps) {
  const isDir = node.kind === "dir";
  const isOpen = expanded.has(node.path);
  const isSelected = !isDir && selectedPath === node.path;

  const onClick = useCallback(() => {
    if (isDir) {
      onToggle(node.path, true, !!node.loaded);
    } else {
      onSelect(node.path);
    }
  }, [isDir, node.path, node.loaded, onSelect, onToggle]);

  const rowClass = useMemo(() => {
    const base =
      "flex w-full items-center gap-1 px-2 py-[1px] text-left hover:bg-vellum-2/40";
    if (isSelected) return `${base} bg-vellum-2/70 text-ink`;
    return `${base} text-ink`;
  }, [isSelected]);

  return (
    <>
      <button
        type="button"
        onClick={onClick}
        className={rowClass}
        style={{ paddingLeft: 8 + depth * 12 }}
        title={node.path}
      >
        <span className="w-3 shrink-0 text-ink-2">
          {isDir ? (isOpen ? "▾" : "▸") : ""}
        </span>
        <span className="truncate">{node.name}</span>
        {node.is_symlink ? (
          <span className="ml-1 text-ink-2/70">↗</span>
        ) : null}
      </button>
      {isDir && isOpen ? (
        <ChildRows
          node={node}
          depth={depth + 1}
          expanded={expanded}
          selectedPath={selectedPath}
          onToggle={onToggle}
          onSelect={onSelect}
        />
      ) : null}
    </>
  );
}

function ChildRows(props: TreeRowProps) {
  const { node, depth } = props;
  if (!node.loaded) {
    return (
      <div
        className="px-3 py-[1px] text-ink-2/70 italic"
        style={{ paddingLeft: 8 + depth * 12 }}
      >
        loading…
      </div>
    );
  }
  if (node.error) {
    return (
      <div
        className="px-3 py-[1px] text-oxblood"
        style={{ paddingLeft: 8 + depth * 12 }}
      >
        {node.error}
      </div>
    );
  }
  const children = node.children ? Array.from(node.children.values()) : [];
  if (children.length === 0) {
    return (
      <div
        className="px-3 py-[1px] text-ink-2/60 italic"
        style={{ paddingLeft: 8 + depth * 12 }}
      >
        empty
      </div>
    );
  }
  return (
    <>
      {children.map((child) => (
        <TreeRow
          key={child.path}
          node={child}
          depth={depth}
          expanded={props.expanded}
          selectedPath={props.selectedPath}
          onToggle={props.onToggle}
          onSelect={props.onSelect}
        />
      ))}
    </>
  );
}
