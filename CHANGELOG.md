# Changelog

All notable changes to rs-hack will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.5] - 2026-05-01

### ⚠️ Breaking (CLI / scripts only — lib API is fully additive)

- **State env var renamed**: `RS_HACK_STATE_DIR` → `HACK_STATE_DIR`. The new
  variable is treated as the `.hack/` base; `rs/` is appended. Any user
  scripts that exported the old name (or read `runs.json` directly from the
  old layout) will need to update.
- **State directory layout changed**: state now lives under `<base>/rs/`
  (e.g. `./.hack/rs/runs.json`) rather than at `<base>/` directly. The
  `--local-state` flag uses `./.hack/rs/`. This makes room for sibling tools
  in the `.hack/` namespace (e.g. `.hack/ts/`, `.hack/shared/`).
- **`arch_*` MCP tools removed**: the `arch_context`, `arch_query`,
  `arch_validate`, and `arch_schema` tools are gone, along with the
  `rs-hack-arch` crate. The `[workspace.metadata.arch]` schema in
  `Cargo.toml` is removed too.

### Added — discovery commands (response to issue #1)

- **`impls --trait <Name>`**: list trait implementors. Also exposed as a new
  `trait-impl` node-type on `find`; `--kind trait` now expands to both trait
  definitions and their impl blocks.
- **`match-audit --enum <Name>`**: report missing variants per `match` site.
  A `match` is treated as "for enum X" only if at least one arm uses
  `X::Variant` syntax; wildcards count as covering all. Pure syntactic — no
  typecheck.
- **`doc-coverage --paths <p>`**: count missing-doc items, list top offenders.
  `--fields` descends into struct fields, enum variants, impl/trait methods.
- **`summary --path <file>`**: single-file inventory — public items, type
  counts, function names, public re-exports, module-level doc.
- **`neighbors --path <file>`**: pure-filesystem discovery — siblings, twin
  dirs (e.g. `tui` → `tui2`), and matching test files.
- **`find --context N`**: prepend N raw lines before each snippet match,
  similar to `grep -B N`.

### Changed

- The MCP `find` tool now dispatches in-process via the `rs-hack` lib instead
  of shelling out to the CLI binary, and accepts the new `context` parameter
  plus `trait-impl` node-type.
- The MCP server gained five new tools mirroring the discovery commands:
  `impls`, `match_audit`, `doc_coverage`, `summary`, `neighbors` — bringing
  the total to 15.
- `@arch:` annotation comments stripped from all source files (the
  annotation system was deleted along with `rs-hack-arch`).

## [0.5.3] - 2025-11-21

### Added
- **Intelligent Hints for Unmatched Qualified Paths**: When using simple struct names (e.g., `TouchableProps`), rs-hack now detects and reports fully qualified paths that weren't matched (e.g., `crate::view::builder::TouchableProps`)
  - Grouped by qualified path with instance counts
  - Suggests wildcard pattern (`*::StructName`) to match all instances
  - Shows specific path options for targeted matching
  - Works even when some matches were found (warns about unmatched instances)
  - Example output:
    ```
    💡 Hint: Found 6 struct literal(s) with fully qualified paths that didn't match:
       crate::view::builder::TouchableProps (6 instances)

    To match all of these, use:
       rs-hack ... --name "*::TouchableProps" ...
    ```

### Fixed
- **Literal-Only Operations on Imported Structs**: Fixed `add` command failing when operating on struct literals where the struct is imported from another crate
  - Now correctly skips struct definition lookup when using `--field-value` without `--field-type`
  - Allows adding fields to struct literals without needing the struct definition in the file
  - Example: `rs-hack add --name "TouchableProps" --field-name "on_long_press" --field-value "None"` now works on imported structs
  - Previously would fail with "Struct 'TouchableProps' not found" error

## [0.5.1] - 2025-11-12

### Known Issues
- **rshack alias temporarily unavailable**: The `rshack` package (hyphen-free alias) is temporarily unavailable in v0.5.1 due to structural changes. Please use `rs-hack` directly. We'll restore the alias in v0.5.2.

### Added
- **Unified Field API**: New `--field-name`, `--field-type`, and `--field-value` flags for explicit, self-documenting field operations
  - `--field-name` + `--field-type` for struct definitions
  - `--field-name` + `--field-value` for struct literals
  - `--field-name` + both for adding to both definitions and literals
  - Old `--field` API still works but is deprecated
- **Enum Variant Struct Literal Support**: Operations on enum variant struct literals like `View::Grid { ... }`
  - `--kind struct` now includes enum variant struct literals
  - Automatic literal-only mode when targeting enum variants (name contains `::`)
  - Example: `rs-hack add --name View::Grid --field-name layer --field-value None --kind struct --apply`
- **Trait Method Support**: Complete function coverage including trait method definitions
  - `--kind function` now includes trait methods, impl methods, and standalone functions
  - Rename operations work on all function types
  - `visit_trait_item_fn` support for finding and renaming trait methods
- **Struct-Literal Revert Support**: Full revert capability for struct literal operations
  - Backups now preserve exact original source formatting (not token stream)
  - Counter-based matching for reverting specific instances
  - Reverse-order processing to preserve byte offsets

### Changed
- **100% Formatting Preservation**: Surgical editing replaces prettyplease for struct literals
  - `add_struct_literal_field` now uses surgical string insertion
  - `remove_struct_literal_field` now uses surgical string deletion
  - Revert operations preserve exact original formatting
  - Only affected code is reformatted (isolated prettyplease for match arms)
- **Improved Isomorphism**: Consistent behavior across all field operations
  - `add`, `remove`, and `update` commands use same kind expansion logic
  - Enum variants automatically detected and handled correctly
  - Better error messages for unsupported operations

### Fixed
- Revert no longer destroys formatting for struct literal operations
- `--kind function` now correctly finds trait method definitions
- Enum variant struct literals properly recognized by `--kind struct`
- Auto-detection for enum variants in `add` and `remove` commands
- Counter tracking for multiple struct literal instances

### Deprecated
- `--field` flag (use `--field-name` + `--field-type`/`--field-value` instead)
- `--literal-default` flag (use `--field-value` instead)

## [0.5.0] - 2024-XX-XX

### Added
- Unified commands with `--kind` and `--node-type` flags
- MCP server for AI agent integration
- State management and revert functionality
- Surgical editing mode (default) and reformat mode
- Path resolver for safe matching

### Changed
- Major command structure refactoring
- Improved error messages and validation

## [0.4.0] and earlier

See git history for changes in earlier versions.
