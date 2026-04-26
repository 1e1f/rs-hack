//! @arch:layer(kg)
//! @arch:role(extract)
//!
//! `RustIndexer` — entry point implementing `LanguageIndexer`.

use crate::visit::Walker;
use std::path::Path;
use yah_kg::indexer::{IndexError, IndexSink, LanguageIndexer};
use yah_kg::kind::Lang;

/// Default `LanguageIndexer` for `.rs` files.
///
/// Pure: holds no parser state, builds a fresh `Walker` per call so
/// concurrent indexing is safe.
#[derive(Debug, Default, Clone, Copy)]
pub struct RustIndexer;

impl RustIndexer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageIndexer for RustIndexer {
    fn lang(&self) -> Lang {
        Lang::Rust
    }

    fn extensions(&self) -> &[&'static str] {
        &["rs"]
    }

    fn index_file(
        &self,
        path: &Path,
        src: &str,
        sink: &mut dyn IndexSink,
    ) -> Result<(), IndexError> {
        let path_str = path.to_string_lossy().replace('\\', "/");
        let file = match syn::parse_file(src) {
            Ok(f) => f,
            Err(e) => {
                return Err(IndexError::Parse {
                    path: path_str,
                    message: e.to_string(),
                });
            }
        };
        let mut walker = Walker::new(&path_str, sink);
        walker.run(&file);
        Ok(())
    }
}
