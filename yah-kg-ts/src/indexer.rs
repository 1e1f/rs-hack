//! @arch:layer(kg_lang)
//! @arch:role(extract)
//!
//! `TsIndexer` — entry point implementing `LanguageIndexer`.
//!
//! Picks the right tree-sitter grammar based on the file extension:
//! `.ts` → `language_typescript`, `.tsx` → `language_tsx`. Both grammars
//! produce mostly the same node kinds; the TSX grammar additionally
//! recognizes JSX elements, which the walker uses to flag JSX components.

use crate::visit::Walker;
use std::path::Path;
use yah_kg::indexer::{IndexError, IndexSink, LanguageIndexer};
use yah_kg::kind::Lang;

#[derive(Debug, Default, Clone, Copy)]
pub struct TsIndexer;

impl TsIndexer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageIndexer for TsIndexer {
    fn lang(&self) -> Lang {
        Lang::Ts
    }

    fn extensions(&self) -> &[&'static str] {
        &["ts", "tsx"]
    }

    fn index_file(
        &self,
        path: &Path,
        src: &str,
        sink: &mut dyn IndexSink,
    ) -> Result<(), IndexError> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let is_tsx = path_str.ends_with(".tsx");

        let language = if is_tsx {
            tree_sitter_typescript::LANGUAGE_TSX.into()
        } else {
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
        };

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language)
            .map_err(|e| IndexError::Other(format!("set_language: {e}")))?;

        let tree = parser
            .parse(src, None)
            .ok_or_else(|| IndexError::Parse {
                path: path_str.clone(),
                message: "parser returned None".into(),
            })?;

        // tree-sitter is permissive — it produces a tree with ERROR nodes
        // rather than failing. We don't bail on errors; the walker just
        // skips ERROR-tagged subtrees.
        let mut walker = Walker::new(&path_str, src.as_bytes(), is_tsx, sink);
        walker.run(tree.root_node());
        Ok(())
    }
}
