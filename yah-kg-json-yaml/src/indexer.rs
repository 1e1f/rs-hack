//! @arch:layer(kg_lang)
//! @arch:role(extract)
//!
//! Entry-point structs implementing `LanguageIndexer` for JSON and YAML.
//! Both delegate to the shared [`crate::visit::Walker`] which is parser-
//! agnostic — it walks a `serde_json::Value` directly. JSON values feed
//! straight in; YAML values are converted via [`crate::visit::yaml_to_json`].
//!
//! @yah:ticket(R016-T7, "YAML per-key spans via yaml-rust2 Parser events")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R016)
//! @yah:next("yaml-rust2 is already a dep; its Parser/EventReceiver exposes Marker(line, col) on each scalar/mapping start. Walk events in parallel with the value tree to build a PointerSpans, then pass into Walker::with_spans from YamlIndexer.")
//! @yah:next("Flip yaml_and_toml_properties_still_use_file_wide_span_until_followup to assert per-key spans once landed (the test is a tripwire on purpose).")
//!
//! @yah:ticket(R016-T8, "TOML per-key spans via toml_edit")
//! @yah:assignee(agent:claude)
//! @yah:status(review)
//! @yah:parent(R016)
//! @yah:verify("cargo test -p yah-kg-json-yaml -- toml")
//! @yah:handoff("Side-band toml_edit::ImDocument pass in spans::extract_toml_spans builds a JSON-Pointer→Span map from Key/Item ranges; TomlIndexer wires it via Walker::with_spans. toml::Value remains the value tree (toml_to_json untouched). Tripwire test flipped to assert per-key spans (lines 2/3/4/7 in CARGO_TOML).")

use crate::spans::{extract_json_spans, extract_toml_spans, extract_yaml_spans};
use crate::visit::{toml_to_json, yaml_to_json, Walker, YamlExtras};
use std::path::Path;
use yah_kg::indexer::{IndexError, IndexSink, LanguageIndexer};
use yah_kg::kind::Lang;

/// `.json` indexer. Pure: holds no parser state; safe to share across
/// concurrent index passes.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonIndexer;

impl JsonIndexer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageIndexer for JsonIndexer {
    fn lang(&self) -> Lang {
        Lang::Json
    }

    fn extensions(&self) -> &[&'static str] {
        &["json"]
    }

    fn index_file(
        &self,
        path: &Path,
        src: &str,
        sink: &mut dyn IndexSink,
    ) -> Result<(), IndexError> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let value: serde_json::Value =
            serde_json::from_str(src).map_err(|e| IndexError::Parse {
                path: path_str.clone(),
                message: e.to_string(),
            })?;
        // Side-band tree-sitter parse builds a per-pointer span map so
        // the walker can place Property nodes on the exact `key: value`
        // line. serde_json's own positions were stripped during deser.
        let spans = extract_json_spans(src);
        let mut walker = Walker::with_spans(
            Lang::Json,
            &path_str,
            src,
            sink,
            YamlExtras::default(),
            Box::new(spans),
        );
        walker.run(&value);
        Ok(())
    }
}

/// `.yaml` / `.yml` indexer.
#[derive(Debug, Default, Clone, Copy)]
pub struct YamlIndexer;

impl YamlIndexer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageIndexer for YamlIndexer {
    fn lang(&self) -> Lang {
        Lang::Yaml
    }

    fn extensions(&self) -> &[&'static str] {
        &["yaml", "yml"]
    }

    fn index_file(
        &self,
        path: &Path,
        src: &str,
        sink: &mut dyn IndexSink,
    ) -> Result<(), IndexError> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let yaml: serde_yaml::Value =
            serde_yaml::from_str(src).map_err(|e| IndexError::Parse {
                path: path_str.clone(),
                message: e.to_string(),
            })?;
        let value = yaml_to_json(&yaml);
        // YAML `&name` / `*name` aren't preserved through `serde_yaml::Value`
        // (anchors are resolved in-place during deserialization). Recover
        // them by scanning the same source through yaml-rust2's token
        // stream so the walker can emit Anchor nodes + RefersTo edges.
        let extras = YamlExtras::scan(src);
        // Per-key spans come from a second pass over yaml-rust2's
        // event stream. Merge-resolved keys (`<<: *anchor`) aren't
        // visible there and fall back to the file-wide span.
        let spans = extract_yaml_spans(src);
        let mut walker = Walker::with_spans(
            Lang::Yaml,
            &path_str,
            src,
            sink,
            extras,
            Box::new(spans),
        );
        walker.run(&value);
        Ok(())
    }
}

/// `.toml` indexer. Day-one targets are `Cargo.toml` / `tauri.conf.toml`
/// so config files appear in the architecture tab. TOML has no `$ref`
/// or `$schema` convention, so this indexer emits Document + Property
/// nodes only — no SchemaRef / RefersTo edges. Path-style cross-crate
/// links (`[dependencies.foo] path = "..."`) are intentionally out of
/// scope for v1; a future pass can lift them once the daemon resolves
/// workspace-relative paths.
#[derive(Debug, Default, Clone, Copy)]
pub struct TomlIndexer;

impl TomlIndexer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageIndexer for TomlIndexer {
    fn lang(&self) -> Lang {
        Lang::Toml
    }

    fn extensions(&self) -> &[&'static str] {
        &["toml"]
    }

    fn index_file(
        &self,
        path: &Path,
        src: &str,
        sink: &mut dyn IndexSink,
    ) -> Result<(), IndexError> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let value: toml::Value = src.parse().map_err(|e: toml::de::Error| IndexError::Parse {
            path: path_str.clone(),
            message: e.to_string(),
        })?;
        let json = toml_to_json(&value);
        // Side-band toml_edit parse exposes per-key spans (the value
        // tree above is consumed via `toml::Value` because the walker
        // already speaks `serde_json::Value`).
        let spans = extract_toml_spans(src);
        let mut walker = Walker::with_spans(
            Lang::Toml,
            &path_str,
            src,
            sink,
            YamlExtras::default(),
            Box::new(spans),
        );
        walker.run(&json);
        Ok(())
    }
}
