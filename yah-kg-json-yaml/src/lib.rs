//! @arch:layer(kg_lang)
//! @arch:role(extract)
//!
//! `yah-kg-json-yaml` — `LanguageIndexer`s for JSON and YAML config files.
//!
//! Three indexers ship from this crate:
//!
//! * [`JsonIndexer`] for `.json` (also `.jsonc` — comments aren't yet
//!   stripped, so commented files surface as parse errors the daemon
//!   logs and skips).
//! * [`YamlIndexer`] for `.yaml` / `.yml`.
//! * [`TomlIndexer`] for `.toml` (Cargo.toml, tauri.conf.toml, …).
//!   Document + Property nodes only — TOML has no `$ref`/`$schema`
//!   convention, so no RefersTo/ConformsTo edges are emitted.
//!
//! Both produce the same node taxonomy:
//!
//! * **`File`** — the source file itself.
//! * **`Document`** — root container; one per file (multi-doc YAML is
//!   not split — only the first document is walked in v1).
//! * **`Property`** ([`DocKind::Property`]) — every key in an object /
//!   mapping. Qualified path is JSON-Pointer-style:
//!   `<file>#/dependencies/react`. Scalar leaves carry the value as a
//!   `value` property; type kind (`string`, `number`, `bool`, `null`,
//!   `array`, `object`) is on `value_kind`.
//! * **`Anchor`** ([`DocKind::Anchor`]) — YAML `&name` declarations.
//!   Recovered from the `yaml-rust2` token stream (same scanner
//!   libyaml uses). Aliases (`*name`) become `RefersTo` edges from a
//!   synthetic SchemaRef node back to the anchor. `serde_yaml::Value`
//!   resolves anchors in-place before we see them, hence the separate
//!   pre-pass.
//! * **`SchemaRef`** ([`DocKind::SchemaRef`]) — emitted for `$ref`,
//!   `$schema`, package.json `extends`, tsconfig `extends`. Carries
//!   the literal target string as a `target` property and emits a
//!   `RefersTo` edge to the resolved file (or to the anchor for
//!   intra-doc refs).
//!
//! ## Edges
//!
//! * `Contains` — file → document, document → top-level properties,
//!   property → child properties (recursive).
//! * `RefersTo` — schema-ref node → resolved target node. Cross-file
//!   resolution (e.g. `tsconfig.json` `extends` → another file's
//!   Document) is best-effort: we emit the edge with the resolved
//!   path and the store drops it if the target file hasn't been
//!   indexed yet. Pass-3 daemon work to retry on dependency.
//! * `ConformsTo` — document → schema (`$schema` field).
//!
//! ## Span tracking
//!
//! JSON Property nodes carry per-key spans pulled from a side-band
//! tree-sitter-json parse — see [`spans::extract_json_spans`]. YAML
//! Property nodes carry per-key spans from a parallel pass over
//! `yaml-rust2`'s event stream — see [`spans::extract_yaml_spans`].
//! TOML Property nodes carry per-key spans from a side-band
//! `toml_edit::ImDocument` parse — see [`spans::extract_toml_spans`].
//! Merge-resolved YAML keys (`<<: *anchor`) aren't physically present
//! in source and fall back to the file-wide span.
//!
//! ## What this crate intentionally does NOT do
//!
//! * Multi-document YAML streams (`---` separators).
//! * JSON5 / JSONC comment stripping.
//! * Resolution of relative `extends` paths to canonical workspace
//!   paths (the store sees the literal string; Pass 3 normalizes).
//!
//! @yah:ticket(R016-F5, "yah-kg-json-yaml crate using DocKind::Anchor/Property/SchemaRef")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:phase(P1)
//! @yah:parent(R016)
//! @yah:verify("cargo test -p yah-kg-json-yaml")
//! @yah:handoff("Json/Yaml/Toml indexers landed; JSON now also carries per-key spans via tree-sitter-json (spans.rs + Walker::with_spans / SpanLookup trait). YAML+TOML still file-wide — split out as R016-T7 (yaml-rust2 events) and R016-T8 (toml_edit). Workspace green.")
//! @yah:next("Per-format span sources are sub-tickets R016-T7 (YAML) and R016-T8 (TOML); finish those and flip the yaml_and_toml_properties_still_use_file_wide_span_until_followup tripwire test in this crate.")

pub mod indexer;
pub mod spans;
pub mod visit;

pub use indexer::{JsonIndexer, TomlIndexer, YamlIndexer};
