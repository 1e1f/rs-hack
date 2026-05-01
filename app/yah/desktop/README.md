# yah-tauri

Tauri host for the yah desktop app. Wraps `yah-kg-daemon` and exposes the
`arch.*` surface to the React frontend in `yah-ui/`.

## Build prerequisites

Tauri 2 requires a system webview. On Linux:

```sh
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev
```

On macOS the system WebKit is used (no extra install). On Windows, WebView2
is bundled with Windows 11; older systems need the Edge runtime.

This crate is **excluded from the workspace's `default-members`** so
`cargo build --workspace` keeps working on machines without webkit. Build
this crate explicitly with `cargo build -p yah-tauri`.

## Running

```sh
# install Tauri CLI once (Cargo or npm — either is fine)
cargo install tauri-cli --version "^2"

# from app/tauri/, with yah-ui dependencies installed:
cd app/tauri
cargo tauri dev
```

`cargo tauri dev` runs `bun run dev` in `yah-ui/` (per `tauri.conf.json`'s
`beforeDevCommand`), then opens a Tauri window pointed at
`http://localhost:5173`.

## Auto-boot

Set `YAH_RIG_ROOT=/path/to/rig` to have the daemon boot the file watcher
on startup. Without it, the frontend opens the rig manually via the
`arch_open_rig` command.

## Commands exposed to the frontend

| Tauri command         | Daemon method            | Purpose                                  |
| --------------------- | ------------------------ | ---------------------------------------- |
| `arch_open_rig`       | `boot` + `start_watching`| Bind to a rig path; idempotent           |
| `arch_close_rig`      | `stop_watching`          | Stop the file watcher                    |
| `arch_subgraph`       | `subgraph`               | BFS subgraph from a root NodeId          |
| `arch_lookup`         | `lookup`                 | Resolve `path[:line]` → innermost NodeIds|
| `arch_node`           | `node`                   | Full NodeRef + doc + props + annotations |
| `arch_neighbors`      | `neighbors`              | In/out edges by edge-kind filter         |
| `arch_roots`          | `roots`                  | Top-level entry-point nodes              |
| `arch_stats`          | `stats`                  | Node/edge counts + last-index timing     |
| `arch_languages`      | `languages`              | Indexers currently registered            |
| `arch_reindex_path`   | `reindex_path`           | Force a per-file reindex (post-edit)     |
| `arch_touch`          | `touch`                  | Forward pi-mono `path:line` tool results |

## Events

The daemon's `ArchEvent` broadcast is forwarded as `arch:event` on Tauri's
event bus. Frontend listeners:

```ts
import { listen } from '@tauri-apps/api/event';
const unlisten = await listen('arch:event', (e) => render(e.payload));
```

## Icon

`icons/icon.png` is required for `cargo tauri build` (production bundles).
Provide a real one before shipping; the dev build runs without it.
