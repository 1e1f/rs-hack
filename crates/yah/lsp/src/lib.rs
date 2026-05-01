//! @arch:layer(lsp)
//! @arch:role(facade)
//!
//! `yah-lsp` — language server pool for `yah serve`.
//!
//! **What this crate is.** A small Rust library that owns rust-analyzer
//! / typescript-language-server child processes on behalf of `yah serve
//! --stdio`. Callers (the multiplex layer added in R033-T12) hand it a
//! rig root + a path or language id, and get back a [`LanguageServer`]
//! that speaks LSP over a Content-Length-framed JSON-RPC channel. The
//! pool is responsible for:
//!
//! 1. **Language detection** ([`language::detect`]): file extension →
//!    [`language::LanguageId`] → [`language::ServerKind`] (the actual
//!    process to spawn).
//! 2. **Process pool** ([`pool::LspPool`]): one `(rig_root, ServerKind)`
//!    cell, lazy spawn on first reference, shared across concurrent
//!    callers. Per-rig teardown via
//!    [`pool::LspPool::shutdown_rig`] so detaching a rig doesn't leak
//!    rust-analyzer's 100MB resident set.
//! 3. **Framing remap** ([`framing`]): `yah serve --stdio` speaks
//!    line-delimited JSON; LSP servers want HTTP-style `Content-Length`
//!    headers. This crate spans that gap.
//! 4. **Workspace lifecycle**: every freshly spawned server is sent
//!    `initialize` + `initialized` against its rig root before any
//!    caller's request reaches it.
//!
//! **What this crate is _not_.** It does not own the wire shapes for
//! `lsp.request` / `lsp.notification` on `yah serve --stdio` — that's
//! R033-T12's territory. Bidirectional support (server-issued
//! `workspace/configuration` requests, request cancellation) is
//! deliberately out of scope; v1 routes notifications and one-shot
//! requests, which covers the renderer's hover / completion / diagnostic
//! flows.
//!
//! See `.yah/arch/authored/yah-files-tab.md` for the broader Files-tab
//! architecture this slots into.

pub mod framing;
pub mod language;
pub mod pool;
pub mod server;

pub use framing::FramingError;
pub use language::{detect, LanguageId, ServerCommand, ServerKind};
pub use pool::{CommandOverrides, LspPool, PoolError};
pub use server::{
    build_initialize_params, ForwardedResponse, LanguageServer, LspError, NotificationStream,
    ServerNotification,
};
