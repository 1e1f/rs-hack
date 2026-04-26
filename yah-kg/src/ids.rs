//! @arch:layer(kg)
//! @arch:role(schema)
//!
//! Identity types: `NodeId`, `Span`, `NodeRef`, `NodeFull`.
//!
//! `NodeId` is a 16-byte blake3-truncated content hash. It is opaque to
//! consumers and stable across rebuilds as long as the (lang, qualified
//! name, file) triple is unchanged. A rename invalidates the id, which is
//! the correct behaviour: a rename is a remove+add at the graph level.

use crate::kind::{Lang, NodeKind};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

/// Opaque, content-addressed node identifier (16 bytes of blake3).
///
/// Serializes to a lowercase 32-char hex string for JSON friendliness.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub [u8; 16]);

impl NodeId {
    /// Compute a NodeId from the canonical identity triple.
    ///
    /// `qualified` is the language-aware fully qualified name (e.g.
    /// `voice_allocator::mixer::AudioMixer` for Rust, `src/foo.ts::Bar` for
    /// TS). `file` is the rig-relative path. `lang` discriminates so that a
    /// Rust `Foo` and a TS `Foo` at the same path never collide.
    pub fn compute(lang: Lang, qualified: &str, file: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&[lang as u8]);
        hasher.update(qualified.as_bytes());
        hasher.update(&[0]);
        hasher.update(file.as_bytes());
        let hash = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&hash.as_bytes()[..16]);
        NodeId(bytes)
    }

    pub fn to_hex(self) -> String {
        let mut s = String::with_capacity(32);
        for byte in self.0 {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", byte);
        }
        s
    }

    pub fn from_hex(s: &str) -> Result<Self, IdParseError> {
        if s.len() != 32 {
            return Err(IdParseError::Length(s.len()));
        }
        let mut bytes = [0u8; 16];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let hi = hex_nibble(s.as_bytes()[i * 2])?;
            let lo = hex_nibble(s.as_bytes()[i * 2 + 1])?;
            *byte = (hi << 4) | lo;
        }
        Ok(NodeId(bytes))
    }
}

fn hex_nibble(b: u8) -> Result<u8, IdParseError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(IdParseError::NonHex(b as char)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IdParseError {
    #[error("expected 32 hex chars, got {0}")]
    Length(usize),
    #[error("non-hex character {0:?}")]
    NonHex(char),
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.to_hex())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for NodeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for NodeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        NodeId::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// Source span within a file. Lines and columns are 1-based to match
/// the conventions of `proc_macro2::LineColumn` and most editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl Span {
    pub fn point(line: u32, col: u32) -> Self {
        Span { start_line: line, start_col: col, end_line: line, end_col: col }
    }

    pub fn contains_line(&self, line: u32) -> bool {
        line >= self.start_line && line <= self.end_line
    }
}

/// The light-weight node shape returned by subgraph and neighbor queries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRef {
    pub id: NodeId,
    pub lang: Lang,
    pub kind: NodeKind,
    /// Short display name, e.g. `AudioMixer`.
    pub label: String,
    /// Language-aware fully qualified name, e.g.
    /// `voice_allocator::mixer::AudioMixer`.
    pub qualified: String,
    /// Rig-relative path.
    pub file: String,
    pub span: Span,
    /// True when this node was synthesized by macro expansion or a
    /// codegen pass rather than appearing literally in source.
    #[serde(default, skip_serializing_if = "is_false")]
    pub synthetic: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// The full node shape returned by `arch.node` — `NodeRef` plus the
/// indexer-provided property bag and any annotation overlay refs.
///
/// `properties` is a free-form string map per node kind (e.g. Rust
/// `Trait` may carry `auto = "true"`, TS `Decorator` carries
/// `target_kind = "method"`). Keep keys snake_case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeFull {
    #[serde(flatten)]
    pub node: NodeRef,
    /// Doc-comment text attached to the item, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    /// Free-form per-kind metadata.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, String>,
    /// Annotation overlay references (resolved by `rs-hack-arch`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_id_roundtrips_hex() {
        let id = NodeId::compute(Lang::Rust, "foo::bar::Baz", "src/foo.rs");
        let hex = id.to_hex();
        assert_eq!(hex.len(), 32);
        let parsed = NodeId::from_hex(&hex).unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn node_id_is_lang_discriminated() {
        let rust = NodeId::compute(Lang::Rust, "Foo", "src/foo.rs");
        let ts = NodeId::compute(Lang::Ts, "Foo", "src/foo.rs");
        assert_ne!(rust, ts);
    }

    #[test]
    fn node_id_serializes_as_string() {
        let id = NodeId::compute(Lang::Rust, "x", "y");
        let json = serde_json::to_string(&id).unwrap();
        assert!(json.starts_with('"') && json.ends_with('"'));
        let back: NodeId = serde_json::from_str(&json).unwrap();
        assert_eq!(back, id);
    }
}
