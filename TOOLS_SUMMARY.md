# rs-hack MCP Tool Coverage

The `rs-hack-mcp` server exposes the rs-hack CLI as MCP tools for AI agents.
It mirrors the unified-command CLI surface introduced in v0.5.0 — one tool
per top-level subcommand, with auto-detection of the underlying operation
based on arguments.

## Tools (15)

### Refactor & discovery (10 — original surface)

| MCP tool | What it does |
|---|---|
| `find` | List AST nodes by `--node-type` / `--kind` / `--name`. Discovery mode (omit `--node-type`) auto-groups all types. Better than grep: AST-aware, no false positives. v0.5.5: also accepts `trait-impl` node-type and a `context` parameter for grep-style raw-line context. |
| `add` | Unified add — auto-detects struct field, enum variant, impl method, derive, use statement, match arm, or doc comment from arguments. |
| `remove` | Unified remove — same auto-detection across all entity types. |
| `update` | Unified update — fields, variants, match arms, doc comments. |
| `rename` | Rename functions, trait methods, or enum variants. Defaults to surgical mode (preserves formatting). |
| `transform` | Generic find-and-modify: comment out, remove, or replace any AST nodes. |
| `batch` | Run multiple operations from a JSON/YAML spec file. |
| `history` | List past runs (read-only). |
| `revert` | Undo a run by ID. |
| `clean` | Drop old state. |

### Discovery commands (5 — new in v0.5.5)

| MCP tool | What it does |
|---|---|
| `impls` | List trait implementors (one row per `impl Trait for Type` block). |
| `match_audit` | Per-`match`-site missing-variant report for an enum. Wildcard arms count as covering. |
| `doc_coverage` | Count missing-doc items, list top offenders. `fields=true` descends into struct fields, enum variants, and impl/trait methods. |
| `summary` | Single-file inventory — public items, type counts, function names, public re-exports, module-level doc. |
| `neighbors` | Pure-filesystem siblings, twin dirs (e.g. `tui` → `tui2`), and matching test files. No AST parsing. |

All write tools default to dry-run; pass `apply=true` to apply. The discovery and read-only tools (`find`, `history`, `impls`, `match_audit`, `doc_coverage`, `summary`, `neighbors`) skip the dry-run reminder.

## Implementation notes

- **`find` runs in-process** (since v0.5.5) via `rs_hack::commands::find::run`,
  returning structured JSON. The other tools shell out to the `rs-hack` CLI
  binary, which must be on `$PATH`. Read-only tools (`find`, `history`)
  bypass the dry-run reminder.
- **Auto-detection** happens CLI-side, not in the MCP server. The server is
  a thin shape-mapper: arguments → flags.

## Removed in v0.5.5

The `arch_*` tools (`arch_context`, `arch_query`, `arch_validate`,
`arch_schema`) were removed along with the `rs-hack-arch` crate and the
`[workspace.metadata.arch]` schema.
