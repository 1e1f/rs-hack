//! @arch:layer(lsp)
//! @arch:role(detection)
//!
//! Static map: file extension → language id → spawn command.
//!
//! The LSP `language id` is the LSP-level discriminator the client and
//! server agree on at `initialize` time. It also doubles as our `server`
//! tag on the wire (`lsp.request { server, method, params }` per
//! `.yah/arch/authored/yah-files-tab.md`): one running language server handles
//! one language id per rig.
//!
//! v1 is hard-coded. The architecture's open question 2 ("Per-language
//! LSP overrides — user-overridable in `~/.yah/config.toml`") is a
//! follow-up; the [`ServerCommand`] shape is already what a config
//! override would deserialize into, so the wiring later is mechanical.

use std::path::Path;

/// Stable language identifier. The string form matches the LSP spec's
/// `TextDocumentItem.languageId` *and* is what the `server` field on
/// `lsp.request` carries — same string both directions, no translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LanguageId {
    Rust,
    TypeScript,
    JavaScript,
    TypeScriptReact,
    JavaScriptReact,
}

impl LanguageId {
    /// LSP-canonical language id string. Matches both
    /// `TextDocumentItem.languageId` and the wire-level `server` tag.
    pub fn as_str(self) -> &'static str {
        match self {
            LanguageId::Rust => "rust",
            LanguageId::TypeScript => "typescript",
            LanguageId::JavaScript => "javascript",
            LanguageId::TypeScriptReact => "typescriptreact",
            LanguageId::JavaScriptReact => "javascriptreact",
        }
    }

    /// Parse a wire `server` tag back into a [`LanguageId`].
    pub fn parse(s: &str) -> Option<LanguageId> {
        match s {
            "rust" => Some(LanguageId::Rust),
            "typescript" => Some(LanguageId::TypeScript),
            "javascript" => Some(LanguageId::JavaScript),
            "typescriptreact" => Some(LanguageId::TypeScriptReact),
            "javascriptreact" => Some(LanguageId::JavaScriptReact),
            _ => None,
        }
    }

    /// Group of language ids that share one server process.
    /// `typescript-language-server` handles all four JS/TS variants
    /// off a single `initialize` — keying the pool by the *server*
    /// rather than the language avoids spawning four idle children.
    pub fn server_key(self) -> ServerKind {
        match self {
            LanguageId::Rust => ServerKind::RustAnalyzer,
            LanguageId::TypeScript
            | LanguageId::JavaScript
            | LanguageId::TypeScriptReact
            | LanguageId::JavaScriptReact => ServerKind::TypeScriptLanguageServer,
        }
    }
}

/// The actual long-running process. Pool keys index on this — one child
/// per (rig_root, server_kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServerKind {
    RustAnalyzer,
    TypeScriptLanguageServer,
}

impl ServerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ServerKind::RustAnalyzer => "rust-analyzer",
            ServerKind::TypeScriptLanguageServer => "typescript-language-server",
        }
    }

    /// Default spawn command for this server. Both binaries run with
    /// `--stdio` to speak LSP over the child's stdin/stdout.
    pub fn default_command(self) -> ServerCommand {
        match self {
            ServerKind::RustAnalyzer => ServerCommand {
                program: "rust-analyzer".to_string(),
                args: vec![],
                env: vec![],
            },
            ServerKind::TypeScriptLanguageServer => ServerCommand {
                program: "typescript-language-server".to_string(),
                args: vec!["--stdio".to_string()],
                env: vec![],
            },
        }
    }
}

/// What [`crate::server::LanguageServer`] needs to launch one instance.
///
/// Keep this struct serializable-shaped: the v1.5 follow-up (per-rig
/// `~/.yah/config.toml` overrides) deserializes straight into this.
#[derive(Debug, Clone)]
pub struct ServerCommand {
    pub program: String,
    pub args: Vec<String>,
    /// `(name, value)` pairs *added* to the child's inherited environment.
    pub env: Vec<(String, String)>,
}

/// Map a file path's extension to its [`LanguageId`].
///
/// Unrecognised extensions return `None` so the server-side LSP
/// multiplex (R033-T12) can refuse the request with a clear "no server
/// configured for `.ext`" error instead of silently dropping it.
pub fn detect(path: &Path) -> Option<LanguageId> {
    let ext = path.extension()?.to_str()?;
    match ext {
        "rs" => Some(LanguageId::Rust),
        "ts" => Some(LanguageId::TypeScript),
        "tsx" => Some(LanguageId::TypeScriptReact),
        "js" | "mjs" | "cjs" => Some(LanguageId::JavaScript),
        "jsx" => Some(LanguageId::JavaScriptReact),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn rust_extension_maps_to_rust_analyzer() {
        let lang = detect(&PathBuf::from("foo/bar.rs")).unwrap();
        assert_eq!(lang, LanguageId::Rust);
        assert_eq!(lang.server_key(), ServerKind::RustAnalyzer);
    }

    #[test]
    fn ts_tsx_share_a_server() {
        let ts = detect(&PathBuf::from("a.ts")).unwrap();
        let tsx = detect(&PathBuf::from("a.tsx")).unwrap();
        assert_ne!(ts, tsx);
        assert_eq!(ts.server_key(), tsx.server_key());
    }

    #[test]
    fn js_variants_use_tsserver() {
        for ext in ["js", "mjs", "cjs", "jsx"] {
            let p = PathBuf::from(format!("a.{ext}"));
            let lang = detect(&p).expect(ext);
            assert_eq!(lang.server_key(), ServerKind::TypeScriptLanguageServer);
        }
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert!(detect(&PathBuf::from("README.md")).is_none());
        assert!(detect(&PathBuf::from("Makefile")).is_none());
    }

    #[test]
    fn parse_round_trips_with_as_str() {
        for lang in [
            LanguageId::Rust,
            LanguageId::TypeScript,
            LanguageId::JavaScript,
            LanguageId::TypeScriptReact,
            LanguageId::JavaScriptReact,
        ] {
            assert_eq!(LanguageId::parse(lang.as_str()), Some(lang));
        }
    }

    #[test]
    fn default_commands_are_stdio_shaped() {
        let ra = ServerKind::RustAnalyzer.default_command();
        assert_eq!(ra.program, "rust-analyzer");
        let tsls = ServerKind::TypeScriptLanguageServer.default_command();
        assert_eq!(tsls.program, "typescript-language-server");
        assert!(tsls.args.iter().any(|a| a == "--stdio"));
    }
}
